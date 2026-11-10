use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use tracing::error;

#[derive(Debug, Deserialize)]
pub struct UserInfo {
    pub username: String,
    pub displayName: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MqttServerInfo {
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub rebootDeviceOnConnect: bool,
}

fn defaultLogRetentionDays() -> u32 {
    180
}

#[derive(Debug, Deserialize)]
pub struct ConfigFileFormat {
    pub users: Vec<UserInfo>,
    pub httpPort: u16,
    pub mqttServers: Vec<MqttServerInfo>,
    pub mqttClientId: String,
    #[serde(default = "defaultLogRetentionDays")]
    pub logRetentionDays: u32,
    // Tokens accepted by the rule API, see http/httpApi.rs. Without a token there is no API at all.
    #[serde(default)]
    pub apiTokens: Vec<String>,
}

pub struct ConfigStore {
    pub users: HashMap<String, UserInfo>,
    pub httpPort: u16,
    pub mqttServers: Vec<MqttServerInfo>,
    pub mqttClientId: String,
    pub logRetentionDays: u32,
    pub apiTokens: Vec<String>,
}

impl ConfigStore {
    pub fn new(configFile: String) -> Self {
        // Read and parse config file
        let file_content = match fs::read_to_string(configFile) {
            Ok(content) => content,
            Err(e) => ConfigStore::handleError("Failed to read config file", e),
        };
        let config: ConfigFileFormat = match serde_json::from_str(&file_content) {
            Ok(cfg) => cfg,
            Err(e) => ConfigStore::handleError("Failed to parse config file", e),
        };

        // Put users into a hashmap
        let users = config.users.into_iter().map(|u| (u.username.clone(), u)).collect();

        ConfigStore {
            users: users,
            httpPort: config.httpPort,
            mqttServers: config.mqttServers,
            mqttClientId: config.mqttClientId,
            logRetentionDays: config.logRetentionDays,
            apiTokens: config.apiTokens,
        }
    }

    // ---------------------------- Helpers ----------------------------

    pub fn handleError<T>(msg: &str, err: T) -> !
    where
        T: std::fmt::Display,
    {
        error!("{}: {}", msg, err);
        std::process::exit(1);
    }
}
