use chrono::{DateTime, Local};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

// ------------- Global directories -------------

const DATA_DIR: &str = "data";
const SESSION_FILE_NAME: &str = "sessions.json";
const LOG_DB_FILE_NAME: &str = "serverLog.sqlite";
const DELIVERY_DB_FILE_NAME: &str = "deliveries.sqlite";

pub struct AppPaths {
    pub sessionFile: PathBuf,
    pub logDbFile: PathBuf,
    /// Processed items, source positions and the delivery-to-rule mapping.
    pub deliveryDbFile: PathBuf,
}

impl AppPaths {
    pub fn new() -> Self {
        AppPaths {
            sessionFile: Path::new(DATA_DIR).join(SESSION_FILE_NAME),
            logDbFile: Path::new(DATA_DIR).join(LOG_DB_FILE_NAME),
            deliveryDbFile: Path::new(DATA_DIR).join(DELIVERY_DB_FILE_NAME),
        }
    }

    /// The data directory has to exist before the first database is opened.
    pub fn ensureDataDir() {
        if let Err(e) = std::fs::create_dir_all(DATA_DIR) {
            tracing::error!("Failed to create the data directory '{}': {}", DATA_DIR, e);
            std::process::exit(1);
        }
    }
}

pub static APP_PATHS: LazyLock<AppPaths> = LazyLock::new(AppPaths::new);

// ------------- Global application info -------------

pub mod AppInfo {
    pub const VERSION: &str = env!("CARGO_PKG_VERSION");
    pub const NAME: &str = "Delivery2Door";
    pub const BUILD_TIMESTAMP: &str = env!("BUILD_TIMESTAMP");

    pub fn getNameAndVersion() -> String {
        format!("{} v{}", NAME, VERSION)
    }

    pub fn getBuildTimeInfo() -> String {
        format!("Build time: {}", BUILD_TIMESTAMP)
    }
}

// ------------- Crawled item -------------

/// One document fetched by a source, in the form a crawler can search.
///
/// Deliberately source-agnostic: a mail source fills it from an IMAP message, a future source
/// (web page, push notification, ...) fills the same fields from whatever it fetched.
pub struct CrawlItem {
    /// Durable identity of the item, used to recognize it in later poll cycles - the `Message-ID`
    /// header for a mail. Must be unique and must survive everything the source may do to its
    /// numbering, so an item is never processed twice.
    pub itemId: String,
    /// The item's position in the source's own monotonic numbering, if it has one: the IMAP UID for
    /// a mail. Only comparable within one `generation` (see `ProcessedItemStore`), and used as a
    /// high-water mark so the source can ask its server for new items only.
    pub sequence: Option<u32>,
    /// Human readable one-liner for log output, e.g. subject and sender of a mail.
    pub description: String,
    /// When the document was sent by its origin - the `Date` header for a mail.
    ///
    /// This is the reference time for every date the document announces: "heute" means the day it was
    /// sent, and a "05.01." is completed with the year that follows from that day. The fetch time
    /// would be wrong for both, because an item may be crawled hours or days after it was written. A
    /// source that cannot tell falls back to the fetch time.
    pub sentAt: DateTime<Local>,
    /// The undecoded document source - the full RFC822 text for a mail. Only searched if the
    /// document could not be decoded, see `searchTexts`.
    pub rawText: String,
    /// The document's parts decoded to text (quoted-printable, base64, ...). Long links in HTML
    /// mails are soft-wrapped in the raw source, so a delivery id is often only complete here.
    pub decodedTexts: Vec<String>,
}

impl CrawlItem {
    /// All texts a crawler should search.
    ///
    /// The decoded parts are the authoritative content whenever the document could be parsed - the
    /// raw source is only searched as a fallback. Searching both would report artifacts of the
    /// transfer encoding as findings: a link reads `piececode=3D357820075428` in the raw source,
    /// where `=3D` is an encoded "=", so the raw text yields the delivery id `3D357820075428`.
    pub fn searchTexts(&self) -> impl Iterator<Item = &str> {
        if self.decodedTexts.is_empty() {
            Box::new(std::iter::once(self.rawText.as_str())) as Box<dyn Iterator<Item = &str>>
        } else {
            Box::new(self.decodedTexts.iter().map(|t| t.as_str()))
        }
    }
}
