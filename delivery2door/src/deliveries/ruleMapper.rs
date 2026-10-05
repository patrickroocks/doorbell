use crate::configStore::RuleCreationConfig;
use crate::crawlers::deliveryWindow::DeliveryWindow;
use crate::doorbell::ruleApiClient::RuleSpec;

use chrono::{DateTime, Duration, Local, NaiveDate, NaiveTime, Timelike};
use std::fmt;

/// Date and time formats of the doorbell rule API.
const DATE_FORMAT: &str = "%Y-%m-%d";
const TIME_FORMAT: &str = "%H:%M";

/// Why a delivery window cannot become a rule.
#[derive(Debug, PartialEq, Eq)]
pub enum MappingError {
    /// The window has already ended. Creating a rule for it would be pointless, and the doorbell
    /// service would delete it again immediately.
    AlreadyPassed,
    /// A doorbell rule is a daily window between two times of day, so it cannot span more than a day.
    TooLong,
    /// Nothing to open the door for.
    Empty,
}

impl fmt::Display for MappingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MappingError::AlreadyPassed => write!(f, "the delivery window has already ended"),
            MappingError::TooLong => write!(f, "the delivery window is longer than a day"),
            MappingError::Empty => write!(f, "the delivery window is empty"),
        }
    }
}

/// Turns a delivery window into the doorbell rule that opens the door during it.
///
/// A doorbell rule is not an absolute interval but a daily window between two times of day, limited by
/// a date range. A delivery window maps onto that as a single day:
///
/// * `startDate` = `endDate` = the day the window starts on,
/// * `activationTime` / `deactivationTime` = its two times of day.
///
/// A window running past midnight comes out as `deactivationTime < activationTime`, which is exactly
/// the wrap-around form the doorbell's `Rule::isActiveAt` handles - its second half belongs to the
/// window started on the day before.
pub fn ruleSpecForWindow(
    deliveryId: &str,
    namePrefix: &str,
    window: &DeliveryWindow,
    cfg: &RuleCreationConfig,
    now: DateTime<Local>,
) -> Result<RuleSpec, MappingError> {
    // Seconds would be lost anyway: the doorbell stores times as "%H:%M".
    let from = truncateToMinute(window.from);
    let to = truncateToMinute(window.to);

    if to <= from {
        return Err(MappingError::Empty);
    }
    if to - from > Duration::days(1) {
        return Err(MappingError::TooLong);
    }
    if to <= now {
        return Err(MappingError::AlreadyPassed);
    }

    let startDay = from.date_naive();
    let activationTime = from.time();
    let deactivationTime = to.time();
    // The two times alone cannot express a window of exactly 24 hours, and a zero-length daily window
    // never activates - both come out as equal times.
    if activationTime == deactivationTime {
        return Err(MappingError::TooLong);
    }
    // Anything ending later than the morning after cannot be described by two times of day.
    if to.date_naive() > startDay.succ_opt().unwrap_or(startDay) {
        return Err(MappingError::TooLong);
    }

    Ok(RuleSpec {
        id: None,
        name: ruleName(namePrefix, deliveryId),
        enabled: true,
        autoAck: cfg.autoAck,
        singleShot: cfg.singleShot,
        deleteAtEnd: cfg.deleteAtEnd,
        startDate: formatDate(startDay),
        endDate: formatDate(startDay),
        activationTime: formatTime(activationTime),
        deactivationTime: formatTime(deactivationTime),
    })
}

/// Name of the rule in the doorbell UI, e.g. "DHL 357820075428" or "Amazon 305-3203050-2444367" -
/// recognizable, and it says which carrier the rule came from.
pub fn ruleName(namePrefix: &str, deliveryId: &str) -> String {
    let prefix = namePrefix.trim();
    if prefix.is_empty() {
        deliveryId.to_string()
    } else {
        format!("{} {}", prefix, deliveryId)
    }
}

fn truncateToMinute(time: DateTime<Local>) -> DateTime<Local> {
    time.with_second(0).unwrap_or(time).with_nanosecond(0).unwrap_or(time)
}

fn formatDate(date: NaiveDate) -> String {
    date.format(DATE_FORMAT).to_string()
}

