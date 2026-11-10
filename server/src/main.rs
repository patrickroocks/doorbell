#![allow(non_snake_case)]

// Internal modules
mod appState;
mod common;
mod configStore;
mod http;
mod logger;
mod mqtt;
mod ngrokUpdater;
mod rules;
mod secretConfig;
mod util;

// Usings from internal modules
use appState::AppState;
use common::AppInfo;
use common::IntMsg;
use http::httpServer::HttpServer;
use logger::eventTypes::LogEvent;
use logger::loggerService::LoggerService;
use mqtt::mqttAdapter::MqttAdapter;
use rules::ruleService::{RuleService, RulesChangedFlag};
use util::notifyWithState::NotifyWithState;

// Usings from external crates
use clap::Parser;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_subscriber;

const BUFF_SIZE: usize = 32;

// Call with "--help" to see these infos
#[derive(Parser)]
#[command(version = AppInfo::VERSION, about = AppInfo::NAME, long_about = None)]
struct Args {
    /// Do not start ngrok updater
    #[arg(long, short, default_value_t = false)]
    without_ngrok: bool,
    /// Debug log level
    #[arg(long, short, default_value_t = false)]
    log_debug: bool,
    /// Config file json
    #[arg(long, short)]
    config: String,
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // Parse command line arguments
    let args = Args::parse();
    let useNgrok = !args.without_ngrok;
    let logDebug = args.log_debug;

    // Logger
    tracing_subscriber::fmt().with_max_level(if logDebug { tracing::Level::DEBUG } else { tracing::Level::INFO }).init();

    info!(
        "Starting Doorbell HTTP Server v{}\n  log level: {}\n  using ngrok updater: {}\n  using config file: {}",
        AppInfo::VERSION,
        if logDebug { "DEBUG" } else { "INFO" },
        useNgrok,
        args.config
    );

    // Cancellation signal
    let cancelNotify = NotifyWithState::new();

    // Global mutexed application state
    let appState = AppState::newMtx();

    // Read config file
    let cfgStore = Arc::new(configStore::ConfigStore::new(args.config));

    // Optionally setup ngrok updater, spawn no-operation task if not used
    let ngrokUpdaterTask = {
        let cancelNotifyClone = cancelNotify.clone();
        let cfgStoreClone = cfgStore.clone();
        tokio::spawn(async move {
            if useNgrok {
                let mut ngrokUpdater = ngrokUpdater::NgrokUpdater::new(cfgStoreClone, cancelNotifyClone);
                ngrokUpdater.run().await;
            } else {
                cancelNotifyClone.notified().await;
            };
        })
    };

    // Multiple Producer, Single Consumer pattern for all services
    let (mqttTx, mqttRx) = mpsc::channel::<IntMsg>(BUFF_SIZE);
    let (httpTx, httpRx) = mpsc::channel::<IntMsg>(BUFF_SIZE);
    let (ruleTx, ruleRx) = mpsc::channel::<IntMsg>(BUFF_SIZE);
    let (logTx, logRx) = mpsc::channel::<LogEvent>(BUFF_SIZE);

    // Logger service: HTTP and MQTT producers send events, the service writes them to SQLite.
    let mut logger = LoggerService::new(logRx, cfgStore.logRetentionDays, cancelNotify.clone());
    let logQuery = logger.queryService();
    let loggerTask = tokio::spawn(async move {
        logger.run().await;
    });

    // Rule service: owns the rules SQLite DB and reacts to rings while a rule is active. It sends device
    // commands over the same channel the HTTP server uses (mqttTx), reports its own auto-buzz state
    // to the web UI (httpTx), and the HTTP server shares its rule store for the web UI.
    // `rulesChanged` lets the HTTP layer tell the service to re-evaluate after a rule is
    // created/edited/deleted.
    let rulesChanged: RulesChangedFlag = Arc::new(AtomicBool::new(false));
    let mut ruleService = RuleService::new(
        ruleRx,
        mqttTx.clone(),
        httpTx.clone(),
        logTx.clone(),
        rulesChanged.clone(),
        cancelNotify.clone(),
    );
    let ruleStore = ruleService.store();
    let pendingRemovals = ruleService.pendingRemovals();
    let ruleTask = tokio::spawn(async move {
        ruleService.run().await;
    });

    // Construct HttpServer, spwan task
    let mut http = HttpServer::new(
        mqttTx,
        httpRx,
        cfgStore.clone(),
        cancelNotify.clone(),
        appState.clone(),
        logTx.clone(),
        logQuery,
        ruleStore,
        rulesChanged,
        pendingRemovals,
    );
    let httpTask = tokio::spawn(async move {
        http.run().await;
    });

    // Construct MqttAdapter, get task (MQTT event loop task is created in the constructor of the adapter)
    let mqttAdapter = MqttAdapter::new(
        httpTx,
        ruleTx,
        mqttRx,
        cfgStore.clone(),
        cancelNotify.clone(),
        appState,
        logTx,
    )
    .await;
    let mqttTask = mqttAdapter.eventloopTask;

    info!("All tasks started.");

    // Wait for ctrl+c, SIGTERM, or termination due to an (early) error (e.g., port bind failure, config error)
    // The tasks broadcast the termination signal via the cancellation notification, which is listened to in the main loops of the tasks.
    let mut sigterm = signal(SignalKind::terminate()).unwrap();

    // A cancellation that was not requested by a signal comes from a task that gave up (port bind
    // failure, missing MQTT configuration, ...) - that has to be reported as a failed run, so the
    // service manager restarts the server instead of treating it as a clean stop.
    let mut startupFailed = false;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT, shutting down...");
            cancelNotify.signal();
        },
        _ = sigterm.recv() => {
            info!("Received SIGTERM, shutting down...");
            cancelNotify.signal();
        },
        _ = cancelNotify.notified() => {
            info!("Received cancellation signal, shutting down...");
            startupFailed = true;
        },
    }

    // Handle task errors - shut not happen, they should to a clean shutdown on signal termination.
    let handleNamedError = |name: &str, res: Result<(), tokio::task::JoinError>| {
        if let Err(e) = res {
            error!("{} task failed: {:?}", name, e);
        }
    };
    let result = tokio::join!(httpTask, mqttTask, ngrokUpdaterTask, loggerTask, ruleTask);
    handleNamedError("HTTP", result.0);
    handleNamedError("MQTT", result.1);
    handleNamedError("Ngrok", result.2);
    handleNamedError("Logger", result.3);
    handleNamedError("Rule", result.4);

    // All tasks stopped
    info!("Stopped doorbell_http_server.");
    if startupFailed {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}
