use chrono::Local;
use std::collections::VecDeque;

use crate::emulatedCommand::EmulatedCommand;
use crate::mqttProtocol::{MsgCmd, MsgResponse, MsgRing, Topic};

const MAX_LOG_ENTRIES: usize = 10;
const TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S";
const LOG_TIME_FORMAT: &str = "%H:%M:%S";

pub struct Outgoing {
    pub topic: String,
    pub payload: String,
}

struct LogEntry {
    time: String,
    text: String,
}

pub struct Device {
    pub autoBuzz: bool,
    pub ringActive: bool,
    pub startTime: String,
    log: VecDeque<LogEntry>,
}

impl Device {
    pub fn new() -> Self {
        let mut dev = Device {
            autoBuzz: false,
            ringActive: false,
            startTime: Local::now().format(TIMESTAMP_FORMAT).to_string(),
            log: VecDeque::new(),
        };
        dev.logEvent("Emulated device started");
        dev
    }

    // ------------------------------- Console keys -------------------------------

    pub fn onKey(&mut self, key: char) -> Vec<Outgoing> {
        let source = format!("key '{}'", key);
        match EmulatedCommand::fromKey(key) {
            Some(EmulatedCommand::Ring) => self.ring(false, &source),
            Some(EmulatedCommand::ToggleAutoBuzz) => self.toggleAutoBuzz(&source),
            Some(EmulatedCommand::AckAndBuzz) => self.acknowledge(true, &source),
            Some(EmulatedCommand::Ack) => self.acknowledge(false, &source),
            // Quit is handled by the main loop; unknown keys are ignored.
            Some(EmulatedCommand::Quit) | None => Vec::new(),
        }
    }

    // ---------------------------- Incoming commands ----------------------------

    pub fn onCommand(&mut self, payload: &str) -> Vec<Outgoing> {
        match payload.parse::<MsgCmd>() {
            Ok(MsgCmd::Ping) => vec![self.response(MsgResponse::Pong)],
            Ok(MsgCmd::Buzz) => {
                self.buzz("mqtt buzz");
                vec![self.response(MsgResponse::BuzzAck)]
            }
            Ok(MsgCmd::AutoBuzzOn) => self.setAutoBuzz(true, "mqtt"),
            Ok(MsgCmd::AutoBuzzOff) => self.setAutoBuzz(false, "mqtt"),
            Ok(MsgCmd::GetAutoBuzz) => vec![self.autoBuzzResponse()],
            Ok(MsgCmd::TestRing) => self.ring(true, "mqtt"),
            Ok(MsgCmd::AckRing) => self.acknowledge(false, "mqtt"),
            Ok(MsgCmd::GetActionLog) => self.actionLogResponse(),
            Ok(MsgCmd::GetRawData) => vec![
                self.responseText("[dummy raw data]".to_string()),
                self.response(MsgResponse::EndMultiResponse),
            ],
            Ok(MsgCmd::GetStartTime) => vec![self.responseText(self.startTime.clone())],
            Ok(MsgCmd::Reboot) => {
                self.logEvent("Reboot requested by server (mqtt)");
                self.ringActive = false;
                vec![self.response(MsgResponse::AckReboot)]
            }
            Err(_) => Vec::new(),
        }
    }

    // ----------------------------- Device behavior -----------------------------

    fn ring(&mut self, testRing: bool, source: &str) -> Vec<Outgoing> {
        self.ringActive = true;

        // Build the doorRing payload exactly like the Arduino: "ring [auto buzz, ]<ts>" or
        // "testRing <ts>". The "auto buzz, " prefix is only added for a real ring.
        let kind = if testRing { "testRing" } else { "ring" };
        let mut payload = format!("{} ", kind);
        if self.autoBuzz && !testRing {
            payload.push_str("auto buzz, ");
        }
        payload.push_str(&Local::now().format(TIMESTAMP_FORMAT).to_string());

        if testRing {
            self.logEvent(&format!("Test ring received ({})", source));
        } else {
            self.logEvent(&format!("Ring ({})", source));
        }

        // With auto-buzz enabled the device buzzes immediately on a ring.
        if self.autoBuzz {
            self.buzz("auto");
        }

        vec![Outgoing {
            topic: Topic::DoorRing.to_string(),
            payload,
        }]
    }

