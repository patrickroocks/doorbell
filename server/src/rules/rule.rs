use chrono::{DateTime, Local, NaiveDate, NaiveTime, TimeZone};

// Date/time string formats used both in the SQLite store and the web form.
pub const DATE_FORMAT: &str = "%Y-%m-%d";
pub const TIME_FORMAT: &str = "%H:%M";

// The only rule type that currently exists. Stored as a discriminator so more types can be
// added later without a schema migration.
pub const RULE_TYPE_AUTO_BUZZ: &str = "autoBuzz";

// A single automation rule. Currently always an "auto-buzz" rule: it enables the device
// auto-buzzer during its daily [activationTime, deactivationTime] window (within the optional
// date range) and reacts to rings inside that window.
#[derive(Clone, Debug)]
pub struct Rule {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    pub autoAck: bool,
    pub singleShot: bool,
    // Rule deletes itself once its last window has ended, see `deletionTimeAt`. Meant for rules
    // created by another service (delivery announcements), which nobody would clean up by hand.
    pub deleteAtEnd: bool,
    pub startDate: Option<NaiveDate>, // no lower date limit if None
    pub endDate: Option<NaiveDate>,   // no upper date limit if None
    pub activationTime: NaiveTime,
    pub deactivationTime: NaiveTime,
}

impl Rule {
    // The moment the rule deletes itself, or None if it never does - either because the flag is not
    // set, or because there is no end date to end at.
    //
    // For a window running past midnight (deactivation time not after the activation time) the
    // deactivation belongs to the day *after* the end date, the same reasoning as in `isActiveAt`:
    // the end date is the day the last window starts on.
    pub fn deletionTimeAt(&self) -> Option<DateTime<Local>> {
        if !self.deleteAtEnd {
            return None;
        }
        let endDate = self.endDate?;
        let deletionDate = if self.activationTime <= self.deactivationTime {
            endDate
        } else {
            endDate.succ_opt()?
        };
        // An ambiguous or missing local time due to a DST switch must not swallow the deletion, so
        // the earliest matching instant is used instead of unwrapping.
        Local.from_local_datetime(&deletionDate.and_time(self.deactivationTime)).earliest()
    }

    // Builds a rule from the string fields of the web form or the API, applying all validation both
    // of them need. The error is the message shown to the user / returned to the caller.
    //
    // The name is expected to be resolved already: an empty one is replaced by the store's
    // `nextUnnamedName`, which is not reachable from here.
    #[allow(clippy::too_many_arguments)]
    pub fn fromParts(
        id: Option<i64>,
        name: String,
        enabled: bool,
        autoAck: bool,
        singleShot: bool,
        deleteAtEnd: bool,
        startDate: &str,
        endDate: &str,
        activationTime: &str,
        deactivationTime: &str,
    ) -> Result<Rule, String> {
        // Activation and deactivation times are required; equal times would describe a zero-length
        // window, i.e. a rule that can never activate.
        let (activation, deactivation) = match (Self::parseTime(activationTime), Self::parseTime(deactivationTime)) {
            (Some(a), Some(d)) if a != d => (a, d),
            (Some(_), Some(_)) => return Err("Activation and deactivation time must differ".to_string()),
            _ => return Err("Activation and deactivation time are required".to_string()),
        };

        // Both dates are optional ("no limit"), but an unparsable one is rejected instead of being
        // treated as "no limit", which would widen the rule instead of failing.
        let (startDate, endDate) = match (Self::parseDateField(startDate), Self::parseDateField(endDate)) {
            (Some(s), Some(e)) => (s, e),
            _ => return Err("Start and end date must be empty or a valid date".to_string()),
        };
        if let (Some(start), Some(end)) = (startDate, endDate)
            && end < start
        {
            return Err("End date is earlier than start date".to_string());
        }
        // Without an end date there is no end to delete at, so the flag would silently do nothing.
        if deleteAtEnd && endDate.is_none() {
            return Err("A rule that is deleted at the end needs an end date".to_string());
        }

        Ok(Rule {
            id: id.unwrap_or(0),
            name,
            enabled,
            autoAck,
            singleShot,
            deleteAtEnd,
            startDate,
            endDate,
            activationTime: activation,
            deactivationTime: deactivation,
        })
    }

    // Whether the rule should be active at the given local time: enabled, the time of day is within
    // the daily window, and the day that window started on is within [startDate, endDate].
    //
    // The date range is checked against the *start* of the window, not against `now`. That only
    // makes a difference for a window running past midnight (deactivation time not after the
    // activation time, e.g. 23:00 -> 03:00), whose second half belongs to the window started on the
    // previous day. Checking `now` instead would let such a rule run on the end date's evening as
    // well, i.e. one whole day longer than intended.
    pub fn isActiveAt(&self, now: DateTime<Local>) -> bool {
        if !self.enabled {
            return false;
        }

        let Some(windowStartDate) = self.windowStartDate(now) else {
            return false;
        };
        if let Some(start) = self.startDate
            && windowStartDate < start
        {
            return false;
        }
        if let Some(end) = self.endDate
            && windowStartDate > end
        {
            return false;
        }
        true
    }

