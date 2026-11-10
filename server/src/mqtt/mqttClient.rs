use crate::appState::MtxAppState;
use crate::common::{ConnectionState, IntMsg};
use crate::logger::eventTypes::{LogEvent, LogEventType};
use crate::mqtt::mqttProtocol::{MsgCmd, MsgResponse, MsgRing, Topic};

use chrono::{DateTime, Utc};
use std::collections::VecDeque;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::Duration;
use tracing::{error, info};

// --------------------------- Constants and Enums ---------------------------

const DUR_RECONNECT_DEVICE_DELAY: Duration = Duration::from_secs(5);
const DUR_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const DUR_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DUR_ACTIVITY_REFRESH: Duration = Duration::from_secs(20);
const DUR_MIN_REBOOT_INTERVAL: Duration = Duration::from_hours(2);

#[derive(PartialEq, Clone)]
enum SendRecState {
    Idle,
    WaitForResponse,
}

#[derive(PartialEq, Clone, Copy)]
enum ResponseKind {
    Normal,
    Timeout,
    NotConnected,
}

// --------------------------- MQTT Client ---------------------------

pub struct MqttClient {
    httpTx: mpsc::Sender<IntMsg>,
    ruleTx: mpsc::Sender<IntMsg>,
    logTx: mpsc::Sender<LogEvent>,
    publishCallback: Option<Arc<dyn Fn(String, Vec<u8>) -> tokio::task::JoinHandle<()> + Send + Sync>>,
    commandQueue: VecDeque<MsgCmd>,
    notifyNewCommand: tokio::sync::Notify,
    actionLog: Vec<String>,
    commandSent: Option<MsgCmd>,
    connState: ConnectionState,
    tsLastActivity: Option<DateTime<Utc>>,
    tsCommandSent: Option<DateTime<Utc>>,
    tsDeviceDisconnectedSince: Option<DateTime<Utc>>,
    sendRecState: SendRecState,
    appState: MtxAppState,
    // Last rule-based auto-buzz state reported by the RuleService, None until it reported once.
    // Cached to re-send it to the device after a reconnect - the device does not persist it.
    ruleAutoBuzz: Option<bool>,
}

impl MqttClient {
    // ------------------------------- Interface -------------------------------

    pub fn new(httpTx: mpsc::Sender<IntMsg>, ruleTx: mpsc::Sender<IntMsg>, appState: MtxAppState, logTx: mpsc::Sender<LogEvent>) -> Self {
        Self {
            httpTx,
            ruleTx,
            logTx,
            publishCallback: None,
            commandQueue: VecDeque::new(),
            notifyNewCommand: tokio::sync::Notify::new(),
            actionLog: Vec::new(),
            commandSent: None,
            connState: ConnectionState::Disconnected,
            tsLastActivity: None,
            tsCommandSent: None,
            tsDeviceDisconnectedSince: Some(Utc::now()),
            sendRecState: SendRecState::Idle,
            appState: appState.clone(),
            ruleAutoBuzz: None,
        }
    }

    pub fn setPublishCallback(&mut self, cb: Arc<dyn Fn(String, Vec<u8>) -> tokio::task::JoinHandle<()> + Send + Sync>) {
        self.publishCallback = Some(cb);
    }

    pub async fn onConnected(&mut self) {
        self.setConnState(ConnectionState::MqttConnected).await;
        self.handleDeviceConnection();
    }

    pub async fn onDisconnected(&mut self) {
        self.setConnState(ConnectionState::Disconnected).await;
        self.tsLastActivity = None;
    }

