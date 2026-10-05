use crate::configStore::DoorbellConfig;

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;
use tracing::debug;

/// The doorbell service answers within milliseconds on the local network; a hanging request must not
/// block a poll cycle.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Shorter than `REQUEST_TIMEOUT`: the connection check answers a status request, where an
/// unreachable doorbell has to be reported quickly instead of stalling the caller for ten seconds.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// A rule as the doorbell API expects and returns it. Dates and times are strings in the formats the
/// doorbell uses ("%Y-%m-%d" / "%H:%M"); an empty date means "no limit".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSpec {
    /// Only set in answers - which rule is meant is decided by the URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub name: String,
    pub enabled: bool,
    pub autoAck: bool,
    pub singleShot: bool,
    pub deleteAtEnd: bool,
    pub startDate: String,
    pub endDate: String,
    pub activationTime: String,
    pub deactivationTime: String,
}

/// Answer of a successful create.
#[derive(Deserialize)]
struct CreatedRuleResponse {
    id: i64,
}

/// Why a call to the doorbell service failed.
///
/// `NotFound` is deliberately its own case: it means the rule is gone for good (deleted at the end of
/// its lifetime, or by hand), which the caller has to react to differently from a doorbell service
/// that is merely unreachable right now.
#[derive(Debug)]
pub enum ApiError {
    NotFound,
    /// The service answered, but rejected the request (bad token, invalid rule, ...).
    Rejected {
        status: u16,
        message: String,
    },
    /// The service could not be reached or did not answer in time.
    Unreachable(String),
    /// The answer could not be understood.
    BadResponse(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::NotFound => write!(f, "the rule does not exist anymore"),
            ApiError::Rejected { status, message } => write!(f, "doorbell service answered {}: {}", status, message),
            ApiError::Unreachable(msg) => write!(f, "doorbell service unreachable: {}", msg),
            ApiError::BadResponse(msg) => write!(f, "unexpected answer of the doorbell service: {}", msg),
        }
    }
}

/// Whether the doorbell service can be reached and talked to, as reported by `checkConnection`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoorbellConnState {
    /// No answer at all: service down, wrong host or port, network gone.
    Unreachable,
    /// Answered, but did not accept us - a wrong token, or a doorbell with no `apiTokens`
    /// configured, which makes it leave out the whole `/api` tree and answer 404.
    NotAccepted,
    /// Answered and accepted our token: rules can be created.
    Connected,
}

impl fmt::Display for DoorbellConnState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            DoorbellConnState::Unreachable => "Doorbell unreachable",
            DoorbellConnState::NotAccepted => "Doorbell reachable, not accepted",
            DoorbellConnState::Connected => "Connected to doorbell",
        };
        write!(f, "{}", s)
    }
}

/// Client for the rule API of the doorbell service.
///
/// The token is sent as a bearer header and never appears in a URL or a log line - `reqwest` errors
/// carry the URL they were built from, so keeping it out of the URL keeps it out of the logs.
pub struct RuleApiClient {
    client: reqwest::Client,
    baseUrl: String,
    apiToken: String,
}

impl RuleApiClient {
    pub fn new(cfg: &DoorbellConfig) -> Self {
        let client = reqwest::Client::builder().timeout(REQUEST_TIMEOUT).build().unwrap_or_default();
        RuleApiClient {
            client,
            baseUrl: cfg.baseUrl.trim_end_matches('/').to_string(),
            apiToken: cfg.apiToken.clone(),
        }
    }

    /// Creates a rule and returns the id the doorbell service assigned to it. That id is what links a
    /// delivery to its rule from now on.
    pub async fn createRule(&self, rule: &RuleSpec) -> Result<i64, ApiError> {
        let url = format!("{}/api/rules", self.baseUrl);
        let response = self.send(self.client.post(&url).json(rule)).await?;
        let body = response.text().await.map_err(|e| ApiError::BadResponse(e.to_string()))?;
        let created: CreatedRuleResponse = serde_json::from_str(&body).map_err(|e| ApiError::BadResponse(format!("{}: {}", e, body)))?;
        debug!("Created doorbell rule {} for '{}'", created.id, rule.name);
        Ok(created.id)
    }

    /// Replaces an existing rule. `NotFound` means it is gone and must not be recreated.
    pub async fn updateRule(&self, id: i64, rule: &RuleSpec) -> Result<(), ApiError> {
        let url = format!("{}/api/rules/{}", self.baseUrl, id);
        self.send(self.client.put(&url).json(rule)).await?;
        debug!("Updated doorbell rule {} ('{}')", id, rule.name);
        Ok(())
    }

    /// Reads a rule back, e.g. to check whether it still exists.
    ///
    /// Not needed by the reconciler - an update answers 404 if the rule is gone - but part of the
    /// client's surface and useful when diagnosing by hand.
    #[allow(dead_code)]
    pub async fn getRule(&self, id: i64) -> Result<RuleSpec, ApiError> {
        let url = format!("{}/api/rules/{}", self.baseUrl, id);
        let response = self.send(self.client.get(&url)).await?;
        let body = response.text().await.map_err(|e| ApiError::BadResponse(e.to_string()))?;
        serde_json::from_str(&body).map_err(|e| ApiError::BadResponse(format!("{}: {}", e, body)))
    }

    /// Asks the doorbell service whether it is there and whether it accepts our token.
    ///
    /// Reads the rule list instead of an unauthenticated status route on purpose: what matters here
    /// is not that some HTTP server answers, but that the exact path this service needs - the rule
    /// API, with our token - is open.
    pub async fn checkConnection(&self) -> DoorbellConnState {
        let url = format!("{}/api/rules", self.baseUrl);
        match self.send(self.client.get(&url).timeout(PROBE_TIMEOUT)).await {
            Ok(_) => DoorbellConnState::Connected,
            Err(ApiError::Unreachable(msg)) => {
                debug!("Doorbell connection check failed: {}", msg);
                DoorbellConnState::Unreachable
            }
            Err(e) => {
                debug!("Doorbell connection check rejected: {}", e);
                DoorbellConnState::NotAccepted
            }
        }
    }

    /// Sends the request with the token and turns every non-success answer into an `ApiError`.
    async fn send(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response, ApiError> {
        let response = request
            .bearer_auth(&self.apiToken)
            .send()
            .await
            // Without the URL, so a mistyped base URL cannot smuggle anything into a log line - and
            // the message stays short.
            .map_err(|e| ApiError::Unreachable(e.without_url().to_string()))?;

        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(ApiError::NotFound);
        }
        // The body of an error is the doorbell's own explanation, e.g. why a rule was rejected.
        let message = response.text().await.unwrap_or_default();
        Err(ApiError::Rejected {
            status: status.as_u16(),
            message: message.trim().to_string(),
        })
    }
}
