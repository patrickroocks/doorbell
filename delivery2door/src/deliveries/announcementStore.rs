use crate::common::APP_PATHS;
use crate::crawlers::deliveryWindow::DeliveryWindow;

use chrono::{DateTime, Local, TimeZone, Utc};
use rusqlite::Connection;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use tracing::{error, info};

// Table and column names, used to build the SQL statements below.
const TABLE_NAME: &str = "deliveryAnnouncements";
const DELIVERY_ID_COL: &str = "deliveryId";
const SOURCE_COL: &str = "sourceName";
const CRAWLER_COL: &str = "crawlerName";
const RULE_ID_COL: &str = "ruleId";
const WINDOW_FROM_COL: &str = "windowFrom";
const WINDOW_TO_COL: &str = "windowTo";
const STATE_COL: &str = "state";
const FIRST_SEEN_COL: &str = "firstSeen";
const LAST_UPDATED_COL: &str = "lastUpdated";

const STATE_ACTIVE: &str = "active";
const STATE_DONE: &str = "done";

/// Whether a delivery still lies ahead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnouncementState {
    /// The window is still to come or running, and a rule in the doorbell service covers it.
    Active,
    /// The window has passed, or the rule is gone. Kept, so the mails about this delivery can never
    /// produce a rule again.
    Done,
}

impl AnnouncementState {
    fn asStr(&self) -> &'static str {
        match self {
            AnnouncementState::Active => STATE_ACTIVE,
            AnnouncementState::Done => STATE_DONE,
        }
    }

    fn fromStr(stored: &str) -> Self {
        match stored {
            STATE_ACTIVE => AnnouncementState::Active,
            // Anything unknown counts as finished: that is the state that does nothing.
            _ => AnnouncementState::Done,
        }
    }
}

impl fmt::Display for AnnouncementState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.asStr())
    }
}

/// One announced delivery and the doorbell rule that covers it.
#[derive(Debug, Clone)]
pub struct DeliveryAnnouncement {
    /// The delivery id of the carrier, e.g. a DHL piececode. The identity of the parcel.
    pub deliveryId: String,
    pub sourceName: String,
    pub crawlerName: String,
    /// Id of the rule in the doorbell service.
    pub ruleId: i64,
    /// The window the doorbell service *knows*, not the one just parsed. The difference is what makes
    /// the reconciliation idempotent: as long as they are equal there is nothing to send.
    pub window: DeliveryWindow,
    pub state: AnnouncementState,
    pub firstSeen: DateTime<Local>,
    pub lastUpdated: DateTime<Local>,
}

struct StoreInner {
    connection: Connection,
    announcements: HashMap<String, DeliveryAnnouncement>,
}

/// Maps delivery ids to the doorbell rules that cover them.
///
/// Built like the doorbell server's `RuleStore`: the whole table in memory behind one mutex that also
/// guards the connection, so the database and the memory can never drift apart, and cheaply cloneable
/// so the poll service and the HTTP handlers share it.
#[derive(Clone)]
pub struct AnnouncementStore {
    inner: Arc<Mutex<StoreInner>>,
}

impl AnnouncementStore {
    pub fn new() -> Self {
        let connection = Connection::open(&APP_PATHS.deliveryDbFile).unwrap_or_else(|e| {
            error!("Failed to open delivery database {}: {}", APP_PATHS.deliveryDbFile.display(), e);
            std::process::exit(1);
        });
        Self::initSchema(&connection);

        let announcements = Self::loadFromDb(&connection);
        info!("Loaded {} delivery announcement(s)", announcements.len());

        AnnouncementStore {
            inner: Arc::new(Mutex::new(StoreInner { connection, announcements })),
        }
    }

