use crate::common::APP_PATHS;
use crate::common::AppInfo;
use crate::logger::eventTypes::{LogEvent, LogEventType};
use crate::util::notifyWithState::NotifyWithState;

use chrono::{DateTime, Months, Utc};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio::time::{self, Duration};
use tracing::{error, info};

const ROTATION_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

// Server log table and column names, used to build the SQL statements below.
const TABLE_NAME: &str = "serverLog";
const TS_COL: &str = "timestamp";
const SOURCE_COL: &str = "source";
const EVENT_TYPE_COL: &str = "eventType";
const DETAILS_COL: &str = "details";

// A single shared connection, guarded so the service task and the query server can use it.
pub type LogDb = Arc<Mutex<Connection>>;

pub struct LoggerService {
    db: LogDb,
    rx: mpsc::Receiver<LogEvent>,
    cancelNotify: Arc<NotifyWithState>,
    retentionDays: u32,
}

impl LoggerService {
    pub fn new(rx: mpsc::Receiver<LogEvent>, retentionDays: u32, cancelNotify: Arc<NotifyWithState>) -> Self {
        let connection = Connection::open(&APP_PATHS.logDbFile).unwrap_or_else(|e| {
            error!("Failed to open server log database {}: {}", APP_PATHS.logDbFile.display(), e);
            std::process::exit(1);
        });
        let service = LoggerService {
            db: Arc::new(Mutex::new(connection)),
            rx,
            cancelNotify,
            retentionDays,
        };
        service.initSchemaAndLogStart();
        service
    }

    // Read-only query handle for the HTTP layer; shares the underlying connection.
    pub fn queryService(&self) -> LogQueryService {
        LogQueryService::new(self.db.clone())
    }

    fn initSchemaAndLogStart(&self) {
        // Create the schema.
        let schema = format!(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             CREATE TABLE IF NOT EXISTS {TABLE_NAME} (
                 id               INTEGER PRIMARY KEY AUTOINCREMENT,
                 {TS_COL}         INTEGER NOT NULL,
                 {SOURCE_COL}     TEXT NOT NULL,
                 {EVENT_TYPE_COL} TEXT NOT NULL,
                 {DETAILS_COL}    TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_{TABLE_NAME}_{TS_COL} ON {TABLE_NAME}({TS_COL});"
        );
        // Kept to a single statement so the lock is released before writeEvent re-locks it.
        self.db.lock().unwrap().execute_batch(&schema).unwrap_or_else(|e| {
            error!("Failed to initialize server log schema: {}", e);
            std::process::exit(1);
        });

        // Record startup with version/build info, stop on error.
        let startEvent = LogEvent::new(
            LogEventType::ServerStarted,
            format!("version: {} ({})", AppInfo::getNameAndVersion(), AppInfo::getBuildTimeInfo()),
        );
        if !self.writeEvent(&startEvent) {
            error!("Failed to write initial server-started log event, server will be stopped");
            std::process::exit(1);
        }

        // Check for deletion at startup
        self.deleteOlderThanRetention();
    }

    pub async fn run(&mut self) {
        let mut deletionTimer = time::interval(ROTATION_INTERVAL);
        deletionTimer.tick().await; // consume the immediate first tick

        loop {
            tokio::select! {
                // Biased, because the cancellation branch stays ready once signaled: with the
                // random branch order the loop would keep consuming events instead of stopping.
                biased;

                _ = self.cancelNotify.notified() => {
                    info!("LoggerService: cancellation requested, shutting down...");
                    break;
                }
                Some(event) = self.rx.recv() => {
                    self.writeEvent(&event);
                }
                _ = deletionTimer.tick() => {
                    self.deleteOlderThanRetention();
                }
            }
        }

        // Write out what is still queued, so events from just before the shutdown (e.g. the buzz
        // that was clicked a moment ago) are not lost.
        self.rx.close();
        while let Ok(event) = self.rx.try_recv() {
            self.writeEvent(&event);
        }
    }

    // Writes a single event, returning whether it succeeded.
    fn writeEvent(&self, event: &LogEvent) -> bool {
        let detailsStr = if event.details.is_empty() {
            "".to_string()
        } else {
            format!(" ({})", event.details)
        };
        info!(
            "Event({}): {}{}",
            event.eventType.source().asStr(),
            event.eventType.description(),
            detailsStr
        );

        let sql = format!("INSERT INTO {TABLE_NAME} ({TS_COL}, {SOURCE_COL}, {EVENT_TYPE_COL}, {DETAILS_COL}) VALUES (?1, ?2, ?3, ?4)");
        let db = self.db.lock().unwrap();
        match db.execute(
            &sql,
            rusqlite::params![
                event.timestamp.timestamp(),
                event.eventType.source().asStr(),
                event.eventType.typeId(),
                &event.details,
            ],
        ) {
            Ok(_) => true,
            Err(e) => {
                error!("Failed to write server log event: {}", e);
                false
            }
        }
    }

    fn deleteOlderThanRetention(&self) {
        // Execute the deletion of old events.
        let cutoff = (Utc::now() - chrono::Duration::days(self.retentionDays as i64)).timestamp();
        let sql = format!("DELETE FROM {TABLE_NAME} WHERE {TS_COL} < ?1");
        let deletedRowsNum = {
            let db = self.db.lock().unwrap();
            db.execute(&sql, rusqlite::params![cutoff])
        };
        // Write deletion to log, after the db-lock is released.
        match deletedRowsNum {
            Ok(n) => {
                if n >= 1 {
                    self.writeEvent(&LogEvent::new(
                        LogEventType::LogsDeleted,
                        format!("Deleted {} entries older than {} days", n, self.retentionDays),
                    ));
                }
            }
            Err(e) => error!("Server log rotation failed: {}", e),
        }
    }
}

// Read-only query handle over the shared log database, used by the HTTP layer.
// Owns an Arc clone of the connection so it can outlive the LoggerService task.
pub struct LogQueryService {
    db: LogDb,
}

impl LogQueryService {
    pub fn new(db: LogDb) -> Self {
        Self { db }
    }

