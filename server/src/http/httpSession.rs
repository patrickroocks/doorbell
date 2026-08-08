use crate::common::{APP_PATHS, UserSettings};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use time::Duration;
use tokio::sync::{Mutex, MutexGuard};
use tower_cookies::{Cookie, Cookies, cookie::SameSite};
use tracing::error;
use uuid::Uuid;

pub const SESSION_ID_FIELD: &str = "session_id";
pub const SESSION_VALIDITY_DAYS: i64 = 365;

#[derive(PartialEq, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub user: String,
    pub lastErrMessage: Option<String>,
    pub lastSuccessMessage: Option<String>,
    pub userSettings: UserSettings,
    // Sessions written before this field existed are treated as created now, so an update does not
    // log everybody out.
    #[serde(default = "Utc::now")]
    pub createdAt: DateTime<Utc>,
}

impl Session {
    pub fn new(id: String) -> Self {
        Session {
            id,
            user: String::new(),
            lastErrMessage: None,
            lastSuccessMessage: None,
            userSettings: UserSettings::default(),
            createdAt: Utc::now(),
        }
    }

    // The cookie expires on its own, but a stolen id would otherwise stay valid forever - the
    // server has to enforce the lifetime as well.
    pub fn isExpired(&self) -> bool {
        Utc::now() - self.createdAt > chrono::Duration::days(SESSION_VALIDITY_DAYS)
    }
}

pub type SessionMap = HashMap<String, Session>;

pub struct SessionAccessor<'a> {
    sessionId: String,
    sessionStore: MutexGuard<'a, SessionMap>,
}

impl<'a> SessionAccessor<'a> {
    pub fn new(sessionId: String, sessionStore: MutexGuard<'a, SessionMap>) -> Self {
        SessionAccessor { sessionId, sessionStore }
    }

    pub fn currentSession(&mut self) -> Option<&mut Session> {
        self.sessionStore.get_mut(&self.sessionId)
    }

    pub async fn save(&self) {
        let json = serde_json::to_string(&*self.sessionStore).unwrap();
        if let Err(e) = tokio::fs::write(&APP_PATHS.sessionFile, json).await {
            error!("Failed to write sessions to file: {:?}", e);
        }
    }

    pub fn removeCurrentAndGet(&mut self) -> Option<Session> {
        self.sessionStore.remove(&self.sessionId)
    }
}

pub struct SessionManager {
    sessionStore: Arc<Mutex<SessionMap>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessionStore: Arc::new(Mutex::new(Self::loadSessionsFromFile())),
        }
    }

    pub async fn getSessionAccessor<'a>(&'a self, cookies: &Cookies) -> Option<SessionAccessor<'a>> {
        let sessionId = Self::getSessionId(cookies)?;
        let sessionStore = self.sessionStore.lock().await;
        if Self::isValid(&sessionStore, &sessionId) {
            Some(SessionAccessor::new(sessionId, sessionStore))
        } else {
            Self::clearSessionCookie(cookies);
            None
        }
    }

    // Get access to the current session.
    // Intended for read-only access only, as we hold the lock to the session store and hence can store the sessions to file.
    pub async fn getSession<'a>(&'a self, cookies: &Cookies) -> Option<tokio::sync::MappedMutexGuard<'a, Session>> {
        let sessionId = Self::getSessionId(cookies)?;
        let sessionStore = self.sessionStore.lock().await;
        if Self::isValid(&sessionStore, &sessionId) {
            return Some(tokio::sync::MutexGuard::map(sessionStore, |store| {
                store.get_mut(&sessionId).unwrap()
            }));
        } else {
            Self::clearSessionCookie(cookies);
            None
        }
    }

    pub async fn removeSessionAndGet(&self, cookies: &Cookies) -> Option<Session> {
        let mut accessor = self.getSessionAccessor(cookies).await?;
        Self::clearSessionCookie(cookies);
        accessor.removeCurrentAndGet()
    }

    pub async fn createSessionAndGetAccessor(&self, cookies: &Cookies) -> SessionAccessor<'_> {
        let sessionId = Uuid::new_v4().to_string();
        let mut store = self.sessionStore.lock().await;
        let mut cookie = Cookie::new(SESSION_ID_FIELD, sessionId.to_string());
        cookie.set_max_age(Duration::days(SESSION_VALIDITY_DAYS));
        cookie.set_path("/");
        // No script needs to read the session id, and the cookie must not be sent along with
        // cross-site requests. "Secure" is deliberately not set: the server is reached over plain
        // HTTP in the local network, TLS is terminated by the tunnel in front of it.
        cookie.set_http_only(true);
        cookie.set_same_site(SameSite::Lax);
        cookies.add(cookie);
        store.insert(sessionId.clone(), Session::new(sessionId.clone()));
        SessionAccessor::new(sessionId, store)
    }

    // ---------------------- Internal helpers ----------------------

    fn getSessionId(cookies: &Cookies) -> Option<String> {
        cookies.get(SESSION_ID_FIELD).map(|c| c.value().to_string())
    }

    // An expired session counts as unknown. It stays in the map until the next startup, which is
    // enough: nothing can be done with it anymore.
    fn isValid(sessionStore: &SessionMap, sessionId: &str) -> bool {
        sessionStore.get(sessionId).is_some_and(|session| !session.isExpired())
    }

    fn clearSessionCookie(cookies: &Cookies) {
        cookies.remove(Cookie::from(SESSION_ID_FIELD));
    }

    fn loadSessionsFromFile() -> SessionMap {
        let mut sessionMap = match std::fs::read_to_string(&APP_PATHS.sessionFile) {
            Ok(content) => match serde_json::from_str::<SessionMap>(&content) {
                Ok(map) => map,
                Err(e) => {
                    error!("Failed to parse sessions from file: {:?}", e);
                    SessionMap::new()
                }
            },
            Err(_) => SessionMap::new(),
        };
        // Drop expired sessions, otherwise the file grows forever and old ids stay in it.
        sessionMap.retain(|_, session| !session.isExpired());
        for (_, session) in &mut sessionMap {
            session.lastSuccessMessage = Some("Session restored after restart".to_string());
            session.lastErrMessage = None;
        }
        sessionMap
    }
}
