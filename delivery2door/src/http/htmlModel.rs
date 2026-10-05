use crate::common::AppInfo;
use crate::deliveries::announcementStore::{AnnouncementState, DeliveryAnnouncement};
use crate::deliveries::processedItemStore::ProcessedItem;
use crate::logger::eventTypes::{LogEvent, LogSource};

use askama::Template;
use chrono::{DateTime, Datelike, Local, Utc};
use serde::Deserialize;

/// Time format of every timestamp in the UI, as in the doorbell server.
const TIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

/// Fields every page needs, filled by `HtmlModel` before rendering.
struct BaseTemplateFields {
    app_name_and_version: String,
    build_time_info: String,
    asset_version: String, // cache buster for static assets, changes with every build
    current_year: u32,
    page_title: String,
    user_name: Option<String>,
    active_deliveries: usize,
    last_poll: String,
    last_success_message: Option<String>,
    last_err_message: Option<String>,
}

#[derive(Template)]
#[template(path = "not_logged_in.html")]
struct NotLoggedInModel<'a> {
    base: &'a BaseTemplateFields,
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginModel<'a> {
    base: &'a BaseTemplateFields,
}

// Result from the login form
#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

// Result from the re-scan form on the deliveries page.
#[derive(Deserialize)]
pub struct SourceNameForm {
    pub name: String,
}

#[derive(Template)]
#[template(path = "main_page.html")]
struct MainPageModel<'a> {
    base: &'a BaseTemplateFields,
    source_summary: String,
    next_poll: String,
    done_deliveries: usize,
    processed_count: u32,
    doorbell_url: String,
}

// One row of the log table, pre-formatted for display.
struct LogDisplayRow {
    time: String,
    icon: &'static str,
    description: &'static str,
    details: String,
}

struct LogLegendRow {
    icon: &'static str,
    text: &'static str,
}

#[derive(Template)]
#[template(path = "service_log.html")]
struct ServiceLogModel<'a> {
    base: &'a BaseTemplateFields,
    rows: Vec<LogDisplayRow>,
    legend: Vec<LogLegendRow>,
    months: u32,
    next_months: u32,
    row_count: usize,
}

// One row of the deliveries table.
struct DeliveryDisplayRow {
    delivery_id: String,
    window: String,
    state: String,
    active: bool,
    rule_id: i64,
    source_name: String,
    crawler_name: String,
    first_seen: String,
    last_updated: String,
}

// One row of the processed items table.
struct ProcessedDisplayRow {
    time: String,
    sequence: String,
    description: String,
    delivery_ids: String,
}

// One configured source, for the re-scan buttons.
struct SourceRow {
    name: String,
}

#[derive(Template)]
#[template(path = "deliveries.html")]
struct DeliveriesModel<'a> {
    base: &'a BaseTemplateFields,
    deliveries: Vec<DeliveryDisplayRow>,
    processed: Vec<ProcessedDisplayRow>,
    processed_limit: u32,
    sources: Vec<SourceRow>,
}

pub struct HtmlModel {
    baseFields: BaseTemplateFields,
    renderedOutput: Option<String>,
}

impl HtmlModel {
    pub fn new() -> Self {
        HtmlModel {
            baseFields: BaseTemplateFields {
                app_name_and_version: AppInfo::getNameAndVersion(),
                build_time_info: AppInfo::getBuildTimeInfo(),
                asset_version: AppInfo::BUILD_TIMESTAMP.to_string(),
                current_year: Local::now().year() as u32,
                page_title: String::new(),
                user_name: None,
                active_deliveries: 0,
                last_poll: "never".to_string(),
                last_success_message: None,
                last_err_message: None,
            },
            renderedOutput: None,
        }
    }

    // ---------------------------- Setters ----------------------------

    pub fn setUser(&mut self, user: Option<&str>) {
        self.baseFields.user_name = user.map(|u| u.to_string());
    }

    pub fn setActiveDeliveries(&mut self, count: usize) {
        self.baseFields.active_deliveries = count;
    }

    pub fn setLastPoll(&mut self, lastPoll: Option<DateTime<Local>>) {
        self.baseFields.last_poll = match lastPoll {
            Some(time) => time.format(TIME_FORMAT).to_string(),
            None => "never".to_string(),
        };
    }

    pub fn setLastErrMessage(&mut self, msg: &str) {
        self.baseFields.last_err_message = Some(msg.to_string());
    }

    pub fn setLastSuccessMessage(&mut self, msg: &str) {
        self.baseFields.last_success_message = Some(msg.to_string());
    }

    fn setPageTitle(&mut self, title: &str) {
        self.baseFields.page_title = title.to_string();
    }

    // ---------------------------- Rendering ----------------------------

