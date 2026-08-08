use serde::Deserialize;
use std::fs;
use std::process::exit;

#[derive(Debug, Deserialize)]
pub struct ConfigStore {
    pub mqttHost: String,
    pub mqttPort: u16,
    pub mqttClientId: String,
}

impl ConfigStore {
    pub fn new(configFile: String) -> Self {
        // Read and parse config file
        let fileContent = match fs::read_to_string(&configFile) {
            Ok(content) => content,
            Err(e) => ConfigStore::handleError("Failed to read config file", e),
        };
        match serde_json::from_str(&fileContent) {
            Ok(cfg) => cfg,
            Err(e) => ConfigStore::handleError("Failed to parse config file", e),
        }
    }

    fn handleError<T>(msg: &str, err: T) -> !
    where
        T: std::fmt::Display,
    {
        eprintln!("{}: {}", msg, err);
        exit(1);
    }
}
