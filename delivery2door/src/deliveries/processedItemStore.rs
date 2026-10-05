use crate::common::APP_PATHS;

use chrono::{DateTime, Local, TimeZone, Utc};
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tracing::{error, info};

// Table and column names, used to build the SQL statements below.
const ITEMS_TABLE: &str = "processedItems";
const ITEM_ID_COL: &str = "itemId";
const SOURCE_COL: &str = "sourceName";
const SEQUENCE_COL: &str = "sequence";
const DESCRIPTION_COL: &str = "description";
const PROCESSED_AT_COL: &str = "processedAt";
const DELIVERY_IDS_COL: &str = "deliveryIds";

const STATE_TABLE: &str = "sourceState";
const GENERATION_COL: &str = "generation";
const HIGHEST_SEQUENCE_COL: &str = "highestSequence";

// Separator of the delivery ids of one item. Delivery ids are alphanumeric, so this cannot clash.
const DELIVERY_ID_SEPARATOR: &str = ",";

/// Where a source got to in its own numbering.
///
/// `generation` guards the sequence numbers: an IMAP mailbox reports a `UIDVALIDITY`, and if that
/// changes the UIDs were reused for other mails, which makes every remembered number meaningless.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcePosition {
    pub generation: u32,
    pub highestSequence: u32,
}

/// One item that has already been looked at, as shown in the web UI.
#[derive(Debug, Clone)]
pub struct ProcessedItem {
    pub itemId: String,
    pub sourceName: String,
    pub sequence: Option<u32>,
    pub description: String,
    pub processedAt: DateTime<Local>,
    pub deliveryIds: Vec<String>,
}

struct StoreInner {
    connection: Connection,
    // Ids of all processed items, so the check whether an item is new needs no query.
    knownIds: HashSet<String>,
    positions: HashMap<String, SourcePosition>,
}

/// Remembers which items a source has already delivered, so none of them is ever fetched or parsed
/// twice.
///
/// This is what keeps the traffic on the mail account down: without it every poll cycle would
/// download the whole folder again, including all the mail that has nothing to do with deliveries.
/// Two mechanisms, because neither is sufficient on its own:
///
/// * `SourcePosition` lets the source ask its server for new items only - nothing else is even
///   transferred. Useless after a generation change, and blind to an item that appears with an
///   already-seen number (a mail moved into the folder).
/// * `knownIds` catches exactly those cases, at the price of one cheap identity request per
///   candidate.
///
/// Built like the doorbell server's `RuleStore`: SQLite for durability with the whole state in
/// memory behind one mutex that guards both, and cheaply cloneable so the poll service and the HTTP
/// handlers share it.
#[derive(Clone)]
pub struct ProcessedItemStore {
    inner: Arc<Mutex<StoreInner>>,
}

impl ProcessedItemStore {
    pub fn new() -> Self {
        let connection = Connection::open(&APP_PATHS.deliveryDbFile).unwrap_or_else(|e| {
            error!("Failed to open delivery database {}: {}", APP_PATHS.deliveryDbFile.display(), e);
            std::process::exit(1);
        });
        Self::initSchema(&connection);

        let knownIds = Self::loadKnownIds(&connection);
        let positions = Self::loadPositions(&connection);
        info!(
            "Loaded {} processed item(s) and {} source position(s) from {}",
            knownIds.len(),
            positions.len(),
            APP_PATHS.deliveryDbFile.display()
        );

        ProcessedItemStore {
            inner: Arc::new(Mutex::new(StoreInner {
                connection,
                knownIds,
                positions,
            })),
        }
    }

