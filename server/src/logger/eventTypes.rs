use chrono::{DateTime, Utc};

use strum::EnumMessage as _;
use strum_macros::{EnumMessage, EnumString, IntoStaticStr};
use tracing::error;

pub enum LogSource {
    Device,
    System,
    User,
    Rule,
}

impl LogSource {
    pub const ALL: [LogSource; 4] = [LogSource::Device, LogSource::System, LogSource::User, LogSource::Rule];

    pub fn asStr(&self) -> &'static str {
        match self {
            LogSource::Device => "device",
            LogSource::System => "system",
            LogSource::User => "user",
            LogSource::Rule => "rule",
        }
    }

    // Emoji shown in the web UI
    pub fn asIcon(&self) -> &'static str {
        match self {
            LogSource::Device => "🔔",
            LogSource::System => "⚙️",
            LogSource::User => "👤",
            LogSource::Rule => "📋",
        }
    }

    // Explanation of the icon, shown in the legend of the server log
    pub fn asLegend(&self) -> &'static str {
        match self {
            LogSource::Device => "device event",
            LogSource::System => "core/connection event",
            LogSource::User => "web UI event",
            LogSource::Rule => "rule event",
        }
    }
}

// All loggable event types.
#[derive(Clone, Copy, IntoStaticStr, EnumString, EnumMessage, PartialEq)]
pub enum LogEventType {
    // ------- Device events (observed on MQTT) -------
    #[strum(serialize = "ring", message = "Ring")]
    Ring,
    #[strum(serialize = "ringAutoBuzz", message = "Ring (auto buzz)")]
    RingAutoBuzz,
    #[strum(serialize = "testRing", message = "Test ring")]
    TestRing,
    #[strum(serialize = "deviceBuzzerAuto", message = "Door buzzer (auto)")]
    DeviceBuzzerAuto,
    #[strum(serialize = "deviceBuzzerManual", message = "Door buzzer (manual)")]
    DeviceBuzzerManual,
    #[strum(serialize = "deviceBuzzerServerCommanded", message = "Door buzzer (server-commanded)")]
    DeviceBuzzerServerCommanded,
    #[strum(serialize = "ackRing", message = "Ring acknowledged")]
    AckRing,
    #[strum(serialize = "deviceAutoBuzzOn", message = "Auto-buzz turned on")]
    DeviceAutoBuzzOn,
    #[strum(serialize = "deviceAutoBuzzOff", message = "Auto-buzz turned off")]
    DeviceAutoBuzzOff,
    // ------- System events (server-observed) -------
    #[strum(serialize = "serverStarted", message = "Server started")]
    ServerStarted,
    #[strum(serialize = "logsDeleted", message = "Logs deleted")]
    LogsDeleted,
    #[strum(serialize = "deviceConnected", message = "Device connected")]
    DeviceConnected,
    #[strum(serialize = "deviceDisconnected", message = "Device disconnected")]
    DeviceDisconnected,
    #[strum(serialize = "autoReboot", message = "Device rebooted (automatic)")]
    AutoReboot,
    // ------- User events (HTTP UI) -------
    #[strum(serialize = "login", message = "Login")]
    Login,
    #[strum(serialize = "loginFailed", message = "Login failed")]
    LoginFailed,
    #[strum(serialize = "logout", message = "Logout")]
    Logout,
    #[strum(serialize = "userAutoBuzzOn", message = "Request Auto-buzz on")]
    UserAutoBuzzOn,
    #[strum(serialize = "userAutoBuzzOff", message = "Request Auto-buzz off")]
    UserAutoBuzzOff,
    #[strum(serialize = "userBuzz", message = "Request door buzzer")]
    UserBuzz,
    #[strum(serialize = "userReboot", message = "Request device reboot")]
    UserReboot,
    // ------- Rule events (RuleService) -------
    #[strum(serialize = "ruleActivate", message = "Rule became active")]
    RuleActivate,
    #[strum(serialize = "ruleDeactivate", message = "Rule became inactive")]
    RuleDeactivate,
    #[strum(serialize = "ruleDeactivateDelete", message = "Rule became inactive due to deletion")]
    RuleDeactivateDelete,
    #[strum(serialize = "ruleAutoBuzzOn", message = "Rule: auto-buzz on")]
    RuleAutoBuzzOn,
    #[strum(serialize = "ruleAutoBuzzOff", message = "Rule: auto-buzz off")]
    RuleAutoBuzzOff,
    #[strum(serialize = "ruleBuzz", message = "Rule: door buzzer")]
    RuleBuzz,
    #[strum(serialize = "ruleAutoAck", message = "Rule: ring acknowledged")]
    RuleAutoAck,
}

impl LogEventType {
    pub fn source(&self) -> LogSource {
        match self {
            LogEventType::Ring
            | LogEventType::RingAutoBuzz
            | LogEventType::TestRing
            | LogEventType::DeviceBuzzerAuto
            | LogEventType::DeviceBuzzerManual
            | LogEventType::DeviceBuzzerServerCommanded
            | LogEventType::AckRing
            | LogEventType::DeviceAutoBuzzOn
            | LogEventType::DeviceAutoBuzzOff => LogSource::Device,
            LogEventType::DeviceConnected
            | LogEventType::DeviceDisconnected
            | LogEventType::AutoReboot
            | LogEventType::ServerStarted
            | LogEventType::LogsDeleted => LogSource::System,
            LogEventType::RuleActivate
            | LogEventType::RuleDeactivate
            | LogEventType::RuleDeactivateDelete
            | LogEventType::RuleAutoBuzzOn
            | LogEventType::RuleAutoBuzzOff
            | LogEventType::RuleBuzz
            | LogEventType::RuleAutoAck => LogSource::Rule,
            _ => LogSource::User,
        }
    }

    // Stable identifier stored in the database (strum `serialize`). Inverse of `fromTypeId`.
    pub fn typeId(&self) -> &'static str {
        self.into()
    }

    // Reconstructs the event type from a stored `typeId`. Returns None for ids
    // this version does not recognise.
    pub fn fromTypeId(typeId: &str) -> Option<Self> {
        typeId.parse().ok()
    }

    // Human-readable text shown in the server log UI (strum `message`). Kept separate
    // from the stored `typeId` so wording can change without rewriting past entries.
    pub fn description(&self) -> &'static str {
        self.get_message().unwrap_or_default()
    }
}

pub struct LogEvent {
    pub timestamp: DateTime<Utc>,
    pub eventType: LogEventType,
    pub details: String,
}

impl LogEvent {
    // An event to be logged now.
    pub fn new(eventType: LogEventType, details: impl Into<String>) -> Self {
        LogEvent {
            timestamp: Utc::now(),
            eventType,
            details: details.into(),
        }
    }

    // Rebuilds an event read back from the database.
    pub fn fromDb(timestampSecs: i64, typeId: &str, details: String) -> Option<Self> {
        let optTs = DateTime::from_timestamp(timestampSecs, 0);
        if optTs.is_none() {
            error!("Invalid timestamp in server log database: {}", timestampSecs);
            return None;
        }
        let optEventType = LogEventType::fromTypeId(typeId);
        if optEventType.is_none() {
            error!("Unknown event type in server log database: {}", typeId);
            return None;
        }
        Some(LogEvent {
            timestamp: optTs.unwrap(),
            eventType: optEventType.unwrap(),
            details,
        })
    }
}
