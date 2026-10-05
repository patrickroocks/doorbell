use crate::common::CrawlItem;
use crate::configStore::MailSourceConfig;
use crate::deliveries::processedItemStore::{ProcessedItemStore, SourcePosition};
use crate::sources::source::{FetchFuture, FetchedItems, Source, SourceError};

use chrono::{DateTime, Duration, Local};
use imap::types::Fetch;
use mailparse::{MailHeaderMap, ParsedMail};
use std::collections::HashSet;
use tracing::{debug, info, warn};

/// IMAP date format of a `SINCE` search key, e.g. "04-Aug-2026".
const IMAP_SEARCH_DATE_FORMAT: &str = "%d-%b-%Y";

/// Source reading mails of one IMAP account, restricted to one folder.
///
/// The `imap` crate is blocking, so a whole fetch runs in `spawn_blocking`. At a poll interval of
/// minutes that is by far simpler than an async IMAP client.
pub struct MailSource {
    name: String,
    cfg: MailSourceConfig,
    processedItems: ProcessedItemStore,
}

impl MailSource {
    pub fn new(cfg: MailSourceConfig, processedItems: ProcessedItemStore) -> Self {
        MailSource {
            name: format!("mail[{}/{}]", cfg.server, cfg.restrictToFolder),
            cfg,
            processedItems,
        }
    }
}

impl Source for MailSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn crawlerNames(&self) -> &[String] {
        &self.cfg.crawlers
    }

    fn fetchItems(&self) -> FetchFuture<'_> {
        let cfg = self.cfg.clone();
        let name = self.name.clone();
        let processedItems = self.processedItems.clone();
        Box::pin(async move {
            match tokio::task::spawn_blocking(move || fetchMailsBlocking(&name, &cfg, &processedItems)).await {
                Ok(result) => result,
                Err(e) => Err(SourceError::Fetch(format!("fetch task failed: {}", e))),
            }
        })
    }
}

/// The complete IMAP conversation: connect, login, select the folder, search, fetch, logout.
///
/// Only mails that have never been processed are downloaded. Three filters, of which the first two
/// are applied by the server, so nothing else is even transferred:
///
/// 1. `SINCE`, from the configured `lookBackDays`.
/// 2. `UID <highest+1>:*`, from the position the store remembers for this source.
/// 3. the mail's `Message-ID`, checked against the store - one small header fetch decides whether the
///    body (tens of kilobytes for a DHL mail) is worth downloading at all.
fn fetchMailsBlocking(sourceName: &str, cfg: &MailSourceConfig, processedItems: &ProcessedItemStore) -> Result<FetchedItems, SourceError> {
    // `AutoTls` is the builder default: TLS on port 993, STARTTLS on every other port, and an error
    // if the server offers neither - so an unencrypted login can never happen by accident.
    let client = imap::ClientBuilder::new(&cfg.server, cfg.port)
        .connect()
        .map_err(|e| SourceError::Connect(format!("{}:{}: {}", cfg.server, cfg.port, e)))?;

    let mut session = client.login(&cfg.user, &cfg.pass).map_err(|(e, _client)| SourceError::Auth(format!("user {}: {}", cfg.user, e)))?;

    // Select the folder. A typo in the folder name is the most likely config mistake here, so the
    // error lists what the account actually offers.
    let mailbox = match session.select(&cfg.restrictToFolder) {
        Ok(mailbox) => mailbox,
        Err(e) => {
            let available = listMailboxes(&mut session);
            let _ = session.logout();
            return Err(SourceError::Folder(format!(
                "\"{}\": {} - available mailboxes: {}",
                cfg.restrictToFolder, e, available
            )));
        }
    };

    // Without a UIDVALIDITY the UIDs carry no promise at all, so the high-water mark cannot be used
    // and only the Message-ID check protects against a second download.
    let generation = mailbox.uid_validity;
    let lastPosition = processedItems.position(sourceName).filter(|position| Some(position.generation) == generation);
    if let Some(position) = processedItems.position(sourceName)
        && lastPosition.is_none()
    {
        warn!(
            "{}: mailbox generation changed ({} -> {:?}), the remembered UIDs are void and the folder is read again",
            sourceName, position.generation, generation
        );
    }

    let query = buildSearchQuery(cfg, lastPosition);
    let foundUids = session.uid_search(&query).map_err(|e| SourceError::Fetch(format!("search {}: {}", query, e)))?;

    // A range "N:*" also matches the highest existing UID when that is below N (RFC 3501), so the
    // server's answer can still contain mails that were handled long ago.
    let highestHandled = lastPosition.map(|position| position.highestSequence).unwrap_or(0);
    let mut candidateUids: Vec<u32> = foundUids.into_iter().filter(|uid| *uid > highestHandled).collect();
    candidateUids.sort_unstable();

    debug!("{}: search '{}' -> {} candidate(s)", sourceName, query, candidateUids.len());
    if candidateUids.is_empty() {
        let _ = session.logout();
        return Ok(FetchedItems {
            items: Vec::new(),
            generation,
            highestSettledSequence: None,
        });
    }

    // Ask for the identities only. This is the request that makes a folder full of unrelated mail
    // cheap: it costs a few hundred bytes per mail instead of the whole body.
    let newUids = match selectUnknownUids(&mut session, &candidateUids, processedItems) {
        Ok(uids) => uids,
        Err(e) => {
            let _ = session.logout();
            return Err(e);
        }
    };
    // Candidates that are not new were processed in an earlier cycle, so they are settled already.
    let alreadyProcessed: HashSet<u32> = candidateUids.iter().copied().filter(|uid| !newUids.contains(uid)).collect();
    if newUids.len() < candidateUids.len() {
        info!(
            "{}: {} of {} candidate(s) were already processed, only {} body/bodies fetched",
            sourceName,
            candidateUids.len() - newUids.len(),
            candidateUids.len(),
            newUids.len()
        );
    }
    if newUids.is_empty() {
        let _ = session.logout();
        return Ok(FetchedItems {
            items: Vec::new(),
            generation,
            highestSettledSequence: settledWatermark(&candidateUids, &alreadyProcessed),
        });
    }

    // BODY.PEEK instead of BODY: fetching must not mark the mails as read.
    let fetches = session
        .uid_fetch(uidSetOf(&newUids), "(UID BODY.PEEK[])")
        .map_err(|e| SourceError::Fetch(format!("fetch of {} mail(s): {}", newUids.len(), e)))?;

    let mut items: Vec<CrawlItem> = fetches.iter().filter_map(|fetched| toCrawlItem(cfg, fetched)).collect();
    // Oldest first, so a later mail about the same delivery overrides an earlier one.
    items.sort_by_key(|item| item.sequence);

    // Everything that was skipped as known plus everything that could really be read.
    let mut settled = alreadyProcessed;
    settled.extend(items.iter().filter_map(|item| item.sequence));
    let highestSettledSequence = settledWatermark(&candidateUids, &settled);

    let _ = session.logout();
    Ok(FetchedItems {
        items,
        generation,
        highestSettledSequence,
    })
}

