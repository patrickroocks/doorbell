use crate::common::CrawlItem;

use std::fmt;
use std::future::Future;
use std::pin::Pin;

/// Why a source could not deliver its items. A failing source must not take the service down - the
/// poll cycle logs the error and continues with the other sources.
#[derive(Debug)]
pub enum SourceError {
    /// The server could not be reached, or the transport could not be established.
    Connect(String),
    /// The credentials were rejected.
    Auth(String),
    /// The configured location (IMAP folder, ...) does not exist or cannot be opened.
    Folder(String),
    /// The items themselves could not be read.
    Fetch(String),
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceError::Connect(msg) => write!(f, "connection failed: {}", msg),
            SourceError::Auth(msg) => write!(f, "authentication failed: {}", msg),
            SourceError::Folder(msg) => write!(f, "folder not accessible: {}", msg),
            SourceError::Fetch(msg) => write!(f, "fetching items failed: {}", msg),
        }
    }
}

/// What one fetch delivered: the items that have never been processed before, and the generation the
/// source's own numbering currently belongs to.
pub struct FetchedItems {
    pub items: Vec<CrawlItem>,
    /// Identifies the numbering the items' `sequence` values belong to (the IMAP `UIDVALIDITY`).
    /// `None` for a source without a stable numbering - then only the item ids protect against
    /// processing something twice, see `ProcessedItemStore`.
    pub generation: Option<u32>,
    /// The sequence number up to which everything is now settled, i.e. the source may skip it next
    /// time. Deliberately not the highest number of `items`: an item that this cycle skipped because
    /// it was already processed is settled as well, while one the source could not read is *not* -
    /// so the watermark stops below it and it is retried.
    pub highestSettledSequence: Option<u32>,
}

/// The future returned by [`Source::fetchItems`].
///
/// `Source`s are kept in a `Vec<Box<dyn Source>>`, so the trait has to stay object-safe - which
/// rules out a plain `async fn` in the trait and makes this hand-boxed future necessary.
pub type FetchFuture<'a> = Pin<Box<dyn Future<Output = Result<FetchedItems, SourceError>> + Send + 'a>>;

/// A place that delivery announcements can be read from (a mail account today, more later).
pub trait Source: Send + Sync {
    /// Short identification of the source for log messages.
    fn name(&self) -> &str;

    /// Names of the crawlers that are applied to this source's items - one folder can hold the
    /// announcements of several carriers.
    fn crawlerNames(&self) -> &[String];

    /// Reads everything the source offers that has not been processed yet. Called once per poll
    /// cycle. Items already recorded in the `ProcessedItemStore` are not returned again - and, where
    /// the source's protocol allows it, not even transferred.
    fn fetchItems(&self) -> FetchFuture<'_>;
}
