use crate::common::AppInfo;
use crate::configStore::ConfigStore;
use crate::deliveries::announcementStore::{AnnouncementState, AnnouncementStore};
use crate::deliveries::processedItemStore::ProcessedItemStore;
use crate::doorbell::ruleApiClient::RuleApiClient;
use crate::http::htmlModel::{HtmlModel, LoginForm, SourceNameForm, formatLocal};
use crate::http::httpSession::SessionManager;
use crate::logger::eventTypes::{LogEvent, LogEventType};
use crate::logger::loggerService::LogQueryService;
use crate::pollService::PollStatus;
use crate::util::notifyWithState::NotifyWithState;

use axum::{
    Json, Router,
    extract::{Form, Query, State},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use chrono::{DateTime, Local};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_cookies::{CookieManagerLayer, Cookies};
use tower_http::services::ServeDir;
use tracing::{error, info};

/// How many of the processed items the deliveries page shows.
const PROCESSED_ITEMS_SHOWN: u32 = 50;

/*
HttpServer implementation
-------------------------

The operator surface: login, an overview, the delivery-to-rule mapping and the service log. Built like
the doorbell server's HTTP layer, minus everything that belongs to a device - there is no live state
worth streaming here, so there are no server-sent events either.

Arguments:
- cfg: The global config (users, http port, doorbell address)
- announcements / processedItems: the stores shown on the deliveries page
- logQuery: read handle on the service log
- pollStatus: when the poll service last ran and when it runs next
- ruleApiClient: shared with the reconciler, asked by the status route whether the doorbell answers
- serviceStartTime: when the process came up, reported by the status route
- cancelNotify: Notification for cancellation, and signaled if the port cannot be opened
*/
pub struct HttpState {
    cfg: Arc<ConfigStore>,
    sessionManager: Arc<SessionManager>,
    announcements: AnnouncementStore,
    processedItems: ProcessedItemStore,
    logQuery: LogQueryService,
    logTx: tokio::sync::mpsc::Sender<LogEvent>,
    pollStatus: PollStatus,
    ruleApiClient: Arc<RuleApiClient>,
    serviceStartTime: DateTime<Local>,
}

/// Answer of the public `/serviceState` route, in the same shape the doorbell server answers it -
/// a monitor can read both services the same way. Carries only what a status page needs, nothing
/// about the users, the deliveries or the rules.
#[derive(Serialize)]
struct ServiceStateResponse {
    serviceStartTime: String,
    connectionState: String,
    doorbellServer: String,
    version: String,
    /// When the running binary was built. Two deployments of the same version are only
    /// distinguishable by this, so a monitor can tell a stale binary from an up-to-date one.
    buildTime: String,
}

pub struct HttpServer {
    state: Arc<Mutex<HttpState>>,
    cancelNotify: Arc<NotifyWithState>,
}

impl HttpServer {
    pub fn new(
        cfg: Arc<ConfigStore>,
        announcements: AnnouncementStore,
        processedItems: ProcessedItemStore,
        logQuery: LogQueryService,
        logTx: tokio::sync::mpsc::Sender<LogEvent>,
        pollStatus: PollStatus,
        ruleApiClient: Arc<RuleApiClient>,
        serviceStartTime: DateTime<Local>,
        cancelNotify: Arc<NotifyWithState>,
    ) -> Self {
        let state = HttpState {
            cfg,
            sessionManager: Arc::new(SessionManager::new()),
            announcements,
            processedItems,
            logQuery,
            logTx,
            pollStatus,
            ruleApiClient,
            serviceStartTime,
        };
        HttpServer {
            state: Arc::new(Mutex::new(state)),
            cancelNotify,
        }
    }

    pub async fn run(&mut self) {
        // Everything that changes state, protected by the session layer - a re-scan drops the memory
        // of what was already processed, so it must never be reachable anonymously.
        let protectedRoutes = Router::new()
            .route("/doLogout", post(Self::handleLogout))
            .route("/rescanSource", post(Self::handleRescanSource))
            .route_layer(axum::middleware::from_fn_with_state(self.state.clone(), Self::requireSession));

        // The pages render the "not logged in" page themselves, so they stay reachable without a
        // session - as does the login itself.
        let app = Router::new()
            .route("/", get(Self::showMainPage))
            .route("/login", get(Self::showLoginPage))
            .route("/deliveries", get(Self::showDeliveriesPage))
            .route("/serviceLog", get(Self::showServiceLogPage))
            .route("/doLogin", post(Self::handleLogin))
            // Public state, no session needed
            .route("/serviceState", get(Self::getServiceState))
            .merge(protectedRoutes)
            .nest_service("/static", ServeDir::new("static"))
            .with_state(self.state.clone())
            .layer(CookieManagerLayer::new());

        // Do not use "127.0.0.1", as it would not be accessible from outside.
        let port = self.state.lock().await.cfg.httpPort;
        let listener = match tokio::net::TcpListener::bind(format!("[::]:{}", port)).await {
            Ok(l) => l,
            Err(e) => {
                error!("HTTP server stopped: Could not open port {}: {}", port, e);
                self.cancelNotify.signal();
                return;
            }
        };
        let server = axum::serve(listener, app);

        let cancelNotifyClone = self.cancelNotify.clone();
        let serverTask = tokio::spawn(async move {
            if let Err(e) = server.await {
                error!("Server error: {:?}", e);
                cancelNotifyClone.signal();
            }
        });

        info!("HTTP Server started on http://localhost:{}", port);

        self.cancelNotify.notified().await;
        info!("HttpServer: cancellation requested, shutting down...");
        serverTask.abort();
    }

    // ----------------- Route service state handler ----------------------

    /// Readable without a session, so an external monitor can see whether the service is alive and
    /// whether it can still reach the doorbell, without holding credentials.
    ///
    /// The connection state is probed on every call rather than remembered from the last rule the
    /// reconciler wrote: between two deliveries nothing talks to the doorbell for hours, so a
    /// remembered state would say nothing about right now.
    async fn getServiceState(State(state): State<Arc<Mutex<HttpState>>>) -> Json<ServiceStateResponse> {
        // The probe waits for the network, so the state lock is released before it runs - a status
        // request must not block the pages.
        let (client, startTime, doorbellUrl) = {
            let state = state.lock().await;
            (state.ruleApiClient.clone(), state.serviceStartTime, state.cfg.doorbell.baseUrl.clone())
        };
        Json(ServiceStateResponse {
            serviceStartTime: formatLocal(startTime),
            connectionState: client.checkConnection().await.to_string(),
            doorbellServer: doorbellUrl,
            version: AppInfo::VERSION.to_string(),
            buildTime: Self::formattedBuildTime(),
        })
    }

    /// `BUILD_TIMESTAMP` is RFC3339 as `build.rs` wrote it; the route reports it in the same wall
    /// clock format as every other time, and falls back to the raw value rather than failing.
    fn formattedBuildTime() -> String {
        match DateTime::parse_from_rfc3339(AppInfo::BUILD_TIMESTAMP) {
            Ok(t) => formatLocal(t.with_timezone(&Local)),
            Err(_) => AppInfo::BUILD_TIMESTAMP.to_string(),
        }
    }

    // ----------------- Page handling helpers ----------------------

    /// Fills the fields every page shows. Leaves `user_name` empty for an anonymous caller, which
    /// makes `renderingDone` produce the "not logged in" page.
    async fn startHtmlModel(state: &HttpState, cookies: &Cookies) -> HtmlModel {
        let mut htmlModel = HtmlModel::new();
        let Some(mut session) = state.sessionManager.getSession(cookies).await else {
            htmlModel.setUser(None);
            return htmlModel;
        };
        htmlModel.setUser(Some(session.user.as_str()));
        if let Some(msg) = &session.lastSuccessMessage {
            // Delete the message once it has been shown.
            htmlModel.setLastSuccessMessage(msg);
            session.lastSuccessMessage = None;
        }
        if let Some(msg) = &session.lastErrMessage {
            htmlModel.setLastErrMessage(msg);
            session.lastErrMessage = None;
        }
        htmlModel.setActiveDeliveries(state.announcements.countByState(AnnouncementState::Active));
        htmlModel.setLastPoll(state.pollStatus.lastPoll());
        htmlModel
    }

    // ----------------- Pages ----------------------

    async fn showMainPage(State(state): State<Arc<Mutex<HttpState>>>, cookies: Cookies) -> Html<String> {
        let state = state.lock().await;
        let mut htmlModel = Self::startHtmlModel(&state, &cookies).await;
        if htmlModel.renderingDone() {
            return htmlModel.getRenderedOutput();
        }

        let sourceSummary = state.cfg.sources.iter().map(|source| source.describe()).collect::<Vec<_>>().join(", ");
        let processedCount: u32 = state.cfg.sources.iter().map(|s| state.processedItems.countForSource(&s.describe())).sum();
        htmlModel.renderMainPage(
            sourceSummary,
            state.pollStatus.nextPoll(),
            state.announcements.countByState(AnnouncementState::Done),
            processedCount,
            state.cfg.doorbell.baseUrl.clone(),
        )
    }

    async fn showLoginPage(Query(params): Query<HashMap<String, String>>) -> Html<String> {
        let mut htmlModel = HtmlModel::new();
        if params.contains_key("failed") {
            htmlModel.setLastErrMessage("Login failed");
        }
        htmlModel.renderLoginPage()
    }

    async fn showDeliveriesPage(State(state): State<Arc<Mutex<HttpState>>>, cookies: Cookies) -> Html<String> {
        let state = state.lock().await;
        let mut htmlModel = Self::startHtmlModel(&state, &cookies).await;
        if htmlModel.renderingDone() {
            return htmlModel.getRenderedOutput();
        }
        let sourceNames = state.cfg.sources.iter().map(|source| source.describe()).collect();
        htmlModel.renderDeliveriesPage(
            state.announcements.all(),
            state.processedItems.recentItems(PROCESSED_ITEMS_SHOWN),
            PROCESSED_ITEMS_SHOWN,
            sourceNames,
        )
    }

    async fn showServiceLogPage(
        State(state): State<Arc<Mutex<HttpState>>>,
        cookies: Cookies,
        Query(params): Query<HashMap<String, String>>,
    ) -> Html<String> {
        let state = state.lock().await;
        let mut htmlModel = Self::startHtmlModel(&state, &cookies).await;
        if htmlModel.renderingDone() {
            return htmlModel.getRenderedOutput();
        }
        // Default: show the last month; "earlier events" expands the range a month at a time.
        let months = params.get("months").and_then(|m| m.parse::<u32>().ok()).filter(|m| *m >= 1).unwrap_or(1);
        let rows = state.logQuery.queryLogs(months);
        htmlModel.renderServiceLogPage(rows, months)
    }

    // ----------------- Actions ----------------------

    async fn handleLogin(
        State(state): State<Arc<Mutex<HttpState>>>,
        cookies: Cookies,
        Form(form): Form<LoginForm>,
    ) -> axum::response::Response {
        let state = state.lock().await;
        let loginSuccess = state.cfg.users.get(&form.username).is_some_and(|user| user.password == form.password);

        if !loginSuccess {
            info!("Login failed for user: {}", form.username);
            Self::logEvent(&state, LogEventType::LoginFailed, format!("user: {}", form.username)).await;
            return Redirect::to("/login?failed=1").into_response();
        }

        info!("Login successful for user: {}", form.username);
        let mut accessor = state.sessionManager.createSessionAndGetAccessor(&cookies).await;
        if let Some(session) = accessor.currentSession() {
            session.user = form.username.clone();
            session.lastSuccessMessage = Some("Logged in".to_string());
        }
        accessor.save().await;
        Self::logEvent(&state, LogEventType::Login, format!("user: {}", form.username)).await;
        Redirect::to("/").into_response()
    }

    async fn handleLogout(State(state): State<Arc<Mutex<HttpState>>>, cookies: Cookies) -> axum::response::Response {
        let state = state.lock().await;
        if let Some(session) = state.sessionManager.removeSessionAndGet(&cookies).await {
            Self::logEvent(&state, LogEventType::Logout, format!("user: {}", session.user)).await;
        }
        Redirect::to("/login").into_response()
    }

    /// Forgets everything about a source, so the next poll reads it from scratch.
    ///
    /// This is the only way to make a source deliver an item again: an item is otherwise processed
    /// exactly once, forever.
    async fn handleRescanSource(
        State(state): State<Arc<Mutex<HttpState>>>,
        cookies: Cookies,
        Form(form): Form<SourceNameForm>,
    ) -> axum::response::Response {
        let state = state.lock().await;
        let known = state.cfg.sources.iter().any(|source| source.describe() == form.name);
        if !known {
            Self::setSessionMessage(&state, &cookies, None, Some("Unknown source")).await;
            return Redirect::to("/deliveries").into_response();
        }

        state.processedItems.forgetSource(&form.name);
        state.announcements.forgetSource(&form.name);
        Self::logEvent(&state, LogEventType::SourceRescan, format!("Source: {}", form.name)).await;
        let msg = format!("Source {} will be read again at the next poll", form.name);
        Self::setSessionMessage(&state, &cookies, Some(&msg), None).await;
        Redirect::to("/deliveries").into_response()
    }

    // ----------------- Helpers ----------------------

    /// Guards the routes that change state. Form posts are sent back to the login page.
    async fn requireSession(
        State(state): State<Arc<Mutex<HttpState>>>,
        cookies: Cookies,
        request: axum::extract::Request,
        next: axum::middleware::Next,
    ) -> axum::response::Response {
        let hasSession = {
            let state = state.lock().await;
            state.sessionManager.getSession(&cookies).await.is_some()
        };
        if hasSession {
            return next.run(request).await;
        }
        if request.method() == axum::http::Method::GET {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
        Redirect::to("/login").into_response()
    }

    /// Stores a one-off message on the current session, shown once on the next page load.
    async fn setSessionMessage(state: &HttpState, cookies: &Cookies, success: Option<&str>, error: Option<&str>) {
        if let Some(mut accessor) = state.sessionManager.getSessionAccessor(cookies).await
            && let Some(session) = accessor.currentSession()
        {
            if let Some(msg) = success {
                session.lastSuccessMessage = Some(msg.to_string());
            }
            if let Some(msg) = error {
                session.lastErrMessage = Some(msg.to_string());
            }
        }
    }

    async fn logEvent(state: &HttpState, eventType: LogEventType, details: String) {
        state.logTx.send(LogEvent::new(eventType, details)).await.unwrap_or_default();
    }
}
