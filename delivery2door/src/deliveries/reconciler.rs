use crate::configStore::RuleCreationConfig;
use crate::crawlers::crawler::DeliveryAnnouncement as FoundAnnouncement;
use crate::crawlers::deliveryWindow::DeliveryWindow;
use crate::deliveries::announcementStore::{AnnouncementState, AnnouncementStore, DeliveryAnnouncement};
use crate::deliveries::ruleMapper::ruleSpecForWindow;
use crate::doorbell::ruleApiClient::{ApiError, RuleApiClient};
use crate::logger::eventTypes::{LogEvent, LogEventType};
use crate::notifications::notifier::{DeliveryEvent, DeliveryNotification, SharedNotifier};

use chrono::{DateTime, Local};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

/*
Reconciler implementation
-------------------------

Keeps the rules of the doorbell service in sync with what the sources announced: one rule per delivery
id, created when the delivery is first seen and refined when a later mail knows more.

The store's window is the one the doorbell service *knows*. Comparing the parsed window against it is
what keeps this idempotent - without it, every poll cycle would send the same rule again.

A delivery whose window has passed becomes `Done` and is never turned into a rule again. That is what
makes an old mail harmless: the doorbell deletes such a rule immediately anyway (`deleteAtEnd`), and
after a restart every mail looks new.
*/
/// Where an announcement came from, and what the rule it produces is named after.
pub struct AnnouncementOrigin<'a> {
    pub sourceName: &'a str,
    pub crawlerName: &'a str,
    /// Prefix of the rule name, from the crawler that found the announcement.
    pub namePrefix: &'a str,
    /// The item the announcement was found in, for the log.
    pub itemDescription: &'a str,
}

pub struct Reconciler {
    store: AnnouncementStore,
    /// Shared with the HTTP server, which uses the same client to report whether the doorbell can be
    /// reached - one connection pool, one place where the base URL and the token come from.
    client: Arc<RuleApiClient>,
    ruleCreation: RuleCreationConfig,
    notifier: SharedNotifier,
    logTx: mpsc::Sender<LogEvent>,
}

impl Reconciler {
    pub fn new(
        store: AnnouncementStore,
        client: Arc<RuleApiClient>,
        ruleCreation: RuleCreationConfig,
        notifier: SharedNotifier,
        logTx: mpsc::Sender<LogEvent>,
    ) -> Self {
        Reconciler {
            store,
            client,
            ruleCreation,
            notifier,
            logTx,
        }
    }

    /// Handles the announcements found in one item, in the order the crawler reported them.
    ///
    /// Items reach this in arrival order, so for two mails about the same parcel the later one wins -
    /// which is what a postponed delivery needs, even though its window may be wider.
    pub async fn handleAnnouncements(&self, origin: &AnnouncementOrigin<'_>, announcements: &[FoundAnnouncement], now: DateTime<Local>) {
        for announcement in announcements {
            self.handleOne(origin, announcement, now).await;
        }
    }

    async fn handleOne(&self, origin: &AnnouncementOrigin<'_>, announcement: &FoundAnnouncement, now: DateTime<Local>) {
        let deliveryId = &announcement.deliveryId;
        let Some(window) = &announcement.window else {
            // A delivery id without a date is not actionable: there is no window to open the door in.
            debug!("Delivery {} has no announced window, nothing to do", deliveryId);
            return;
        };

        let known = self.store.get(deliveryId);
        match &known {
            // The delivery is over. Its mails stay in the folder, and they must never bring it back.
            Some(entry) if entry.state == AnnouncementState::Done => {
                debug!("Delivery {} is already done, ignoring the announcement", deliveryId);
                return;
            }
            // Exactly what the doorbell service already knows.
            Some(entry) if entry.window == *window => {
                debug!("Delivery {} is unchanged, no call to the doorbell service", deliveryId);
                return;
            }
            _ => {}
        }

        // A window that has already ended is not worth a rule. Reached after a restart, when every
        // mail looks new again.
        // The prefix comes from the crawler, so an Amazon parcel is not named after DHL.
        let namePrefix = if origin.namePrefix.trim().is_empty() {
            self.ruleCreation.namePrefix.as_str()
        } else {
            origin.namePrefix
        };
        let spec = match ruleSpecForWindow(deliveryId, namePrefix, window, &self.ruleCreation, now) {
            Ok(spec) => spec,
            Err(e) => {
                debug!("Delivery {} not turned into a rule: {}", deliveryId, e);
                self.logEvent(
                    LogEventType::DeliveryNotUsable,
                    format!("Delivery {}: {} ({})", deliveryId, e, window),
                )
                .await;
                // If it was tracked, it is over now - otherwise it is not tracked at all.
                if known.is_some() {
                    self.store.markDone(deliveryId, now);
                }
                return;
            }
        };

        match known {
            Some(entry) => self.updateRuleFor(&entry, namePrefix, window, &spec, now).await,
            None => self.createRuleFor(origin, namePrefix, deliveryId, window, &spec, now).await,
        }
    }

