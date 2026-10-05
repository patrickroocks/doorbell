use crate::common::CrawlItem;
use crate::configStore::ConfigStore;
use crate::crawlers::crawler::Crawler;
use crate::crawlers::createCrawler;
use crate::crawlers::deliveryWindow::WindowRules;
use crate::deliveries::processedItemStore::{ProcessedItem, ProcessedItemStore, SourcePosition};
use crate::deliveries::reconciler::{AnnouncementOrigin, SharedReconciler};
use crate::sources::createSource;
use crate::sources::source::{FetchedItems, Source};
use crate::util::notifyWithState::NotifyWithState;

use chrono::{DateTime, Duration as ChronoDuration, Local};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, error, info};

/// When the poll service last ran and when it runs next, shared with the HTTP layer.
///
/// Read-only for everybody but the poll service itself, and cheaply cloneable.
#[derive(Clone)]
pub struct PollStatus {
    inner: Arc<Mutex<PollTimes>>,
}

#[derive(Default)]
struct PollTimes {
    lastPoll: Option<DateTime<Local>>,
    nextPoll: Option<DateTime<Local>>,
}

impl PollStatus {
    pub fn new() -> Self {
        PollStatus {
            inner: Arc::new(Mutex::new(PollTimes::default())),
        }
    }

    pub fn lastPoll(&self) -> Option<DateTime<Local>> {
        self.inner.lock().unwrap().lastPoll
    }

    pub fn nextPoll(&self) -> Option<DateTime<Local>> {
        self.inner.lock().unwrap().nextPoll
    }

    fn record(&self, finished: DateTime<Local>, interval: Duration) {
        let mut inner = self.inner.lock().unwrap();
        inner.lastPoll = Some(finished);
        inner.nextPoll = ChronoDuration::from_std(interval).ok().map(|delay| finished + delay);
    }
}

/*
PollService implementation
--------------------------

The single worker of the service: queries every configured source, hands the fetched items to the
source's crawler and reports the delivery ids that were found. The first query runs at startup, the
following ones every `queryIntervalSeconds`.

Findings are remembered per (item, delivery id), so a mail that stays in the folder is only reported
once and the hourly log stays short. The memory is not persisted - after a restart everything is new
again, which is intended for now.

Arguments:
- cfg: The global config
- cancelNotify: Notification for cancellation, so a running service can be shut down between polls.
*/
pub struct PollService {
    sources: Vec<Box<dyn Source>>,
    crawlers: HashMap<String, Box<dyn Crawler>>,
    queryInterval: Duration,
    processedItems: ProcessedItemStore,
    reconciler: SharedReconciler,
    pollStatus: PollStatus,
    cancelNotify: Arc<NotifyWithState>,
}

impl PollService {
    pub fn new(
        cfg: Arc<ConfigStore>,
        processedItems: ProcessedItemStore,
        reconciler: SharedReconciler,
        pollStatus: PollStatus,
        cancelNotify: Arc<NotifyWithState>,
    ) -> Self {
        // Crawlers are built once - a broken pattern is a config error and must not be discovered on
        // every poll.
        let windowRules = WindowRules::fromConfig(&cfg.general);
        let mut crawlers: HashMap<String, Box<dyn Crawler>> = HashMap::new();
        for (name, crawlerCfg) in &cfg.crawlers {
            match createCrawler(name, crawlerCfg, windowRules.clone()) {
                Ok(crawler) => {
                    crawlers.insert(name.clone(), crawler);
                }
                Err(e) => ConfigStore::handleError(&format!("Invalid pattern of crawler \"{}\"", name), e),
            }
        }

        let sources: Vec<Box<dyn Source>> = cfg.sources.iter().map(|source| createSource(source, processedItems.clone())).collect();

        PollService {
            sources,
            crawlers,
            queryInterval: Duration::from_secs(cfg.general.queryIntervalSeconds),
            processedItems,
            reconciler,
            pollStatus,
            cancelNotify,
        }
    }

    pub async fn run(&mut self) {
        let cancelNotify = self.cancelNotify.clone();
        loop {
            // Poll first, wait afterwards - the first query has to happen at startup.
            self.runPollCycle().await;
            tokio::select! {
                _ = cancelNotify.notified() => {
                    info!("PollService: cancellation requested, shutting down...");
                    break;
                }
                _ = tokio::time::sleep(self.queryInterval) => {}
            }
        }
    }