    // The day the currently running window started on, or None if the given time is outside the
    // daily window altogether.
    fn windowStartDate(&self, now: DateTime<Local>) -> Option<NaiveDate> {
        let date = now.date_naive();
        let time = now.time();
        if self.activationTime <= self.deactivationTime {
            // Same-day window. Equal times describe a zero-length window, which is never active.
            (self.activationTime <= time && time < self.deactivationTime).then_some(date)
        } else if time >= self.activationTime {
            // Wrap-around window, first half: started today and runs until midnight.
            Some(date)
        } else if time < self.deactivationTime {
            // Wrap-around window, second half: belongs to the window started yesterday.
            date.pred_opt()
        } else {
            None
        }
    }

    // --------------------------- String conversions ---------------------------

    pub fn startDateStr(&self) -> String {
        Self::dateToStr(self.startDate)
    }

    pub fn endDateStr(&self) -> String {
        Self::dateToStr(self.endDate)
    }

    pub fn activationTimeStr(&self) -> String {
        self.activationTime.format(TIME_FORMAT).to_string()
    }

    pub fn deactivationTimeStr(&self) -> String {
        self.deactivationTime.format(TIME_FORMAT).to_string()
    }

    // An empty string means "no limit" (None).
    pub fn parseDate(value: &str) -> Option<NaiveDate> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return None;
        }
        NaiveDate::parse_from_str(trimmed, DATE_FORMAT).ok()
    }

    // Parses a date field of the web form, telling "no limit" and "invalid" apart: `Some(None)` is
    // an empty field, `None` is an unparsable value. Without that distinction a garbled date would
    // silently widen the rule instead of being rejected.
    pub fn parseDateField(value: &str) -> Option<Option<NaiveDate>> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Some(None);
        }
        NaiveDate::parse_from_str(trimmed, DATE_FORMAT).ok().map(Some)
    }

    pub fn parseTime(value: &str) -> Option<NaiveTime> {
        NaiveTime::parse_from_str(value.trim(), TIME_FORMAT).ok()
    }

    fn dateToStr(date: Option<NaiveDate>) -> String {
        date.map(|d| d.format(DATE_FORMAT).to_string()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, DATE_FORMAT).unwrap()
    }

    fn time(text: &str) -> NaiveTime {
        NaiveTime::parse_from_str(text, TIME_FORMAT).unwrap()
    }

    // Local time from "YYYY-MM-DD HH:MM". All test dates are far from a DST switch, so the mapping
    // is unambiguous.
    fn at(text: &str) -> DateTime<Local> {
        let naive = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").unwrap();
        Local.from_local_datetime(&naive).unwrap()
    }

    fn rule(activation: &str, deactivation: &str, startDate: Option<&str>, endDate: Option<&str>) -> Rule {
        Rule {
            id: 1,
            name: "test".to_string(),
            enabled: true,
            autoAck: false,
            singleShot: false,
            deleteAtEnd: false,
            startDate: startDate.map(date),
            endDate: endDate.map(date),
            activationTime: time(activation),
            deactivationTime: time(deactivation),
        }
    }

    #[test]
    fn sameDayWindowIsActiveInsideTheTimesOnly() {
        let r = rule("10:00", "14:00", None, None);
        assert!(!r.isActiveAt(at("2026-08-08 09:59")));
        assert!(r.isActiveAt(at("2026-08-08 10:00")));
        assert!(r.isActiveAt(at("2026-08-08 13:59")));
        // The deactivation time itself is already outside the window.
        assert!(!r.isActiveAt(at("2026-08-08 14:00")));
    }

    #[test]
    fn sameDayWindowRespectsTheDateRange() {
        let r = rule("10:00", "14:00", Some("2026-08-08"), Some("2026-08-09"));
        assert!(!r.isActiveAt(at("2026-08-07 12:00")));
        assert!(r.isActiveAt(at("2026-08-08 12:00")));
        assert!(r.isActiveAt(at("2026-08-09 12:00")));
        assert!(!r.isActiveAt(at("2026-08-10 12:00")));
    }

    #[test]
    fn wrapAroundWindowRunsIntoTheNextMorningButNotIntoTheNextEvening() {
        // Single day: the window starts on 08-08 and ends on the morning of 08-09.
        let r = rule("23:00", "03:00", Some("2026-08-08"), Some("2026-08-08"));
        assert!(!r.isActiveAt(at("2026-08-08 22:59")));
        assert!(r.isActiveAt(at("2026-08-08 23:30")));
        assert!(r.isActiveAt(at("2026-08-09 02:59")));
        assert!(!r.isActiveAt(at("2026-08-09 03:00")));
        // The regression this fix is about: no second evening.
        assert!(!r.isActiveAt(at("2026-08-09 23:30")));
        // ... and no morning before the start date either.
        assert!(!r.isActiveAt(at("2026-08-08 02:00")));
    }

    #[test]
    fn wrapAroundWindowRepeatsOnEveryDayOfTheRange() {
        let r = rule("23:00", "03:00", Some("2026-08-08"), Some("2026-08-10"));
        assert!(r.isActiveAt(at("2026-08-09 23:30")));
        assert!(r.isActiveAt(at("2026-08-10 23:30")));
        // The last window still ends on the morning after the end date.
        assert!(r.isActiveAt(at("2026-08-11 02:00")));
        assert!(!r.isActiveAt(at("2026-08-11 23:30")));
    }

    #[test]
    fn openEndedRangesHaveNoLimit() {
        let r = rule("23:00", "03:00", None, None);
        assert!(r.isActiveAt(at("2020-01-01 23:30")));
        assert!(r.isActiveAt(at("2030-01-01 02:00")));
        assert!(!r.isActiveAt(at("2030-01-01 12:00")));
    }

    #[test]
    fn equalTimesAreNeverActive() {
        let r = rule("10:00", "10:00", None, None);
        assert!(!r.isActiveAt(at("2026-08-08 10:00")));
        assert!(!r.isActiveAt(at("2026-08-08 09:59")));
        assert!(!r.isActiveAt(at("2026-08-08 23:59")));
    }

    #[test]
    fn disabledRuleIsNeverActive() {
        let mut r = rule("10:00", "14:00", None, None);
        r.enabled = false;
        assert!(!r.isActiveAt(at("2026-08-08 12:00")));
    }

    #[test]
    fn deletionTimeIsTheEndOfTheLastWindow() {
        let mut r = rule("10:00", "14:00", Some("2026-08-08"), Some("2026-08-09"));
        // Without the flag the rule stays forever, no matter the dates.
        assert_eq!(r.deletionTimeAt(), None);
        r.deleteAtEnd = true;
        assert_eq!(r.deletionTimeAt(), Some(at("2026-08-09 14:00")));
    }

    #[test]
    fn deletionTimeOfWrapAroundWindowIsOnTheDayAfterTheEndDate() {
        // The last window starts on 08-09 and ends on the morning of 08-10.
        let mut r = rule("23:00", "03:00", Some("2026-08-08"), Some("2026-08-09"));
        r.deleteAtEnd = true;
        assert_eq!(r.deletionTimeAt(), Some(at("2026-08-10 03:00")));
        // The rule is still active shortly before its deletion, so the two agree.
        assert!(r.isActiveAt(at("2026-08-10 02:59")));
        assert!(!r.isActiveAt(at("2026-08-10 03:00")));
    }

    #[test]
    fn withoutEndDateThereIsNothingToDeleteAt() {
        let mut r = rule("10:00", "14:00", Some("2026-08-08"), None);
        r.deleteAtEnd = true;
        assert_eq!(r.deletionTimeAt(), None);
    }

    #[test]
    fn fromPartsAcceptsAValidRule() {
        let r = Rule::fromParts(
            Some(7),
            "delivery".to_string(),
            true,
            true,
            false,
            true,
            "2026-08-12",
            "2026-08-12",
            "11:45",
            "17:15",
        )
        .unwrap();
        assert_eq!(r.id, 7);
        assert!(r.deleteAtEnd);
        assert_eq!(r.startDate, Some(date("2026-08-12")));
        assert_eq!(r.activationTime, time("11:45"));
        assert_eq!(r.deletionTimeAt(), Some(at("2026-08-12 17:15")));
    }

    #[test]
    fn fromPartsRejectsDeleteAtEndWithoutEndDate() {
        let error = Rule::fromParts(None, "x".to_string(), true, false, false, true, "2026-08-12", "", "11:45", "17:15").unwrap_err();
        assert!(error.contains("end date"), "unexpected error: {}", error);
    }

    #[test]
    fn fromPartsRejectsBadTimesAndDates() {
        let parts = |start: &str, end: &str, act: &str, deact: &str| {
            Rule::fromParts(None, "x".to_string(), true, false, false, false, start, end, act, deact)
        };
        assert!(parts("", "", "", "").is_err());
        assert!(parts("", "", "10:00", "10:00").unwrap_err().contains("differ"));
        assert!(parts("nonsense", "", "10:00", "14:00").is_err());
        assert!(parts("2026-08-10", "2026-08-09", "10:00", "14:00").unwrap_err().contains("earlier"));
        // Open ended ranges stay allowed as long as deleteAtEnd is off.
        assert!(parts("", "", "10:00", "14:00").is_ok());
    }

    #[test]
    fn dateFieldTellsEmptyAndInvalidApart() {
        assert_eq!(Rule::parseDateField(""), Some(None));
        assert_eq!(Rule::parseDateField("  "), Some(None));
        assert_eq!(Rule::parseDateField("2026-08-08"), Some(Some(date("2026-08-08"))));
        assert_eq!(Rule::parseDateField("08/08/2026"), None);
        assert_eq!(Rule::parseDateField("xyz"), None);
    }
}