/// The highest candidate up to which *every* candidate is settled.
///
/// Stopping at the first unsettled one is what keeps a mail that could not be read from being skipped
/// for good: the watermark stays below it, so the next cycle offers it again. The cost is one small
/// identity request per cycle for the mails above it, which the caller filters out anyway.
fn settledWatermark(candidateUids: &[u32], settled: &HashSet<u32>) -> Option<u32> {
    let mut watermark = None;
    for uid in candidateUids {
        if !settled.contains(uid) {
            break;
        }
        watermark = Some(*uid);
    }
    watermark
}

/// The IMAP search that leaves only mails which can still be new.
fn buildSearchQuery(cfg: &MailSourceConfig, lastPosition: Option<SourcePosition>) -> String {
    let mut keys: Vec<String> = Vec::new();

    // The internal date is what SINCE compares against, and it is a date, so the day itself is
    // included - one day more than asked for is fine, one day less would lose a mail.
    let since = (Local::now() - Duration::days(cfg.lookBackDays as i64)).format(IMAP_SEARCH_DATE_FORMAT);
    keys.push(format!("SINCE {}", since));

    // Restrict to the configured senders. IMAP "FROM" is a substring match on the From header, which
    // is exactly what a sender mask means.
    if let Some(fromKey) = senderKey(&cfg.senderMasks) {
        keys.push(fromKey);
    }
    if let Some(position) = lastPosition {
        keys.push(format!("UID {}:*", position.highestSequence.saturating_add(1)));
    }
    keys.join(" ")
}

/// The `FROM` part of the search, or `None` if every sender is welcome.
///
/// IMAP has no "one of these" key: `OR` takes exactly two keys in prefix notation, so several masks
/// become nested ORs. A carrier writing from more than one address (Amazon uses "shipment-tracking@"
/// and "versandbestaetigung@") is the reason this is a list at all.
fn senderKey(senderMasks: &[String]) -> Option<String> {
    let (first, rest) = senderMasks.split_first()?;
    let firstKey = format!("FROM \"{}\"", first);
    match senderKey(rest) {
        None => Some(firstKey),
        Some(restKey) => Some(format!("(OR {} {})", firstKey, restKey)),
    }
}

