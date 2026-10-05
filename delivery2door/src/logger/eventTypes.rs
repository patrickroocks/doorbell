use chrono::{DateTime, Utc};

use strum::EnumMessage as _;
use strum_macros::{EnumMessage, EnumString, IntoStaticStr};
use tracing::error;

/// Where an event came from. Same idea as in the doorbell server, with the sources this service has.
pub enum LogSource {
    System,
    User,
    Delivery,
    Doorbell,
    Notification,
}

impl LogSource {
    pub const ALL: [LogSource; 5] = [
        LogSource::System,
        LogSource::User,
        LogSource::Delivery,
        LogSource::Doorbell,
        LogSource::Notification,
    ];

    pub fn asStr(&self) -> &'static str {
        match self {
            LogSource::System => "system",
            LogSource::User => "user",
            LogSource::Delivery => "delivery",
            LogSource::Doorbell => "doorbell",
            LogSource::Notification => "notification",
        }
    }

    // Emoji shown in the web UI
    pub fn asIcon(&self) -> &'static str {
        match self {
            LogSource::System => "⚙️",
            LogSource::User => "👤",
            LogSource::Delivery => "📦",
            LogSource::Doorbell => "🔔",
            LogSource::Notification => "✉️",
        }
    }

    // Explanation of the icon, shown in the legend of the service log
    pub fn asLegend(&self) -> &'static str {
        match self {
            LogSource::System => "core event",
            LogSource::User => "web UI event",
            LogSource::Delivery => "delivery announcement found in a source",
            LogSource::Doorbell => "rule in the doorbell service",
            LogSource::Notification => "message to a notification sink",
        }
    }
}

// All loggable event types.
#[derive(Clone, Copy, IntoStaticStr, EnumString, EnumMessage, PartialEq)]
pub enum LogEventType {
    // ------- System events -------
    #[strum(serialize = "serviceStarted", message = "Service started")]
    ServiceStarted,
    #[strum(serialize = "logsDeleted", message = "Logs deleted")]
    LogsDeleted,
    #[strum(serialize = "sourceFailed", message = "Source could not be read")]
    SourceFailed,
    #[strum(serialize = "sourceRescan", message = "Source re-scan requested")]
    SourceRescan,
    // ------- User events (web UI) -------
    #[strum(serialize = "login", message = "Login")]
    Login,
    #[strum(serialize = "loginFailed", message = "Login failed")]
    LoginFailed,
    #[strum(serialize = "logout", message = "Logout")]
    Logout,
    // ------- Delivery events -------
    #[strum(serialize = "deliveryFound", message = "Delivery announced")]
    DeliveryFound,
    #[strum(serialize = "deliveryWindowChanged", message = "Delivery window changed")]
    DeliveryWindowChanged,
    #[strum(serialize = "deliveryDone", message = "Delivery window over")]
    DeliveryDone,
    #[strum(serialize = "deliveryNotUsable", message = "Delivery announcement not usable")]
    DeliveryNotUsable,
    // ------- Doorbell events -------
    #[strum(serialize = "ruleCreated", message = "Door rule created")]
    RuleCreated,
    #[strum(serialize = "ruleUpdated", message = "Door rule updated")]
    RuleUpdated,
    #[strum(serialize = "ruleGone", message = "Door rule no longer exists")]
    RuleGone,
    #[strum(serialize = "ruleApiFailed", message = "Doorbell service could not be reached")]
    RuleApiFailed,
    // ------- Notification events -------
    #[strum(serialize = "notificationSent", message = "Notification sent")]
    NotificationSent,
    #[strum(serialize = "notificationFailed", message = "Notification could not be sent")]
    NotificationFailed,
}

impl LogEventType {
    pub fn source(&self) -> LogSource {
        match self {
            LogEventType::ServiceStarted | LogEventType::LogsDeleted | LogEventType::SourceFailed | LogEventType::SourceRescan => {
                LogSource::System
            }
            LogEventType::Login | LogEventType::LoginFailed | LogEventType::Logout => LogSource::User,
            LogEventType::DeliveryFound
            | LogEventType::DeliveryWindowChanged
            | LogEventType::DeliveryDone
            | LogEventType::DeliveryNotUsable => LogSource::Delivery,
            LogEventType::RuleCreated | LogEventType::RuleUpdated | LogEventType::RuleGone | LogEventType::RuleApiFailed => {
                LogSource::Doorbell
            }
            LogEventType::NotificationSent | LogEventType::NotificationFailed => LogSource::Notification,
        }
    }

    // Stable identifier stored in the database (strum `serialize`). Inverse of `fromTypeId`.
    pub fn typeId(&self) -> &'static str {
        self.into()
    }

    // Reconstructs the event type from a stored `typeId`. Returns None for ids this version does not
    // recognise.
    pub fn fromTypeId(typeId: &str) -> Option<Self> {
        typeId.parse().ok()
    }

    // Human-readable text shown in the log UI (strum `message`). Kept separate from the stored
    // `typeId` so wording can change without rewriting past entries.
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
        let Some(timestamp) = DateTime::from_timestamp(timestampSecs, 0) else {
            error!("Invalid timestamp in the service log database: {}", timestampSecs);
            return None;
        };
        let Some(eventType) = LogEventType::fromTypeId(typeId) else {
            error!("Unknown event type in the service log database: {}", typeId);
            return None;
        };
        Some(LogEvent {
            timestamp,
            eventType,
            details,
        })
    }
}
