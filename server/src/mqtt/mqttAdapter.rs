use crate::appState::MtxAppState;
use crate::common::IntMsg;
use crate::configStore::ConfigStore;
use crate::configStore::MqttServerInfo;
use crate::logger::eventTypes::LogEvent;
use crate::mqtt::mqttClient::MqttClient;
use crate::util::notifyWithState::NotifyWithState;

use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::Duration;
use tracing::{error, info};

const DUR_RECONNECT_MQTT_DELAY: Duration = Duration::from_secs(5);

struct MqttConfigState {
    cfg: Arc<ConfigStore>,
    currentIndex: usize,
    countConfigs: usize,
    currentAddressAndPort: Option<String>,
}

impl MqttConfigState {
    pub fn new(cfg: Arc<ConfigStore>) -> Self {
        Self {
            cfg: cfg.clone(),
            currentIndex: 0,
            countConfigs: cfg.mqttServers.len(),
            currentAddressAndPort: None,
        }
    }

    pub fn getCurrentConfig(&mut self) -> Option<MqttServerInfo> {
        if let Some(currentConfig) = self.cfg.mqttServers.get(self.currentIndex) {
            self.currentAddressAndPort = Some(format!(
                "{}:{} (entry {})",
                currentConfig.host,
                currentConfig.port,
                self.currentIndex + 1
            ));
            return Some(currentConfig.clone());
        }
        None
    }

    pub fn moveToNextConfig(&mut self) -> bool {
        if self.countConfigs <= 1 {
            return false;
        }
        self.currentIndex = (self.currentIndex + 1) % self.countConfigs;
        true
    }

    pub fn getCurrentConnInfoStr(&self) -> String {
        self.currentAddressAndPort.clone().unwrap_or_else(|| "Unknown".to_string())
    }
}

struct MqttState {
    eventloop: Option<rumqttc::EventLoop>,
    mqttClient: MqttClient,
    rx: mpsc::Receiver<IntMsg>,
    connected: bool,
    cfgState: MqttConfigState,
    cancelNotify: Arc<NotifyWithState>,
}

pub struct MqttAdapter {
    pub eventloopTask: tokio::task::JoinHandle<()>,
}

impl MqttAdapter {
    pub async fn new(
        tx: mpsc::Sender<IntMsg>,
        ruleTx: mpsc::Sender<IntMsg>,
        rx: mpsc::Receiver<IntMsg>,
        cfg: Arc<ConfigStore>,
        cancelNotify: Arc<NotifyWithState>,
        appState: MtxAppState,
        logTx: mpsc::Sender<LogEvent>,
    ) -> Self {
        let eventloopTask = tokio::spawn(async move {
            let mut state = MqttState {
                eventloop: None,
                mqttClient: MqttClient::new(tx.clone(), ruleTx.clone(), appState.clone(), logTx.clone()),
                rx,
                connected: false,
                cfgState: MqttConfigState::new(cfg.clone()),
                cancelNotify: cancelNotify.clone(),
            };

            // Main reconnect loop
            // Here we connect to the MQTT broker, which creates the client and eventloop, and run the main event loop.
            loop {
                // Pick config
                let currentConfigOpt = state.cfgState.getCurrentConfig();
                if currentConfigOpt.is_none() {
                    error!("No MQTT server configuration found");
                    state.cancelNotify.signal();
                    break;
                }
                let currentConfig = currentConfigOpt.unwrap();
                appState.write().await.volatileState.mqttHost = Some(currentConfig.host.clone());
                tx.send(IntMsg::StateUpdate).await.unwrap_or_default();

                // This creates the inner MQTT client/eventloop and tries to connect to the broker.
                let client = establishConnectionAndMqttClient(&mut state, &cfg, &currentConfig).await;

                // Run the main events loop, which exists
                runMainEventAndMessageLoop(&mut state).await;
                if state.cancelNotify.isSignaled() {
                    break;
                }

                if state.connected {
                    info!("Disconnecting MQTT client from {}.", state.cfgState.getCurrentConnInfoStr());
                    client.disconnect().await.ok();
                    state.connected = false;
                }
                state.eventloop = None; // Drop eventloop first to stop processing
                state.mqttClient = MqttClient::new(tx.clone(), ruleTx.clone(), appState.clone(), logTx.clone());
            }
        });

        Self {
            eventloopTask: eventloopTask,
        }
    }
}

