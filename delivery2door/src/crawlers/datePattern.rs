use regex::{Captures, Regex};
use std::fmt;

/// The German weekday names, in the order of `chrono::Weekday::num_days_from_monday`.
pub const WEEKDAY_NAMES: [&str; 7] = ["Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag", "Sonntag"];

/// The German words for a day relative to the day a message was written, and how many days ahead each
/// of them is. Longest first, so the alternation cannot match "morgen" inside "uebermorgen".
pub const RELATIVE_DAY_NAMES: [(&str, i64); 3] = [("\u{fc}bermorgen", 2), ("morgen", 1), ("heute", 0)];

/// Why a configured date pattern could not be compiled.
#[derive(Debug)]
pub enum PatternError {
    UnknownPlaceholder(String),
    UnclosedPlaceholder,
    Regex(regex::Error),
}

impl fmt::Display for PatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PatternError::UnknownPlaceholder(name) => {
                write!(
                    f,
                    "unknown placeholder \"{{{}}}\" - known are {{Weekday}}, {{RelativeDay}}, {{DD.MM}} and {{HH:MM}}",
                    name
                )
            }
            PatternError::UnclosedPlaceholder => write!(f, "a \"{{\" is not closed by a \"}}\""),
            PatternError::Regex(e) => write!(f, "resulting regex is invalid: {}", e),
        }
    }
}

/// A date or time announcement pattern from the config, compiled into a regex.
///
/// The config does not contain a regex but readable text with placeholders, e.g.
/// "am {Weekday}, den {DD.MM}". Literal text is matched verbatim, except that a run of spaces
/// matches any whitespace - mails put line breaks and non-breaking spaces in the middle of a
/// sentence, and the normalization cannot always reduce them to exactly one space.
///
/// The captured values are numbered by their occurrence, so a pattern can contain two times:
/// "zwischen {HH:MM} - {HH:MM} Uhr" fills `hour1`/`minute1` and `hour2`/`minute2`.
#[derive(Debug)]
pub struct DatePattern {
    pattern: String,
    regex: Regex,
    placeholderCount: usize,
}

/// One match of a `DatePattern`, with the placeholder values it captured.
pub struct DateMatch<'a> {
    captures: Captures<'a>,
    placeholderCount: usize,
}

impl DatePattern {
    pub fn new(pattern: &str) -> Result<Self, PatternError> {
        let mut regexStr = String::new();
        let mut placeholderCount = 0usize;
        let mut rest = pattern;

        while let Some(start) = rest.find('{') {
            appendLiteral(&mut regexStr, &rest[..start]);
            let afterBrace = &rest[start + 1..];
            let end = afterBrace.find('}').ok_or(PatternError::UnclosedPlaceholder)?;
            let name = &afterBrace[..end];

            placeholderCount += 1;
            let index = placeholderCount;
            match name {
                "Weekday" => regexStr.push_str(&format!("(?P<weekday{}>{})", index, WEEKDAY_NAMES.join("|"))),
                "RelativeDay" => {
                    let names: Vec<&str> = RELATIVE_DAY_NAMES.iter().map(|(name, _)| *name).collect();
                    regexStr.push_str(&format!("(?P<relativeDay{}>{})", index, names.join("|")))
                }
                "DD.MM" => regexStr.push_str(&format!(r"(?P<day{}>\d{{1,2}})\.(?P<month{}>\d{{1,2}})", index, index)),
                "HH:MM" => regexStr.push_str(&format!(r"(?P<hour{}>\d{{1,2}}):(?P<minute{}>\d{{2}})", index, index)),
                other => return Err(PatternError::UnknownPlaceholder(other.to_string())),
            }
            rest = &afterBrace[end + 1..];
        }
        appendLiteral(&mut regexStr, rest);

        Ok(DatePattern {
            pattern: pattern.to_string(),
            regex: Regex::new(&regexStr).map_err(PatternError::Regex)?,
            placeholderCount,
        })
    }

    /// The pattern as it was configured, for log messages.
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub fn findIn<'a>(&self, text: &'a str) -> Option<DateMatch<'a>> {
        self.regex.captures(text).map(|captures| DateMatch {
            captures,
            placeholderCount: self.placeholderCount,
        })
    }
}

impl DateMatch<'_> {
    /// The weekday name, if the pattern contains a `{Weekday}`.
    ///
    /// The placeholders are looked up by scanning the occurrences, so where exactly the placeholder
    /// sits in the pattern does not matter.
    pub fn weekday(&self) -> Option<&str> {
        (1..=self.placeholderCount).find_map(|index| self.named(&format!("weekday{}", index)))
    }

    /// How many days ahead the named relative day is, if the pattern contains a `{RelativeDay}`.
    pub fn relativeDayOffset(&self) -> Option<i64> {
        let name = (1..=self.placeholderCount).find_map(|index| self.named(&format!("relativeDay{}", index)))?;
        RELATIVE_DAY_NAMES.iter().find(|(candidate, _)| *candidate == name).map(|(_, offset)| *offset)
    }

    /// Day and month, if the pattern contains a `{DD.MM}`.
    pub fn dayAndMonth(&self) -> Option<(u32, u32)> {
        (1..=self.placeholderCount).find_map(|index| {
            let day = self.namedNumber(&format!("day{}", index))?;
            let month = self.namedNumber(&format!("month{}", index))?;
            Some((day, month))
        })
    }

    /// All times of the pattern, in the order of their `{HH:MM}` placeholders.
    pub fn times(&self) -> Vec<(u32, u32)> {
        (1..=self.placeholderCount)
            .filter_map(|index| {
                let hour = self.namedNumber(&format!("hour{}", index))?;
                let minute = self.namedNumber(&format!("minute{}", index))?;
                Some((hour, minute))
            })
            .collect()
    }

    /// The whole matched text, for log messages.
    pub fn matchedText(&self) -> &str {
        self.captures.get(0).map_or("", |m| m.as_str())
    }

    fn named(&self, name: &str) -> Option<&str> {
        self.captures.name(name).map(|m| m.as_str())
    }

    fn namedNumber(&self, name: &str) -> Option<u32> {
        // The groups only ever match digits, so a parse failure is impossible in practice.
        self.named(name).and_then(|s| s.parse().ok())
    }
}

