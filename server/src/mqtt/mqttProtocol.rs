// MQTT protocol contract (topics, messages, examples): see doc/mqtt-contract.md - keep in sync.

use strum_macros::{Display, EnumIter, EnumString};

#[derive(EnumString, Display)]
pub enum Topic {
    #[strum(serialize = "doorRing")]
    DoorRing,
    #[strum(serialize = "response")]
    Response,
    #[strum(serialize = "cmd")]
    Command,
}

// Messages received on the "doorRing" topic. The device appends a trailing
// "[auto buzz, ]<timestamp>" to "ring" and a "<timestamp>" to "testRing"
// (see doc/mqtt-contract.md), so these are parsed by MsgRing::parse, not by exact match.
#[derive(PartialEq, Eq, Clone, Debug)]
pub enum MsgRing {
    AutoBuzzOn,
    AutoBuzzOff,
    Ring { autoBuzz: bool, timestamp: String },
    TestRing { timestamp: String },
    AckRing,
    DeviceBuzz,
}

impl MsgRing {
    pub fn parse(payload: &str) -> Option<MsgRing> {
        match payload {
            "autoBuzzOn" => Some(MsgRing::AutoBuzzOn),
            "autoBuzzOff" => Some(MsgRing::AutoBuzzOff),
            "ackRing" => Some(MsgRing::AckRing),
            "deviceBuzz" => Some(MsgRing::DeviceBuzz),
            _ => {
                // Split the leading keyword from the trailing "[auto buzz, ]<timestamp>".
                let (keyword, rest) = match payload.split_once(' ') {
                    Some((keyword, rest)) => (keyword, rest.trim()),
                    None => (payload, ""),
                };
                match keyword {
                    "ring" => {
                        let autoBuzz = rest.starts_with("auto buzz,");
                        let timestamp = if autoBuzz {
                            rest.trim_start_matches("auto buzz,").trim().to_string()
                        } else {
                            rest.to_string()
                        };
                        Some(MsgRing::Ring { autoBuzz, timestamp })
                    }
                    "testRing" => Some(MsgRing::TestRing {
                        timestamp: rest.to_string(),
                    }),
                    _ => None,
                }
            }
        }
    }
}

#[derive(EnumString, Display, EnumIter, PartialEq, Eq, Clone, Debug, Copy)]
pub enum MsgCmd {
    // commands needing a response:
    #[strum(serialize = "getActionLog")]
    GetActionLog,
    #[strum(serialize = "getAutoBuzz")]
    GetAutoBuzz,
    #[strum(serialize = "ping")]
    Ping,
    #[strum(serialize = "getStartTime")]
    GetStartTime,
    #[strum(serialize = "reboot")]
    Reboot,
    // commands not needing a response:
    #[strum(serialize = "buzz")]
    BuzzDoor,
    #[strum(serialize = "ackRing")]
    AckRing,
    #[strum(serialize = "autoBuzzOn")]
    SetAutoBuzzOn,
    #[strum(serialize = "autoBuzzOff")]
    SetAutoBuzzOff,
    // Rule-based auto-buzz: display only on the device (LED), the buzz itself is sent as "buzz".
    #[strum(serialize = "ruleAutoBuzzOn")]
    SetRuleAutoBuzzOn,
    #[strum(serialize = "ruleAutoBuzzOff")]
    SetRuleAutoBuzzOff,
}

impl MsgCmd {
    pub fn answeredByMultiResponse(&self) -> bool {
        matches!(self, MsgCmd::GetActionLog)
    }
    pub fn needsResponse(&self) -> bool {
        matches!(
            self,
            MsgCmd::GetActionLog | MsgCmd::GetAutoBuzz | MsgCmd::Ping | MsgCmd::GetStartTime | MsgCmd::Reboot
        )
    }
}

#[derive(EnumString, Display, PartialEq, Eq, Clone, Debug)]
pub enum MsgResponse {
    #[strum(serialize = "pong")]
    Pong,
    #[strum(serialize = "buzzAck")]
    BuzzAck,
    #[strum(serialize = "autoBuzzOn")]
    AutoBuzzOn,
    #[strum(serialize = "autoBuzzOff")]
    AutoBuzzOff,
    #[strum(serialize = "endMultiResponse")]
    EndMultiResponse,
    #[strum(serialize = "ackReboot")]
    AckReboot,
}