    // Time of the newest event of one of the given types, None if there is none (or on error).
    pub fn queryLastEventTime(&self, eventTypes: &[LogEventType]) -> Option<DateTime<Utc>> {
        if eventTypes.is_empty() {
            return None;
        }
        let placeholders = (1..=eventTypes.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(", ");
        let sql = format!("SELECT MAX({TS_COL}) FROM {TABLE_NAME} WHERE {EVENT_TYPE_COL} IN ({placeholders})");
        let typeIds: Vec<&str> = eventTypes.iter().map(|t| t.typeId()).collect();
        let db = self.db.lock().unwrap();
        match db.query_row(&sql, rusqlite::params_from_iter(typeIds), |row| row.get::<_, Option<i64>>(0)) {
            Ok(ts) => ts.and_then(|ts| DateTime::from_timestamp(ts, 0)),
            Err(e) => {
                error!("Failed to query last event time from server log: {}", e);
                None
            }
        }
    }

    // Reads events from the last `monthsBack` months, newest first.
    pub fn queryLogs(&self, monthsBack: u32) -> Vec<LogEvent> {
        // Prepare query.
        let cutoff = Utc::now().checked_sub_months(Months::new(monthsBack)).unwrap_or_else(Utc::now).timestamp();
        let sql = format!(
            "SELECT {TS_COL}, {EVENT_TYPE_COL}, {DETAILS_COL} FROM {TABLE_NAME} \
             WHERE {TS_COL} >= ?1 ORDER BY {TS_COL} DESC"
        );
        let db = self.db.lock().unwrap();
        let mut stmt = match db.prepare(&sql) {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to prepare server log query: {}", e);
                return Vec::new();
            }
        };
        // Execute query and convert to LogEvent.
        let recRows = stmt.query_map(rusqlite::params![cutoff], |row| {
            let ts: i64 = row.get(0)?;
            let typeId: String = row.get(1)?;
            let details: Option<String> = row.get(2)?;
            Ok(LogEvent::fromDb(ts, &typeId, details.unwrap_or_default()))
        });
        match recRows {
            Ok(iter) => iter.filter_map(|r| r.ok()).flatten().collect(),
            Err(e) => {
                error!("Failed to query server log: {}", e);
                Vec::new()
            }
        }
    }
}
