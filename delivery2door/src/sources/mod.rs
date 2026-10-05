#![allow(non_snake_case)]

pub mod mailSource;
pub mod source;

use crate::configStore::SourceConfig;
use crate::deliveries::processedItemStore::ProcessedItemStore;
use mailSource::MailSource;
use source::Source;

/// Builds the source for one entry of the "sources" config array.
///
/// Every source gets the store of already processed items: it is what lets the source ask its server
/// for new items only, instead of fetching everything again on every poll.
pub fn createSource(cfg: &SourceConfig, processedItems: ProcessedItemStore) -> Box<dyn Source> {
    match cfg {
        SourceConfig::Mail(mailCfg) => Box::new(MailSource::new(mailCfg.clone(), processedItems)),
    }
}
