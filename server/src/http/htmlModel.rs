use crate::common::AppInfo;
use crate::common::AutoBuzzState;
use crate::common::ConnectionState;
use crate::common::ToStateInfo;
use crate::common::UpdateBehavior;
use crate::logger::eventTypes::LogEvent;
use crate::logger::eventTypes::LogSource;
use crate::rules::rule::{DATE_FORMAT, Rule};
use crate::util::time::formatLocal;

use askama::Template;
use chrono::Datelike;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::HashSet;

// Daily window a new rule starts with: a plain "during the day" window, in TIME_FORMAT.
const NEW_RULE_ACTIVATION_TIME: &str = "08:00";
const NEW_RULE_DEACTIVATION_TIME: &str = "20:00";

struct BaseTemplateFields {
    app_name_and_version: String,
    build_time_info: String,
    asset_version: String, // cache buster for static assets, changes with every build
    current_year: u32,
    page_title: String,
    user_name: Option<String>,
    sse_disabled: bool,
    auto_buzz_state: AutoBuzzState,      // auto-buzz state of the device
    rule_auto_buzz_state: AutoBuzzState, // auto-buzz by an active rule
    connection_state: ConnectionState,
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

// Result from the Login form
#[derive(Deserialize)]
pub struct LoginForm {
    pub username: String,
    pub password: String,
}

#[derive(Template)]
#[template(path = "main_page.html")]
struct MainPageModel<'a> {
    base: &'a BaseTemplateFields,
}

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsPageModel<'a> {
    base: &'a BaseTemplateFields,
    update_behaviors: &'static [UpdateBehavior],
    selected_update_behavior: UpdateBehavior,
}

#[derive(Template)]
#[template(path = "device_log.html")]
struct DeviceLogModel<'a> {
    base: &'a BaseTemplateFields,
}

// One row of the server log, pre-formatted for display.
struct ServerLogDisplayRow {
    time: String,
    icon: String,
    description: String,
    details: String,
}

// One row of the icon legend on the server log page.
struct ServerLogLegendRow {
    icon: &'static str,
    text: &'static str,
}

#[derive(Template)]
#[template(path = "server_log.html")]
struct ServerLogModel<'a> {
    base: &'a BaseTemplateFields,
    rows: Vec<ServerLogDisplayRow>,
    legend: Vec<ServerLogLegendRow>,
    months: u32,
    next_months: u32,
    row_count: usize,
}

#[derive(Template)]
#[template(path = "diagnostics.html")]
struct DiagnosticsModel<'a> {
    base: &'a BaseTemplateFields,
    device_start_time: String,
    device_reboot_time: String,
    service_start_time: String,
    mqtt_server: String,
}

// One row of the rules table, pre-formatted for display.
struct RuleDisplayRow {
    id: i64,
    name: String,
    enabled: bool,
    active: bool, // rule is enabled and within its activity window right now
    // Single-shot rule which has just fired: it is about to be deleted, so it must not be
    // edited, deleted or toggled by hand anymore.
    pending_delete: bool,
    auto_ack: bool,
    single_shot: bool,
    start_date: String,
    end_date: String,
    activation_time: String,
    deactivation_time: String,
}

#[derive(Template)]
#[template(path = "rules.html")]
struct RulesPageModel<'a> {
    base: &'a BaseTemplateFields,
    rows: Vec<RuleDisplayRow>,
}

// Form-facing view of a rule (string fields bound to the edit form inputs).
struct RuleFormView {
    id: String, // empty for a new rule
    name: String,
    enabled: bool,
    auto_ack: bool,
    single_shot: bool,
    start_date: String,
    end_date: String,
    activation_time: String,
    deactivation_time: String,
    is_new: bool,
}

#[derive(Template)]
#[template(path = "rule_edit.html")]
struct RuleEditModel<'a> {
    base: &'a BaseTemplateFields,
    rule: RuleFormView,
}

// Result from the rule edit form. Unchecked checkboxes are absent from the body, hence Option;
// text/date/time fields default to empty so a missing field yields a validation error in the
// handler rather than a 422 from the extractor.
#[derive(Deserialize)]
pub struct RuleForm {
    pub id: Option<String>,
    #[serde(default)]
    pub name: String,
    pub enabled: Option<String>,
    pub autoAck: Option<String>,
    pub singleShot: Option<String>,
    #[serde(default)]
    pub startDate: String,
    #[serde(default)]
    pub endDate: String,
    #[serde(default)]
    pub activationTime: String,
    #[serde(default)]
    pub deactivationTime: String,
}

// Result from the small per-row forms (delete, enabled-toggle).
#[derive(Deserialize)]
pub struct RuleIdForm {
    pub id: String,
}

// Result from the "single shot auto ack for N hours" shortcut form on the rules page.
#[derive(Deserialize)]
pub struct ShortcutRuleForm {
    #[serde(default)]
    pub hours: String,
}

