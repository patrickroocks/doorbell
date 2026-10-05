use crate::configStore::{MailSecurity, MailSinkConfig};
use crate::notifications::sink::{NotificationSink, SendFuture, SinkError};
use crate::notifications::template::{NotificationValues, render};

use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use tracing::debug;

/// Notification sink that sends a mail over SMTP.
///
/// The transport is built once and reused: `AsyncSmtpTransport` holds a connection pool, so a burst
/// of announcements in one poll cycle does not open a new connection per mail.
pub struct MailSink {
    cfg: MailSinkConfig,
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

impl MailSink {
    /// Builds the sink. Fails on a server name or a TLS setup the transport cannot be built from -
    /// which is a config error, so it is reported at startup.
    pub fn new(cfg: MailSinkConfig) -> Result<Self, String> {
        let builder = match cfg.security {
            // Connect plain and upgrade; lettre refuses to send the credentials if the server does
            // not offer STARTTLS, so the password cannot leak by accident.
            MailSecurity::Starttls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.server),
            MailSecurity::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.server),
            MailSecurity::None => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&cfg.server)),
        }
        .map_err(|e| format!("{}: {}", cfg.server, e))?;

        let mut builder = builder.port(cfg.port);
        // No user means an unauthenticated relay - sending the empty string as a password would be
        // rejected by every server that does want credentials.
        if !cfg.user.is_empty() {
            builder = builder.credentials(Credentials::new(cfg.user.clone(), cfg.pass.clone()));
        }

        Ok(MailSink {
            cfg,
            transport: builder.build(),
        })
    }

    /// Builds the mail from the templates and the values of this notification.
    fn buildMessage(&self, values: &NotificationValues) -> Result<Message, SinkError> {
        let subject = render(&self.cfg.subject, values).map_err(|e| SinkError::Message(format!("subject: {}", e)))?;
        let body = render(&self.cfg.body, values).map_err(|e| SinkError::Message(format!("body: {}", e)))?;

        Message::builder()
            .from(self.cfg.from.parse().map_err(|e| SinkError::Message(format!("from \"{}\": {}", self.cfg.from, e)))?)
            .to(self.cfg.to.parse().map_err(|e| SinkError::Message(format!("to \"{}\": {}", self.cfg.to, e)))?)
            .subject(subject)
            // The bodies are plain text with real line breaks; saying so keeps the mail readable.
            .header(ContentType::TEXT_PLAIN)
            .body(body)
            .map_err(|e| SinkError::Message(e.to_string()))
    }
}

impl NotificationSink for MailSink {
    fn name(&self) -> &str {
        &self.cfg.name
    }

    fn send(&self, values: &NotificationValues) -> SendFuture<'_> {
        let message = self.buildMessage(values);
        Box::pin(async move {
            let message = message?;
            self.transport.send(message).await.map_err(|e| SinkError::Send(e.to_string()))?;
            debug!("Sink {}: mail sent to {}", self.cfg.name, self.cfg.to);
            Ok(())
        })
    }
}
