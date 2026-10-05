use crate::common::APP_PATHS;
use crate::rules::rule::{RULE_TYPE_AUTO_BUZZ, Rule};

use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::{error, info, warn};

// Rule table and column names, used to build the SQL statements below.
const TABLE_NAME: &str = "rules";
const ID_COL: &str = "id";
const NAME_COL: &str = "name";
const ENABLED_COL: &str = "enabled";
const AUTO_ACK_COL: &str = "autoAck";
const SINGLE_SHOT_COL: &str = "singleShot";
const DELETE_AT_END_COL: &str = "deleteAtEnd";
const START_DATE_COL: &str = "startDate";
const END_DATE_COL: &str = "endDate";
const ACTIVATION_TIME_COL: &str = "activationTime";
const DEACTIVATION_TIME_COL: &str = "deactivationTime";
const RULE_TYPE_COL: &str = "ruleType";

// Prefix for auto-generated names of unnamed rules: "unnamed (1)", "unnamed (2)", ...
const UNNAMED_PREFIX: &str = "unnamed";

// The rule set held in memory, together with the SQLite connection that persists it. A single
// mutex guards both, so a write is applied to the database and the in-memory rules under the same
// lock and the two can never drift apart.
struct RuleStoreInner {
    connection: Connection,
    rules: HashMap<i64, Rule>,
}

// Owns the rules in memory, backed by SQLite for durability. The database is read once at startup;
// afterwards all reads are served from memory and only writes touch the database.
// Cheaply cloneable (shares the state), so the RuleService task and the HTTP handlers operate on
// the same rules.
#[derive(Clone)]
pub struct RuleStore {
    inner: Arc<Mutex<RuleStoreInner>>,
}

impl RuleStore {
    pub fn new() -> Self {
        let connection = Connection::open(&APP_PATHS.ruleDbFile).unwrap_or_else(|e| {
            error!("Failed to open rule database {}: {}", APP_PATHS.ruleDbFile.display(), e);
            std::process::exit(1);
        });
        Self::initSchema(&connection);
        Self::migrateSchema(&connection);

        // The only read of the rule table; from here on the in-memory map is the source of truth.
        let rules = Self::loadRulesFromDb(&connection);
        info!("Loaded {} rule(s) from {}", rules.len(), APP_PATHS.ruleDbFile.display());

        RuleStore {
            inner: Arc::new(Mutex::new(RuleStoreInner { connection, rules })),
        }
    }

    fn initSchema(connection: &Connection) {
        let schema = format!(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS {TABLE_NAME} (
                 {ID_COL}                INTEGER PRIMARY KEY AUTOINCREMENT,
                 {NAME_COL}              TEXT NOT NULL,
                 {ENABLED_COL}           INTEGER NOT NULL,
                 {AUTO_ACK_COL}          INTEGER NOT NULL,
                 {SINGLE_SHOT_COL}       INTEGER NOT NULL,
                 {DELETE_AT_END_COL}     INTEGER NOT NULL DEFAULT 0,
                 {START_DATE_COL}        TEXT,
                 {END_DATE_COL}          TEXT,
                 {ACTIVATION_TIME_COL}   TEXT NOT NULL,
                 {DEACTIVATION_TIME_COL} TEXT NOT NULL,
                 {RULE_TYPE_COL}         TEXT NOT NULL
             );"
        );
        connection.execute_batch(&schema).unwrap_or_else(|e| {
            error!("Failed to initialize rule schema: {}", e);
            std::process::exit(1);
        });
    }

    // Brings a database created by an older version up to the current schema. `CREATE TABLE IF NOT
    // EXISTS` above does nothing for a table that already exists, so a column added later has to be
    // added here - otherwise every statement below fails on an existing rules database.
    fn migrateSchema(connection: &Connection) {
        for (column, definition) in [(DELETE_AT_END_COL, "INTEGER NOT NULL DEFAULT 0")] {
            if Self::hasColumn(connection, column) {
                continue;
            }
            let sql = format!("ALTER TABLE {TABLE_NAME} ADD COLUMN {} {}", column, definition);
            match connection.execute(&sql, []) {
                Ok(_) => info!("Rule database migrated: added column '{}'", column),
                Err(e) => {
                    error!("Failed to add column '{}' to the rule table: {}", column, e);
                    std::process::exit(1);
                }
            }
        }
    }

