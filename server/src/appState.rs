use crate::common::APP_PATHS;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

// -------------- Volatile state ---------------

pub struct VolatileState {
    pub deviceStartTime: Option<String>,
    pub serviceStartTime: DateTime<Utc>,
    pub mqttHost: Option<String>,
    pub rebootDeviceOnConnect: bool,
}

impl VolatileState {
    pub fn new() -> Self {
        VolatileState {
            deviceStartTime: None,
            serviceStartTime: Utc::now(),
            mqttHost: None,
            rebootDeviceOnConnect: false,
        }
    }
}

// -------------- Persistent state ---------------

#[derive(Debug, Deserialize, Serialize)]
pub struct PersistentState {
    lastDeviceDeviceRebootTime: Option<DateTime<Utc>>,
}

impl PersistentState {
    pub fn new() -> Self {
        PersistentState {
            lastDeviceDeviceRebootTime: None,
        }
    }

    pub async fn setLastDeviceRebootTime(&mut self, rebootTime: DateTime<Utc>) {
        self.lastDeviceDeviceRebootTime = Some(rebootTime);
        self.saveState().await;
    }

    pub fn getLastDeviceRebootTime(&self) -> Option<DateTime<Utc>> {
        self.lastDeviceDeviceRebootTime
    }

    async fn saveState(&self) {
        let json = serde_json::to_string(self).unwrap();
        if let Err(e) = tokio::fs::write(&APP_PATHS.stateFile, json).await {
            eprintln!("Failed to write state to file: {:?}", e);
        }
    }
}

// -------------- Common app state ---------------

pub struct AppState {
    pub volatileState: VolatileState,
    pub persistentState: PersistentState,
}

pub type MtxAppState = Arc<RwLock<AppState>>;

impl AppState {
    fn new() -> Self {
        AppState {
            volatileState: VolatileState::new(),
            persistentState: Self::loadPersistentStateFromFile(),
        }
    }

    fn loadPersistentStateFromFile() -> PersistentState {
        if !APP_PATHS.stateFile.exists() {
            info!("State file {} does not exist, using default state.", APP_PATHS.stateFile.display());
            return PersistentState::new();
        }

        match std::fs::read_to_string(&APP_PATHS.stateFile) {
            Ok(content) => match serde_json::from_str::<PersistentState>(&content) {
                Ok(state) => state,
                Err(e) => {
                    error!(
                        "Failed to parse application state from {}: {:?}, using default state.",
                        APP_PATHS.stateFile.display(),
                        e
                    );
                    PersistentState::new()
                }
            },
            Err(e) => {
                error!(
                    "Failed to read state file {}: {:?}, using default state.",
                    APP_PATHS.stateFile.display(),
                    e
                );
                PersistentState::new()
            }
        }
    }

    pub fn newMtx() -> MtxAppState {
        Arc::new(RwLock::new(AppState::new()))
    }
}
