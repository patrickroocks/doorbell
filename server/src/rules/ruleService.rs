use crate::common::IntMsg;
use crate::logger::eventTypes::{LogEvent, LogEventType};
use crate::rules::{rule::Rule, ruleStore::RuleStore};
use crate::util::notifyWithState::NotifyWithState;

use chrono::DateTime;
use chrono::Local;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::time::{self, Duration};
use tracing::{error, info};

// How often the service wakes up.
const TICK_INTERVAL: Duration = Duration::from_secs(1);

// Grace period between a single-shot rule's first ring and its removal. People at the door are
// sometimes too slow walking from the bell to the buzzed door and press the bell a second time
const HUMAN_DOOR_OPEN_TOLERANCE_TIME: Duration = Duration::from_secs(10);

// After a rule change, we need a immediate evaluation.
pub type RulesChangedFlag = Arc<AtomicBool>;

// Single-shot rules whose removal is already scheduled, i.e. which have just fired and are waiting
// out the grace period. Shared with the HTTP layer, which shows these rules as "pending delete".
#[derive(Clone)]
pub struct PendingRemovals {
    inner: Arc<Mutex<HashMap<i64, DateTime<Local>>>>,
}

impl PendingRemovals {
    fn new() -> Self {
        PendingRemovals {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // Schedules the removal, keeping an already scheduled time - a second ring within the grace
    // period must not postpone the removal.
    fn scheduleIfAbsent(&self, id: i64, removeAt: DateTime<Local>) {
        self.inner.lock().unwrap().entry(id).or_insert(removeAt);
    }

    fn remove(&self, id: i64) {
        self.inner.lock().unwrap().remove(&id);
    }

    fn anyDue(&self, now: DateTime<Local>) -> bool {
        self.inner.lock().unwrap().values().any(|removeAt| now >= *removeAt)
    }

    fn dueIds(&self, now: DateTime<Local>) -> Vec<i64> {
        self.inner.lock().unwrap().iter().filter(|(_, removeAt)| now >= **removeAt).map(|(&id, _)| id).collect()
    }

    pub fn ids(&self) -> HashSet<i64> {
        self.inner.lock().unwrap().keys().copied().collect()
    }
}

pub struct RuleService {
    store: RuleStore,
    rxMqtt: mpsc::Receiver<IntMsg>, // ring events from the device
    mqttTx: mpsc::Sender<IntMsg>,   // commands to the device (Buzz/AckRing)
    httpTx: mpsc::Sender<IntMsg>,   // rule-based auto-buzz state for the web UI
    logTx: mpsc::Sender<LogEvent>,
    rulesChanged: RulesChangedFlag, // set by the HTTP layer after a rule CRUD change
    cancelNotify: Arc<NotifyWithState>,
    // Rules currently in their active window (id -> last known state of the rule).
    // We need the rule for the deactivation log.
    activated: HashMap<i64, Rule>,
    // Single-shot rules whose removal is already scheduled at a given time.
    pendingRemoval: PendingRemovals,
    // Minute in which `evaluateTransitions` last ran.
    lastEvaluateMinute: i64,
}

impl RuleService {
    pub fn new(
        rxMqtt: mpsc::Receiver<IntMsg>,
        mqttTx: mpsc::Sender<IntMsg>,
        httpTx: mpsc::Sender<IntMsg>,
        logTx: mpsc::Sender<LogEvent>,
        rulesChanged: RulesChangedFlag,
        cancelNotify: Arc<NotifyWithState>,
    ) -> Self {
        RuleService {
            store: RuleStore::new(),
            rxMqtt,
            mqttTx,
            httpTx,
            logTx,
            rulesChanged,
            cancelNotify,
            activated: HashMap::new(),
            pendingRemoval: PendingRemovals::new(),
            // Set properly by the initial evaluation at the start of `run`.
            lastEvaluateMinute: 0,
        }
    }

    // Read/write handle shared with the HTTP layer.
    pub fn store(&self) -> RuleStore {
        self.store.clone()
    }

    // Read handle on the just-fired single-shot rules, shared with the HTTP layer.
    pub fn pendingRemovals(&self) -> PendingRemovals {
        self.pendingRemoval.clone()
    }

    pub async fn run(&mut self) {
        // Evaluate once at startup so a rule that is already within its window takes effect.
        self.evaluateTransitions().await;
        // `evaluateTransitions` only reports state changes, so the initial state has to be reported
        // explicitly - the web UI also needs to know that no rule is active.
        self.sendRuleAutoBuzzState(!self.activated.is_empty()).await;

        let mut tickTimer = time::interval(TICK_INTERVAL);
        tickTimer.tick().await; // consume the immediate first tick

        loop {
            tokio::select! {
                _ = self.cancelNotify.notified() => {
                    info!("RuleService: cancellation requested, shutting down...");
                    break;
                }
                _ = tickTimer.tick() => {
                    self.checkForEvaluate().await;
                }
                Some(msg) = self.rxMqtt.recv() => {
                    if let IntMsg::RingOccurred(autoBuzz) = msg {
                        self.handleRing(autoBuzz).await;
                    }
                }
            }
        }
    }

    // Runs on every tick and evaluates the rules only if something can actually have happened:
    // a rule was changed in the web UI, a single-shot removal has come due, or the clock moved on
    // to a new minute. Otherwise there is nothing to do.
    async fn checkForEvaluate(&mut self) {
        let now = Local::now();
        // All three are evaluated (no short-circuit), so the changed-flag is always consumed.
        let rulesChanged = self.rulesChanged.swap(false, Ordering::Acquire);
        let removalDue = self.pendingRemoval.anyDue(now);
        let newMinute = Self::minuteIndex(now) != self.lastEvaluateMinute;

        if rulesChanged || removalDue || newMinute {
            self.evaluateTransitions().await;
        }
    }

    async fn evaluateTransitions(&mut self) {
        let now = Local::now();
        self.lastEvaluateMinute = Self::minuteIndex(now);

        // Handle single shot removals. Remember which rules we removed ourselves, to tell them apart
        // from rules deleted in the web UI when logging the deactivation below.
        let toRemove: Vec<i64> = self.pendingRemoval.dueIds(now);
        let mut autoRemovedIds: HashSet<i64> = HashSet::new();
        for id in toRemove {
            if self.handleSingleShotRemoval(id).await {
                autoRemovedIds.insert(id);
            }
        }

        let autoBuzzWasActiveByRule = !self.activated.is_empty();

        // Check the stored rules for active rules.
        let rules = self.store.getRules();
        for rule in &rules {
            let isActive = rule.isActiveAt(now);
            let wasActive = self.activated.contains_key(&rule.id);
            if isActive {
                // Refresh the stored rule state.
                self.activated.insert(rule.id, rule.clone());
                if !wasActive {
                    self.logForRule(LogEventType::RuleActivate, rule).await;
                }
            } else if wasActive {
                self.activated.remove(&rule.id);
                self.logForRule(LogEventType::RuleDeactivate, rule).await;
            }
        }

        // Get all rules which have to be deactivated because they were deleted from the store.
        let allStoredRuleIds: HashSet<i64> = rules.iter().map(|r| r.id).collect();
        let deletedRules: Vec<Rule> = self.activated.extract_if(|id, _| !allStoredRuleIds.contains(id)).map(|(_, rule)| rule).collect();
        for rule in &deletedRules {
            // The event type already says "due to deletion", so only our own removal needs a remark.
            let additionalInfo = if autoRemovedIds.contains(&rule.id) { ", auto removed" } else { "" };
            self.logForRuleWithInfo(LogEventType::RuleDeactivateDelete, rule, additionalInfo).await;
        }

        // Now we have the new rule based auto buzz state. The rules never switch the auto-buzzer of the
        // device - they buzz themselves on a ring - so only the web UI state is updated here. Otherwise
        // the auto-buzz switch on the device would appear broken to the user.
        let autoBuzzIsActiveByRule = !self.activated.is_empty();
        if autoBuzzIsActiveByRule != autoBuzzWasActiveByRule {
            self.sendRuleAutoBuzzState(autoBuzzIsActiveByRule).await;
            if autoBuzzIsActiveByRule {
                let details = format!("Active rules: '{}'", Self::joinRuleNames(self.activated.values()));
                self.logEvent(LogEventType::RuleAutoBuzzOn, details).await;
            } else {
                self.logEvent(LogEventType::RuleAutoBuzzOff, "").await;
            }
        }
    }

    // Reacts to a real ring for all rules which are active right now.
    async fn handleRing(&mut self, autoBuzz: bool) {
        let rules = self.store.getRules();
        let now = Local::now();
        let mut buzzRules: Vec<&Rule> = Vec::new();
        let mut ackRules: Vec<&Rule> = Vec::new();
        for rule in rules.iter().filter(|r| r.isActiveAt(now)) {
            if !autoBuzz {
                // The device auto-buzzer was off, so imitate it by buzzing ourselves. If it was on, the
                // device already opened the door and a rule must not buzz a second time.
                buzzRules.push(rule);
            }
            if rule.autoAck {
                ackRules.push(rule);
            }
            if rule.singleShot {
                self.pendingRemoval.scheduleIfAbsent(rule.id, now + HUMAN_DOOR_OPEN_TOLERANCE_TIME);
            }
        }

        // Do not send two ACKs or two BUZZes for multiple rules at the same time.
        if !buzzRules.is_empty() {
            self.sendMqtt(IntMsg::Buzz).await;
            self.logForRules(LogEventType::RuleBuzz, buzzRules.into_iter()).await;
        }
        if !ackRules.is_empty() {
            self.sendMqtt(IntMsg::AckRing).await;
            self.logForRules(LogEventType::RuleAutoAck, ackRules.into_iter()).await;
        }
    }

    async fn logForRule(&self, eventType: LogEventType, rule: &Rule) {
        self.logForRuleWithInfo(eventType, rule, "").await;
    }

    async fn logForRuleWithInfo(&self, eventType: LogEventType, rule: &Rule, additionalInfo: &str) {
        self.logEvent(eventType, format!("Rule: '{}'{}", rule.name, additionalInfo)).await;
    }

    async fn logForRules<'a>(&self, eventType: LogEventType, rules: impl Iterator<Item = &'a Rule>) {
        self.logEvent(eventType, format!("Rules: '{}'", Self::joinRuleNames(rules))).await;
    }

    // Removes a single-shot rule after the grace period. Returns whether the rule was really removed,
    // so the caller can tell its deactivation apart from a deletion in the web UI. The auto-buzzer is
    // switched off by the next evaluation (unless another rule still needs it on).
    async fn handleSingleShotRemoval(&mut self, id: i64) -> bool {
        self.pendingRemoval.remove(id);
        if self.store.getRule(id).is_none() {
            info!("RuleService: single-shot rule {} not found for removal, skipping", id);
            return false;
        }
        self.store.deleteRule(id);
        true
    }

    // --------------------------- Helpers ---------------------------

    // The minute the given time falls into (minutes since the epoch), used to detect that the
    // clock advanced to a new minute - the accuracy of the rule activation times.
    fn minuteIndex(time: DateTime<Local>) -> i64 {
        time.timestamp().div_euclid(60)
    }

    // Comma separated rule names, sorted to get a stable order also for the unordered `activated`.
    fn joinRuleNames<'a>(rules: impl Iterator<Item = &'a Rule>) -> String {
        let mut names: Vec<&str> = rules.map(|rule| rule.name.as_str()).collect();
        names.sort_unstable();
        names.join(", ")
    }

    async fn sendMqtt(&self, msg: IntMsg) {
        if let Err(e) = self.mqttTx.send(msg).await {
            // Losing a device command means a ring stays unanswered, so this must not stay silent.
            error!("RuleService: failed to send device command: {}", e);
        }
    }

    // Tells the web UI whether a rule is currently armed to buzz on a ring.
    async fn sendRuleAutoBuzzState(&self, activeByRule: bool) {
        if let Err(e) = self.httpTx.send(IntMsg::SetRuleAutoBuzz(activeByRule)).await {
            error!("RuleService: failed to report the rule auto-buzz state: {}", e);
        }
    }

    async fn logEvent(&self, eventType: LogEventType, details: impl Into<String>) {
        self.logTx.send(LogEvent::new(eventType, details)).await.unwrap_or_default();
    }
}