/// Fetches only the `Message-ID` of the candidates and keeps the ones the store does not know yet.
fn selectUnknownUids(
    session: &mut imap::Session<imap::Connection>,
    candidateUids: &[u32],
    processedItems: &ProcessedItemStore,
) -> Result<Vec<u32>, SourceError> {
    let fetches = session
        .uid_fetch(uidSetOf(candidateUids), "(UID BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)])")
        .map_err(|e| SourceError::Fetch(format!("identity fetch of {} mail(s): {}", candidateUids.len(), e)))?;

    let mut unknown = Vec::new();
    for fetched in fetches.iter() {
        let Some(uid) = fetched.uid else { continue };
        match messageIdOf(fetched) {
            // A mail without a Message-ID cannot be recognized later, so it is processed now and its
            // synthetic id keeps it out of the next round.
            None => {
                warn!("Mail uid {} has no Message-ID header, it is identified by its UID instead", uid);
                unknown.push(uid);
            }
            Some(messageId) => {
                if processedItems.isKnown(&messageId) {
                    debug!("Mail uid {} ({}) was already processed, its body is not fetched", uid, messageId);
                } else {
                    unknown.push(uid);
                }
            }
        }
    }
    unknown.sort_unstable();
    Ok(unknown)
}

/// The `Message-ID` of a mail, from a header-only or a full fetch.
fn messageIdOf(fetched: &Fetch<'_>) -> Option<String> {
    // A header-only fetch answers in `header()`, a full one in `body()`.
    let headerBytes = fetched.header().or_else(|| fetched.body())?;
    let parsed = mailparse::parse_mail(headerBytes).ok()?;
    let messageId = parsed.headers.get_first_value("Message-ID")?;
    let trimmed = messageId.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn uidSetOf(uids: &[u32]) -> String {
    uids.iter().map(|uid| uid.to_string()).collect::<Vec<_>>().join(",")
}

/// Turns one fetched mail into a `CrawlItem`. Mails without a body are skipped.
fn toCrawlItem(cfg: &MailSourceConfig, fetched: &Fetch<'_>) -> Option<CrawlItem> {
    let uid = fetched.uid?;
    let rawBytes = fetched.body()?;

    // Mails are not guaranteed to be valid UTF-8 (8bit transfer encoding with some other charset),
    // and a delivery id is ASCII anyway - so a lossy conversion loses nothing that matters.
    let rawText = String::from_utf8_lossy(rawBytes).to_string();

    let (description, decodedTexts, sentAt) = match mailparse::parse_mail(rawBytes) {
        Ok(parsed) => {
            let subject = parsed.headers.get_first_value("Subject").unwrap_or_else(|| "(no subject)".to_string());
            let from = parsed.headers.get_first_value("From").unwrap_or_else(|| "(no sender)".to_string());
            let mut texts = Vec::new();
            collectDecodedTexts(&parsed, &mut texts);
            (format!("{} / {}", subject, from), texts, sentAtOf(&parsed, uid))
        }
        Err(e) => {
            // The raw source is still searchable, so a mail that cannot be parsed is kept. Its Date
            // header is out of reach as well, so the fetch time has to do.
            warn!("Could not parse mail uid {} in {}: {}", uid, cfg.restrictToFolder, e);
            (format!("(unparsable mail, uid {})", uid), Vec::new(), Local::now())
        }
    };

    debug!(
        "Fetched mail uid {}: {} (sent {})",
        uid,
        description,
        sentAt.format("%Y-%m-%d %H:%M")
    );

    Some(CrawlItem {
        // The Message-ID is the durable identity; a mail without one falls back to folder and UID,
        // which is at least stable as long as the mailbox keeps its generation.
        itemId: messageIdOf(fetched).unwrap_or_else(|| format!("mail:{}:{}", cfg.restrictToFolder, uid)),
        sequence: Some(uid),
        description,
        sentAt,
        rawText,
        decodedTexts,
    })
}

/// Send time of a mail, from its `Date` header.
///
/// Every date the mail announces is relative to this instant, so a mail without a usable `Date` would
/// silently shift its announcement. That cannot be fixed here - the fetch time is the closest guess
/// available - but it is worth a warning.
fn sentAtOf(parsed: &ParsedMail<'_>, uid: u32) -> DateTime<Local> {
    match dateHeaderOf(parsed) {
        Some(sentAt) => sentAt,
        None => {
            warn!("Mail uid {} has no usable Date header, using the fetch time as its send time", uid);
            Local::now()
        }
    }
}

/// The parsed `Date` header, or `None` if it is missing, malformed or out of range.
fn dateHeaderOf(parsed: &ParsedMail<'_>) -> Option<DateTime<Local>> {
    let header = parsed.headers.get_first_value("Date")?;
    // `dateparse` resolves the header's own offset into seconds since the epoch, so the instant is
    // unambiguous; only the zone it is looked at in has to become the local one.
    let seconds = mailparse::dateparse(&header).ok()?;
    Some(DateTime::from_timestamp(seconds, 0)?.with_timezone(&Local))
}

/// Collects the decoded text of every part of a mail, walking multipart containers recursively.
///
/// This is what un-breaks the tracking links of HTML mails: quoted-printable soft line breaks may
/// split a delivery id in the middle, and only the decoded part has it in one piece.
fn collectDecodedTexts(part: &ParsedMail<'_>, texts: &mut Vec<String>) {
    if part.subparts.is_empty() {
        if let Ok(body) = part.get_body()
            && !body.is_empty()
        {
            texts.push(body);
        }
    } else {
        for subpart in &part.subparts {
            collectDecodedTexts(subpart, texts);
        }
    }
}

/// Names of all mailboxes of the account, for the "folder does not exist" error message.
fn listMailboxes(session: &mut imap::Session<imap::Connection>) -> String {
    match session.list(None, Some("*")) {
        Ok(names) => {
            let list = names.iter().map(|n| n.name().to_string()).collect::<Vec<_>>();
            if list.is_empty() { "(none)".to_string() } else { list.join(", ") }
        }
        Err(e) => format!("(could not be listed: {})", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settled(uids: &[u32]) -> HashSet<u32> {
        uids.iter().copied().collect()
    }

    #[test]
    fn watermarkCoversAContiguousRunOfSettledMails() {
        assert_eq!(settledWatermark(&[1, 2, 3], &settled(&[1, 2, 3])), Some(3));
        assert_eq!(settledWatermark(&[4, 7, 9], &settled(&[4, 7, 9])), Some(9));
    }

    #[test]
    fn watermarkStopsBelowAMailThatCouldNotBeRead() {
        // Mail 2 could not be read, so 3 must not be skipped either - the next cycle offers both
        // again and the identity check makes that cheap.
        assert_eq!(settledWatermark(&[1, 2, 3], &settled(&[1, 3])), Some(1));
        assert_eq!(settledWatermark(&[1, 2, 3], &settled(&[2, 3])), None);
    }

    #[test]
    fn watermarkOfNothingIsNothing() {
        assert_eq!(settledWatermark(&[], &settled(&[])), None);
        assert_eq!(settledWatermark(&[5], &settled(&[])), None);
    }

    #[test]
    fn senderKeyIsNestedOrsForSeveralAddresses() {
        assert_eq!(senderKey(&[]), None);
        assert_eq!(senderKey(&["a@x.de".to_string()]), Some("FROM \"a@x.de\"".to_string()));
        assert_eq!(
            senderKey(&["a@x.de".to_string(), "b@x.de".to_string()]),
            Some("(OR FROM \"a@x.de\" FROM \"b@x.de\")".to_string())
        );
        // Three addresses: the third one is OR-ed against the pair of the other two.
        assert_eq!(
            senderKey(&["a@x.de".to_string(), "b@x.de".to_string(), "c@x.de".to_string()]),
            Some("(OR FROM \"a@x.de\" (OR FROM \"b@x.de\" FROM \"c@x.de\"))".to_string())
        );
    }

    #[test]
    fn searchQueryCombinesLookBackSenderAndUidRange() {
        let cfg = MailSourceConfig {
            server: "mail.example.de".to_string(),
            port: 143,
            user: "u".to_string(),
            pass: "p".to_string(),
            restrictToFolder: "Inbox/test".to_string(),
            senderMasks: vec!["noreply@dhl.de".to_string()],
            lookBackDays: 14,
            crawlers: vec!["dhl-mail".to_string()],
        };
        let query = buildSearchQuery(
            &cfg,
            Some(SourcePosition {
                generation: 1,
                highestSequence: 3,
            }),
        );
        assert!(query.starts_with("SINCE "), "no SINCE key: {}", query);
        assert!(query.contains("FROM \"noreply@dhl.de\""), "no FROM key: {}", query);
        assert!(query.ends_with("UID 4:*"), "no UID range: {}", query);

        // Without a remembered position only the date and the sender limit the search.
        let firstRun = buildSearchQuery(&cfg, None);
        assert!(!firstRun.contains("UID"), "unexpected UID range: {}", firstRun);
    }
}