    fn toggleAutoBuzz(&mut self, source: &str) -> Vec<Outgoing> {
        self.setAutoBuzz(!self.autoBuzz, source)
    }

    // Mirrors the Arduino's setAutoBuzzState: only publishes and logs on an actual change.
    fn setAutoBuzz(&mut self, newState: bool, source: &str) -> Vec<Outgoing> {
        if newState == self.autoBuzz {
            return Vec::new();
        }
        self.autoBuzz = newState;
        let ringMsg = if newState {
            MsgRing::AutoBuzzOn
        } else {
            MsgRing::AutoBuzzOff
        };
        self.logEvent(&format!(
            "Auto-buzz {} ({})",
            if newState { "ON" } else { "OFF" },
            source
        ));
        vec![Outgoing {
            topic: Topic::DoorRing.to_string(),
            payload: ringMsg.to_string(),
        }]
    }

    // Acknowledge an active ring. With alsoBuzz (switch 2 / "ack & buzz") a manual buzz is
    // triggered when auto-buzz did not already do it.
    fn acknowledge(&mut self, alsoBuzz: bool, source: &str) -> Vec<Outgoing> {
        if !self.ringActive {
            self.logEvent(&format!(
                "Acknowledge ignored - no active ring ({})",
                source
            ));
            return Vec::new();
        }
        self.ringActive = false;

        let mut outgoing = vec![Outgoing {
            topic: Topic::DoorRing.to_string(),
            payload: MsgRing::AckRing.to_string(),
        }];

        if alsoBuzz {
            self.logEvent(&format!("Acknowledge & buzz ({})", source));
            // The physical "ack & buzz" button buzzes only when auto-buzz did not already do it,
            // and announces it to the controllers via a deviceBuzz message.
            if !self.autoBuzz {
                self.buzz("manual");
                outgoing.push(Outgoing {
                    topic: Topic::DoorRing.to_string(),
                    payload: MsgRing::DeviceBuzz.to_string(),
                });
            }
        } else {
            self.logEvent(&format!("Acknowledge ({})", source));
        }

        outgoing
    }

    fn buzz(&mut self, kind: &str) {
        // The door buzzer relay is not emulated; the activation is only reported.
        self.logEvent(&format!("Door buzzer activated ({})", kind));
    }

    // ------------------------------ Response helpers ------------------------------

    fn response(&self, msg: MsgResponse) -> Outgoing {
        self.responseText(msg.to_string())
    }

    fn responseText(&self, payload: String) -> Outgoing {
        Outgoing {
            topic: Topic::Response.to_string(),
            payload,
        }
    }

    fn autoBuzzResponse(&self) -> Outgoing {
        let msg = if self.autoBuzz {
            MsgResponse::AutoBuzzOn
        } else {
            MsgResponse::AutoBuzzOff
        };
        self.response(msg)
    }

    fn actionLogResponse(&self) -> Vec<Outgoing> {
        // Replay the rolling log oldest-first, terminated like the Arduino's multi-response.
        let mut outgoing: Vec<Outgoing> = self
            .logLines()
            .into_iter()
            .rev()
            .map(|line| self.responseText(line))
            .collect();
        outgoing.push(self.response(MsgResponse::EndMultiResponse));
        outgoing
    }

    // -------------------------------- Event log --------------------------------

    fn logEvent(&mut self, text: &str) {
        self.log.push_front(LogEntry {
            time: Local::now().format(LOG_TIME_FORMAT).to_string(),
            text: text.to_string(),
        });
        if self.log.len() > MAX_LOG_ENTRIES {
            self.log.pop_back();
        }
    }

    pub fn logLines(&self) -> Vec<String> {
        self.log
            .iter()
            .map(|e| format!("{}  {}", e.time, e.text))
            .collect()
    }
}
