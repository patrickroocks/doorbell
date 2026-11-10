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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Local, NaiveDate, TimeZone};

    /// Excerpt of a real DPD "Predict" mail, as its decoded `text/html` part reads. DPD sends no plain
    /// text part, and the announcement has a `<span>` in the middle of the sentence.
    const DPD_HTML: &str = r#"<td align="center" valign="top" style="font-family:Arial, sans-serif; font-size:21px; line-height:25px; color:#FFFFFF; font-weight:300;">Ihr Paket kommt heute<span class="em_mob_block"></span> zwischen 13:11 und 15:11 Uhr.</td></tr>
<td align="left" valign="top" style="font-family:Arial, sans-serif; font-size:16px; line-height:21px; color:#414042; font-weight:300;">Gebrüder Weiss GmbH (Intern. B<br /><a href="https://my.dpd.de/redirect.aspx?action=2&parcelno=MPS0626501639956320261109&locale=de_DE&utm_source=predict&utm_medium=email&utm_campaign=pr-einr&utm_content=link-psnr" target="_blank" style="text-decoration:underline; color:#414042;"><span style="text-decoration:underline; color:#414042;">06265016399563</span></a></td>
<a href="https://my.dpd.de/redirect.aspx?action=2&parcelno=MPS0626501639956320261109&locale=de_DE&utm_source=predict&utm_medium=email&utm_campaign=pr-ausr&utm_content=btn-live-track" target="_blank">Paket live verfolgen</a>"#;

    fn localTime(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> chrono::DateTime<Local> {
        let naive = NaiveDate::from_ymd_opt(year, month, day).unwrap().and_hms_opt(hour, minute, 0).unwrap();
        Local.from_local_datetime(&naive).earliest().unwrap()
    }

    /// The "dpd-mail" entry of the config.
    fn dpdCrawler() -> RegexCrawler {
        let cfg = CrawlerConfig {
            namePrefix: Some("DPD".to_string()),
            deliveryIdentifier: r#"dpd\.de/[^"\s]*parcelno=(?:MPS)?([0-9]{14})"#.to_string(),
            deliveryDateToday: Some("zwischen {HH:MM} und {HH:MM} Uhr".to_string()),
            deliveryDateRelativeDay: None,
            deliveryDateFuture: None,
        };
        let rules = WindowRules {
            tolerance: Duration::minutes(120),
            fullDayStartHour: 8,
            fullDayEndHour: 20,
        };
        RegexCrawler::new("dpd-mail", &cfg, rules).unwrap()
    }

    fn itemOf(decodedText: &str) -> CrawlItem {
        CrawlItem {
            itemId: "<test@service.dpd.de>".to_string(),
            sequence: None,
            description: "test".to_string(),
            sentAt: localTime(2026, 11, 10, 9, 52),
            rawText: String::new(),
            decodedTexts: vec![decodedText.to_string()],
        }
    }

    #[test]
    fn dpdMailYieldsParcelNumberAndTodayWindow() {
        let announcements = dpdCrawler().findAnnouncements(&itemOf(DPD_HTML));
        // The same parcel is linked twice; the "MPS" prefix and the date suffix are not part of the id,
        // which is the parcel number the mail shows.
        assert_eq!(announcements.len(), 1);
        assert_eq!(announcements[0].deliveryId, "06265016399563");
        let window = announcements[0].window.as_ref().expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 11, 10, 11, 11));
        assert_eq!(window.to, localTime(2026, 11, 10, 17, 11));
    }

    #[test]
    fn dpdCrawlerIgnoresOtherCarriers() {
        let dhl = r#"<a href="https://www.dhl.de/de/privatkunden/pakete-empfangen/verfolgen.html?piececode=357820075428">"#;
        assert!(dpdCrawler().findAnnouncements(&itemOf(dhl)).is_empty());
    }
}
