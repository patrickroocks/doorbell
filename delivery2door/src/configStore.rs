use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use tracing::error;

fn defaultToleranceMinutes() -> i64 {
    120
}

fn defaultFullDayStartHour() -> u32 {
    8
}

fn defaultFullDayEndHour() -> u32 {
    20
}

fn defaultLookBackDays() -> u32 {
    14
}

fn defaultLogRetentionDays() -> u32 {
    180
}

fn defaultTrue() -> bool {
    true
}

#[derive(Debug, Deserialize, Clone)]
pub struct GeneralConfig {
    /// How often the sources are queried. The first query happens at startup, not after this delay.
    pub queryIntervalSeconds: u64,
    /// Added before and after an announced delivery time, since such an announcement is an estimate.
    #[serde(default = "defaultToleranceMinutes")]
    pub toleranceMinutes: i64,
    /// Start of the assumed delivery window if only a day is announced, but no time - and the
    /// earliest time of day a rule may ever open the door at.
    #[serde(default = "defaultFullDayStartHour")]
    pub fullDayStartHour: u32,
    /// End of the assumed delivery window if only a day is announced, but no time - and the latest
    /// time of day a rule may ever keep the door open until.
    #[serde(default = "defaultFullDayEndHour")]
    pub fullDayEndHour: u32,
    /// How long the events of the service log are kept.
    #[serde(default = "defaultLogRetentionDays")]
    pub logRetentionDays: u32,
}

/// A user of the web UI. Same shape as in the doorbell server.
#[derive(Debug, Deserialize, Clone)]
pub struct UserInfo {
    pub username: String,
    pub password: String,
}

/// How the doorbell service is reached.
#[derive(Debug, Deserialize, Clone)]
pub struct DoorbellConfig {
    /// Base URL of the doorbell service, e.g. "http://localhost:3001".
    pub baseUrl: String,
    /// One of the tokens the doorbell service lists in its own `apiTokens`.
    pub apiToken: String,
}

/// What the rules created for a delivery look like.
#[derive(Debug, Deserialize, Clone)]
pub struct RuleCreationConfig {
    /// Used to name a rule when the crawler that found the delivery has no `namePrefix` of its own.
    #[serde(default)]
    pub namePrefix: String,
    /// Acknowledge the ring, so the bell stops after the door was opened.
    #[serde(default = "defaultTrue")]
    pub autoAck: bool,
    /// Off by default: the courier may ring twice, and a second parcel can arrive in the same window.
    #[serde(default)]
    pub singleShot: bool,
    /// On by default: nobody would clean up these rules by hand.
    #[serde(default = "defaultTrue")]
    pub deleteAtEnd: bool,
}

impl Default for RuleCreationConfig {
    fn default() -> Self {
        RuleCreationConfig {
            namePrefix: String::new(),
            autoAck: true,
            singleShot: false,
            deleteAtEnd: true,
        }
    }
}

/// How a mail sink reaches its SMTP server.
#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MailSecurity {
    /// Connect plain, then upgrade - the usual setup on port 587.
    Starttls,
    /// TLS from the first byte - the usual setup on port 465.
    Tls,
    /// No encryption at all. Only sensible for a relay on localhost.
    None,
}

/// One mail notification sink: an SMTP account plus the message to send.
#[derive(Debug, Deserialize, Clone)]
pub struct MailSinkConfig {
    /// Identifies the sink in the log.
    pub name: String,
    pub server: String,
    pub port: u16,
    /// Credentials. An empty user means the server is not authenticated against at all.
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub pass: String,
    pub security: MailSecurity,
    pub from: String,
    pub to: String,
    /// Subject and body, with `{placeholder}`s - see `notifications::template`.
    pub subject: String,
    pub body: String,
}

/// One entry of the "notificationSinks" array, tagged by its "type" field. A future sink kind (push
/// message, webhook, ...) is one more variant here plus one arm in `notifications::createSink`.
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum NotificationSinkConfig {
    Mail(MailSinkConfig),
}

impl NotificationSinkConfig {
    pub fn name(&self) -> &str {
        match self {
            NotificationSinkConfig::Mail(cfg) => &cfg.name,
        }
    }