async fn establishConnectionAndMqttClient(
    state: &mut MqttState,
    cfg: &Arc<ConfigStore>,
    currentConfig: &crate::configStore::MqttServerInfo,
) -> AsyncClient {
    info!("Trying to connect to MQTT broker at {}...", state.cfgState.getCurrentConnInfoStr());

    let mqttOptions = MqttOptions::new(cfg.mqttClientId.clone(), currentConfig.host.clone(), currentConfig.port);
    let (client, eventloop) = AsyncClient::new(mqttOptions.clone(), 10);

    for topic in state.mqttClient.getSubscribedTopics() {
        if let Err(e) = client.subscribe(topic.clone(), QoS::AtMostOnce).await {
            error!("Subscribe error for topic '{}': {:?}", topic.clone(), e);
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }
    }

    let clientForCallback = client.clone();
    let publishCallback = Arc::new(move |topic: String, payload: Vec<u8>| {
        let client = clientForCallback.clone();
        tokio::spawn(async move {
            if let Err(err) = client.publish(topic, QoS::AtMostOnce, false, payload).await {
                error!("Failed to publish from callback: {:?}", err);
            }
        })
    });
    state.mqttClient.setPublishCallback(publishCallback);
    state.eventloop = Some(eventloop);

    client
}

async fn runMainEventAndMessageLoop(state: &mut MqttState) {
    loop {
        if let Some(eventloop) = &mut state.eventloop {
            tokio::select! {
                // Notification from the main function (e.g., SIGINT)
                _ = state.cancelNotify.notified() => {
                    info!("MqttAdapter: cancellation requested, shutting down...");
                    break;
                },

                // Incoming MQTT IntMsg
                event = eventloop.poll() => {
                    if !handleIncomingEvent(state, event).await {
                        if state.cfgState.moveToNextConfig() {
                            info!("Moving to next MQTT config");
                        }
                        break;
                    }
                },

                // Internal message (e.g., from HTTP server)
                Some(msg) = state.rx.recv() => {
                    state.mqttClient.handleInternalMessage(msg).await;
                },

                // MQTT client requests immediate tick, e.g., for sending a message
                _ = state.mqttClient.needsTickImmediately() => {
                    state.mqttClient.tick().await;
                },

                // At latest, we tick every second to keep track about timeouts
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    state.mqttClient.tick().await;
                },
            }

            // If the inner client is not connected for a while and we have multiple configs,
            // request new connection parameters and move to the next config.
            if state.mqttClient.requestNewConnectionParams() && state.cfgState.moveToNextConfig() {
                info!(
                    "MQTT connection has been disconnected for a while from {}, moving to next config.",
                    state.cfgState.getCurrentConnInfoStr()
                );
                break;
            }
        } else {
            // This should not happen.
            error!("MQTT event loop is not initialized");
            state.cancelNotify.signal();
            break;
        }
    }
}

async fn handleIncomingEvent(state: &mut MqttState, event: Result<rumqttc::Event, rumqttc::ConnectionError>) -> bool {
    match event {
        Ok(Event::Incoming(Packet::Publish(p))) => {
            let topic = p.topic;
            let payload = String::from_utf8_lossy(&p.payload).to_string();
            state.mqttClient.handleMqttMessage(topic, payload).await;
        }
        Ok(_) => {
            if !state.connected {
                info!("MQTT connected to {}.", state.cfgState.getCurrentConnInfoStr());
                state.mqttClient.onConnected().await;
                state.connected = true;
            }
        }
        Err(_) => {
            if state.connected {
                info!("MQTT disconnected from {}.", state.cfgState.getCurrentConnInfoStr());
                state.mqttClient.onDisconnected().await;
                state.connected = false;
            } else {
                info!("MQTT connection to {} failed.", state.cfgState.getCurrentConnInfoStr());
            }
            tokio::time::sleep(DUR_RECONNECT_MQTT_DELAY).await;
            return false;
        }
    }
    return true;
}