    pub async fn handleMqttMessage(&mut self, topic: String, message: String) {
        match topic.parse::<Topic>() {
            Ok(Topic::DoorRing) => {
                match MsgRing::parse(&message) {
                    Some(MsgRing::AutoBuzzOn) => {
                        self.logEvent(LogEvent::new(LogEventType::DeviceAutoBuzzOn, "")).await;
                        self.sendInternal(IntMsg::SetDeviceAutoBuzz(true)).await;
                    }
                    Some(MsgRing::AutoBuzzOff) => {
                        self.logEvent(LogEvent::new(LogEventType::DeviceAutoBuzzOff, "")).await;
                        self.sendInternal(IntMsg::SetDeviceAutoBuzz(false)).await;
                    }
                    Some(MsgRing::Ring { autoBuzz, timestamp }) => {
                        info!("Ring received at {} (auto buzz: {})", timestamp, autoBuzz);
                        let ringEvent = if autoBuzz { LogEventType::RingAutoBuzz } else { LogEventType::Ring };
                        self.logEvent(LogEvent::new(ringEvent, "")).await;
                        // An auto-buzz on a ring means the device opened the door itself.
                        if autoBuzz {
                            self.logEvent(LogEvent::new(LogEventType::DeviceBuzzerAuto, "")).await;
                        }
                        // Notify the RuleService so active rules can react (buzz/auto-ack/single-shot).
                        self.ruleTx.send(IntMsg::RingOccurred(autoBuzz)).await.unwrap_or_default();
                    }
                    Some(MsgRing::TestRing { timestamp }) => {
                        info!("Test ring received at {}", timestamp);
                        self.logEvent(LogEvent::new(LogEventType::TestRing, "")).await;
                    }
                    Some(MsgRing::AckRing) => {
                        info!("Ring acknowledged by device");
                        self.logEvent(LogEvent::new(LogEventType::AckRing, "")).await;
                    }
                    Some(MsgRing::DeviceBuzz) => {
                        info!("Device buzzer activated (physical button)");
                        self.logEvent(LogEvent::new(LogEventType::DeviceBuzzerManual, "")).await;
                    }
                    None => {}
                }
            }
            Ok(Topic::Response) => {
                // buzzAck is the reply to the fire-and-forget "buzz" command, so it is not awaited
                // by the command-response state machine. Log it here before handing off.
                if MsgResponse::from_str(&message) == Ok(MsgResponse::BuzzAck) {
                    self.logEvent(LogEvent::new(LogEventType::DeviceBuzzerServerCommanded, "")).await;
                }
                self.handleCommandResponse(ResponseKind::Normal, &message).await;
            }
            _ => {}
        }

        self.tsLastActivity = Some(Utc::now());
    }

    pub async fn handleInternalMessage(&mut self, message: IntMsg) {
        // Handle internal messages from HTTP server.
        // Don't send MQTT commands directly, as we could wait for another command response.
        match message {
            IntMsg::Buzz => {
                self.addToCommandQueue(MsgCmd::BuzzDoor);
            }
            IntMsg::AckRing => {
                self.addToCommandQueue(MsgCmd::AckRing);
            }
            IntMsg::Reboot => {
                info!("Rebooting device due to user request from HTTP interface.");
                self.doReboot().await;
            }
            IntMsg::GetDeviceLog => {
                self.addToCommandQueue(MsgCmd::GetActionLog);
            }
            IntMsg::SetDeviceAutoBuzz(autoBuzzOnOff) => {
                if autoBuzzOnOff {
                    self.addToCommandQueue(MsgCmd::SetAutoBuzzOn);
                } else {
                    self.addToCommandQueue(MsgCmd::SetAutoBuzzOff);
                }
            }
            IntMsg::SetRuleAutoBuzz(activeByRule) => {
                self.ruleAutoBuzz = Some(activeByRule);
                self.queueRuleAutoBuzzState(activeByRule);
            }
            _ => {}
        }
    }

    pub async fn publishMessage(&self, topic: String, message: String) {
        if let Some(cb) = &self.publishCallback {
            cb(topic, message.into_bytes());
        }
    }

    // Answers a command which is still waiting for its response as "not connected" when stopping the client.
    pub async fn failPendingCommand(&mut self) {
        if self.sendRecState == SendRecState::WaitForResponse {
            info!("Connection lost while waiting for a response, answering as not connected.");
            self.handleCommandResponse(ResponseKind::NotConnected, "[not_connected]").await;
        }
    }

    pub fn getSubscribedTopics(&self) -> Vec<String> {
        [Topic::DoorRing, Topic::Response].iter().map(|topic| topic.to_string()).collect()
    }

    pub async fn tick(&mut self) {
        self.handleDeviceConnection();
        self.handleCommandQueueAndResponseFailures().await;
    }

