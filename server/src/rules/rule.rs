use chrono::{DateTime, Local, NaiveDate, NaiveTime};

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
    pub startDate: Option<NaiveDate>, // no lower date limit if None
    pub endDate: Option<NaiveDate>,   // no upper date limit if None
    pub activationTime: NaiveTime,
    pub deactivationTime: NaiveTime,
}

impl Rule {
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
    fn dateFieldTellsEmptyAndInvalidApart() {
        assert_eq!(Rule::parseDateField(""), Some(None));
        assert_eq!(Rule::parseDateField("  "), Some(None));
        assert_eq!(Rule::parseDateField("2026-08-08"), Some(Some(date("2026-08-08"))));
        assert_eq!(Rule::parseDateField("08/08/2026"), None);
        assert_eq!(Rule::parseDateField("xyz"), None);
    }
}
