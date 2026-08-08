use crate::configStore::ConfigStore;
// This contains secretConfig::generateUploadURL(ngrokTunnelAddress) and secretConfig::getFetchURL()
use crate::secretConfig;
use crate::util::notifyWithState::NotifyWithState;

use reqwest;
use serde_json::Value;
use std::process::Stdio;
use std::sync::Arc;
use tracing::{error, info};

/*
Upload IP address of ngrok tunnel to a remote service.

Setup and start ngrok according to:
https://dashboard.ngrok.com/get-started/setup/linux

Tunnel config is on:
http://127.0.0.1:4040/api/tunnels
*/

pub struct NgrokUpdater {
    cfg: Arc<ConfigStore>,
    tunnelAddress: Option<String>,
    uploaded: bool,
    ngrokChild: Option<tokio::process::Child>,
    cancelNotify: Arc<NotifyWithState>,
}

/*
NgrokUpdater implementation
---------------------------

Uploads the ngrok tunnel address to a remote service (PHP script with forwarding).
Starts and stops the ngrok process as needed.

Arguments:
- cfg: The global config
- cancelNotify: Notification for cancellation. The ngrok updater process has to be stopped before the main program exits.
*/
impl NgrokUpdater {
    pub fn new(cfg: Arc<ConfigStore>, cancelNotify: Arc<NotifyWithState>) -> Self {
        NgrokUpdater {
            cfg,
            tunnelAddress: None,
            uploaded: false,
            ngrokChild: None,
            cancelNotify,
        }
    }

    //
    pub async fn run(&mut self) {
        if !self.initNgrok().await {
            error!("NgrokUpdater: Failed to initialize ngrok, exiting...");
            self.cancelNotify.signal();
            return;
        }

        let cancelNotify = self.cancelNotify.clone();
        loop {
            tokio::select! {
                _ = cancelNotify.notified() => {
                    info!("NgrokUpdater: cancellation requested, shutting down...");
                    break;
                }
                _ = self.runMainLoopIteration() => {}
            }
        }
        self.stopNgrok().await;
    }

    pub async fn initNgrok(&mut self) -> bool {
        info!("Starting ngrok process in the background...");
        // Start: `ngrok http <port>`
        let httpPort = self.cfg.httpPort;
        let mut cmd = tokio::process::Command::new("ngrok");
        cmd.arg("http").arg(httpPort.to_string()).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        match cmd.spawn() {
            Ok(child) => {
                self.ngrokChild = Some(child);
            }
            Err(e) => {
                error!("Failed to start ngrok process: {}", e);
                return false;
            }
        }

        info!("Started ngrok process, waiting 5 seconds for it to initialize...");
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        true
    }

    pub async fn stopNgrok(&mut self) {
        if let Some(mut child) = self.ngrokChild.take() {
            match child.kill().await {
                Ok(_) => {
                    info!("Ngrok process terminated.");
                }
                Err(e) => {
                    error!("Failed to terminate ngrok process: {}", e);
                }
            }
        }
    }

    pub async fn runMainLoopIteration(&mut self) {
        // Reach tunnel address
        if let Err(e) = self.getTunnelAddress().await {
            self.tunnelAddress = None;
            error!("Failed to get ngrok tunnel, will try again in 10 seconds: {:?}", e);
        }

        // Upload tunnel address if it changed
        if !self.uploaded {
            if let Err(e) = self.uploadAddress().await {
                error!("Failed to upload ngrok tunnel address, will try again in 10 seconds: {:?}", e);
            }
        }

        // Wait for 10 seconds before checking again
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    }

    pub async fn getTunnelAddress(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = "http://127.0.0.1:4040/api/tunnels";
        let resp = reqwest::get(addr).await?;
        let text = resp.text().await?;
        let resp: Value = serde_json::from_str(&text)?;
        // Pick the https tunnel; ngrok may report several (e.g. an additional http one), and
        // comparing every one of them against the stored address would flip back and forth.
        let newTunnelAddress = resp["tunnels"]
            .as_array()
            .and_then(|tunnels| {
                tunnels
                    .iter()
                    .filter_map(|tunnel| tunnel["public_url"].as_str())
                    .find(|url| url.starts_with("https://"))
            })
            .map(|url| url.to_string());

        if let Some(newTunnelAddress) = newTunnelAddress
            && self.tunnelAddress.as_deref() != Some(newTunnelAddress.as_str())
        {
            info!("Ngrok tunnel updated: {}", newTunnelAddress);
            self.tunnelAddress = Some(newTunnelAddress);
            self.uploaded = false;
        }
        Ok(())
    }

    pub async fn uploadAddress(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(tunnelAddress) = &self.tunnelAddress {
            let uploadCommand = secretConfig::generateUploadURL(tunnelAddress);
            // The upload URL carries the API key in its query string, and reqwest errors carry the
            // URL they were built from - so the URL must be stripped before the error is logged.
            let resp = reqwest::get(uploadCommand).await.map_err(|e| e.without_url())?;
            let respText = resp.text().await.map_err(|e| e.without_url())?;
            if respText == "success" {
                info!("Tunnel address uploaded successfully: {}", tunnelAddress);
                info!("Service is accessible at: {}", secretConfig::getFetchURL());
                self.uploaded = true;
            } else {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Service reported: {}", respText),
                )));
            }
        }
        Ok(())
    }
}
