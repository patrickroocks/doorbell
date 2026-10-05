#![allow(non_snake_case)]

pub mod crawler;
pub mod datePattern;
pub mod deliveryWindow;
pub mod regexCrawler;

use crate::configStore::CrawlerConfig;
use crawler::Crawler;
use deliveryWindow::WindowRules;
use regexCrawler::{CrawlerError, RegexCrawler};

/// Builds the crawler for one entry of the "crawlers" config section.
///
/// Every crawler is a regex crawler today; a crawler that needs more than patterns gets its own
/// match arm on the name here.
pub fn createCrawler(name: &str, cfg: &CrawlerConfig, windowRules: WindowRules) -> Result<Box<dyn Crawler>, CrawlerError> {
    Ok(Box::new(RegexCrawler::new(name, cfg, windowRules)?))
}
