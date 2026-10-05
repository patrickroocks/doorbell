use crate::http::httpServer::HttpState;
use crate::logger::eventTypes::{LogEvent, LogEventType};
use crate::rules::rule::Rule;

use axum::{
    Json, Router,
    extract::{ConnectInfo, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::sync::Mutex;
use tracing::warn;

const AUTH_HEADER: &str = "authorization";
const BEARER_PREFIX: &str = "Bearer ";

/*
Rule API for other services
---------------------------

Lets another service (like delivery2door, parcel notification -> auto buzz) create and edit rules.
*/

#[derive(Deserialize, Serialize)]
pub struct RuleApiPayload {
    // ID is ignored on input for a new rule and filled in on output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub name: String,
    #[serde(default = "defaultEnabled")]
    pub enabled: bool,
    #[serde(default)]
    pub autoAck: bool,
    #[serde(default)]
    pub singleShot: bool,
    #[serde(default)]
    pub deleteAtEnd: bool,
    // Date and time fields are strings in the same formats the web form
    // uses (see `rule::DATE_FORMAT` / `rule::TIME_FORMAT`); an empty date means "no limit".
    #[serde(default)]
    pub startDate: String,
    #[serde(default)]
    pub endDate: String,
    pub activationTime: String,
    pub deactivationTime: String,
}

fn defaultEnabled() -> bool {
    true
}

// Answer of a successful create: the id the caller has to remember to edit the rule later.
#[derive(Serialize)]
struct CreatedRuleResponse {
    id: i64,
}

impl RuleApiPayload {
    fn fromRule(rule: &Rule) -> Self {
        RuleApiPayload {
            id: Some(rule.id),
            name: rule.name.clone(),
            enabled: rule.enabled,
            autoAck: rule.autoAck,
            singleShot: rule.singleShot,
            deleteAtEnd: rule.deleteAtEnd,
            startDate: rule.startDateStr(),
            endDate: rule.endDateStr(),
            activationTime: rule.activationTimeStr(),
            deactivationTime: rule.deactivationTimeStr(),
        }
    }

    // Validation is shared with the web form, so the two cannot accept different things.
    fn toRule(&self, id: Option<i64>) -> Result<Rule, String> {
        if self.name.trim().is_empty() {
            return Err("A rule needs a name".to_string());
        }
        Rule::fromParts(
            id,
            self.name.trim().to_string(),
            self.enabled,
            self.autoAck,
            self.singleShot,
            self.deleteAtEnd,
            &self.startDate,
            &self.endDate,
            &self.activationTime,
            &self.deactivationTime,
        )
    }
}

// All API routes, already guarded by the token check. Merged into the main router by `HttpServer`,
// which only does so if at least one token is configured.
pub fn apiRouter(state: Arc<Mutex<HttpState>>) -> Router<Arc<Mutex<HttpState>>> {
    Router::new()
        .route("/api/rules", post(createRule).get(listRules))
        .route("/api/rules/{id}", get(getRule).put(updateRule).delete(deleteRule))
        .route_layer(axum::middleware::from_fn_with_state(state, requireApiToken))
}

// Rejects every request that does not carry a configured token. Answers 401 without a hint about
// what was wrong, and logs the attempt - a wrong token has to be visible in the server log.
async fn requireApiToken(
    State(state): State<Arc<Mutex<HttpState>>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let presentedToken = request
        .headers()
        .get(AUTH_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix(BEARER_PREFIX))
        .unwrap_or_default()
        .to_string();

    let accepted = {
        let state = state.lock().await;
        state.cfg.apiTokens.iter().any(|token| tokensMatch(token, &presentedToken))
    };
    if accepted {
        return next.run(request).await;
    }

    let caller = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let details = format!("{} {} from {}", request.method(), request.uri().path(), caller);
    warn!("API request rejected: {}", details);
    logApiEvent(&*state.lock().await, LogEventType::ApiAuthFailed, details).await;
    StatusCode::UNAUTHORIZED.into_response()
}

// Compares two tokens without giving away where they start to differ, and without leaking the
// length through an early return.
fn tokensMatch(configured: &str, presented: &str) -> bool {
    if configured.is_empty() {
        return false;
    }
    let configured = configured.as_bytes();
    let presented = presented.as_bytes();
    let mut difference = configured.len() ^ presented.len();
    for (index, configuredByte) in configured.iter().enumerate() {
        // Comparing against a byte of the configured token when the presented one is exhausted keeps
        // the loop length independent of the presented token.
        difference |= usize::from(configuredByte ^ presented.get(index).copied().unwrap_or(0));
    }
    difference == 0
}

// ---------------------------- Handlers ----------------------------

async fn createRule(State(state): State<Arc<Mutex<HttpState>>>, Json(payload): Json<RuleApiPayload>) -> axum::response::Response {
    let rule = match payload.toRule(None) {
        Ok(rule) => rule,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };

    let state = state.lock().await;
    let Some(id) = state.ruleStore.insertRule(&rule) else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Rule could not be stored").into_response();
    };
    signalRulesChanged(&state);
    logApiEvent(&state, LogEventType::ApiRuleCreated, format!("Rule: '{}' (id {})", rule.name, id)).await;
    (StatusCode::CREATED, Json(CreatedRuleResponse { id })).into_response()
}

async fn listRules(State(state): State<Arc<Mutex<HttpState>>>) -> Json<Vec<RuleApiPayload>> {
    let state = state.lock().await;
    Json(state.ruleStore.getRules().iter().map(RuleApiPayload::fromRule).collect())
}

async fn getRule(State(state): State<Arc<Mutex<HttpState>>>, Path(id): Path<i64>) -> axum::response::Response {
    let state = state.lock().await;
    match state.ruleStore.getRule(id) {
        Some(rule) => Json(RuleApiPayload::fromRule(&rule)).into_response(),
        None => ruleNotFound(id),
    }
}

async fn updateRule(
    State(state): State<Arc<Mutex<HttpState>>>,
    Path(id): Path<i64>,
    Json(payload): Json<RuleApiPayload>,
) -> axum::response::Response {
    let rule = match payload.toRule(Some(id)) {
        Ok(rule) => rule,
        Err(e) => return (StatusCode::BAD_REQUEST, e).into_response(),
    };

    let state = state.lock().await;
    // 404 instead of a silent insert: the caller has to learn that its rule is gone (deleted at the
    // end of its lifetime, or by hand in the web UI) instead of resurrecting it.
    if !state.ruleStore.updateRule(&rule) {
        return ruleNotFound(id);
    }
    signalRulesChanged(&state);
    logApiEvent(&state, LogEventType::ApiRuleUpdated, format!("Rule: '{}' (id {})", rule.name, id)).await;
    Json(RuleApiPayload::fromRule(&rule)).into_response()
}

async fn deleteRule(State(state): State<Arc<Mutex<HttpState>>>, Path(id): Path<i64>) -> axum::response::Response {
    let state = state.lock().await;
    let Some(rule) = state.ruleStore.getRule(id) else {
        return ruleNotFound(id);
    };
    state.ruleStore.deleteRule(id);
    signalRulesChanged(&state);
    logApiEvent(&state, LogEventType::ApiRuleDeleted, format!("Rule: '{}' (id {})", rule.name, id)).await;
    StatusCode::NO_CONTENT.into_response()
}

// ---------------------------- Helpers ----------------------------

fn ruleNotFound(id: i64) -> axum::response::Response {
    (StatusCode::NOT_FOUND, format!("Rule {} does not exist", id)).into_response()
}

// Tells the RuleService to re-evaluate, exactly as the web form handlers do - a rule created for a
// window that is already running has to take effect within a second, not at the next minute.
fn signalRulesChanged(state: &HttpState) {
    state.rulesChanged.store(true, Ordering::Release);
}

async fn logApiEvent(state: &HttpState, eventType: LogEventType, details: String) {
    state.logTx.send(LogEvent::new(eventType, details)).await.unwrap_or_default();
}