pub struct HtmlModel {
    baseFields: BaseTemplateFields,
    renderedOutput: Option<String>,
    selectedUpdateBehavior: UpdateBehavior,
    deviceStartTime: Option<String>, // delivered as string from the device
    deviceRebootTime: Option<DateTime<Utc>>,
    serviceStartTime: Option<DateTime<Utc>>,
    mqttServer: Option<String>,
}

impl HtmlModel {
    pub fn new() -> Self {
        HtmlModel {
            baseFields: BaseTemplateFields {
                app_name_and_version: AppInfo::getNameAndVersion(),
                build_time_info: AppInfo::getBuildTimeInfo(),
                asset_version: AppInfo::BUILD_TIMESTAMP.to_string(),
                current_year: chrono::Local::now().year() as u32,
                page_title: "".to_string(),
                user_name: None,
                sse_disabled: false,
                auto_buzz_state: AutoBuzzState::Unknown,
                rule_auto_buzz_state: AutoBuzzState::Unknown,
                connection_state: ConnectionState::Disconnected,
                last_success_message: None,
                last_err_message: None,
            },
            renderedOutput: None,
            selectedUpdateBehavior: UpdateBehavior::UpdateViaSSE,
            deviceRebootTime: None,
            deviceStartTime: None,
            serviceStartTime: None,
            mqttServer: None,
        }
    }

    // ---------------------------- External Setters for model data ----------------------------

    pub fn setUser(&mut self, user: Option<&str>) {
        self.baseFields.user_name = user.map(|s| s.to_string());
        if !user.is_some() {
            self.setPageTitle("Not logged in");
            let model = NotLoggedInModel { base: &self.baseFields };
            self.renderedOutput = Some(model.render().unwrap_or_default());
        }
    }

    pub fn setAutoBuzzState(&mut self, state: AutoBuzzState) {
        self.baseFields.auto_buzz_state = state;
    }

    pub fn setRuleAutoBuzzState(&mut self, state: AutoBuzzState) {
        self.baseFields.rule_auto_buzz_state = state;
    }

    pub fn setConnectionState(&mut self, state: ConnectionState) {
        self.baseFields.connection_state = state;
    }

    pub fn setSelectedUpdateBehavior(&mut self, behavior: UpdateBehavior) {
        self.selectedUpdateBehavior = behavior;
    }

    pub fn setLastErrMessage(&mut self, msg: &str) {
        self.baseFields.last_err_message = Some(msg.to_string());
    }

    pub fn setLastSuccessMessage(&mut self, msg: &str) {
        self.baseFields.last_success_message = Some(msg.to_string());
    }

    pub fn setDeviceStartTime(&mut self, startTime: Option<String>) {
        self.deviceStartTime = startTime;
    }

    pub fn setServiceStartTime(&mut self, startTime: Option<DateTime<Utc>>) {
        self.serviceStartTime = startTime;
    }

    pub fn setDeviceRebootTime(&mut self, rebootTime: Option<DateTime<Utc>>) {
        self.deviceRebootTime = rebootTime;
    }

    pub fn setMqttServer(&mut self, mqttServer: Option<String>) {
        self.mqttServer = mqttServer;
    }

    // ---------------------------- Internal Setters for model data ----------------------------

    fn setPageTitle(&mut self, title: &str) {
        self.baseFields.page_title = title.to_string();
    }

    // ---------------------------- Render pages ----------------------------

    pub fn renderLoginPage(&mut self) -> axum::response::Html<String> {
        self.setPageTitle("Login");
        let model = LoginModel { base: &self.baseFields };
        self.renderedOutput = Some(model.render().unwrap_or_default());
        return self.getRenderedOutput();
    }

    pub fn renderMainPage(&mut self) -> axum::response::Html<String> {
        self.updateFlags();
        self.setPageTitle("Main Page");
        let model = MainPageModel { base: &self.baseFields };
        self.renderedOutput = Some(model.render().unwrap_or_default());
        return self.getRenderedOutput();
    }

    pub fn renderDiagnosticsPage(&mut self) -> axum::response::Html<String> {
        self.updateFlags();
        self.setPageTitle("Diagnostics");
        let model = DiagnosticsModel {
            base: &self.baseFields,
            device_start_time: self.deviceStartTime.clone().unwrap_or("Unknown".to_string()),
            device_reboot_time: formatLocal(self.deviceRebootTime),
            service_start_time: formatLocal(self.serviceStartTime),
            mqtt_server: self.mqttServer.clone().unwrap_or("Unknown".to_string()),
        };
        self.renderedOutput = Some(model.render().unwrap_or_default());
        return self.getRenderedOutput();
    }