    /// One query of all sources.
    async fn runPollCycle(&mut self) {
        info!("Poll cycle started ({} source(s))", self.sources.len());
        let mut itemCount = 0usize;
        let mut deliveryCount = 0usize;

        for source in &self.sources {
            let fetched = match source.fetchItems().await {
                Ok(fetched) => fetched,
                Err(e) => {
                    // One unreachable source must not stop the others or the service.
                    error!("{}: {}", source.name(), e);
                    continue;
                }
            };
            let items = &fetched.items;
            if items.is_empty() {
                debug!("{}: nothing new", source.name());
                // Still advance: a cycle in which everything turned out to be known settles those
                // numbers, and skipping this would make the source offer them again forever.
                self.advancePosition(source.name(), &fetched);
                continue;
            }
            info!("{}: {} new item(s)", source.name(), items.len());

            for item in items {
                // Every crawler of the source sees every item: one folder can hold the announcements
                // of several carriers, and only the crawler whose identifier matches reports anything.
                let mut deliveryIds: Vec<String> = Vec::new();
                for crawlerName in source.crawlerNames() {
                    // Existence of the crawlers is validated when the config is read.
                    let Some(crawler) = self.crawlers.get(crawlerName) else {
                        error!("{}: unknown crawler \"{}\"", source.name(), crawlerName);
                        continue;
                    };

                    let announcements = crawler.findAnnouncements(item);
                    if announcements.is_empty() {
                        continue;
                    }
                    for announcement in &announcements {
                        let window = match &announcement.window {
                            Some(window) => window.to_string(),
                            None => "unknown".to_string(),
                        };
                        info!(
                            "  delivery id {}  (crawler {}, window {}, {})",
                            announcement.deliveryId,
                            crawler.name(),
                            window,
                            item.description
                        );
                        deliveryCount += 1;
                    }

                    // Create or refine the doorbell rules before the item counts as processed: a crash
                    // in between costs one more download, while the other order could lose an
                    // announcement.
                    let origin = AnnouncementOrigin {
                        sourceName: source.name(),
                        crawlerName: crawler.name(),
                        namePrefix: crawler.namePrefix(),
                        itemDescription: &item.description,
                    };
                    self.reconciler.handleAnnouncements(&origin, &announcements, Local::now()).await;
                    deliveryIds.extend(announcements.into_iter().map(|a| a.deliveryId));
                }

                if deliveryIds.is_empty() {
                    // The rest of the inbox. Not worth a log entry, but it is recorded, so it is
                    // never downloaded again.
                    debug!("  no delivery id in: {}", item.description);
                }
                self.recordProcessed(source.name(), item, deliveryIds);
                itemCount += 1;
            }

            // Only now, with every item of this source durably recorded, the source may skip them
            // next time. A crash in between costs one more download, never a lost mail.
            self.advancePosition(source.name(), &fetched);
        }

        // Deliveries whose window has passed are closed, so their mails can never bring them back.
        self.reconciler.closePassedDeliveries(Local::now()).await;

        self.pollStatus.record(Local::now(), self.queryInterval);
        info!(
            "Poll cycle finished: {} new item(s), {} delivery id(s) in them",
            itemCount, deliveryCount
        );
    }

    /// Remembers an item as processed, so neither its body nor its content is ever fetched again.
    fn recordProcessed(&self, sourceName: &str, item: &CrawlItem, deliveryIds: Vec<String>) {
        self.processedItems.recordProcessed(&ProcessedItem {
            itemId: item.itemId.clone(),
            sourceName: sourceName.to_string(),
            sequence: item.sequence,
            description: item.description.clone(),
            processedAt: Local::now(),
            deliveryIds,
        });
    }

    /// Moves the source's high-water mark up to what this cycle settled, so the next poll asks its
    /// server only for what came after it.
    ///
    /// The mark never moves backwards: a cycle in which only an older item was new must not lose that
    /// a newer one is long done. A source without its own numbering reports no generation; it relies
    /// on the item ids alone, so there is no mark to advance.
    fn advancePosition(&self, sourceName: &str, fetched: &FetchedItems) {
        let Some(generation) = fetched.generation else {
            return;
        };
        let Some(settled) = fetched.highestSettledSequence else {
            return;
        };
        // A remembered position of another generation says nothing about these numbers.
        let previous = self.processedItems.position(sourceName).filter(|position| position.generation == generation);
        let highestSequence = match previous {
            Some(position) => settled.max(position.highestSequence),
            None => settled,
        };
        self.processedItems.setPosition(
            sourceName,
            SourcePosition {
                generation,
                highestSequence,
            },
        );
    }
}
