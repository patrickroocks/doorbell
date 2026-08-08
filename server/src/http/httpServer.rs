use crate::appState::MtxAppState;
use crate::common::{AutoBuzzState, ConnectionState, IntMsg, ToStateInfo, UserSettings};
use crate::configStore::ConfigStore;
use crate::http::htmlModel::{HtmlModel, LoginForm, RuleForm, RuleIdForm, ShortcutRuleForm};
use crate::http::httpSession::SessionManager;
use crate::logger::eventTypes::{LogEvent, LogEventType};
use crate::logger::loggerService::LogQueryService;
use crate::rules::rule::Rule;
use crate::rules::ruleService::{PendingRemovals, RulesChangedFlag};
use crate::rules::ruleStore::RuleStore;
use crate::util::notifyWithState::NotifyWithState;

use axum::{
    Router,
    extract::{Form, Query, State},
    response::{
        Html, IntoResponse, Redirect,
        sse::{Event, Sse},
    },
    routing::{get, post},
};
use chrono::{DateTime, Local, Timelike, Utc};
use futures::stream::{self, Stream, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::{Mutex, mpsc};
use tower_cookies::{CookieManagerLayer, Cookies};
use tower_http::services::ServeDir;
use tracing::{error, info};

// Accepted range of the "single shot auto ack" shortcut, mirrored by the min/max of the
// hours input on the rules page.
const SHORTCUT_MIN_HOURS: u32 = 1;
const SHORTCUT_MAX_HOURS: u32 = 23;

// ------------------ Http Server State ----------------------

// Everything optional, translation to "Unknown" or similar is done in the model.
struct DiagonsticData {
    mqttServer: Option<String>,
    deviceStartTime: Option<String>, // delivered as string from the device
    deviceRebootTime: Option<DateTime<Utc>>,
    serviceStartTime: Option<DateTime<Utc>>,
}

impl DiagonsticData {
    pub fn new() -> Self {
        Self {
            mqttServer: None,
            deviceStartTime: None,
            deviceRebootTime: None,
            serviceStartTime: None,
        }
    }
}

struct HttpState {
    tx: mpsc::Sender<IntMsg>,
    logTx: mpsc::Sender<LogEvent>,
    logQuery: LogQueryService,
    autoBuzzState: AutoBuzzState,
    autoBuzzStateNotify: Arc<tokio::sync::Notify>,
    // Auto-buzz by an active rule, reported by the RuleService. Independent of the device state above.
    ruleAutoBuzzState: AutoBuzzState,
    ruleAutoBuzzStateNotify: Arc<tokio::sync::Notify>,
    connState: ConnectionState,
    connStateNotify: Arc<tokio::sync::Notify>,
    sessionManager: Arc<SessionManager>,
    cfg: Arc<ConfigStore>,
    diagData: DiagonsticData,
    deviceLog: Vec<String>,
    waitingForLog: bool,
    deviceLogNotify: Arc<tokio::sync::Notify>,
    ruleStore: RuleStore,
    rulesChanged: RulesChangedFlag,
    // Single-shot rules which have just fired and are about to be deleted by the RuleService.
    pendingRemovals: PendingRemovals,
}

impl HttpState {
    fn setAutoBuzzState(&mut self, newState: AutoBuzzState) {
        if self.autoBuzzState != newState {
            self.autoBuzzState = newState;
            self.autoBuzzStateNotify.notify_waiters();
        }
    }

    // Not reset when the device disconnects: this state lives in the server, not in the device.
    fn setRuleAutoBuzzState(&mut self, newState: AutoBuzzState) {
        if self.ruleAutoBuzzState != newState {
            self.ruleAutoBuzzState = newState;
            self.ruleAutoBuzzStateNotify.notify_waiters();
        }
    }

    fn setConnectionState(&mut self, newState: ConnectionState) {
        if self.connState != newState {
            self.connState = newState;
            self.connStateNotify.notify_waiters();
            if self.connState != ConnectionState::DeviceConnected {
                self.setAutoBuzzState(AutoBuzzState::Unknown);
            }
        }
    }

    fn setDeviceLog(&mut self, newLog: Vec<String>) {
        self.deviceLog = newLog;
        self.waitingForLog = false;
        self.deviceLogNotify.notify_waiters();
    }
}

// ------------------ Http Server ----------------------

pub struct HttpServer {
    state: Arc<Mutex<HttpState>>,
    rx: mpsc::Receiver<IntMsg>,
    cancelNotify: Arc<NotifyWithState>,
    appState: MtxAppState,
}

impl HttpServer {
    pub fn new(
        tx: mpsc::Sender<IntMsg>,
        rx: mpsc::Receiver<IntMsg>,
        cfg: Arc<ConfigStore>,
        cancelNotify: Arc<NotifyWithState>,
        appState: MtxAppState,
        logTx: mpsc::Sender<LogEvent>,
        logQuery: LogQueryService,
        ruleStore: RuleStore,
        rulesChanged: RulesChangedFlag,
        pendingRemovals: PendingRemovals,
    ) -> Self {
        let state = HttpState {
            tx: tx,
            logTx: logTx,
            logQuery: logQuery,
            autoBuzzState: AutoBuzzState::Unknown,
            autoBuzzStateNotify: Arc::new(tokio::sync::Notify::new()),
            ruleAutoBuzzState: AutoBuzzState::Unknown,
            ruleAutoBuzzStateNotify: Arc::new(tokio::sync::Notify::new()),
            connState: ConnectionState::Disconnected,
            connStateNotify: Arc::new(tokio::sync::Notify::new()),
            sessionManager: Arc::new(SessionManager::new()),
            cfg: cfg,
            diagData: DiagonsticData::new(),
            deviceLog: Vec::new(),
            waitingForLog: false,
            deviceLogNotify: Arc::new(tokio::sync::Notify::new()),
            ruleStore: ruleStore,
            rulesChanged: rulesChanged,
            pendingRemovals: pendingRemovals,
        };
        Self {
            state: Arc::new(Mutex::new(state)),
            rx: rx,
            cancelNotify: cancelNotify,
            appState: appState,
        }
    }

    // ------------------ IntMsg handling ----------------------

    pub async fn run(&mut self) {
        self.state.lock().await.diagData.serviceStartTime = Some(self.appState.read().await.volatileState.serviceStartTime);

        // Everything that acts on the device or the rules, plus the state streams. The
        // `requireSession` layer rejects anonymous callers, so a route added here is protected by
        // default - the door must never be opened by an unauthenticated request.
        let protectedRoutes = Router::new()
            // # Action handlers
            .route("/doLogout", post(Self::handleLogout))
            .route("/buzz", post(Self::handleDoorBuzzer))
            .route("/reboot", post(Self::handleReboot))
            .route("/saveSettings", post(Self::handleSaveSettings))
            .route("/saveRule", post(Self::handleSaveRule))
            .route("/deleteRule", post(Self::handleDeleteRule))
            .route("/toggleRule", post(Self::handleToggleRule))
            .route("/shortcutRule", post(Self::handleShortcutRule))
            .route("/toggleAutoBuzz", post(Self::toggleAutoBuzz))
            // # States & their streams
            .route("/autoBuzzStateStream", get(Self::getAutoBuzzStateStream))
            .route("/ruleAutoBuzzStateStream", get(Self::getRuleAutoBuzzStateStream))
            .route("/connStateStream", get(Self::getConnStateStream))
            .route("/deviceLogStream", get(Self::getDeviceLogStream))
            .route_layer(axum::middleware::from_fn_with_state(self.state.clone(), Self::requireSession));

        // The pages render the "not logged in" page themselves (see `startHtmlModel`), so they stay
        // reachable without a session - as does the login itself.
        let app = Router::new()
            // # Pages
            .route("/", get(Self::showMainPage))
            .route("/login", get(Self::showLoginPage))
            .route("/settings", get(Self::showSettingsPage))
            .route("/deviceLog", get(Self::showDeviceLogPage))
            .route("/serverLog", get(Self::showServerLogPage))
            .route("/diagnostics", get(Self::showDiagnosticsPage))
            .route("/rules", get(Self::showRulesPage))
            .route("/editRule", get(Self::showEditRulePage))
            .route("/doLogin", post(Self::handleLogin))
            .merge(protectedRoutes)
            // # Static files (like CSS)
            .nest_service("/static", ServeDir::new("static"))
            // # State & Cookies
            .with_state(self.state.clone())
            .layer(CookieManagerLayer::new());

        // Do not use "127.0.0.1", as it will not be accessible from outside.
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
        let mut serverTask = tokio::spawn(async move {
            if let Err(e) = server.await {
                error!("Server error: {:?}", e);
                cancelNotifyClone.signal();
            }
        });

        info!("HTTP Server started on http://localhost:{}", port);

        loop {
            tokio::select! {
                // Notification from the main function (e.g., SIGINT)
                _ = self.cancelNotify.notified() => {
                    info!("HttpServer: cancellation requested, shutting down...");
                    break;
                }

                // Internal messages (e.g., from MQTT client)
                Some(message) = self.rx.recv() => {
                    self.handleInternalMessage(message).await;
                }

                // Check if the server task has ended
                res = &mut serverTask => {
                        if let Err(err) = res {
                            error!("HTTP server stopped: {err}");
                            self.cancelNotify.signal();
                        }
                        return;
                    }
            }
        }

        // Stop accepting requests. Without this the server would keep serving - and keep sending
        // device commands - while the MQTT and rule tasks are already shutting down.
        serverTask.abort();
    }

    pub async fn handleInternalMessage(&mut self, message: IntMsg) {
        let mut state = self.state.lock().await;
        match message {
            IntMsg::SetAutoBuzz(onOff) => {
                state.setAutoBuzzState(if onOff { AutoBuzzState::On } else { AutoBuzzState::Off });
            }
            IntMsg::SetRuleAutoBuzz(onOff) => {
                state.setRuleAutoBuzzState(if onOff { AutoBuzzState::On } else { AutoBuzzState::Off });
            }
            IntMsg::SetConnState(connState) => {
                state.setConnectionState(connState);
            }
            IntMsg::DeviceLogContent(log) => {
                state.setDeviceLog(log);
            }
            IntMsg::StateUpdate => {
                let appState = self.appState.read().await;
                let diagData = &mut state.diagData;
                diagData.mqttServer = appState.volatileState.mqttHost.clone();
                diagData.deviceRebootTime = appState.persistentState.getLastDeviceRebootTime();
                diagData.deviceStartTime = appState.volatileState.deviceStartTime.clone();
            }
            _ => {}
        }
    }

    // ----------------- Page handling helpers ----------------------

    async fn startHtmlModel(httpState: &HttpState, cookies: &Cookies) -> HtmlModel {
        let mut htmlModel = HtmlModel::new();
        if let Some(mut session) = httpState.sessionManager.getSession(&cookies).await {
            htmlModel.setUser(Some(session.user.as_str()));
            if let Some(msg) = &session.lastSuccessMessage {
                // Delete message if displayed once
                htmlModel.setLastSuccessMessage(msg);
                session.lastSuccessMessage = None;
            }
            if let Some(msg) = &session.lastErrMessage {
                htmlModel.setLastErrMessage(msg);
                session.lastErrMessage = None;
            }
            let diagData = &httpState.diagData;
            htmlModel.setDeviceStartTime(diagData.deviceStartTime.clone());
            htmlModel.setServiceStartTime(diagData.serviceStartTime);
            htmlModel.setDeviceRebootTime(diagData.deviceRebootTime);
            htmlModel.setMqttServer(diagData.mqttServer.clone());
            htmlModel.setSelectedUpdateBehavior(session.userSettings.updateBehavior.clone());
            htmlModel.setConnectionState(httpState.connState.clone());
            htmlModel.setAutoBuzzState(httpState.autoBuzzState.clone());
            htmlModel.setRuleAutoBuzzState(httpState.ruleAutoBuzzState.clone());
            return htmlModel;
        }
        htmlModel.setUser(None);
        htmlModel
    }

    // ----------------- Route main page handler ----------------------

    async fn showMainPage(State(state): State<Arc<Mutex<HttpState>>>, cookies: Cookies) -> Html<String> {
        let state = state.lock().await;

        let mut htmlModel = HttpServer::startHtmlModel(&state, &cookies).await;
        if htmlModel.renderingDone() {
            // already rendered (e.g., not logged in)
            return htmlModel.getRenderedOutput();
        }

        return htmlModel.renderMainPage();
    }

    // ----------------- Route login page handler ----------------------

    async fn showLoginPage(Query(params): Query<HashMap<String, String>>) -> Html<String> {
        let failed = params.get("failed").is_some();
        let mut htmlModel = HtmlModel::new();
        if failed {
            htmlModel.setLastErrMessage("Login failed");
        }
        htmlModel.renderLoginPage()
    }

    // ----------------- Route diagnostics page handler ----------------------

    async fn showDiagnosticsPage(State(state): State<Arc<Mutex<HttpState>>>, cookies: Cookies) -> Html<String> {
        let state = state.lock().await;

        let mut htmlModel = HttpServer::startHtmlModel(&state, &cookies).await;
        if htmlModel.renderingDone() {
            return htmlModel.getRenderedOutput();
        }

        return htmlModel.renderDiagnosticsPage();
    }

    // ----------------- Routes for device log (fetched from the device) handler ----------------------

    async fn showDeviceLogPage(State(state): State<Arc<Mutex<HttpState>>>, cookies: Cookies) -> Html<String> {
        let state = state.lock().await;
        let mut htmlModel = HttpServer::startHtmlModel(&state, &cookies).await;
        if htmlModel.renderingDone() {
            return htmlModel.getRenderedOutput();
        }

        // The log is requested, when the client opens the stream.
        htmlModel.renderDeviceLogPage()
    }

    // ----------------- Route for the server log (persistent SQLite log) handler ----------------------

    async fn showServerLogPage(
        State(state): State<Arc<Mutex<HttpState>>>,
        cookies: Cookies,
        Query(params): Query<HashMap<String, String>>,
    ) -> Html<String> {
        let state = state.lock().await;
        let mut htmlModel = HttpServer::startHtmlModel(&state, &cookies).await;
        if htmlModel.renderingDone() {
            return htmlModel.getRenderedOutput();
        }

        // Default: show the last month; "earlier events" expands the range a month at a time.
        let months = params.get("months").and_then(|m| m.parse::<u32>().ok()).filter(|m| *m >= 1).unwrap_or(1);
        let rows = state.logQuery.queryLogs(months);
        htmlModel.renderServerLogPage(rows, months)
    }

    async fn handleLogin(
        State(state): State<Arc<Mutex<HttpState>>>,
        cookies: Cookies,
        Form(form): Form<LoginForm>,
    ) -> axum::response::Response {
        let state = state.lock().await;

        let loginSuccess = if let Some(userInfo) = state.cfg.users.get(&form.username) {
            userInfo.password == form.password
        } else {
            false
        };

        if loginSuccess {
            info!("Login successful for user: {}", form.username);
            let mut accessor = state.sessionManager.createSessionAndGetAccessor(&cookies).await;
            let session = accessor.currentSession().unwrap();
            session.user = form.username.clone();
            session.lastSuccessMessage = Some("Login successful".to_string());
            accessor.save().await;
            Self::logUserEvent(&state, LogEventType::Login, &form.username).await;
            Redirect::to("/").into_response()
        } else {
            Self::logUserEvent(&state, LogEventType::LoginFailed, &form.username).await;
            Redirect::to("/login?failed=1").into_response()
        }
    }

    async fn handleLogout(State(state): State<Arc<Mutex<HttpState>>>, cookies: Cookies) -> axum::response::Response {
        let state = state.lock().await;
        if let Some(session) = state.sessionManager.removeSessionAndGet(&cookies).await {
            info!("User {} logged out", session.user);
            Self::logUserEvent(&state, LogEventType::Logout, &session.user).await;
            return Redirect::to("/login?loggedout=1").into_response();
        }
        error!("Logout attempted but no valid session found");
        Redirect::to("/login").into_response()
    }

    // ---------------------- Routes for status updates ----------------------

    async fn getAutoBuzzStateStream(State(state): State<Arc<Mutex<HttpState>>>) -> Sse<impl Stream<Item = Result<Event, std::io::Error>>> {
        Self::getStreamByNotifier(state, |s| s.autoBuzzStateNotify.clone(), |s| s.autoBuzzState.toStateInfo().toJson()).await
    }

    async fn getRuleAutoBuzzStateStream(
        State(state): State<Arc<Mutex<HttpState>>>,
    ) -> Sse<impl Stream<Item = Result<Event, std::io::Error>>> {
        Self::getStreamByNotifier(
            state,
            |s| s.ruleAutoBuzzStateNotify.clone(),
            |s| s.ruleAutoBuzzState.toStateInfo().toJson(),
        )
        .await
    }

    async fn getConnStateStream(State(state): State<Arc<Mutex<HttpState>>>) -> Sse<impl Stream<Item = Result<Event, std::io::Error>>> {
        Self::getStreamByNotifier(state, |s| s.connStateNotify.clone(), |s| s.connState.toStateInfo().toJson()).await
    }

    async fn getDeviceLogStream(State(state): State<Arc<Mutex<HttpState>>>) -> Sse<impl Stream<Item = Result<Event, std::io::Error>>> {
        // Start receiving the log unless already waiting, will be delivered via SSE
        {
            let mut state = state.lock().await;
            if !state.waitingForLog {
                state.waitingForLog = true;
                state.deviceLog.clear();
                Self::sendToDevice(&state, IntMsg::GetDeviceLog).await;
            }
        }
        // The stream. Lines are joined with "\n" and rendered as text by the page - the log comes
        // from the device and must not end up in the DOM as markup.
        Self::getUpdatesOnlyStreamByNotifier(state, |s| s.deviceLogNotify.clone(), |s| s.deviceLog.join("\n")).await
    }

    async fn toggleAutoBuzz(State(state): State<Arc<Mutex<HttpState>>>, cookies: Cookies) -> Redirect {
        let mut state = state.lock().await;
        if state.autoBuzzState != AutoBuzzState::Unknown {
            let switchOn = state.autoBuzzState == AutoBuzzState::Off;
            // Go "Unknown" until the device confirms the new state.
            state.autoBuzzState = AutoBuzzState::Unknown;
            Self::sendToDevice(&state, IntMsg::SetAutoBuzz(switchOn)).await;
            let eventType = if switchOn {
                LogEventType::UserAutoBuzzOn
            } else {
                LogEventType::UserAutoBuzzOff
            };
            Self::emitUserEvent(&state, &cookies, eventType).await;
        }
        Redirect::to("/")
    }

    async fn handleDoorBuzzer(State(state): State<Arc<Mutex<HttpState>>>, cookies: Cookies) -> Redirect {
        let state = state.lock().await;
        Self::sendToDevice(&state, IntMsg::Buzz).await;
        Self::emitUserEvent(&state, &cookies, LogEventType::UserBuzz).await;
        Redirect::to("/")
    }

    async fn handleReboot(State(state): State<Arc<Mutex<HttpState>>>, cookies: Cookies) -> Redirect {
        let state = state.lock().await;
        Self::sendToDevice(&state, IntMsg::Reboot).await;
        Self::emitUserEvent(&state, &cookies, LogEventType::UserReboot).await;
        Redirect::to("/diagnostics")
    }

    // Hands a command to the MQTT task. Failing means that task is gone (shutdown), which must not
    // take the request handler down with it.
    async fn sendToDevice(state: &HttpState, message: IntMsg) {
        if let Err(e) = state.tx.send(message).await {
            error!("Failed to send device command: {}", e);
        }
    }

    // Guards all routes that act on the device or the rules. Without a valid session the request
    // never reaches the handler: form posts are sent back to the login page, the SSE streams - which
    // are polled by scripts, not navigated to - get a plain 401.
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

    // Records a user event in the server log, storing the acting user as details.
    async fn logUserEvent(state: &HttpState, eventType: LogEventType, userName: &str) {
        let event = LogEvent::new(eventType, format!("user: {}", userName));
        state.logTx.send(event).await.unwrap_or_default();
    }

    // Resolves the logged-in username from the session and records a user event in the server log.
    async fn emitUserEvent(state: &HttpState, cookies: &Cookies, eventType: LogEventType) {
        let user = match state.sessionManager.getSession(cookies).await {
            Some(session) => session.user.clone(),
            None => return,
        };
        Self::logUserEvent(state, eventType, &user).await;
    }

    // ---------------------- Route settings page handler ----------------------

    async fn showSettingsPage(State(state): State<Arc<Mutex<HttpState>>>, cookies: Cookies) -> Html<String> {
        let state = state.lock().await;

        let mut htmlModel = HttpServer::startHtmlModel(&state, &cookies).await;
        if htmlModel.renderingDone() {
            return htmlModel.getRenderedOutput();
        }

        return htmlModel.renderSettingsPage();
    }

    // ---------------------- Route save settings handler ----------------------

    async fn handleSaveSettings(
        State(state): State<Arc<Mutex<HttpState>>>,
        cookies: Cookies,
        Form(form): Form<UserSettings>,
    ) -> axum::response::Response {
        let state = state.lock().await;

        if let Some(mut sessionAccessor) = state.sessionManager.getSessionAccessor(&cookies).await {
            if let Some(session) = sessionAccessor.currentSession() {
                session.userSettings = form.clone();
                session.lastSuccessMessage = Some("Settings saved".to_string());
                sessionAccessor.save().await;
                return Redirect::to("/settings").into_response();
            }
        }
        Redirect::to("/").into_response()
    }

    // ---------------------- Rule page handlers ----------------------

    async fn showRulesPage(State(state): State<Arc<Mutex<HttpState>>>, cookies: Cookies) -> Html<String> {
        let state = state.lock().await;
        let mut htmlModel = HttpServer::startHtmlModel(&state, &cookies).await;
        if htmlModel.renderingDone() {
            return htmlModel.getRenderedOutput();
        }
        let rules = state.ruleStore.getRules();
        htmlModel.renderRulesPage(rules, &state.pendingRemovals.ids())
    }

    async fn showEditRulePage(
        State(state): State<Arc<Mutex<HttpState>>>,
        cookies: Cookies,
        Query(params): Query<HashMap<String, String>>,
    ) -> axum::response::Response {
        let state = state.lock().await;
        let mut htmlModel = HttpServer::startHtmlModel(&state, &cookies).await;
        if htmlModel.renderingDone() {
            return htmlModel.getRenderedOutput().into_response();
        }

        // Without an id this is the "new rule" form. With an id the rule has to exist - otherwise
        // the form would silently turn into a "new rule" one and saving would create a second rule.
        let rule = match params.get("id") {
            None => None,
            Some(id) => match id.parse::<i64>().ok().and_then(|id| state.ruleStore.getRule(id)) {
                Some(rule) => Some(rule),
                None => {
                    Self::setSessionMessage(&state, &cookies, None, Some("Rule does not exist anymore")).await;
                    return Redirect::to("/rules").into_response();
                }
            },
        };
        htmlModel.renderEditRulePage(rule).into_response()
    }

    async fn handleSaveRule(
        State(state): State<Arc<Mutex<HttpState>>>,
        cookies: Cookies,
        Form(form): Form<RuleForm>,
    ) -> axum::response::Response {
        let state = state.lock().await;

        // The id decides between update and insert. A non-empty but unparsable id means a tampered
        // form: it must not fall back to "new rule", and it must never reach the redirect URL below
        // (`Redirect::to` panics on a control character in the header value).
        let ruleId: Option<i64> = match form.id.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(id) => match id.parse::<i64>() {
                Ok(id) => Some(id),
                Err(_) => {
                    Self::setSessionMessage(&state, &cookies, None, Some("Invalid rule id")).await;
                    return Redirect::to("/rules").into_response();
                }
            },
        };
        let backToForm = match ruleId {
            Some(id) => format!("/editRule?id={}", id),
            None => "/editRule".to_string(),
        };
        let backWithError = async |error: &str| {
            Self::setSessionMessage(&state, &cookies, None, Some(error)).await;
            Redirect::to(&backToForm).into_response()
        };

        // Activation and deactivation times are required; equal times would describe a zero-length
        // window, i.e. a rule that can never activate.
        let (activation, deactivation) = match (Rule::parseTime(&form.activationTime), Rule::parseTime(&form.deactivationTime)) {
            (Some(a), Some(d)) if a != d => (a, d),
            (Some(_), Some(_)) => return backWithError("Activation and deactivation time must differ").await,
            _ => return backWithError("Activation and deactivation time are required").await,
        };

        // Both dates are optional ("no limit"), but an unparsable one is rejected instead of being
        // treated as "no limit", which would widen the rule instead of failing.
        let (startDate, endDate) = match (Rule::parseDateField(&form.startDate), Rule::parseDateField(&form.endDate)) {
            (Some(s), Some(e)) => (s, e),
            _ => return backWithError("Start and end date must be empty or a valid date").await,
        };
        if let (Some(start), Some(end)) = (startDate, endDate)
            && end < start
        {
            return backWithError("End date is earlier than start date").await;
        }

        let name = if form.name.trim().is_empty() {
            state.ruleStore.nextUnnamedName()
        } else {
            form.name.trim().to_string()
        };

        let rule = Rule {
            id: ruleId.unwrap_or(0),
            name,
            enabled: form.enabled.is_some(),
            autoAck: form.autoAck.is_some(),
            singleShot: form.singleShot.is_some(),
            startDate,
            endDate,
            activationTime: activation,
            deactivationTime: deactivation,
        };

        // Report a failed write instead of claiming success: the rule may have been deleted
        // meanwhile (e.g. an auto-removed single shot) while its edit form was still open.
        let saved = match ruleId {
            Some(_) => state.ruleStore.updateRule(&rule),
            None => state.ruleStore.insertRule(&rule).is_some(),
        };
        if !saved {
            let error = if ruleId.is_some() {
                "Rule could not be saved, it does not exist anymore"
            } else {
                "Rule could not be saved"
            };
            Self::setSessionMessage(&state, &cookies, None, Some(error)).await;
            return Redirect::to("/rules").into_response();
        }

        state.rulesChanged.store(true, Ordering::Release);
        Self::setSessionMessage(&state, &cookies, Some("Rule saved"), None).await;
        Redirect::to("/rules").into_response()
    }

    async fn handleDeleteRule(
        State(state): State<Arc<Mutex<HttpState>>>,
        cookies: Cookies,
        Form(form): Form<RuleIdForm>,
    ) -> axum::response::Response {
        let state = state.lock().await;
        match form.id.parse::<i64>() {
            Ok(id) => {
                state.ruleStore.deleteRule(id);
                state.rulesChanged.store(true, Ordering::Release);
                Self::setSessionMessage(&state, &cookies, Some("Rule deleted"), None).await;
            }
            Err(_) => Self::setSessionMessage(&state, &cookies, None, Some("Invalid rule id")).await,
        }
        Redirect::to("/rules").into_response()
    }

    async fn handleToggleRule(
        State(state): State<Arc<Mutex<HttpState>>>,
        cookies: Cookies,
        Form(form): Form<RuleIdForm>,
    ) -> axum::response::Response {
        let state = state.lock().await;
        let rule = form.id.parse::<i64>().ok().and_then(|id| state.ruleStore.getRule(id));
        match rule {
            Some(mut rule) => {
                rule.enabled = !rule.enabled;
                if state.ruleStore.updateRule(&rule) {
                    state.rulesChanged.store(true, Ordering::Release);
                    let msg = format!("Rule '{}' {}", rule.name, if rule.enabled { "enabled" } else { "disabled" });
                    Self::setSessionMessage(&state, &cookies, Some(&msg), None).await;
                } else {
                    Self::setSessionMessage(&state, &cookies, None, Some("Rule does not exist anymore")).await;
                }
            }
            None => Self::setSessionMessage(&state, &cookies, None, Some("Rule does not exist anymore")).await,
        }
        Redirect::to("/rules").into_response()
    }

    // Shortcut: create a single-shot, auto-ack rule active from now for the given number of hours.
    async fn handleShortcutRule(
        State(state): State<Arc<Mutex<HttpState>>>,
        cookies: Cookies,
        Form(form): Form<ShortcutRuleForm>,
    ) -> axum::response::Response {
        let state = state.lock().await;
        // A whole day or more cannot be expressed by the daily activation window, as the
        // deactivation time would then be at or before the activation time.
        let hours = match form.hours.trim().parse::<u32>() {
            Ok(h) if (SHORTCUT_MIN_HOURS..=SHORTCUT_MAX_HOURS).contains(&h) => h,
            _ => {
                let msg = format!("Hours must be a number between {} and {}", SHORTCUT_MIN_HOURS, SHORTCUT_MAX_HOURS);
                Self::setSessionMessage(&state, &cookies, None, Some(&msg)).await;
                return Redirect::to("/rules").into_response();
            }
        };

        let now = Local::now();
        let end = now + chrono::Duration::hours(hours as i64);
        let rule = Rule {
            id: 0,
            name: format!("single shot for {}h", hours),
            enabled: true,
            autoAck: true,
            singleShot: true,
            startDate: Some(now.date_naive()),
            // Both dates are the day the window *starts* on - for a window running past midnight
            // the part after midnight belongs to the window started today, see `Rule::isActiveAt`.
            endDate: Some(now.date_naive()),
            activationTime: chrono::NaiveTime::from_hms_opt(now.hour(), now.minute(), 0).unwrap(),
            deactivationTime: chrono::NaiveTime::from_hms_opt(end.hour(), end.minute(), 0).unwrap(),
        };
        if state.ruleStore.insertRule(&rule).is_none() {
            Self::setSessionMessage(&state, &cookies, None, Some("Rule could not be created")).await;
            return Redirect::to("/rules").into_response();
        }
        state.rulesChanged.store(true, Ordering::Release);
        let msg = format!("Created single shot rule for {}h", hours);
        Self::setSessionMessage(&state, &cookies, Some(&msg), None).await;
        Redirect::to("/rules").into_response()
    }

    // Stores a one-off success and/or error message on the current session (shown once on the next page load).
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
            accessor.save().await;
        }
    }

    // ---------------------- Route handling helper ----------------------

    // Helper for route endpoints for SSE streams without initial event
    async fn getUpdatesOnlyStreamByNotifier<F, G>(
        state: Arc<Mutex<HttpState>>,
        notifyFunc: F,
        dataFunc: G,
    ) -> Sse<impl Stream<Item = Result<Event, std::io::Error>>>
    where
        F: FnOnce(&HttpState) -> Arc<tokio::sync::Notify>,
        G: FnOnce(&HttpState) -> String,
    {
        // Get the notify handle without holding the lock
        let notify = {
            let state = state.lock().await;
            notifyFunc(&state)
        };

        notify.notified().await;

        let data = {
            let state = state.lock().await;
            dataFunc(&state)
        };
        let stream = stream::once(async move { Ok(Event::default().data(data)) });
        Sse::new(stream)
    }

    // Helper for route endpoints for SSE streams with initial event
    async fn getStreamByNotifier<F, G>(
        state: Arc<Mutex<HttpState>>,
        notifyFunc: F,
        dataFunc: G,
    ) -> Sse<impl Stream<Item = Result<Event, std::io::Error>>>
    where
        F: Fn(&HttpState) -> Arc<tokio::sync::Notify> + Copy + Send + Sync + 'static,
        G: Fn(&HttpState) -> String + Copy + Send + Sync + 'static,
    {
        // Initial value (sent immediately after subscribe)
        let initialValue = {
            let state = state.lock().await;
            dataFunc(&state)
        };

        let notify = {
            let state = state.lock().await;
            notifyFunc(&state)
        };

        // Stream of follow-up updates
        let updates = stream::unfold((state.clone(), notify, dataFunc), |(state, notify, dataFunc)| async move {
            notify.notified().await;
            let data = {
                let state = state.lock().await;
                dataFunc(&state)
            };
            Some((Ok(Event::default().data(data)), (state, notify, dataFunc)))
        });

        // Prepend initial event
        let combined = stream::once(async move { Ok(Event::default().data(initialValue)) }).chain(updates);
        Sse::new(combined)
    }
}