    pub fn renderSettingsPage(&mut self) -> axum::response::Html<String> {
        self.updateFlags();
        self.setPageTitle("Settings");
        let model = SettingsPageModel {
            base: &self.baseFields,
            update_behaviors: &UpdateBehavior::ALL,
            selected_update_behavior: self.selectedUpdateBehavior,
        };
        self.renderedOutput = Some(model.render().unwrap_or_default());
        return self.getRenderedOutput();
    }

    pub fn renderDeviceLogPage(&mut self) -> axum::response::Html<String> {
        self.updateFlags();
        self.setPageTitle("Device log");
        let model = DeviceLogModel { base: &self.baseFields };
        self.renderedOutput = Some(model.render().unwrap_or_default());
        return self.getRenderedOutput();
    }

    pub fn renderServerLogPage(&mut self, rows: Vec<LogEvent>, months: u32) -> axum::response::Html<String> {
        self.updateFlags();
        self.setPageTitle("Server log");
        let displayRows: Vec<ServerLogDisplayRow> = rows
            .into_iter()
            .map(|r| ServerLogDisplayRow {
                time: formatLocal(Some(r.timestamp)),
                icon: r.eventType.source().asIcon().to_string(),
                description: r.eventType.description().to_string(),
                details: r.details,
            })
            .collect();
        let legend: Vec<ServerLogLegendRow> = LogSource::ALL
            .iter()
            .map(|s| ServerLogLegendRow {
                icon: s.asIcon(),
                text: s.asLegend(),
            })
            .collect();
        let model = ServerLogModel {
            base: &self.baseFields,
            row_count: displayRows.len(),
            rows: displayRows,
            legend,
            months,
            next_months: months + 1,
        };
        self.renderedOutput = Some(model.render().unwrap_or_default());
        return self.getRenderedOutput();
    }

    pub fn renderRulesPage(&mut self, rules: Vec<Rule>, pendingDeleteIds: &HashSet<i64>) -> axum::response::Html<String> {
        self.updateFlags();
        self.setPageTitle("Rules");
        let now = chrono::Local::now();
        let rows: Vec<RuleDisplayRow> = rules
            .into_iter()
            .map(|r| RuleDisplayRow {
                id: r.id,
                name: r.name.clone(),
                enabled: r.enabled,
                active: r.isActiveAt(now),
                pending_delete: pendingDeleteIds.contains(&r.id),
                auto_ack: r.autoAck,
                single_shot: r.singleShot,
                start_date: r.startDateStr(),
                end_date: r.endDateStr(),
                activation_time: r.activationTimeStr(),
                deactivation_time: r.deactivationTimeStr(),
            })
            .collect();
        let model = RulesPageModel {
            base: &self.baseFields,
            rows,
        };
        self.renderedOutput = Some(model.render().unwrap_or_default());
        return self.getRenderedOutput();
    }

    pub fn renderEditRulePage(&mut self, rule: Option<Rule>) -> axum::response::Html<String> {
        self.updateFlags();
        let ruleView = match rule {
            Some(r) => {
                self.setPageTitle("Edit rule");
                RuleFormView {
                    id: r.id.to_string(),
                    name: r.name.clone(),
                    enabled: r.enabled,
                    auto_ack: r.autoAck,
                    single_shot: r.singleShot,
                    start_date: r.startDateStr(),
                    end_date: r.endDateStr(),
                    activation_time: r.activationTimeStr(),
                    deactivation_time: r.deactivationTimeStr(),
                    is_new: false,
                }
            }
            None => {
                self.setPageTitle("New rule");
                // A new rule defaults to "today only", which is the most common case and can be
                // widened with the +/- day buttons in the form.
                let today = chrono::Local::now().format(DATE_FORMAT).to_string();
                RuleFormView {
                    id: String::new(),
                    name: String::new(),
                    enabled: true,
                    auto_ack: false,
                    single_shot: false,
                    start_date: today.clone(),
                    end_date: today,
                    activation_time: NEW_RULE_ACTIVATION_TIME.to_string(),
                    deactivation_time: NEW_RULE_DEACTIVATION_TIME.to_string(),
                    is_new: true,
                }
            }
        };
        let model = RuleEditModel {
            base: &self.baseFields,
            rule: ruleView,
        };
        self.renderedOutput = Some(model.render().unwrap_or_default());
        return self.getRenderedOutput();
    }

    // ---------------------------- Finalize rendering ----------------------------

    pub fn renderingDone(&mut self) -> bool {
        self.renderedOutput.is_some()
    }

    pub fn getRenderedOutput(&self) -> axum::response::Html<String> {
        if let Some(ref output) = self.renderedOutput {
            axum::response::Html(output.to_string())
        } else {
            axum::response::Html("(Failed to render page)".to_string())
        }
    }

    // ---------------------------- Internal Helpers ----------------------------

    fn updateFlags(&mut self) {
        self.baseFields.sse_disabled = self.selectedUpdateBehavior == UpdateBehavior::UpdateNever;
    }
}
