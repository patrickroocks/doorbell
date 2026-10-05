use crate::configStore::GeneralConfig;
use crate::crawlers::datePattern::{DatePattern, WEEKDAY_NAMES};

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone};
use std::fmt;
use tracing::{info, warn};

/// The time span in which a delivery is expected.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct DeliveryWindow {
    pub from: DateTime<Local>,
    pub to: DateTime<Local>,
}

impl fmt::Display for DeliveryWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Same time format as the doorbell server uses in its log views.
        write!(f, "{} - {}", self.from.format("%Y-%m-%d %H:%M"), self.to.format("%Y-%m-%d %H:%M"))
    }
}

/// The date patterns of one crawler, compiled. Any of them may be absent; a crawler without a single
/// one reports no window at all.
#[derive(Debug, Default)]
pub struct DatePatterns {
    pub today: Option<DatePattern>,
    pub relativeDay: Option<DatePattern>,
    pub future: Option<DatePattern>,
}

impl DatePatterns {
    pub fn isEmpty(&self) -> bool {
        self.today.is_none() && self.relativeDay.is_none() && self.future.is_none()
    }
}

/// How an announcement is turned into a window, from the "general" config section.
#[derive(Debug, Clone)]
pub struct WindowRules {
    /// Added before and after an announced time span.
    pub tolerance: Duration,
    /// Assumed span if only a day is announced - and the hours no window may ever leave, see
    /// `clampToAllowedHours`.
    pub fullDayStartHour: u32,
    pub fullDayEndHour: u32,
}

impl WindowRules {
    pub fn fromConfig(cfg: &GeneralConfig) -> Self {
        WindowRules {
            tolerance: Duration::minutes(cfg.toleranceMinutes),
            fullDayStartHour: cfg.fullDayStartHour,
            fullDayEndHour: cfg.fullDayEndHour,
        }
    }
}

/// Finds the announced delivery window in an already normalized text.
///
/// `sentAt` is the time the document was written, not the time it is read: both the meaning of "heute"
/// and the year of a "12.08." are relative to the moment the announcement was made. Reading the clock
/// instead would shift every announcement by the delay until the next poll cycle.
///
/// An announcement for the day of writing wins over one for a later day: a mail that says "heute
/// zwischen ..." is the more recent, more precise information.
pub fn findWindow(text: &str, patterns: &DatePatterns, rules: &WindowRules, sentAt: DateTime<Local>) -> Option<DeliveryWindow> {
    clampToAllowedHours(findAnnouncedWindow(text, patterns, rules, sentAt)?, rules)
}

/// The window exactly as the text announces it, before it is narrowed to the allowed hours.
fn findAnnouncedWindow(text: &str, patterns: &DatePatterns, rules: &WindowRules, sentAt: DateTime<Local>) -> Option<DeliveryWindow> {
    // Most precise first: two times of day beat a whole day, and a day named relative to the mail is
    // as precise as a date but cannot be misread as one.
    if let Some(pattern) = patterns.today.as_ref()
        && let Some(window) = findTodayWindow(text, pattern, rules, sentAt)
    {
        return Some(window);
    }
    if let Some(pattern) = patterns.relativeDay.as_ref()
        && let Some(window) = findRelativeDayWindow(text, pattern, rules, sentAt)
    {
        return Some(window);
    }
    if let Some(pattern) = patterns.future.as_ref() {
        return findFutureWindow(text, pattern, rules, sentAt);
    }
    None
}

