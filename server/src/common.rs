use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::{fmt, sync::LazyLock};

// ------------- Global directories -------------

const DATA_DIR: &str = "data";
const SESSION_FILE_NAME: &str = "sessions.json";
const STATE_FILE_NAME: &str = "state.json";
const LOG_DB_FILE_NAME: &str = "serverLog.sqlite";
const RULE_DB_FILE_NAME: &str = "rules.sqlite";

pub struct AppPaths {
    pub sessionFile: PathBuf,
    pub stateFile: PathBuf,
    pub logDbFile: PathBuf,
    pub ruleDbFile: PathBuf,
}

impl AppPaths {
    pub fn new() -> Self {
        AppPaths {
            sessionFile: Path::new(DATA_DIR).join(SESSION_FILE_NAME),
            stateFile: Path::new(DATA_DIR).join(STATE_FILE_NAME),
            logDbFile: Path::new(DATA_DIR).join(LOG_DB_FILE_NAME),
            ruleDbFile: Path::new(DATA_DIR).join(RULE_DB_FILE_NAME),
        }
    }
}

pub static APP_PATHS: LazyLock<AppPaths> = LazyLock::new(|| AppPaths::new());

// ------------- Global application info -------------

pub mod AppInfo {
    pub const VERSION: &str = env!("CARGO_PKG_VERSION");
    pub const NAME: &str = "Doorbell HTTP server";
    pub const BUILD_TIMESTAMP: &str = env!("BUILD_TIMESTAMP");

    pub fn getNameAndVersion() -> String {
        format!("{} v{}", NAME, VERSION)
    }

    pub fn getBuildTimeInfo() -> String {
        format!("Build time: {}", BUILD_TIMESTAMP)
    }
}

// ------------- System states -------------

// For State updates via API
#[derive(Serialize)]
pub struct StateInfo {
    stateStr: String,
    styleClass: &'static str,
}

impl StateInfo {
    pub fn toJson(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

pub trait ToStateInfo: fmt::Display {
    fn styleClass(&self) -> &'static str;
    fn toStateInfo(&self) -> StateInfo {
        StateInfo {
            stateStr: self.to_string(),
            styleClass: self.styleClass(),
        }
    }
}

// "Debug" implements the default JSON serialization
#[derive(PartialEq, Clone, Debug)]
pub enum ConnectionState {
    Disconnected,
    MqttConnected,
    DeviceConnected,
}

impl fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ConnectionState::Disconnected => "Disconnected",
            ConnectionState::MqttConnected => "Broker only",
            ConnectionState::DeviceConnected => "Connected to device",
        };
        write!(f, "{}", s)
    }
}

impl ToStateInfo for ConnectionState {
    fn styleClass(&self) -> &'static str {
        match self {
            ConnectionState::Disconnected => "DotRed",
            ConnectionState::MqttConnected => "DotOrange",
            ConnectionState::DeviceConnected => "DotGreen",
        }
    }
}

#[derive(PartialEq, Clone)]
pub enum AutoBuzzState {
    Unknown,
    On,
    Off,
}

impl fmt::Display for AutoBuzzState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            AutoBuzzState::Unknown => "unknown",
            AutoBuzzState::On => "on",
            AutoBuzzState::Off => "off",
        };
        write!(f, "{}", s)
    }
}

impl ToStateInfo for AutoBuzzState {
    fn styleClass(&self) -> &'static str {
        match self {
            AutoBuzzState::Unknown => "DotQuestion",
            AutoBuzzState::On => "DotGreen",
            AutoBuzzState::Off => "DotGrey",
        }
    }
}

// What the auto-buzz rules are doing now or will do next. Computed by the RuleService on a rule
// change or a rule transition only, not on every page load.
#[derive(PartialEq, Clone, Debug)]
pub enum RuleSchedule {
    NoRules,
    ActiveUntil { ruleName: String, deactivation: DateTime<Local> },
    NextActivation { ruleName: String, activation: DateTime<Local> },
}

impl fmt::Display for RuleSchedule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const FORMAT: &str = "%Y-%m-%d %H:%M";
        match self {
            RuleSchedule::NoRules => write!(f, "No current/planned rules"),
            RuleSchedule::ActiveUntil { ruleName, deactivation } => {
                write!(f, "Current rule \"{}\" active until {}", ruleName, deactivation.format(FORMAT))
            }
            RuleSchedule::NextActivation { ruleName, activation } => {
                write!(f, "Next rule \"{}\" will be activated at {}", ruleName, activation.format(FORMAT))
            }
        }
    }
}

// ------------- Internal message between services -------------

#[derive(Debug)]
pub enum IntMsg {
    SetDeviceAutoBuzz(bool),
    SetRuleAutoBuzz(bool),
    SetRuleSchedule(RuleSchedule),
    SetConnState(ConnectionState),
    Buzz,
    AckRing,
    Reboot,
    GetDeviceLog,
    DeviceLogContent(Vec<String>),
    StateUpdate,
    RingOccurred(bool /* deviceAutoBuzzActive */),
}

// ------------- Settings -------------

#[derive(PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum UpdateBehavior {
    UpdateViaSSE,
    UpdateNever,
}

impl UpdateBehavior {
    pub const ALL: [UpdateBehavior; 2] = [UpdateBehavior::UpdateViaSSE, UpdateBehavior::UpdateNever];
}

impl fmt::Display for UpdateBehavior {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            UpdateBehavior::UpdateViaSSE => "Update by Server-Side-Events (default)",
            UpdateBehavior::UpdateNever => "Update on page load",
        };
        write!(f, "{}", s)
    }
}

#[derive(PartialEq, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    pub updateBehavior: UpdateBehavior,
}

impl Default for UserSettings {
    fn default() -> Self {
        UserSettings {
            updateBehavior: UpdateBehavior::UpdateViaSSE,
        }
    }
}
