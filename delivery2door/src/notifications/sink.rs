use crate::notifications::template::NotificationValues;

use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Why a notification could not be delivered.
#[derive(Debug)]
pub enum SinkError {
    /// The message could not be built from the templates or the configured addresses.
    Message(String),
    /// The sink could not be reached, or refused the message.
    Send(String),
}

impl fmt::Display for SinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SinkError::Message(msg) => write!(f, "message could not be built: {}", msg),
            SinkError::Send(msg) => write!(f, "sending failed: {}", msg),
        }
    }
}

/// The future returned by [`NotificationSink::send`]. Boxed by hand for the same reason as in
/// `sources::source`: the sinks live in a `Vec<Box<dyn NotificationSink>>`, so the trait has to stay
/// object-safe.
pub type SendFuture<'a> = Pin<Box<dyn Future<Output = Result<(), SinkError>> + Send + 'a>>;

/// Somewhere a notification can be delivered to - a mail account today, more later.
pub trait NotificationSink: Send + Sync {
    /// Identifies the sink in the log.
    fn name(&self) -> &str;

    /// Delivers one notification. Failing must not stop anything else: the caller only logs it.
    fn send(&self, values: &NotificationValues) -> SendFuture<'_>;
}
