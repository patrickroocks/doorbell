#![allow(non_snake_case)]

pub mod mailSink;
pub mod notifier;
pub mod sink;
pub mod template;

use crate::configStore::NotificationSinkConfig;
use mailSink::MailSink;
use sink::NotificationSink;

/// Builds the sink for one entry of the "notificationSinks" config array.
pub fn createSink(cfg: &NotificationSinkConfig) -> Result<Box<dyn NotificationSink>, String> {
    match cfg {
        NotificationSinkConfig::Mail(mailCfg) => Ok(Box::new(MailSink::new(mailCfg.clone())?)),
    }
}
