use crate::common::CrawlItem;
use crate::crawlers::deliveryWindow::DeliveryWindow;

/// One announced delivery: what is coming and when.
pub struct DeliveryAnnouncement {
    pub deliveryId: String,
    /// `None` if the crawler has no date patterns configured or none of them matched.
    pub window: Option<DeliveryWindow>,
}

/// Extracts delivery announcements out of a fetched document.
///
/// A crawler knows the format of the announcement (DHL mail, ...) but not where it came from - the
/// source has already reduced everything to a `CrawlItem`.
pub trait Crawler: Send + Sync {
    /// Name of the crawler as configured in the "crawlers" section.
    fn name(&self) -> &str;

    /// What the rules of this crawler are named after, e.g. "DHL" or "Amazon". Empty if the crawler
    /// does not say, in which case the global default is used.
    fn namePrefix(&self) -> &str;

    /// All announcements found in the item. One document announces one delivery window but may
    /// mention several delivery ids, which then share that window.
    fn findAnnouncements(&self, item: &CrawlItem) -> Vec<DeliveryAnnouncement>;
}