/// Appends literal pattern text to the regex, letting every run of spaces match any whitespace.
///
/// `\s` covers the non-breaking space too, so a pattern also matches text that was not normalized.
fn appendLiteral(regexStr: &mut String, literal: &str) {
    if literal.is_empty() {
        return;
    }
    // Whitespace only - it still separates two placeholders, e.g. in "{HH:MM} {HH:MM}".
    if literal.trim().is_empty() {
        regexStr.push_str(r"\s+");
        return;
    }

    if literal.starts_with(char::is_whitespace) {
        regexStr.push_str(r"\s+");
    }
    for (index, chunk) in literal.split_whitespace().enumerate() {
        if index > 0 {
            regexStr.push_str(r"\s+");
        }
        regexStr.push_str(&regex::escape(chunk));
    }
    if literal.ends_with(char::is_whitespace) {
        regexStr.push_str(r"\s+");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matchesFuturePatternDespiteWhitespaceNoise() {
        let pattern = DatePattern::new("am {Weekday}, den {DD.MM}").unwrap();
        let found = pattern.findIn("voraussichtlich am Mittwoch, den 12.08. zugestellt.").unwrap();
        assert_eq!(found.weekday(), Some("Mittwoch"));
        assert_eq!(found.dayAndMonth(), Some((12, 8)));
        assert!(found.times().is_empty());
    }

    #[test]
    fn matchesTwoTimesOfTodayPattern() {
        let pattern = DatePattern::new("heute zwischen {HH:MM} - {HH:MM} Uhr durch Ihre Paketzustellkraft zugestellt").unwrap();
        let found = pattern.findIn("heute zwischen 13:45 - 15:15 Uhr durch Ihre Paketzustellkraft zugestellt.").unwrap();
        assert_eq!(found.times(), vec![(13, 45), (15, 15)]);
        assert_eq!(found.weekday(), None);
        assert_eq!(found.dayAndMonth(), None);
    }

    #[test]
    fn findsPlaceholdersRegardlessOfTheirPosition() {
        // Reversed order compared to the configured pattern: {DD.MM} is the first placeholder here,
        // the second one in "am {Weekday}, den {DD.MM}".
        let pattern = DatePattern::new("den {DD.MM}., ein {Weekday}").unwrap();
        let found = pattern.findIn("den 12.08., ein Mittwoch").unwrap();
        assert_eq!(found.dayAndMonth(), Some((12, 8)));
        assert_eq!(found.weekday(), Some("Mittwoch"));
    }

    #[test]
    fn matchesRelativeDayOfTheAmazonPattern() {
        let pattern = DatePattern::new("Ankunft {RelativeDay}").unwrap();
        assert_eq!(pattern.findIn("Ankunft morgen").unwrap().relativeDayOffset(), Some(1));
        assert_eq!(pattern.findIn("Ankunft heute").unwrap().relativeDayOffset(), Some(0));
        // "morgen" must not be matched inside the longer word.
        assert_eq!(pattern.findIn("Ankunft \u{fc}bermorgen").unwrap().relativeDayOffset(), Some(2));
        assert!(pattern.findIn("Ankunft am Dienstag").is_none());
    }

    #[test]
    fn relativeDayIsAbsentInOtherPatterns() {
        let pattern = DatePattern::new("am {Weekday}, den {DD.MM}").unwrap();
        assert_eq!(pattern.findIn("am Mittwoch, den 12.08.").unwrap().relativeDayOffset(), None);
    }

    #[test]
    fn literalSpacesMatchAnyWhitespace() {
        // Even without normalization the pattern has to survive a line break in the sentence.
        let pattern = DatePattern::new("am {Weekday}, den {DD.MM}").unwrap();
        assert!(pattern.findIn("am\r\n  Mittwoch,\tden 12.08.").is_some());
    }

    #[test]
    fn doesNotMatchOtherText() {
        let pattern = DatePattern::new("heute zwischen {HH:MM} - {HH:MM} Uhr").unwrap();
        assert!(pattern.findIn("Ihre Sendung wurde von uns bearbeitet").is_none());
    }

    #[test]
    fn rejectsUnknownPlaceholder() {
        let error = DatePattern::new("am {Nonsense}").unwrap_err();
        assert!(matches!(error, PatternError::UnknownPlaceholder(name) if name == "Nonsense"));
    }

    #[test]
    fn rejectsUnclosedPlaceholder() {
        assert!(matches!(
            DatePattern::new("am {Weekday").unwrap_err(),
            PatternError::UnclosedPlaceholder
        ));
    }

    #[test]
    fn escapesRegexCharactersInLiteralText() {
        // "." and "(" are literal text in a pattern, not regex syntax.
        let pattern = DatePattern::new("(Zustellung) am {DD.MM}.").unwrap();
        assert!(pattern.findIn("(Zustellung) am 12.08.").is_some());
        assert!(pattern.findIn("XZustellungY am 12.08.").is_none());
    }
}
