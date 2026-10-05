use crate::configStore::ConfigStore;
use crate::crawlers::deliveryWindow::DeliveryWindow;
use crate::logger::eventTypes::{LogEvent, LogEventType};
use crate::notifications::createSink;
use crate::notifications::sink::NotificationSink;
use crate::notifications::template::{NotificationValues, validate};

use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info};

/// Date format of `{deliveryDate}`, the same the doorbell uses for a rule's dates.
const DATE_FORMAT: &str = "%Y-%m-%d";

/// What happened to a delivery, i.e. what the notification is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryEvent {
    /// A delivery was announced for the first time and got its rule.
    RuleCreated,
    /// A later announcement moved the window of an existing rule.
    RuleMoved,
}

impl DeliveryEvent {
    /// The word a template puts into its sentence, so one template reads correctly for both cases.
    fn asWord(&self) -> &'static str {
        match self {
            DeliveryEvent::RuleCreated => "created",
            DeliveryEvent::RuleMoved => "moved",
        }
    }
}

/// Everything a notification can talk about.
pub struct DeliveryNotification<'a> {
    pub event: DeliveryEvent,
    /// Which carrier, from the crawler that found the announcement.
    pub carrier: &'a str,
    pub deliveryId: &'a str,
    pub sourceName: &'a str,
    pub ruleId: i64,
    pub ruleName: &'a str,
    pub window: &'a DeliveryWindow,
    /// Only set for `RuleMoved`.
    pub previousWindow: Option<&'a DeliveryWindow>,
}

/*
Notifier implementation
-----------------------

Sends a message to every configured sink when a delivery got a rule or its rule was moved.

Deliberately fire-and-forget from the caller's point of view: a sink that is unreachable must never
keep a door rule from being created, so a failure is logged and nothing else. The service log is what
tells whether the mails really went out.
*/
pub struct Notifier {
    sinks: Vec<Box<dyn NotificationSink>>,
    doorbellAddress: String,
    logTx: mpsc::Sender<LogEvent>,
}

impl Notifier {
    /// Builds all sinks and checks their templates. A bad template or an unusable SMTP setup is a
    /// config error, so it stops the service here instead of failing at the first delivery.
    pub fn new(cfg: &ConfigStore, logTx: mpsc::Sender<LogEvent>) -> Self {
        let mut sinks: Vec<Box<dyn NotificationSink>> = Vec::new();
        for sinkCfg in &cfg.notificationSinks {
            for template in sinkCfg.templates() {
                if let Err(e) = validate(template) {
                    ConfigStore::handleError(&format!("Invalid template of notification sink \"{}\"", sinkCfg.name()), e);
                }
            }
            match createSink(sinkCfg) {
                Ok(sink) => sinks.push(sink),
                Err(e) => ConfigStore::handleError(&format!("Invalid notification sink \"{}\"", sinkCfg.name()), e),
            }
        }

        if sinks.is_empty() {
            info!("No notification sinks configured, deliveries are only written to the service log");
        } else {
            info!(
                "Notification sink(s): {}",
                sinks.iter().map(|sink| sink.name()).collect::<Vec<_>>().join(", ")
            );
        }
        Notifier {
            sinks,
            doorbellAddress: cfg.doorbell.baseUrl.clone(),
            logTx,
        }
    }

    /// Sends one notification to every sink. Errors are logged, never propagated.
    pub async fn notify(&self, notification: &DeliveryNotification<'_>) {
        if self.sinks.is_empty() {
            return;
        }
        let values = self.valuesOf(notification);
        for sink in &self.sinks {
            match sink.send(&values).await {
                Ok(()) => {
                    info!("Sink {}: notified about delivery {}", sink.name(), notification.deliveryId);
                    self.logEvent(
                        LogEventType::NotificationSent,
                        format!("Sink {}: delivery {} ({})", sink.name(), notification.deliveryId, notification.window),
                    )
                    .await;
                }
                Err(e) => {
                    // A rule was created either way - the notification is the only thing lost.
                    error!("Sink {}: could not notify about delivery {}: {}", sink.name(), notification.deliveryId, e);
                    self.logEvent(
                        LogEventType::NotificationFailed,
                        format!("Sink {}: delivery {} failed: {}", sink.name(), notification.deliveryId, e),
                    )
                    .await;
                }
            }
        }
    }

    fn valuesOf(&self, notification: &DeliveryNotification<'_>) -> NotificationValues {
        // The note is empty for a new rule, so a single template reads correctly either way - a
        // template cannot say "only if there was a previous window".
        let previousWindowNote = match notification.previousWindow {
            Some(window) => format!(" (previously {})", window),
            None => String::new(),
        };
        NotificationValues::new()
            .set("event", notification.event.asWord())
            .set("previousWindowNote", previousWindowNote)
            .set("carrier", notification.carrier)
            .set("trackingNumber", notification.deliveryId)
            .set("deliveryDate", notification.window.from.format(DATE_FORMAT).to_string())
            .set("deliveryWindow", notification.window.to_string())
            .set(
                "previousWindow",
                notification.previousWindow.map(|window| window.to_string()).unwrap_or_default(),
            )
            .set("ruleId", notification.ruleId.to_string())
            .set("ruleName", notification.ruleName)
            .set("source", notification.sourceName)
            .set("doorbellServerAddress", &self.doorbellAddress)
    }

    async fn logEvent(&self, eventType: LogEventType, details: String) {
        self.logTx.send(LogEvent::new(eventType, details)).await.unwrap_or_default();
    }
}

/// Handed to the reconciler, which does not need to know how many sinks there are.
pub type SharedNotifier = Arc<Notifier>;