    fn hasColumn(connection: &Connection, column: &str) -> bool {
        let sql = format!("SELECT COUNT(*) FROM pragma_table_info('{TABLE_NAME}') WHERE name = ?1");
        match connection.query_row(&sql, rusqlite::params![column], |row| row.get::<_, i64>(0)) {
            Ok(count) => count > 0,
            Err(e) => {
                error!("Failed to inspect the rule table: {}", e);
                std::process::exit(1);
            }
        }
    }

    // Reads the whole rule table into a map. Only called once, at startup.
    fn loadRulesFromDb(connection: &Connection) -> HashMap<i64, Rule> {
        let sql = format!(
            "SELECT {ID_COL}, {NAME_COL}, {ENABLED_COL}, {AUTO_ACK_COL}, {SINGLE_SHOT_COL}, \
             {START_DATE_COL}, {END_DATE_COL}, {ACTIVATION_TIME_COL}, {DEACTIVATION_TIME_COL}, {DELETE_AT_END_COL} \
             FROM {TABLE_NAME}"
        );
        let mut stmt = match connection.prepare(&sql) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to prepare rule query: {}", e);
                return HashMap::new();
            }
        };
        let rows = stmt.query_map([], Self::rowToRule);
        match rows {
            Ok(iter) => iter.filter_map(|r| r.ok()).flatten().map(|rule| (rule.id, rule)).collect(),
            Err(e) => {
                error!("Failed to load rules: {}", e);
                HashMap::new()
            }
        }
    }

    // --------------------------- Reads (in-memory) ---------------------------

    // Returns all rules ordered by id.
    pub fn getRules(&self) -> Vec<Rule> {
        let inner = self.inner.lock().unwrap();
        let mut rules: Vec<Rule> = inner.rules.values().cloned().collect();
        // Map iteration order is arbitrary, so sort to keep the order in the web UI stable.
        rules.sort_by_key(|rule| rule.id);
        rules
    }

    pub fn getRule(&self, id: i64) -> Option<Rule> {
        self.inner.lock().unwrap().rules.get(&id).cloned()
    }

    // Builds the next free "unnamed (N)" name (smallest N >= 1 not already taken).
    pub fn nextUnnamedName(&self) -> String {
        let inner = self.inner.lock().unwrap();
        let mut n = 1;
        loop {
            let candidate = format!("{} ({})", UNNAMED_PREFIX, n);
            if !inner.rules.values().any(|rule| rule.name == candidate) {
                return candidate;
            }
            n += 1;
        }
    }

    // --------------------------- Writes (database and memory) ---------------------------
    // The in-memory rules are only updated once the database write succeeded, so they always
    // mirror the persisted state.

    // Inserts a new rule (its `id` field is ignored) and returns the generated id.
    pub fn insertRule(&self, rule: &Rule) -> Option<i64> {
        let sql = format!(
            "INSERT INTO {TABLE_NAME} ({NAME_COL}, {ENABLED_COL}, {AUTO_ACK_COL}, {SINGLE_SHOT_COL}, \
             {START_DATE_COL}, {END_DATE_COL}, {ACTIVATION_TIME_COL}, {DEACTIVATION_TIME_COL}, {RULE_TYPE_COL}, \
             {DELETE_AT_END_COL}) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
        );
        let mut inner = self.inner.lock().unwrap();
        let result = inner.connection.execute(
            &sql,
            rusqlite::params![
                rule.name,
                rule.enabled as i64,
                rule.autoAck as i64,
                rule.singleShot as i64,
                Self::dateParam(rule.startDate),
                Self::dateParam(rule.endDate),
                rule.activationTimeStr(),
                rule.deactivationTimeStr(),
                RULE_TYPE_AUTO_BUZZ,
                rule.deleteAtEnd as i64,
            ],
        );
        match result {
            Ok(_) => {
                // Cache the rule under the id the database generated for it.
                let id = inner.connection.last_insert_rowid();
                let mut storedRule = rule.clone();
                storedRule.id = id;
                inner.rules.insert(id, storedRule);
                Some(id)
            }
            Err(e) => {
                error!("Failed to insert rule: {}", e);
                None
            }
        }
    }

    // Updates an existing rule. Returns whether the rule was really updated: a rule that has been
    // deleted meanwhile (e.g. auto-removed single shot) must not be resurrected in memory.
    pub fn updateRule(&self, rule: &Rule) -> bool {
        let sql = format!(
            "UPDATE {TABLE_NAME} SET {NAME_COL} = ?1, {ENABLED_COL} = ?2, {AUTO_ACK_COL} = ?3, \
             {SINGLE_SHOT_COL} = ?4, {START_DATE_COL} = ?5, {END_DATE_COL} = ?6, \
             {ACTIVATION_TIME_COL} = ?7, {DEACTIVATION_TIME_COL} = ?8, {DELETE_AT_END_COL} = ?9 \
             WHERE {ID_COL} = ?10"
        );
        let mut inner = self.inner.lock().unwrap();
        let result = inner.connection.execute(
            &sql,
            rusqlite::params![
                rule.name,
                rule.enabled as i64,
                rule.autoAck as i64,
                rule.singleShot as i64,
                Self::dateParam(rule.startDate),
                Self::dateParam(rule.endDate),
                rule.activationTimeStr(),
                rule.deactivationTimeStr(),
                rule.deleteAtEnd as i64,
                rule.id,
            ],
        );
        match result {
            // No row matched the id, so the rule is gone from the database.
            Ok(0) => {
                warn!("Rule {} no longer exists, update ignored", rule.id);
                false
            }
            Ok(_) => {
                inner.rules.insert(rule.id, rule.clone());
                true
            }
            Err(e) => {
                error!("Failed to update rule {}: {}", rule.id, e);
                false
            }
        }
    }

    pub fn deleteRule(&self, id: i64) {
        let sql = format!("DELETE FROM {TABLE_NAME} WHERE {ID_COL} = ?1");
        let mut inner = self.inner.lock().unwrap();
        match inner.connection.execute(&sql, rusqlite::params![id]) {
            Ok(_) => {
                inner.rules.remove(&id);
            }
            Err(e) => error!("Failed to delete rule {}: {}", id, e),
        }
    }

    // --------------------------- Helpers ---------------------------

    fn dateParam(date: Option<chrono::NaiveDate>) -> Option<String> {
        date.map(|d| d.format(crate::rules::rule::DATE_FORMAT).to_string())
    }

    // Maps a DB row (in the SELECT column order used above) to a Rule. Rows with unparsable
    // required times are skipped (returns Ok(None)).
    fn rowToRule(row: &rusqlite::Row) -> rusqlite::Result<Option<Rule>> {
        let id: i64 = row.get(0)?;
        let name: String = row.get(1)?;
        let enabled: i64 = row.get(2)?;
        let autoAck: i64 = row.get(3)?;
        let singleShot: i64 = row.get(4)?;
        let startDate: Option<String> = row.get(5)?;
        let endDate: Option<String> = row.get(6)?;
        let activationTime: String = row.get(7)?;
        let deactivationTime: String = row.get(8)?;
        let deleteAtEnd: i64 = row.get(9)?;

        let activation = match Rule::parseTime(&activationTime) {
            Some(t) => t,
            None => {
                error!("Rule {} has invalid activation time '{}', skipping", id, activationTime);
                return Ok(None);
            }
        };
        let deactivation = match Rule::parseTime(&deactivationTime) {
            Some(t) => t,
            None => {
                error!("Rule {} has invalid deactivation time '{}', skipping", id, deactivationTime);
                return Ok(None);
            }
        };

        Ok(Some(Rule {
            id,
            name,
            enabled: enabled != 0,
            autoAck: autoAck != 0,
            singleShot: singleShot != 0,
            deleteAtEnd: deleteAtEnd != 0,
            startDate: startDate.as_deref().and_then(Rule::parseDate),
            endDate: endDate.as_deref().and_then(Rule::parseDate),
            activationTime: activation,
            deactivationTime: deactivation,
        }))
    }
}