/// Narrows a window to the hours the door may be opened in at all.
///
/// `fullDayStartHour` / `fullDayEndHour` are not only the span assumed for a day without a time, they
/// are the outer bound of every window. An announcement is an unauthenticated mail and could be
/// faked, and a rule reaching into the night would let anyone buzz the house door open while the
/// house sleeps. During the day it costs nothing: whoever is at home opens for a parcel service
/// anyway, so the only thing lost is convenience at an hour nobody expects a parcel at.
///
/// The bound belongs to the day the window starts on, so a window running past midnight is cut back
/// to that evening instead of wrapping into the next morning. An announcement lying outside the
/// hours completely is refused - there is nothing left to open the door for.
fn clampToAllowedHours(window: DeliveryWindow, rules: &WindowRules) -> Option<DeliveryWindow> {
    let startDay = window.from.date_naive();
    let earliest = toLocal(startDay, rules.fullDayStartHour, 0)?;
    let latest = toLocal(startDay, rules.fullDayEndHour, 0)?;

    let clamped = DeliveryWindow {
        from: window.from.max(earliest),
        to: window.to.min(latest),
    };
    if clamped.to <= clamped.from {
        warn!(
            "Ignoring delivery announcement {}: it lies outside {:02}:00 - {:02}:00, the hours the door may be opened in",
            window, rules.fullDayStartHour, rules.fullDayEndHour
        );
        return None;
    }
    if clamped != window {
        info!("Delivery window {} narrowed to the allowed hours: {}", window, clamped);
    }
    Some(clamped)
}

/// "Ankunft morgen" -> the assumed full-day span of the day the mail names, counted from the day it
/// was sent. No tolerance is added, the span already covers a whole day.
fn findRelativeDayWindow(text: &str, pattern: &DatePattern, rules: &WindowRules, sentAt: DateTime<Local>) -> Option<DeliveryWindow> {
    let found = pattern.findIn(text)?;
    let offset = found.relativeDayOffset()?;
    let date = sentAt.date_naive().checked_add_signed(Duration::days(offset))?;
    Some(DeliveryWindow {
        from: toLocal(date, rules.fullDayStartHour, 0)?,
        to: toLocal(date, rules.fullDayEndHour, 0)?,
    })
}

/// "heute zwischen 13:45 - 15:15 Uhr" -> the two times on the day of writing, widened by the tolerance.
fn findTodayWindow(text: &str, pattern: &DatePattern, rules: &WindowRules, sentAt: DateTime<Local>) -> Option<DeliveryWindow> {
    let found = pattern.findIn(text)?;
    let times = found.times();
    // Two times are needed to span a window; a pattern with fewer is a config mistake.
    let [(fromHour, fromMinute), (toHour, toMinute)] = times[..] else {
        warn!(
            "Pattern \"{}\" has to contain two {{HH:MM}} placeholders, found {}",
            pattern.pattern(),
            times.len()
        );
        return None;
    };
    let today = sentAt.date_naive();

    let from = toLocal(today, fromHour, fromMinute)?;
    let to = toLocal(today, toHour, toMinute)?;
    if to < from {
        warn!(
            "Ignoring today's delivery announcement \"{}\": it ends before it starts",
            found.matchedText()
        );
        return None;
    }

    Some(DeliveryWindow {
        from: from - rules.tolerance,
        to: to + rules.tolerance,
    })
}

/// Find a time window in the future, where we do not know the exact time yet
fn findFutureWindow(text: &str, pattern: &DatePattern, rules: &WindowRules, sentAt: DateTime<Local>) -> Option<DeliveryWindow> {
    let found = pattern.findIn(text)?;
    let (day, month) = found.dayAndMonth()?;
    let date = inferDate(day, month, sentAt)?;

    if let Some(weekdayName) = found.weekday() {
        let expected = WEEKDAY_NAMES[date.weekday().num_days_from_monday() as usize];
        if weekdayName != expected {
            warn!(
                "Delivery announcement \"{}\" names {}, but {} is a {}",
                found.matchedText(),
                weekdayName,
                date,
                expected
            );
            return None;
        }
    }

    Some(DeliveryWindow {
        from: toLocal(date, rules.fullDayStartHour, 0)?,
        to: toLocal(date, rules.fullDayEndHour, 0)?,
    })
}