    pub fn renderLoginPage(&mut self) -> axum::response::Html<String> {
        self.setPageTitle("Login");
        let model = LoginModel { base: &self.baseFields };
        self.renderedOutput = Some(model.render().unwrap_or_default());
        self.getRenderedOutput()
    }

    pub fn renderMainPage(
        &mut self,
        sourceSummary: String,
        nextPoll: Option<DateTime<Local>>,
        doneDeliveries: usize,
        processedCount: u32,
        doorbellUrl: String,
    ) -> axum::response::Html<String> {
        self.setPageTitle("Main Page");
        let model = MainPageModel {
            base: &self.baseFields,
            source_summary: sourceSummary,
            next_poll: match nextPoll {
                Some(time) => time.format(TIME_FORMAT).to_string(),
                None => "unknown".to_string(),
            },
            done_deliveries: doneDeliveries,
            processed_count: processedCount,
            doorbell_url: doorbellUrl,
        };
        self.renderedOutput = Some(model.render().unwrap_or_default());
        self.getRenderedOutput()
    }

    pub fn renderServiceLogPage(&mut self, rows: Vec<LogEvent>, months: u32) -> axum::response::Html<String> {
        self.setPageTitle("Service log");
        let displayRows: Vec<LogDisplayRow> = rows
            .into_iter()
            .map(|event| LogDisplayRow {
                time: event.timestamp.with_timezone(&Local).format(TIME_FORMAT).to_string(),
                icon: event.eventType.source().asIcon(),
                description: event.eventType.description(),
                details: event.details,
            })
            .collect();
        let legend: Vec<LogLegendRow> = LogSource::ALL
            .iter()
            .map(|source| LogLegendRow {
                icon: source.asIcon(),
                text: source.asLegend(),
            })
            .collect();
        let model = ServiceLogModel {
            base: &self.baseFields,
            row_count: displayRows.len(),
            rows: displayRows,
            legend,
            months,
            next_months: months + 1,
        };
        self.renderedOutput = Some(model.render().unwrap_or_default());
        self.getRenderedOutput()
    }

    pub fn renderDeliveriesPage(
        &mut self,
        announcements: Vec<DeliveryAnnouncement>,
        processed: Vec<ProcessedItem>,
        processedLimit: u32,
        sourceNames: Vec<String>,
    ) -> axum::response::Html<String> {
        self.setPageTitle("Deliveries");
        let deliveries: Vec<DeliveryDisplayRow> = announcements
            .into_iter()
            .map(|a| DeliveryDisplayRow {
                delivery_id: a.deliveryId,
                window: a.window.to_string(),
                state: a.state.to_string(),
                active: a.state == AnnouncementState::Active,
                rule_id: a.ruleId,
                source_name: a.sourceName,
                crawler_name: a.crawlerName,
                first_seen: a.firstSeen.format(TIME_FORMAT).to_string(),
                last_updated: a.lastUpdated.format(TIME_FORMAT).to_string(),
            })
            .collect();
        let processedRows: Vec<ProcessedDisplayRow> = processed
            .into_iter()
            .map(|item| ProcessedDisplayRow {
                time: item.processedAt.format(TIME_FORMAT).to_string(),
                sequence: item.sequence.map(|s| s.to_string()).unwrap_or_else(|| "–".to_string()),
                description: item.description,
                delivery_ids: item.deliveryIds.join(", "),
            })
            .collect();
        let model = DeliveriesModel {
            base: &self.baseFields,
            deliveries,
            processed: processedRows,
            processed_limit: processedLimit,
            sources: sourceNames.into_iter().map(|name| SourceRow { name }).collect(),
        };
        self.renderedOutput = Some(model.render().unwrap_or_default());
        self.getRenderedOutput()
    }

    /// Renders the "not logged in" page if there is no user, so every page handler can bail out early.
    pub fn renderingDone(&mut self) -> bool {
        if self.baseFields.user_name.is_none() {
            self.setPageTitle("Not logged in");
            let model = NotLoggedInModel { base: &self.baseFields };
            self.renderedOutput = Some(model.render().unwrap_or_default());
        }
        self.renderedOutput.is_some()
    }

    pub fn getRenderedOutput(&self) -> axum::response::Html<String> {
        axum::response::Html(self.renderedOutput.clone().unwrap_or_default())
    }
}

/// The one wall-clock format every page and the status route use.
pub fn formatLocal(time: DateTime<Local>) -> String {
    time.format(TIME_FORMAT).to_string()
}

/// Kept for symmetry with the doorbell server, where the same conversion lives in `util::time`.
#[allow(dead_code)]
pub fn formatUtc(time: Option<DateTime<Utc>>) -> String {
    match time {
        Some(t) => t.with_timezone(&Local).format(TIME_FORMAT).to_string(),
        None => "Unknown".to_string(),
    }
}
