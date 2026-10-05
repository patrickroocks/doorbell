#![allow(non_snake_case)]

// Internal modules
mod common;
mod configStore;
mod crawlers;
mod deliveries;
mod doorbell;
mod http;
mod logger;
mod notifications;
mod pollService;
mod sources;
mod util;

// Usings from internal modules
use common::{AppInfo, AppPaths};
use deliveries::announcementStore::AnnouncementStore;
use deliveries::processedItemStore::ProcessedItemStore;
use deliveries::reconciler::Reconciler;
use doorbell::ruleApiClient::RuleApiClient;
use http::httpServer::HttpServer;
use logger::eventTypes::LogEvent;
use logger::loggerService::LoggerService;
use notifications::notifier::Notifier;
use pollService::{PollService, PollStatus};
use util::notifyWithState::NotifyWithState;

// Usings from external crates
use chrono::Local;
use clap::Parser;
use std::sync::Arc;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::mpsc;
use tracing::{error, info};

const BUFF_SIZE: usize = 32;

// Call with "--help" to see these infos
#[derive(Parser)]
#[command(version = AppInfo::VERSION, about = AppInfo::NAME, long_about = None)]
struct Args {
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
    let logDebug = args.log_debug;

    // Logger
    tracing_subscriber::fmt().with_max_level(if logDebug { tracing::Level::DEBUG } else { tracing::Level::INFO }).init();

    info!(
        "Starting {}\n  {}\n  log level: {}\n  using config file: {}",
        AppInfo::getNameAndVersion(),
        AppInfo::getBuildTimeInfo(),
        if logDebug { "DEBUG" } else { "INFO" },
        args.config
    );

    // Taken before anything else is built, so the status route reports when the process came up and
    // not when the last service finished starting.
    let serviceStartTime = Local::now();

    // Cancellation signal
    let cancelNotify = NotifyWithState::new();

    // Read config file
    let cfgStore = Arc::new(configStore::ConfigStore::new(args.config));

    // Every database lives in the data directory, which may not exist yet on a fresh installation.
    AppPaths::ensureDataDir();

    // Logger service: the other services send events, this one writes them to SQLite.
    let (logTx, logRx) = mpsc::channel::<LogEvent>(BUFF_SIZE);
    let mut logger = LoggerService::new(logRx, cfgStore.general.logRetentionDays, cancelNotify.clone());
    let logQuery = logger.queryService();
    let loggerTask = tokio::spawn(async move {
        logger.run().await;
    });

    // Remembers every item a source has already delivered, so nothing is fetched or parsed twice.
    let processedItems = ProcessedItemStore::new();

    // Maps delivery ids to the rules that cover them in the doorbell service.
    let announcements = AnnouncementStore::new();

    // Notifier: tells the configured sinks about a delivery that got a rule, or whose rule moved.
    // Built before the reconciler, because a broken template or SMTP setup is a config error.
    let notifier = Arc::new(Notifier::new(&cfgStore, logTx.clone()));

    // Reconciler: creates a door rule for a new delivery and refines it when a later announcement
    // knows more.
    // Shared with the HTTP server, which asks the same client whether the doorbell answers.
    let ruleApiClient = Arc::new(RuleApiClient::new(&cfgStore.doorbell));
    let reconciler = Arc::new(Reconciler::new(
        announcements.clone(),
        ruleApiClient.clone(),
        cfgStore.ruleCreation.clone(),
        notifier,
        logTx.clone(),
    ));

    // Poll service: queries all sources at startup and then every queryIntervalSeconds, hands what it
    // finds to the reconciler.
    let pollStatus = PollStatus::new();
    let mut pollService = PollService::new(
        cfgStore.clone(),
        processedItems.clone(),
        reconciler.clone(),
        pollStatus.clone(),
        cancelNotify.clone(),
    );
    let pollTask = tokio::spawn(async move {
        pollService.run().await;
    });

    // Web UI: login, overview, the delivery-to-rule mapping and the service log.
    let mut http = HttpServer::new(
        cfgStore.clone(),
        announcements.clone(),
        processedItems.clone(),
        logQuery,
        logTx.clone(),
        pollStatus,
        ruleApiClient,
        serviceStartTime,
        cancelNotify.clone(),
    );
    let httpTask = tokio::spawn(async move {
        http.run().await;
    });

    info!("All tasks started.");

    // Wait for ctrl+c, SIGTERM, or termination due to an (early) error.
    // The tasks broadcast the termination signal via the cancellation notification, which is listened to in the main loops of the tasks.
    let mut sigterm = signal(SignalKind::terminate()).unwrap();

    // A cancellation that was not requested by a signal comes from a task that gave up - that has to
    // be reported as a failed run, so the service manager restarts the service instead of treating
    // it as a clean stop.
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

    // Handle task errors - should not happen, they should do a clean shutdown on signal termination.
    let handleNamedError = |name: &str, res: Result<(), tokio::task::JoinError>| {
        if let Err(e) = res {
            error!("{} task failed: {:?}", name, e);
        }
    };
    let result = tokio::join!(pollTask, loggerTask, httpTask);
    handleNamedError("Poll", result.0);
    handleNamedError("Logger", result.1);
    handleNamedError("HTTP", result.2);

    // All tasks stopped
    info!("Stopped delivery2door.");
    if startupFailed {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}