/// Infer date from day and month, get a "naive date", i.e., without time zone.
///
/// An announcement is for the day it was written or for the near future, so the year it was written in
/// is the answer - unless that date lies before it, which happens for a "05.01." announced in December.
fn inferDate(day: u32, month: u32, sentAt: DateTime<Local>) -> Option<NaiveDate> {
    let sentOn = sentAt.date_naive();
    let sameYear = NaiveDate::from_ymd_opt(sentOn.year(), month, day)?;
    if sameYear >= sentOn {
        return Some(sameYear);
    }
    NaiveDate::from_ymd_opt(sentOn.year() + 1, month, day)
}

/// Local time of a date, tolerant against the hour not existing due to a DST switch.
///
/// "24:00" is midnight at the *end* of the date, which is how an announcement and a `fullDayEndHour`
/// of 24 name it - `NaiveDate` only knows the hours 0 to 23, where that is the next day's 00:00.
fn toLocal(date: NaiveDate, hour: u32, minute: u32) -> Option<DateTime<Local>> {
    let (date, hour) = if hour == 24 && minute == 0 { (date.succ_opt()?, 0) } else { (date, hour) };
    let naive = date.and_hms_opt(hour, minute, 0)?;
    // On the day of a DST switch a local time can be ambiguous or missing - the earliest matching
    // instant is the safe choice for the start as well as for the end of a window.
    Local.from_local_datetime(&naive).earliest()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::textNormalize::normalizeForParsing;

    const TODAY_PATTERN: &str = "heute zwischen {HH:MM} - {HH:MM} Uhr durch Ihre Paketzustellkraft zugestellt";
    const FUTURE_PATTERN: &str = "am {Weekday}, den {DD.MM}";
    // The Amazon patterns: a precise window for the day of writing, and a day named relative to it.
    const AMAZON_TODAY_PATTERN: &str = "Zustellung heute {HH:MM} - {HH:MM}";
    const AMAZON_RELATIVE_PATTERN: &str = "Ankunft {RelativeDay}";

    /// Send time of the mails under test. 2026-08-11 is a Tuesday, so the "Mittwoch, den 12.08." of
    /// the real mail is consistent with it.
    fn testSentAt() -> DateTime<Local> {
        localTime(2026, 8, 11, 10, 0)
    }

    fn localTime(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> DateTime<Local> {
        let naive = NaiveDate::from_ymd_opt(year, month, day).unwrap().and_hms_opt(hour, minute, 0).unwrap();
        Local.from_local_datetime(&naive).earliest().unwrap()
    }

    fn testRules() -> WindowRules {
        WindowRules {
            tolerance: Duration::minutes(120),
            fullDayStartHour: 8,
            fullDayEndHour: 20,
        }
    }

    fn patternsOf(today: Option<&str>, relativeDay: Option<&str>, future: Option<&str>) -> DatePatterns {
        let compile = |pattern: Option<&str>| pattern.map(|p| DatePattern::new(p).unwrap());
        DatePatterns {
            today: compile(today),
            relativeDay: compile(relativeDay),
            future: compile(future),
        }
    }

    /// Runs the whole chain a DHL mail goes through: normalization, then the patterns.
    fn windowOf(rawText: &str, sentAt: DateTime<Local>) -> Option<DeliveryWindow> {
        let patterns = patternsOf(Some(TODAY_PATTERN), None, Some(FUTURE_PATTERN));
        findWindow(&normalizeForParsing(rawText), &patterns, &testRules(), sentAt)
    }

    /// The same for an Amazon mail.
    fn amazonWindowOf(rawText: &str, sentAt: DateTime<Local>) -> Option<DeliveryWindow> {
        let patterns = patternsOf(Some(AMAZON_TODAY_PATTERN), Some(AMAZON_RELATIVE_PATTERN), None);
        findWindow(&normalizeForParsing(rawText), &patterns, &testRules(), sentAt)
    }

    #[test]
    fn futureAnnouncementSpansTheWholeAnnouncedDay() {
        // Example one, exactly as it stands in the raw mail source.
        let raw = "am Mittwoch, den 12.08.\r\n=C2=A0**\r\n\r\nzugestellt.";
        let window = windowOf(raw, testSentAt()).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 8, 12, 8, 0));
        assert_eq!(window.to, localTime(2026, 8, 12, 20, 0));
        assert_eq!(window.to_string(), "2026-08-12 08:00 - 2026-08-12 20:00");
    }

    #[test]
    fn todayAnnouncementAddsToleranceOnBothSides() {
        // Example two: 13:45 - 15:15 with 120 minutes tolerance.
        let raw = "**heute zwischen 13:45=C2=A0 - 15:15 Uhr=C2=A0**\r\ndurch Ihre Paketzustellkraft zugestellt.";
        let window = windowOf(raw, testSentAt()).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 8, 11, 11, 45));
        assert_eq!(window.to, localTime(2026, 8, 11, 17, 15));
        assert_eq!(window.to_string(), "2026-08-11 11:45 - 2026-08-11 17:15");
    }

    #[test]
    fn todayAnnouncementWinsOverFutureOne() {
        // A mail may contain both sentences; the precise one for today has to be used.
        let raw = "am Mittwoch, den 12.08. zugestellt. \
                   **heute zwischen 13:45=C2=A0 - 15:15 Uhr=C2=A0** durch Ihre Paketzustellkraft zugestellt.";
        let window = windowOf(raw, testSentAt()).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 8, 11, 11, 45));
    }

    #[test]
    fn dayAndMonthInDecemberRollOverToNextYear() {
        // Announced on 2026-12-28 for the 5th of January - that is 2027.
        let window = windowOf("am Dienstag, den 05.01. zugestellt.", localTime(2026, 12, 28, 10, 0)).expect("window has to be found");
        assert_eq!(window.from, localTime(2027, 1, 5, 8, 0));
        assert_eq!(window.to, localTime(2027, 1, 5, 20, 0));
    }

    #[test]
    fn dayAndMonthOfTheSendDayStaysInThatYear() {
        let window = windowOf("am Dienstag, den 11.08. zugestellt.", testSentAt()).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 8, 11, 8, 0));
    }

    #[test]
    fn announcementSentBeforeMidnightStaysOnItsOwnDay() {
        // A mail written at 23:55 for its own day is only read in the next poll cycle - possibly after
        // midnight. Its send time keeps the announcement on the 11th instead of pushing it a year out.
        let sentAt = localTime(2026, 8, 11, 23, 55);
        let window = windowOf("am Dienstag, den 11.08. zugestellt.", sentAt).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 8, 11, 8, 0));
        assert_eq!(window.to, localTime(2026, 8, 11, 20, 0));
    }

    #[test]
    fn heuteMeansTheDayTheMailWasSent() {
        // "heute" is resolved against the send time, so the window does not move when the mail is
        // picked up on the following day.
        let raw = "**heute zwischen 13:45=C2=A0 - 15:15 Uhr=C2=A0**\r\ndurch Ihre Paketzustellkraft zugestellt.";
        let window = windowOf(raw, localTime(2026, 8, 11, 23, 55)).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 8, 11, 11, 45));
        assert_eq!(window.to, localTime(2026, 8, 11, 17, 15));
    }

    #[test]
    fn dayAndMonthAtTheTurnOfTheYearStaysInTheSendYear() {
        // The mirror case of the December roll-over: a mail sent on 2025-12-31 - a Wednesday - for that
        // same day must not be read as 2026-12-31, which is a Thursday and would fail the weekday check.
        let window = windowOf("am Mittwoch, den 31.12. zugestellt.", localTime(2025, 12, 31, 9, 0)).expect("window has to be found");
        assert_eq!(window.from, localTime(2025, 12, 31, 8, 0));
    }

    #[test]
    fn fullDayHoursComeFromTheRules() {
        let patterns = patternsOf(None, None, Some(FUTURE_PATTERN));
        let rules = WindowRules {
            tolerance: Duration::minutes(0),
            fullDayStartHour: 6,
            fullDayEndHour: 22,
        };
        let window = findWindow("am Mittwoch, den 12.08.", &patterns, &rules, testSentAt()).unwrap();
        assert_eq!(window.from, localTime(2026, 8, 12, 6, 0));
        assert_eq!(window.to, localTime(2026, 8, 12, 22, 0));
    }

    // ---------------------- The allowed hours as a hard bound ----------------------

    #[test]
    fn aLateAnnouncementIsCutBackToTheEveningInsteadOfWrapping() {
        // "heute zwischen 15:00 - 24:00" plus 120 minutes tolerance would run to 02:00 of the next
        // day. The door must not stand open at night, so the window ends at fullDayEndHour.
        let raw = "heute zwischen 15:00 - 24:00 Uhr durch Ihre Paketzustellkraft zugestellt.";
        let window = windowOf(raw, testSentAt()).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 8, 11, 13, 0));
        assert_eq!(window.to, localTime(2026, 8, 11, 20, 0));
    }

    #[test]
    fn anEarlyAnnouncementIsCutBackToTheMorning() {
        // 07:00 - 09:00 minus the tolerance would start at 05:00.
        let raw = "heute zwischen 07:00 - 09:00 Uhr durch Ihre Paketzustellkraft zugestellt.";
        let window = windowOf(raw, testSentAt()).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 8, 11, 8, 0));
        assert_eq!(window.to, localTime(2026, 8, 11, 11, 0));
    }

    #[test]
    fn anAnnouncementOutsideTheAllowedHoursIsRefused() {
        // 23:00 - 23:30 is 21:00 - 01:30 with the tolerance, and nothing at all once it is cut back
        // to 20:00 - so no window, and no rule.
        let raw = "heute zwischen 23:00 - 23:30 Uhr durch Ihre Paketzustellkraft zugestellt.";
        assert_eq!(windowOf(raw, testSentAt()), None);
    }

    #[test]
    fn anAnnouncementEndingAtMidnightIsCutBackToTheEvening() {
        // "24:00" is the end of the day, not an hour NaiveTime knows - without the roll-over the
        // whole announcement would be dropped silently.
        let raw = "heute zwischen 20:00 - 24:00 Uhr durch Ihre Paketzustellkraft zugestellt.";
        let window = windowOf(raw, testSentAt()).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 8, 11, 18, 0));
        assert_eq!(window.to, localTime(2026, 8, 11, 20, 0));
    }

    #[test]
    fn anEndHourOfMidnightLeavesTheEveningOpen() {
        // A config that does not want an evening bound at all: 24 is the end of the day.
        let patterns = patternsOf(Some(TODAY_PATTERN), None, None);
        let rules = WindowRules {
            tolerance: Duration::minutes(120),
            fullDayStartHour: 8,
            fullDayEndHour: 24,
        };
        let raw = normalizeForParsing("heute zwischen 20:00 - 22:00 Uhr durch Ihre Paketzustellkraft zugestellt.");
        let window = findWindow(&raw, &patterns, &rules, testSentAt()).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 8, 11, 18, 0));
        assert_eq!(window.to, localTime(2026, 8, 12, 0, 0));
    }

    #[test]
    fn aWindowInsideTheAllowedHoursIsLeftAlone() {
        // The ordinary case: nothing is narrowed, so the tolerance is kept in full.
        let raw = "heute zwischen 13:45 - 15:15 Uhr durch Ihre Paketzustellkraft zugestellt.";
        let window = windowOf(raw, testSentAt()).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 8, 11, 11, 45));
        assert_eq!(window.to, localTime(2026, 8, 11, 17, 15));
    }

    #[test]
    fn theBoundFollowsTheConfiguredHours() {
        // The same announcement, with a config that allows the door to stay open until 22:00.
        let patterns = patternsOf(Some(TODAY_PATTERN), None, None);
        let rules = WindowRules {
            tolerance: Duration::minutes(120),
            fullDayStartHour: 6,
            fullDayEndHour: 22,
        };
        let raw = normalizeForParsing("heute zwischen 15:00 - 24:00 Uhr durch Ihre Paketzustellkraft zugestellt.");
        let window = findWindow(&raw, &patterns, &rules, testSentAt()).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 8, 11, 13, 0));
        assert_eq!(window.to, localTime(2026, 8, 11, 22, 0));
    }

    #[test]
    fn theBoundBelongsToTheDayTheWindowStartsOn() {
        // An announcement for the next day is bounded by that day's hours, not by the send day's.
        let raw = "am Mittwoch, den 12.08.";
        let window = windowOf(raw, localTime(2026, 8, 11, 19, 0)).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 8, 12, 8, 0));
        assert_eq!(window.to, localTime(2026, 8, 12, 20, 0));
    }

    #[test]
    fn missingPatternsYieldNoWindow() {
        // A crawler without configured date patterns reports no window instead of failing.
        assert_eq!(
            findWindow("am Mittwoch, den 12.08.", &DatePatterns::default(), &testRules(), testSentAt()),
            None
        );
    }

    // ---------------------------- Amazon ----------------------------

    #[test]
    fn amazonTodayAnnouncementAddsToleranceOnBothSides() {
        // The "In Zustellung" mail of shipment-tracking@amazon.de, sent 2026-08-28 12:04.
        let raw = "Zustellung heute 15:15 - 18:15\r\nPatrick \u{2013} M\u{fc}nchen";
        let sentAt = localTime(2026, 8, 28, 12, 4);
        let window = amazonWindowOf(raw, sentAt).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 8, 28, 13, 15));
        // 20:15 with the tolerance, but the door is never opened past fullDayEndHour.
        assert_eq!(window.to, localTime(2026, 8, 28, 20, 0));
    }

    #[test]
    fn amazonArrivalTomorrowIsTheWholeNextDay() {
        // The "Versendet" mail of versandbestaetigung@amazon.de, sent 2026-08-27 14:23: the parcel
        // arrives the day *after* the mail was written, so the send time is what decides.
        let raw = "Ankunft morgen\r\n\r\n\r\n\r\nPatrick \u{2013} M\u{fc}nchen\r\n\r\nBestellnr.\r\n305-3203050-2444367";
        let sentAt = localTime(2026, 8, 27, 14, 23);
        let window = amazonWindowOf(raw, sentAt).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 8, 28, 8, 0));
        assert_eq!(window.to, localTime(2026, 8, 28, 20, 0));
    }

    #[test]
    fn amazonArrivalTomorrowCrossesTheMonthEnd() {
        // "morgen" is a day of arithmetic, not a string: the last day of a month rolls over.
        let window = amazonWindowOf("Ankunft morgen", localTime(2026, 8, 31, 14, 23)).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 9, 1, 8, 0));
    }

    #[test]
    fn amazonArrivalTodayIsTheSendDay() {
        let window = amazonWindowOf("Ankunft heute", localTime(2026, 8, 27, 14, 23)).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 8, 27, 8, 0));
        assert_eq!(window.to, localTime(2026, 8, 27, 20, 0));
    }

    #[test]
    fn amazonPreciseWindowWinsOverTheRelativeDay() {
        // A mail carrying both has to be read as the precise one.
        let raw = "Ankunft heute\r\nZustellung heute 15:15 - 18:15";
        let window = amazonWindowOf(raw, localTime(2026, 8, 28, 12, 4)).expect("window has to be found");
        assert_eq!(window.from, localTime(2026, 8, 28, 13, 15));
    }

    #[test]
    fn amazonPatternsDoNotMatchADhlMail() {
        let raw = "**heute zwischen 13:45=C2=A0 - 15:15 Uhr=C2=A0**\r\ndurch Ihre Paketzustellkraft zugestellt.";
        assert_eq!(amazonWindowOf(raw, testSentAt()), None);
    }

    #[test]
    fn dhlPatternsDoNotMatchAnAmazonMail() {
        assert_eq!(windowOf("Zustellung heute 15:15 - 18:15", testSentAt()), None);
        assert_eq!(windowOf("Ankunft morgen", testSentAt()), None);
    }
}