    fn initSchema(connection: &Connection) {
        let schema = format!(
            "CREATE TABLE IF NOT EXISTS {TABLE_NAME} (
                 {DELIVERY_ID_COL}  TEXT PRIMARY KEY,
                 {SOURCE_COL}       TEXT NOT NULL,
                 {CRAWLER_COL}      TEXT NOT NULL,
                 {RULE_ID_COL}      INTEGER NOT NULL,
                 {WINDOW_FROM_COL}  INTEGER NOT NULL,
                 {WINDOW_TO_COL}    INTEGER NOT NULL,
                 {STATE_COL}        TEXT NOT NULL,
                 {FIRST_SEEN_COL}   INTEGER NOT NULL,
                 {LAST_UPDATED_COL} INTEGER NOT NULL
             );"
        );
        connection.execute_batch(&schema).unwrap_or_else(|e| {
            error!("Failed to initialize the announcement schema: {}", e);
            std::process::exit(1);
        });
    }

    // --------------------------- Reads (in-memory) ---------------------------

    pub fn get(&self, deliveryId: &str) -> Option<DeliveryAnnouncement> {
        self.inner.lock().unwrap().announcements.get(deliveryId).cloned()
    }

    /// All announcements, newest first, for the web UI.
    pub fn all(&self) -> Vec<DeliveryAnnouncement> {
        let inner = self.inner.lock().unwrap();
        let mut all: Vec<DeliveryAnnouncement> = inner.announcements.values().cloned().collect();
        // Map order is arbitrary, so sort to keep the web UI stable.
        all.sort_by(|left, right| right.lastUpdated.cmp(&left.lastUpdated).then(left.deliveryId.cmp(&right.deliveryId)));
        all
    }

    pub fn countByState(&self, state: AnnouncementState) -> usize {
        self.inner.lock().unwrap().announcements.values().filter(|a| a.state == state).count()
    }

    /// The active announcements whose window has passed by `now`, i.e. the deliveries that are over.
    pub fn activeAndPassed(&self, now: DateTime<Local>) -> Vec<DeliveryAnnouncement> {
        self.inner
            .lock()
            .unwrap()
            .announcements
            .values()
            .filter(|a| a.state == AnnouncementState::Active && a.window.to <= now)
            .cloned()
            .collect()
    }

    // --------------------------- Writes ---------------------------

    /// Stores a new announcement, or replaces one completely.
    pub fn upsert(&self, announcement: &DeliveryAnnouncement) {
        let sql = format!(
            "INSERT OR REPLACE INTO {TABLE_NAME} ({DELIVERY_ID_COL}, {SOURCE_COL}, {CRAWLER_COL}, {RULE_ID_COL}, \
             {WINDOW_FROM_COL}, {WINDOW_TO_COL}, {STATE_COL}, {FIRST_SEEN_COL}, {LAST_UPDATED_COL}) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
        );
        let mut inner = self.inner.lock().unwrap();
        let result = inner.connection.execute(
            &sql,
            rusqlite::params![
                announcement.deliveryId,
                announcement.sourceName,
                announcement.crawlerName,
                announcement.ruleId,
                announcement.window.from.timestamp(),
                announcement.window.to.timestamp(),
                announcement.state.asStr(),
                announcement.firstSeen.timestamp(),
                announcement.lastUpdated.timestamp(),
            ],
        );
        match result {
            Ok(_) => {
                inner.announcements.insert(announcement.deliveryId.clone(), announcement.clone());
            }
            Err(e) => error!("Failed to store delivery announcement {}: {}", announcement.deliveryId, e),
        }
    }

    /// Records the window the doorbell service now knows.
    pub fn setWindow(&self, deliveryId: &str, window: &DeliveryWindow, now: DateTime<Local>) {
        let Some(mut announcement) = self.get(deliveryId) else {
            error!("Cannot update the window of unknown delivery {}", deliveryId);
            return;
        };
        announcement.window = window.clone();
        announcement.lastUpdated = now;
        self.upsert(&announcement);
    }

    /// Marks a delivery as finished, so no mail about it can ever create a rule again.
    pub fn markDone(&self, deliveryId: &str, now: DateTime<Local>) {
        let Some(mut announcement) = self.get(deliveryId) else {
            return;
        };
        if announcement.state == AnnouncementState::Done {
            return;
        }
        announcement.state = AnnouncementState::Done;
        announcement.lastUpdated = now;
        self.upsert(&announcement);
    }

    /// Drops every announcement of a source, for the "re-scan" button of the web UI. The rules in the
    /// doorbell service are left alone - they delete themselves at the end of their window.
    pub fn forgetSource(&self, sourceName: &str) {
        let sql = format!("DELETE FROM {TABLE_NAME} WHERE {SOURCE_COL} = ?1");
        let mut inner = self.inner.lock().unwrap();
        match inner.connection.execute(&sql, rusqlite::params![sourceName]) {
            Ok(count) => {
                inner.announcements.retain(|_, a| a.sourceName != sourceName);
                info!("Forgot {} delivery announcement(s) of source {}", count, sourceName);
            }
            Err(e) => error!("Failed to forget the announcements of source {}: {}", sourceName, e),
        }
    }

    // --------------------------- Loading ---------------------------

    fn loadFromDb(connection: &Connection) -> HashMap<String, DeliveryAnnouncement> {
        let sql = format!(
            "SELECT {DELIVERY_ID_COL}, {SOURCE_COL}, {CRAWLER_COL}, {RULE_ID_COL}, {WINDOW_FROM_COL}, \
             {WINDOW_TO_COL}, {STATE_COL}, {FIRST_SEEN_COL}, {LAST_UPDATED_COL} FROM {TABLE_NAME}"
        );
        let mut stmt = match connection.prepare(&sql) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to prepare the announcement query: {}", e);
                return HashMap::new();
            }
        };
        let rows = stmt.query_map([], |row| {
            let windowFrom: i64 = row.get(4)?;
            let windowTo: i64 = row.get(5)?;
            let state: String = row.get(6)?;
            let firstSeen: i64 = row.get(7)?;
            let lastUpdated: i64 = row.get(8)?;
            Ok(DeliveryAnnouncement {
                deliveryId: row.get(0)?,
                sourceName: row.get(1)?,
                crawlerName: row.get(2)?,
                ruleId: row.get(3)?,
                window: DeliveryWindow {
                    from: toLocal(windowFrom),
                    to: toLocal(windowTo),
                },
                state: AnnouncementState::fromStr(&state),
                firstSeen: toLocal(firstSeen),
                lastUpdated: toLocal(lastUpdated),
            })
        });
        match rows {
            Ok(iter) => iter.filter_map(|r| r.ok()).map(|a| (a.deliveryId.clone(), a)).collect(),
            Err(e) => {
                error!("Failed to load the delivery announcements: {}", e);
                HashMap::new()
            }
        }
    }
}

/// Local time of a stored unix timestamp. A timestamp out of range cannot happen for a value this
/// service wrote itself, so the epoch is a good enough fallback.
fn toLocal(timestamp: i64) -> DateTime<Local> {
    Utc.timestamp_opt(timestamp, 0).single().unwrap_or_else(|| Utc.timestamp_opt(0, 0).unwrap()).with_timezone(&Local)
}
