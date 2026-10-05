// MQTT protocol contract (topics, messages, examples): see doc/mqtt-contract.md - keep in sync.

use strum_macros::{Display, EnumString};

#[derive(EnumString, Display)]
pub enum Topic {
    #[strum(serialize = "doorRing")]
    DoorRing,
    #[strum(serialize = "response")]
    Response,
    #[strum(serialize = "cmd")]
    Command,
}

// Commands received from a controller on the "cmd" topic.
#[derive(EnumString, Display, PartialEq, Eq, Clone, Copy)]
pub enum MsgCmd {
    #[strum(serialize = "ping")]
    Ping,
    #[strum(serialize = "buzz")]
    Buzz,
    #[strum(serialize = "autoBuzzOn")]
    AutoBuzzOn,
    #[strum(serialize = "autoBuzzOff")]
    AutoBuzzOff,
    #[strum(serialize = "getAutoBuzz")]
    GetAutoBuzz,
    #[strum(serialize = "ruleAutoBuzzOn")]
    RuleAutoBuzzOn,
    #[strum(serialize = "ruleAutoBuzzOff")]
    RuleAutoBuzzOff,
    #[strum(serialize = "testRing")]
    TestRing,
    #[strum(serialize = "ackRing")]
    AckRing,
    #[strum(serialize = "getActionLog")]
    GetActionLog,
    #[strum(serialize = "getRawData")]
    GetRawData,
    #[strum(serialize = "getStartTime")]
    GetStartTime,
    #[strum(serialize = "reboot")]
    Reboot,
}

// Replies published on the "response" topic.
#[derive(Display)]
pub enum MsgResponse {
    #[strum(serialize = "pong")]
    Pong,
    #[strum(serialize = "buzzAck")]
    BuzzAck,
    #[strum(serialize = "autoBuzzOn")]
    AutoBuzzOn,
    #[strum(serialize = "autoBuzzOff")]
    AutoBuzzOff,
    #[strum(serialize = "ackReboot")]
    AckReboot,
    #[strum(serialize = "endMultiResponse")]
    EndMultiResponse,
}

// Bare state messages published on the "doorRing" topic. Ring/testRing payloads carry a
// trailing timestamp and are built manually (see Device::ring).
#[derive(Display)]
pub enum MsgRing {
    #[strum(serialize = "autoBuzzOn")]
    AutoBuzzOn,
    #[strum(serialize = "autoBuzzOff")]
    AutoBuzzOff,
    #[strum(serialize = "ackRing")]
    AckRing,
    #[strum(serialize = "deviceBuzz")]
    DeviceBuzz,
}
