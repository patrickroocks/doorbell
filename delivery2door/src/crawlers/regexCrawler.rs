use crate::common::CrawlItem;
use crate::configStore::CrawlerConfig;
use crate::crawlers::crawler::{Crawler, DeliveryAnnouncement};
use crate::crawlers::datePattern::{DatePattern, PatternError};
use crate::crawlers::deliveryWindow::{DatePatterns, DeliveryWindow, WindowRules, findWindow};
use crate::util::textNormalize::normalizeForParsing;

use regex::Regex;
use tracing::debug;

/// Crawler that finds delivery ids and delivery windows by applying configured patterns.
///
/// This covers the "dhl-mail" crawler and every other announcement that can be described by a
/// pattern - the crawler itself has no provider specific knowledge.
pub struct RegexCrawler {
    name: String,
    namePrefix: String,
    deliveryIdentifier: Regex,
    datePatterns: DatePatterns,
    windowRules: WindowRules,
}

/// Why a crawler could not be built from its config entry.
///
/// The variants are named after the config keys they belong to, so the shared prefix is intended.
#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub enum CrawlerError {
    DeliveryIdentifier(regex::Error),
    DeliveryDateToday(PatternError),
    DeliveryDateRelativeDay(PatternError),
    DeliveryDateFuture(PatternError),
}

impl std::fmt::Display for CrawlerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CrawlerError::DeliveryIdentifier(e) => write!(f, "deliveryIdentifier: {}", e),
            CrawlerError::DeliveryDateToday(e) => write!(f, "deliveryDateToday: {}", e),
            CrawlerError::DeliveryDateRelativeDay(e) => write!(f, "deliveryDateRelativeDay: {}", e),
            CrawlerError::DeliveryDateFuture(e) => write!(f, "deliveryDateFuture: {}", e),
        }
    }
}

impl RegexCrawler {
    pub fn new(name: &str, cfg: &CrawlerConfig, windowRules: WindowRules) -> Result<Self, CrawlerError> {
        let compilePattern = |pattern: &Option<String>| -> Result<Option<DatePattern>, PatternError> {
            match pattern {
                Some(p) => Ok(Some(DatePattern::new(p)?)),
                None => Ok(None),
            }
        };

        Ok(RegexCrawler {
            name: name.to_string(),
            namePrefix: cfg.namePrefix.clone().unwrap_or_default(),
            deliveryIdentifier: Regex::new(&cfg.deliveryIdentifier).map_err(CrawlerError::DeliveryIdentifier)?,
            datePatterns: DatePatterns {
                today: compilePattern(&cfg.deliveryDateToday).map_err(CrawlerError::DeliveryDateToday)?,
                relativeDay: compilePattern(&cfg.deliveryDateRelativeDay).map_err(CrawlerError::DeliveryDateRelativeDay)?,
                future: compilePattern(&cfg.deliveryDateFuture).map_err(CrawlerError::DeliveryDateFuture)?,
            },
            windowRules,
        })
    }

    /// All delivery ids of the item, without duplicates, in order of appearance.
    fn findDeliveryIds(&self, item: &CrawlItem) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for text in item.searchTexts() {
            for captures in self.deliveryIdentifier.captures_iter(text) {
                // Group 1 is the id; patterns without a group deliver the whole match instead.
                let deliveryId = match captures.get(1) {
                    Some(group) => group.as_str(),
                    None => captures.get(0).map_or("", |m| m.as_str()),
                };
                // The same id usually appears in the raw source and again in the decoded parts.
                if !deliveryId.is_empty() && !ids.iter().any(|id| id == deliveryId) {
                    ids.push(deliveryId.to_string());
                }
            }
        }
        ids
    }

    /// The delivery window of the item, from the first of its texts that announces one.
    ///
    /// Every text is normalized first, because the announcement is torn apart by the mail's
    /// formatting. Usually the mail's plain text part is the one that answers.
    ///
    /// All dates are resolved against the item's send time, so a window does not depend on when the
    /// poll cycle happened to pick the item up.
    fn findDeliveryWindow(&self, item: &CrawlItem) -> Option<DeliveryWindow> {
        if self.datePatterns.isEmpty() {
            return None;
        }
        for text in item.searchTexts() {
            let normalized = normalizeForParsing(text);
            if let Some(window) = findWindow(&normalized, &self.datePatterns, &self.windowRules, item.sentAt) {
                return Some(window);
            }
        }
        debug!("No delivery window found in {}", item.itemId);
        None
    }
}

impl Crawler for RegexCrawler {
    fn name(&self) -> &str {
        &self.name
    }

    fn namePrefix(&self) -> &str {
        &self.namePrefix
    }

    fn findAnnouncements(&self, item: &CrawlItem) -> Vec<DeliveryAnnouncement> {
        let ids = self.findDeliveryIds(item);
        if ids.is_empty() {
            return Vec::new();
        }

        // The window is a property of the document, so it is searched once and shared by all ids.
        let window = self.findDeliveryWindow(item);
        ids.into_iter()
            .map(|deliveryId| DeliveryAnnouncement {
                deliveryId,
                window: window.clone(),
            })
            .collect()
    }
}