    /// The templates of this sink, so they can all be checked at startup.
    pub fn templates(&self) -> [&str; 2] {
        match self {
            NotificationSinkConfig::Mail(cfg) => [&cfg.subject, &cfg.body],
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct MailSourceConfig {
    pub server: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
    /// IMAP mailbox to search, e.g. "Inbox/test-delivery". Nothing outside of it is looked at.
    pub restrictToFolder: String,
    /// Sender addresses to look at, as substrings - e.g. "noreply@dhl.de". A carrier may write from
    /// several addresses (Amazon uses "shipment-tracking@" and "versandbestaetigung@"), so this is a
    /// list. An empty list looks at every mail of the folder.
    #[serde(default)]
    pub senderMasks: Vec<String>,
    /// Mails older than this are never even looked at. A delivery is announced a few days ahead at
    /// most, so an older mail cannot be relevant - and the IMAP server filters by it, which keeps the
    /// traffic on the mail account down.
    #[serde(default = "defaultLookBackDays")]
    pub lookBackDays: u32,
    /// Names of the crawlers in the "crawlers" section that are applied to every mail of this folder.
    ///
    /// A list, because one folder can hold the announcements of several carriers: each mail is fetched
    /// once and offered to all of them, and the crawler whose identifier matches is the one that
    /// reports something. Two sources on the same folder would not work - they share the record of
    /// what has already been processed and would skip each other's mails.
    pub crawlers: Vec<String>,
}

/// One entry of the "sources" array, tagged by its "type" field. A future source kind (web page,
/// push notification, ...) is one more variant here plus one arm in `sources::createSource`.
#[derive(Debug, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SourceConfig {
    Mail(MailSourceConfig),
}

impl SourceConfig {
    pub fn crawlerNames(&self) -> &[String] {
        match self {
            SourceConfig::Mail(cfg) => &cfg.crawlers,
        }
    }

    /// Short identification of the source for log messages.
    pub fn describe(&self) -> String {
        match self {
            SourceConfig::Mail(cfg) => format!("mail[{}/{}]", cfg.server, cfg.restrictToFolder),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct CrawlerConfig {
    /// Prefixed to the delivery id to name the rule, e.g. "DHL" or "Amazon". Per crawler, because it
    /// says which carrier the announcement came from; falls back to `ruleCreation.namePrefix`.
    #[serde(default)]
    pub namePrefix: Option<String>,
    /// Regex extracting the delivery id. Capture group 1 is the id, if the pattern has no group the
    /// whole match is used.
    pub deliveryIdentifier: String,
    /// Pattern of an announcement for the current day, containing two `{HH:MM}` placeholders, e.g.
    /// "heute zwischen {HH:MM} - {HH:MM} Uhr". Checked before `deliveryDateFuture`.
    pub deliveryDateToday: Option<String>,
    /// Pattern of an announcement naming a day relative to the mail, containing a `{RelativeDay}`
    /// placeholder, e.g. "Ankunft {RelativeDay}". The delivery window is the whole of that day.
    /// Checked after `deliveryDateToday` and before `deliveryDateFuture`.
    pub deliveryDateRelativeDay: Option<String>,
    /// Pattern of an announcement for a later day, containing a `{DD.MM}` placeholder, e.g.
    /// "am {Weekday}, den {DD.MM}". The delivery window is then the whole day.
    pub deliveryDateFuture: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ConfigFileFormat {
    pub general: GeneralConfig,
    pub httpPort: u16,
    pub users: Vec<UserInfo>,
    pub doorbell: DoorbellConfig,
    #[serde(default)]
    pub ruleCreation: RuleCreationConfig,
    pub sources: Vec<SourceConfig>,
    /// Optional: without a sink nothing is sent, the events are only logged.
    #[serde(default)]
    pub notificationSinks: Vec<NotificationSinkConfig>,
    pub crawlers: HashMap<String, CrawlerConfig>,
}

pub struct ConfigStore {
    pub general: GeneralConfig,
    pub httpPort: u16,
    /// Users of the web UI, by name - same as in the doorbell server.
    pub users: HashMap<String, UserInfo>,
    pub doorbell: DoorbellConfig,
    pub ruleCreation: RuleCreationConfig,
    pub sources: Vec<SourceConfig>,
    pub notificationSinks: Vec<NotificationSinkConfig>,
    pub crawlers: HashMap<String, CrawlerConfig>,
}

impl ConfigStore {
    pub fn new(configFile: String) -> Self {
        // Read and parse config file
        let fileContent = match fs::read_to_string(configFile) {
            Ok(content) => content,
            Err(e) => ConfigStore::handleError("Failed to read config file", e),
        };
        let config: ConfigFileFormat = match serde_json::from_str(&fileContent) {
            Ok(cfg) => cfg,
            Err(e) => ConfigStore::handleError("Failed to parse config file", e),
        };

        // A source referring to a crawler that does not exist would only fail on the first poll,
        // which may be minutes later - so it is rejected right at startup.
        for source in &config.sources {
            if source.crawlerNames().is_empty() {
                ConfigStore::handleError(
                    "Invalid config file",
                    format!("source {} has no crawlers configured", source.describe()),
                );
            }
            for crawlerName in source.crawlerNames() {
                if !config.crawlers.contains_key(crawlerName) {
                    ConfigStore::handleError(
                        "Invalid config file",
                        format!("source {} refers to unknown crawler \"{}\"", source.describe(), crawlerName),
                    );
                }
            }
        }
        if config.sources.is_empty() {
            ConfigStore::handleError("Invalid config file", "no sources configured");
        }
        if config.users.is_empty() {
            ConfigStore::handleError("Invalid config file", "no users configured, nobody could log in");
        }
        // Without a token the doorbell service would reject every call, and the whole point of this
        // service is to reach it - so this is a startup error, not a surprise hours later.
        if config.doorbell.apiToken.trim().is_empty() {
            ConfigStore::handleError("Invalid config file", "doorbell.apiToken is empty");
        }
        // Every window is narrowed to these hours, so a pair that spans nothing would silently
        // refuse every announcement instead of just the ones outside the allowed hours.
        let general = &config.general;
        if general.fullDayStartHour >= general.fullDayEndHour || general.fullDayEndHour > 24 {
            ConfigStore::handleError(
                "Invalid config file",
                format!(
                    "general.fullDayStartHour ({}) has to be smaller than general.fullDayEndHour ({}), which is at most 24 (midnight)",
                    general.fullDayStartHour, general.fullDayEndHour
                ),
            );
        }

        // Users into a hashmap, as in the doorbell server.
        let users = config.users.into_iter().map(|u| (u.username.clone(), u)).collect();

        ConfigStore {
            general: config.general,
            httpPort: config.httpPort,
            users,
            doorbell: config.doorbell,
            ruleCreation: config.ruleCreation,
            sources: config.sources,
            notificationSinks: config.notificationSinks,
            crawlers: config.crawlers,
        }
    }

    // ---------------------------- Helpers ----------------------------

    pub fn handleError<T>(msg: &str, err: T) -> !
    where
        T: std::fmt::Display,
    {
        error!("{}: {}", msg, err);
        std::process::exit(1);
    }
}