fn formatTime(time: NaiveTime) -> String {
    time.format(TIME_FORMAT).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn testConfig() -> RuleCreationConfig {
        RuleCreationConfig {
            namePrefix: "DHL".to_string(),
            autoAck: true,
            singleShot: false,
            deleteAtEnd: true,
        }
    }

    fn localTime(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Local> {
        let naive = NaiveDate::from_ymd_opt(year, month, day).unwrap().and_hms_opt(hour, minute, 0).unwrap();
        Local.from_local_datetime(&naive).earliest().unwrap()
    }

    fn windowOf(from: DateTime<Local>, to: DateTime<Local>) -> DeliveryWindow {
        DeliveryWindow { from, to }
    }

    fn mapAt(window: &DeliveryWindow, now: DateTime<Local>) -> Result<RuleSpec, MappingError> {
        ruleSpecForWindow("357820075428", "DHL", window, &testConfig(), now)
    }

    #[test]
    fn mapsTheFullDayWindowOfAFutureAnnouncement() {
        // The "am Mittwoch, den 12.08." case of the real mail.
        let window = windowOf(localTime(2026, 8, 12, 8, 0), localTime(2026, 8, 12, 20, 0));
        let spec = mapAt(&window, localTime(2026, 8, 11, 10, 0)).unwrap();
        assert_eq!(spec.name, "DHL 357820075428");
        assert_eq!(spec.startDate, "2026-08-12");
        assert_eq!(spec.endDate, "2026-08-12");
        assert_eq!(spec.activationTime, "08:00");
        assert_eq!(spec.deactivationTime, "20:00");
        assert!(spec.enabled && spec.autoAck && spec.deleteAtEnd);
        assert!(!spec.singleShot);
    }

    #[test]
    fn mapsTheNarrowWindowOfATodayAnnouncement() {
        // The "heute zwischen 13:45 - 15:15" case, with the tolerance already applied.
        let window = windowOf(localTime(2026, 8, 11, 11, 45), localTime(2026, 8, 11, 17, 15));
        let spec = mapAt(&window, localTime(2026, 8, 11, 10, 0)).unwrap();
        assert_eq!(spec.startDate, "2026-08-11");
        assert_eq!(spec.activationTime, "11:45");
        assert_eq!(spec.deactivationTime, "17:15");
    }

    #[test]
    fn mapsAWindowRunningPastMidnightToAWrapAroundRule() {
        // A late announcement plus the tolerance ends after midnight. Both dates stay on the day the
        // window starts on - that is what the doorbell expects for a wrap-around window.
        let window = windowOf(localTime(2026, 8, 11, 22, 30), localTime(2026, 8, 12, 1, 30));
        let spec = mapAt(&window, localTime(2026, 8, 11, 20, 0)).unwrap();
        assert_eq!(spec.startDate, "2026-08-11");
        assert_eq!(spec.endDate, "2026-08-11");
        assert_eq!(spec.activationTime, "22:30");
        assert_eq!(spec.deactivationTime, "01:30");
        // Deactivation before activation is what makes it a wrap-around window.
        assert!(spec.deactivationTime < spec.activationTime);
    }

    #[test]
    fn keepsAWindowThatIsAlreadyRunning() {
        // Half of the window has passed - the rule still has to be created, for the rest of it.
        let window = windowOf(localTime(2026, 8, 11, 11, 45), localTime(2026, 8, 11, 17, 15));
        let spec = mapAt(&window, localTime(2026, 8, 11, 14, 0)).unwrap();
        assert_eq!(spec.activationTime, "11:45");
    }

    #[test]
    fn rejectsAWindowThatHasPassed() {
        let window = windowOf(localTime(2026, 8, 12, 8, 0), localTime(2026, 8, 12, 20, 0));
        // Reading an old mail again after a restart must not create a rule for a past delivery.
        assert_eq!(mapAt(&window, localTime(2026, 8, 17, 10, 0)), Err(MappingError::AlreadyPassed));
        // The very end of the window counts as passed.
        assert_eq!(mapAt(&window, localTime(2026, 8, 12, 20, 0)), Err(MappingError::AlreadyPassed));
    }

    #[test]
    fn rejectsWindowsADailyRuleCannotExpress() {
        let now = localTime(2026, 8, 11, 10, 0);
        // Longer than a day.
        let tooLong = windowOf(localTime(2026, 8, 12, 8, 0), localTime(2026, 8, 14, 8, 0));
        assert_eq!(mapAt(&tooLong, now), Err(MappingError::TooLong));
        // Exactly 24 hours: the two times of day would be equal, which never activates.
        let fullDay = windowOf(localTime(2026, 8, 12, 8, 0), localTime(2026, 8, 13, 8, 0));
        assert_eq!(mapAt(&fullDay, now), Err(MappingError::TooLong));
        // Empty.
        let empty = windowOf(localTime(2026, 8, 12, 8, 0), localTime(2026, 8, 12, 8, 0));
        assert_eq!(mapAt(&empty, now), Err(MappingError::Empty));
    }

    #[test]
    fn secondsAreDroppedInsteadOfBeingRounded() {
        // The doorbell stores "%H:%M", so a window with seconds must not silently gain a minute.
        let from =
            Local.from_local_datetime(&NaiveDate::from_ymd_opt(2026, 8, 12).unwrap().and_hms_opt(8, 0, 45).unwrap()).earliest().unwrap();
        let to =
            Local.from_local_datetime(&NaiveDate::from_ymd_opt(2026, 8, 12).unwrap().and_hms_opt(20, 0, 45).unwrap()).earliest().unwrap();
        let spec = mapAt(&windowOf(from, to), localTime(2026, 8, 11, 10, 0)).unwrap();
        assert_eq!(spec.activationTime, "08:00");
        assert_eq!(spec.deactivationTime, "20:00");
    }

    #[test]
    fn thePrefixSaysWhichCarrierTheRuleCameFrom() {
        // The prefix comes from the crawler that found the delivery, so an Amazon parcel is not named
        // after DHL just because that crawler is configured first.
        let window = windowOf(localTime(2026, 8, 12, 8, 0), localTime(2026, 8, 12, 20, 0));
        let spec = ruleSpecForWindow(
            "305-3203050-2444367",
            "Amazon",
            &window,
            &testConfig(),
            localTime(2026, 8, 11, 10, 0),
        )
        .unwrap();
        assert_eq!(spec.name, "Amazon 305-3203050-2444367");
    }

    #[test]
    fn anEmptyPrefixLeavesJustTheDeliveryId() {
        assert_eq!(ruleName("", "357820075428"), "357820075428");
        assert_eq!(ruleName("  DHL ", "357820075428"), "DHL 357820075428");
    }
}
