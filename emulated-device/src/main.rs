#![allow(non_snake_case)]

mod configStore;
mod consoleInterface;
mod device;
mod emulatedCommand;
mod mqttProtocol;

use clap::Parser;
use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use std::process::exit;
use std::time::Duration;
use tokio::sync::mpsc;

use configStore::ConfigStore;
use device::{Device, Outgoing};
use mqttProtocol::Topic;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const KEEP_ALIVE: Duration = Duration::from_secs(5);

// Call with "--help" to see this info
#[derive(Parser)]
#[command(about = "Doorbell device emulator", long_about = None)]
struct Args {
    /// Config file json
    #[arg(long, short, default_value = "config.json")]
    config: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let cfg = ConfigStore::new(args.config);

    // Build the MQTT client and connect. Exit immediately if the broker is not reachable.
    let mut mqttOptions =
        MqttOptions::new(cfg.mqttClientId.clone(), cfg.mqttHost.clone(), cfg.mqttPort);
    mqttOptions.set_keep_alive(KEEP_ALIVE);
    let (client, mut eventloop) = AsyncClient::new(mqttOptions, 10);
    connectOrExit(&mut eventloop, &cfg).await;

    if let Err(e) = client
        .subscribe(Topic::Command.to_string(), QoS::AtMostOnce)
        .await
    {
        eprintln!("Failed to subscribe to command topic: {}", e);
        exit(1);
    }

    // Capture single key presses on a blocking thread and forward them to the async loop.
    let (keyTx, mut keyRx) = mpsc::unbounded_channel::<char>();
    if let Err(e) = crossterm::terminal::enable_raw_mode() {
        eprintln!("Failed to enable raw terminal mode: {}", e);
        exit(1);
    }
    spawnKeyReader(keyTx);

    let mut device = Device::new();
    let brokerInfo = format!("{}:{}", cfg.mqttHost, cfg.mqttPort);
    consoleInterface::render(&device, &brokerInfo);

    // Main loop: react to incoming MQTT commands and to key presses.
    loop {
        tokio::select! {
            event = eventloop.poll() => {
                match event {
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        let payload = String::from_utf8_lossy(&publish.payload).to_string();
                        let outgoing = device.onCommand(&payload);
                        publishAll(&client, outgoing).await;
                        consoleInterface::render(&device, &brokerInfo);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        cleanupTerminal();
                        eprintln!("Lost connection to MQTT broker: {}", e);
                        exit(1);
                    }
                }
            }
            key = keyRx.recv() => {
                match key {
                    Some('q') => break,
                    Some(c) => {
                        let outgoing = device.onKey(c);
                        publishAll(&client, outgoing).await;
                        consoleInterface::render(&device, &brokerInfo);
                    }
                    None => break,
                }
            }
        }
    }

    cleanupTerminal();
}

// Polls the event loop until the broker accepts the connection, exiting on error or timeout.
async fn connectOrExit(eventloop: &mut EventLoop, cfg: &ConfigStore) {
    let result = tokio::time::timeout(CONNECT_TIMEOUT, async {
        loop {
            match eventloop.poll().await {
                Ok(Event::Incoming(Packet::ConnAck(_))) => return Ok(()),
                Ok(_) => {}
                Err(e) => return Err(e),
            }
        }
    })
    .await;

    match result {
        Ok(Ok(())) => {
            println!(
                "Connected to MQTT broker at {}:{}",
                cfg.mqttHost, cfg.mqttPort
            );
        }
        Ok(Err(e)) => {
            eprintln!(
                "Could not connect to MQTT broker at {}:{}: {}",
                cfg.mqttHost, cfg.mqttPort, e
            );
            exit(1);
        }
        Err(_) => {
            eprintln!(
                "Timed out connecting to MQTT broker at {}:{}",
                cfg.mqttHost, cfg.mqttPort
            );
            exit(1);
        }
    }
}

async fn publishAll(client: &AsyncClient, outgoing: Vec<Outgoing>) {
    for msg in outgoing {
        let _ = client
            .publish(msg.topic, QoS::AtMostOnce, false, msg.payload.into_bytes())
            .await;
    }
}

// Reads key events in a blocking loop and maps them to the single chars the loop understands.
fn spawnKeyReader(keyTx: mpsc::UnboundedSender<char>) {
    std::thread::spawn(move || {
        loop {
            match crossterm::event::read() {
                Ok(crossterm::event::Event::Key(keyEvent)) => {
                    if keyEvent.kind != KeyEventKind::Press {
                        continue;
                    }
                    let ch = match keyEvent.code {
                        KeyCode::Char('c')
                            if keyEvent.modifiers.contains(KeyModifiers::CONTROL) =>
                        {
                            'q'
                        }
                        KeyCode::Char(c) => c.to_ascii_lowercase(),
                        KeyCode::Esc => 'q',
                        _ => continue,
                    };
                    if keyTx.send(ch).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
}

fn cleanupTerminal() {
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(std::io::stdout(), crossterm::cursor::Show);
}