    pub fn needsTickImmediately(&self) -> tokio::sync::futures::Notified<'_> {
        // We want to send a command, so notify the MqttAdapter loop to tick us immediately.
        self.notifyNewCommand.notified()
    }

    pub fn requestNewConnectionParams(&self) -> bool {
        self.connState != ConnectionState::DeviceConnected
            && self.tsDeviceDisconnectedSince.is_some()
            && self.tsDeviceDisconnectedSince.unwrap() + DUR_CONNECT_TIMEOUT < Utc::now()
    }

    // --------------------------- Commands & Responses ---------------------------

    async fn handleCommandQueueAndResponseFailures(&mut self) {
        let now = Utc::now();
        if self.sendRecState == SendRecState::Idle && !self.commandQueue.is_empty() {
            // Prepare to send the current command in the queue
            let cmd = self.commandQueue.pop_front().unwrap();
            self.commandSent = Some(cmd.clone());
            self.sendRecState = if cmd.needsResponse() {
                SendRecState::WaitForResponse
            } else {
                SendRecState::Idle
            };
            if cmd == MsgCmd::GetActionLog {
                self.actionLog.clear();
            }
            self.tsCommandSent = Some(now);

            // Either directly send it or answer as "not connected".
            let connState = self.connState.clone();
            let hasMqttConn = connState == ConnectionState::MqttConnected || connState == ConnectionState::DeviceConnected;
            if connState == ConnectionState::DeviceConnected || (hasMqttConn && cmd == MsgCmd::Ping) {
                // Only send if connected. Ping is allowed if not connected, as it is used to establish the connection.
                self.sendMqttCmd(cmd).await;
            } else {
                self.handleCommandResponse(ResponseKind::NotConnected, "[not_connected]").await;
            }
        } else if self.sendRecState == SendRecState::WaitForResponse {
            // If we are waiting for a response, check for timeout.
            if let Some(ts) = self.tsCommandSent {
                if (ts + DUR_COMMAND_TIMEOUT) < now {
                    self.handleCommandResponse(ResponseKind::Timeout, "[timeout]").await;
                }
            }
        }
    }

    async fn handleCommandResponse(&mut self, responseKind: ResponseKind, response: &str) {
        self.handleConnectivityStateByResponseKind(responseKind).await;
        // If this returns true, we are still waiting for more response content (multi-response).
        if !self.consumeReponseContent(responseKind, response).await {
            self.sendRecState = SendRecState::Idle;
        }
    }

    async fn handleConnectivityStateByResponseKind(&mut self, responseKind: ResponseKind) {
        // (Reduced version of RingListener::handleInternalStateByResponse)
        // Update the internal state according to the response received.

        if responseKind == ResponseKind::Normal {
            self.updateTsLastActivity();
        } else if responseKind == ResponseKind::Timeout {
            if self.connState == ConnectionState::DeviceConnected {
                info!("Command response timeout, disconnecting from device");
                self.setConnState(ConnectionState::MqttConnected).await;
            }
        }
    }

    // Returns true if more response content is expected (multi-response).
    async fn consumeReponseContent(&mut self, responseKind: ResponseKind, response: &str) -> bool {
        if self.sendRecState != SendRecState::WaitForResponse {
            return false; // not waiting for a response
        }

        let cmd = if let Some(cmd) = self.commandSent {
            cmd.clone()
        } else {
            return false; // no command sent, so no response expected
        };

        if cmd.answeredByMultiResponse() {
            return self.consumeMultiResponse(cmd, responseKind, response).await;
        } else {
            self.consumeSingleResponse(cmd, responseKind, response).await;
            // Not expecting multi response, always return false
            false
        }
    }

    async fn consumeSingleResponse(&mut self, cmd: MsgCmd, responseKind: ResponseKind, response: &str) {
        match MsgResponse::from_str(response) {
            Ok(MsgResponse::Pong) => {
                let alreadyConnected = self.connState == ConnectionState::DeviceConnected;
                if !alreadyConnected {
                    self.setConnState(ConnectionState::DeviceConnected).await;
                    self.addToCommandQueue(MsgCmd::GetAutoBuzz);
                }
            }
            Ok(MsgResponse::AutoBuzzOn) => {
                self.sendInternal(IntMsg::SetDeviceAutoBuzz(true)).await;
            }
            Ok(MsgResponse::AutoBuzzOff) => {
                self.sendInternal(IntMsg::SetDeviceAutoBuzz(false)).await;
            }
            // Other responses with content
            _ => {
                if cmd == MsgCmd::GetStartTime {
                    if responseKind == ResponseKind::Normal {
                        self.appState.write().await.volatileState.deviceStartTime = Some(response.to_string());
                    } else {
                        self.appState.write().await.volatileState.deviceStartTime = None;
                    }
                    self.sendInternal(IntMsg::StateUpdate).await;
                }
            }
        }
    }

    // Consumes a multi line response. Returns true if more lines are expected.
    async fn consumeMultiResponse(&mut self, cmd: MsgCmd, responseKind: ResponseKind, response: &str) -> bool {
        if cmd == MsgCmd::GetActionLog {
            if responseKind != ResponseKind::Normal {
                self.sendInternal(IntMsg::DeviceLogContent(vec![response.to_string()])).await;
            } else {
                let enumResponse = MsgResponse::from_str(response);
                if enumResponse == Ok(MsgResponse::EndMultiResponse) {
                    let actionLogContent = self.actionLog.clone();
                    self.actionLog.clear();
                    self.sendInternal(IntMsg::DeviceLogContent(actionLogContent)).await;
                } else if enumResponse.is_err() {
                    self.actionLog.push(response.to_string());
                    self.tsCommandSent = Some(Utc::now()); // prevent timeout
                    return true; // wait for more multi response lines
                } else {
                    error!("Unexpected response in multiresponse: {}", response);
                }
            }
        }
        return false; // no multi response expected or done with multi response
    }

    // --------------------------- Internal Methods ---------------------------

    async fn sendMqttCmd(&self, cmd: MsgCmd) {
        let topic = Topic::Command.to_string();
        let payload = cmd.to_string();
        self.publishMessage(topic, payload).await;
    }

    fn handleDeviceConnection(&mut self) {
        if let Some(tsLastActivity) = self.tsLastActivity {
            if self.connState == ConnectionState::MqttConnected && tsLastActivity + DUR_RECONNECT_DEVICE_DELAY < Utc::now() {
                if self.sendPing() {
                    info!("No activity from device, ping and continue waiting...");
                }
            } else if self.connState == ConnectionState::DeviceConnected && tsLastActivity + DUR_ACTIVITY_REFRESH < Utc::now() {
                // Already connected to device, but no activity for a while -> ping
                self.sendPing();
            }
        } else {
            // First activity, send ping to establish connection
            self.sendPing();
        }
    }

    fn addToCommandQueue(&mut self, cmd: MsgCmd) {
        self.commandQueue.push_back(cmd);
        self.notifyNewCommand.notify_one();
    }

    fn queueRuleAutoBuzzState(&mut self, activeByRule: bool) {
        self.addToCommandQueue(if activeByRule {
            MsgCmd::SetRuleAutoBuzzOn
        } else {
            MsgCmd::SetRuleAutoBuzzOff
        });
    }

    async fn sendInternal(&self, msg: IntMsg) {
        self.httpTx.send(msg).await.unwrap_or_default()
    }

    async fn logEvent(&self, event: LogEvent) {
        self.logTx.send(event).await.unwrap_or_default()
    }

    async fn setConnState(&mut self, newState: ConnectionState) {
        let justConnected = self.connState != ConnectionState::DeviceConnected && newState == ConnectionState::DeviceConnected;
        let justDisconnected = self.connState == ConnectionState::DeviceConnected && newState != ConnectionState::DeviceConnected;

        self.connState = newState.clone();
        info!("Connection state changed to: {}", newState);
        self.sendInternal(IntMsg::SetConnState(newState.clone())).await;

        if justConnected || justDisconnected {
            self.appState.write().await.volatileState.deviceStartTime = None;
            self.sendInternal(IntMsg::StateUpdate).await;
            if justConnected {
                let hostInfo = format!(
                    "MQTT host: {}",
                    self.appState.read().await.volatileState.mqttHost.as_ref().unwrap_or(&"unknown".to_string())
                );
                self.logEvent(LogEvent::new(LogEventType::DeviceConnected, hostInfo)).await;
                self.onDeviceConnected().await;
            } else {
                self.logEvent(LogEvent::new(LogEventType::DeviceDisconnected, "")).await;
                self.tsDeviceDisconnectedSince = Some(Utc::now());
            }
        }
    }

    async fn onDeviceConnected(&mut self) {
        // Get device start time
        self.addToCommandQueue(MsgCmd::GetStartTime);

        // The rule auto-buzz state has to be resent after a reconnect.
        if let Some(activeByRule) = self.ruleAutoBuzz {
            self.queueRuleAutoBuzzState(activeByRule);
        }

        // Check if we need a reboot (when the device is connected to its own MQTT broker instead of our).
        let rebootOnConnect = self.appState.read().await.volatileState.rebootDeviceOnConnect;
        if rebootOnConnect {
            // Prevent reboot loop, if the device does not connect to us after the reboot for some reason.
            if let Some(lastRebootTime) = self.appState.read().await.persistentState.getLastDeviceRebootTime()
                && lastRebootTime + DUR_MIN_REBOOT_INTERVAL > Utc::now()
            {
                info!(
                    "Connected to foreign broker, but device rebooted less than {}min ago at {}, not rebooting.",
                    (DUR_MIN_REBOOT_INTERVAL.as_secs() + 30) / 60,
                    lastRebootTime
                );
            } else {
                info!("Connected to foreign broker, sending reboot command to device.");
                self.logEvent(LogEvent::new(LogEventType::AutoReboot, "")).await;
                self.doReboot().await;
            }
        }
    }

    fn sendPing(&mut self) -> bool {
        // do not send pings while waiting for a response (for another ping or another command)
        if self.sendRecState == SendRecState::Idle {
            self.addToCommandQueue(MsgCmd::Ping);
            self.tsLastActivity = Some(Utc::now());
            return true;
        }
        return false;
    }

    fn updateTsLastActivity(&mut self) {
        self.tsLastActivity = Some(Utc::now());
    }

    async fn doReboot(&mut self) {
        self.sendMqttCmd(MsgCmd::Reboot).await;
        info!("Sent reboot command to device.");
        self.appState.write().await.persistentState.setLastDeviceRebootTime(Utc::now()).await;
        self.sendInternal(IntMsg::StateUpdate).await;
    }
}