    /// First mail about this delivery: create the rule and remember which one it is.
    #[allow(clippy::too_many_arguments)]
    async fn createRuleFor(
        &self,
        origin: &AnnouncementOrigin<'_>,
        // Which carrier the rule is named after, already resolved against the fallback.
        carrier: &str,
        deliveryId: &str,
        window: &DeliveryWindow,
        spec: &crate::doorbell::ruleApiClient::RuleSpec,
        now: DateTime<Local>,
    ) {
        self.logEvent(
            LogEventType::DeliveryFound,
            format!("Delivery {} for {} ({})", deliveryId, window, origin.itemDescription),
        )
        .await;

        match self.client.createRule(spec).await {
            Ok(ruleId) => {
                self.store.upsert(&DeliveryAnnouncement {
                    deliveryId: deliveryId.to_string(),
                    sourceName: origin.sourceName.to_string(),
                    crawlerName: origin.crawlerName.to_string(),
                    ruleId,
                    window: window.clone(),
                    state: AnnouncementState::Active,
                    firstSeen: now,
                    lastUpdated: now,
                });
                info!("Delivery {}: created doorbell rule {} for {}", deliveryId, ruleId, window);
                self.logEvent(
                    LogEventType::RuleCreated,
                    format!("Rule {} for delivery {} ({})", ruleId, deliveryId, window),
                )
                .await;

                // Only now, with the rule in place and remembered: a notification about a rule that
                // could not be stored would be a lie.
                self.notifier
                    .notify(&DeliveryNotification {
                        event: DeliveryEvent::RuleCreated,
                        carrier,
                        deliveryId,
                        sourceName: origin.sourceName,
                        ruleId,
                        ruleName: &spec.name,
                        window,
                        previousWindow: None,
                    })
                    .await;
            }
            Err(e) => {
                // Nothing is stored, so the next cycle tries again - but the mail is already recorded
                // as processed, so the retry only happens if the mail is seen again. Announcements are
                // therefore retried on the next *new* mail; a doorbell outage of minutes is covered by
                // the poll interval, a longer one needs a re-scan.
                error!("Delivery {}: could not create the doorbell rule: {}", deliveryId, e);
                self.logEvent(
                    LogEventType::RuleApiFailed,
                    format!("Creating a rule for delivery {} failed: {}", deliveryId, e),
                )
                .await;
            }
        }
    }

    /// A later mail knows more: move the existing rule to the new window.
    async fn updateRuleFor(
        &self,
        entry: &DeliveryAnnouncement,
        carrier: &str,
        window: &DeliveryWindow,
        spec: &crate::doorbell::ruleApiClient::RuleSpec,
        now: DateTime<Local>,
    ) {
        match self.client.updateRule(entry.ruleId, spec).await {
            Ok(()) => {
                self.store.setWindow(&entry.deliveryId, window, now);
                info!(
                    "Delivery {}: doorbell rule {} moved from {} to {}",
                    entry.deliveryId, entry.ruleId, entry.window, window
                );
                self.logEvent(
                    LogEventType::DeliveryWindowChanged,
                    format!("Delivery {}: {} -> {}", entry.deliveryId, entry.window, window),
                )
                .await;
                self.logEvent(
                    LogEventType::RuleUpdated,
                    format!("Rule {} for delivery {} now covers {}", entry.ruleId, entry.deliveryId, window),
                )
                .await;

                self.notifier
                    .notify(&DeliveryNotification {
                        event: DeliveryEvent::RuleMoved,
                        carrier,
                        deliveryId: &entry.deliveryId,
                        sourceName: &entry.sourceName,
                        ruleId: entry.ruleId,
                        ruleName: &spec.name,
                        window,
                        previousWindow: Some(&entry.window),
                    })
                    .await;
            }
            // The rule is gone - deleted at the end of its lifetime, or by hand. Recreating it would
            // resurrect a delivery that the doorbell service already considers over.
            Err(ApiError::NotFound) => {
                self.store.markDone(&entry.deliveryId, now);
                info!(
                    "Delivery {}: doorbell rule {} no longer exists, the delivery counts as done",
                    entry.deliveryId, entry.ruleId
                );
                self.logEvent(
                    LogEventType::RuleGone,
                    format!("Rule {} for delivery {} no longer exists", entry.ruleId, entry.deliveryId),
                )
                .await;
            }
            Err(e) => {
                // The entry keeps its old window, so the next attempt sees the difference again.
                error!(
                    "Delivery {}: could not update the doorbell rule {}: {}",
                    entry.deliveryId, entry.ruleId, e
                );
                self.logEvent(
                    LogEventType::RuleApiFailed,
                    format!("Updating rule {} for delivery {} failed: {}", entry.ruleId, entry.deliveryId, e),
                )
                .await;
            }
        }
    }

    /// Closes the deliveries whose window has passed. Their rules are deleted by the doorbell service
    /// itself; this only stops the announcements from being reconsidered.
    pub async fn closePassedDeliveries(&self, now: DateTime<Local>) {
        for entry in self.store.activeAndPassed(now) {
            self.store.markDone(&entry.deliveryId, now);
            self.logEvent(
                LogEventType::DeliveryDone,
                format!("Delivery {} ({}) is over", entry.deliveryId, entry.window),
            )
            .await;
        }
    }

    async fn logEvent(&self, eventType: LogEventType, details: String) {
        self.logTx.send(LogEvent::new(eventType, details)).await.unwrap_or_default();
    }
}

/// Handed to the poll service, which does not need to know how reconciliation works.
pub type SharedReconciler = Arc<Reconciler>;