    fn initSchema(connection: &Connection) {
        let schema = format!(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS {ITEMS_TABLE} (
                 {ITEM_ID_COL}      TEXT PRIMARY KEY,
                 {SOURCE_COL}       TEXT NOT NULL,
                 {SEQUENCE_COL}     INTEGER,
                 {DESCRIPTION_COL}  TEXT NOT NULL,
                 {PROCESSED_AT_COL} INTEGER NOT NULL,
                 {DELIVERY_IDS_COL} TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_{ITEMS_TABLE}_{PROCESSED_AT_COL}
                 ON {ITEMS_TABLE}({PROCESSED_AT_COL});
             CREATE TABLE IF NOT EXISTS {STATE_TABLE} (
                 {SOURCE_COL}           TEXT PRIMARY KEY,
                 {GENERATION_COL}       INTEGER NOT NULL,
                 {HIGHEST_SEQUENCE_COL} INTEGER NOT NULL
             );"
        );
        connection.execute_batch(&schema).unwrap_or_else(|e| {
            error!("Failed to initialize the delivery schema: {}", e);
            std::process::exit(1);
        });
    }

    // --------------------------- Reads (in-memory) ---------------------------

    /// Whether the item has already been processed, i.e. must not be fetched or parsed again.
    pub fn isKnown(&self, itemId: &str) -> bool {
        self.inner.lock().unwrap().knownIds.contains(itemId)
    }

    /// How far the source got last time, or None if it has never run (or was reset for a re-scan).
    pub fn position(&self, sourceName: &str) -> Option<SourcePosition> {
        self.inner.lock().unwrap().positions.get(sourceName).copied()
    }

    // --------------------------- Reads (database) ---------------------------

    /// The most recently processed items, newest first, for the web UI. Read from the database
    /// because only the ids are kept in memory - the details are only needed by the UI.
    pub fn recentItems(&self, limit: u32) -> Vec<ProcessedItem> {
        let sql = format!(
            "SELECT {ITEM_ID_COL}, {SOURCE_COL}, {SEQUENCE_COL}, {DESCRIPTION_COL}, {PROCESSED_AT_COL}, {DELIVERY_IDS_COL} \
             FROM {ITEMS_TABLE} ORDER BY {PROCESSED_AT_COL} DESC, {SEQUENCE_COL} DESC LIMIT ?1"
        );
        let inner = self.inner.lock().unwrap();
        let mut stmt = match inner.connection.prepare(&sql) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to prepare the processed items query: {}", e);
                return Vec::new();
            }
        };
        let rows = stmt.query_map(rusqlite::params![limit], |row| {
            let processedAt: i64 = row.get(4)?;
            let deliveryIds: String = row.get(5)?;
            Ok(ProcessedItem {
                itemId: row.get(0)?,
                sourceName: row.get(1)?,
                sequence: row.get(2)?,
                description: row.get(3)?,
                processedAt: Utc.timestamp_opt(processedAt, 0).single().unwrap_or_else(Utc::now).with_timezone(&Local),
                deliveryIds: splitDeliveryIds(&deliveryIds),
            })
        });
        match rows {
            Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                error!("Failed to query the processed items: {}", e);
                Vec::new()
            }
        }
    }

    /// How many items of a source have been processed, for the web UI.
    pub fn countForSource(&self, sourceName: &str) -> u32 {
        let sql = format!("SELECT COUNT(*) FROM {ITEMS_TABLE} WHERE {SOURCE_COL} = ?1");
        let inner = self.inner.lock().unwrap();
        inner.connection.query_row(&sql, rusqlite::params![sourceName], |row| row.get(0)).unwrap_or(0)
    }

    // --------------------------- Writes ---------------------------
    // The in-memory state is only updated once the database write succeeded, so an item counts as
    // processed exactly when it is durably recorded. On a crash in between it is fetched again, which
    // is the harmless direction.

    /// Records an item as processed, together with the delivery ids it yielded (empty for the mail
    /// that has nothing to do with a delivery - that one has to be paid for exactly once, too).
    pub fn recordProcessed(&self, item: &ProcessedItem) {
        let sql = format!(
            "INSERT OR REPLACE INTO {ITEMS_TABLE} \
             ({ITEM_ID_COL}, {SOURCE_COL}, {SEQUENCE_COL}, {DESCRIPTION_COL}, {PROCESSED_AT_COL}, {DELIVERY_IDS_COL}) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        );
        let mut inner = self.inner.lock().unwrap();
        let result = inner.connection.execute(
            &sql,
            rusqlite::params![
                item.itemId,
                item.sourceName,
                item.sequence,
                item.description,
                item.processedAt.timestamp(),
                item.deliveryIds.join(DELIVERY_ID_SEPARATOR),
            ],
        );
        match result {
            Ok(_) => {
                inner.knownIds.insert(item.itemId.clone());
            }
            Err(e) => error!("Failed to record processed item {}: {}", item.itemId, e),
        }
    }

    /// Stores how far the source got. Only called after the items up to `highestSequence` have been
    /// recorded, so a crash can never skip one.
    pub fn setPosition(&self, sourceName: &str, position: SourcePosition) {
        let sql = format!(
            "INSERT OR REPLACE INTO {STATE_TABLE} ({SOURCE_COL}, {GENERATION_COL}, {HIGHEST_SEQUENCE_COL}) \
             VALUES (?1, ?2, ?3)"
        );
        let mut inner = self.inner.lock().unwrap();
        let result = inner.connection.execute(&sql, rusqlite::params![sourceName, position.generation, position.highestSequence]);
        match result {
            Ok(_) => {
                inner.positions.insert(sourceName.to_string(), position);
            }
            Err(e) => error!("Failed to store the position of source {}: {}", sourceName, e),
        }
    }

    /// Forgets everything about a source, so its next poll reads it from scratch. The handle behind
    /// the "re-scan" button of the web UI.
    pub fn forgetSource(&self, sourceName: &str) {
        let mut inner = self.inner.lock().unwrap();
        let deleteItems = format!("DELETE FROM {ITEMS_TABLE} WHERE {SOURCE_COL} = ?1");
        let deleteState = format!("DELETE FROM {STATE_TABLE} WHERE {SOURCE_COL} = ?1");
        for sql in [deleteItems, deleteState] {
            if let Err(e) = inner.connection.execute(&sql, rusqlite::params![sourceName]) {
                error!("Failed to forget source {}: {}", sourceName, e);
                return;
            }
        }
        inner.positions.remove(sourceName);
        // The ids of the other sources stay, so the whole set has to be rebuilt from the database.
        inner.knownIds = Self::loadKnownIds(&inner.connection);
        info!("Forgot all processed items of source {}", sourceName);
    }

    // --------------------------- Loading ---------------------------

    fn loadKnownIds(connection: &Connection) -> HashSet<String> {
        let sql = format!("SELECT {ITEM_ID_COL} FROM {ITEMS_TABLE}");
        let mut stmt = match connection.prepare(&sql) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to prepare the processed item query: {}", e);
                return HashSet::new();
            }
        };
        match stmt.query_map([], |row| row.get::<_, String>(0)) {
            Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                error!("Failed to load the processed items: {}", e);
                HashSet::new()
            }
        }
    }

    fn loadPositions(connection: &Connection) -> HashMap<String, SourcePosition> {
        let sql = format!("SELECT {SOURCE_COL}, {GENERATION_COL}, {HIGHEST_SEQUENCE_COL} FROM {STATE_TABLE}");
        let mut stmt = match connection.prepare(&sql) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to prepare the source state query: {}", e);
                return HashMap::new();
            }
        };
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                SourcePosition {
                    generation: row.get(1)?,
                    highestSequence: row.get(2)?,
                },
            ))
        });
        match rows {
            Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                error!("Failed to load the source states: {}", e);
                HashMap::new()
            }
        }
    }
}

fn splitDeliveryIds(stored: &str) -> Vec<String> {
    if stored.is_empty() {
        return Vec::new();
    }
    stored.split(DELIVERY_ID_SEPARATOR).map(|id| id.to_string()).collect()
}
