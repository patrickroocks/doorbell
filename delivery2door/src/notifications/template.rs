use std::collections::BTreeMap;
use std::fmt;

/// Why a configured template cannot be used.
#[derive(Debug, PartialEq, Eq)]
pub enum TemplateError {
    UnknownPlaceholder(String),
    UnclosedPlaceholder,
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TemplateError::UnknownPlaceholder(name) => {
                write!(f, "unknown placeholder \"{{{}}}\" - known are {}", name, knownPlaceholders())
            }
            TemplateError::UnclosedPlaceholder => write!(f, "a \"{{\" is not closed by a \"}}\""),
        }
    }
}

/// What a notification can talk about. One value per placeholder a template may use.
///
/// A `BTreeMap` and not a struct with fields: the set of placeholders is what the config sees, so
/// having exactly one place that defines it is worth more than field access.
pub struct NotificationValues {
    values: BTreeMap<&'static str, String>,
}

/// Every placeholder a template may contain. A template using anything else is a config error, which
/// is reported at startup instead of silently leaving the word in the message.
pub const PLACEHOLDERS: [&str; 11] = [
    "event",
    "previousWindowNote",
    "carrier",
    "trackingNumber",
    "deliveryDate",
    "deliveryWindow",
    "previousWindow",
    "ruleId",
    "ruleName",
    "source",
    "doorbellServerAddress",
];

fn knownPlaceholders() -> String {
    PLACEHOLDERS.iter().map(|name| format!("{{{}}}", name)).collect::<Vec<_>>().join(", ")
}

impl NotificationValues {
    pub fn new() -> Self {
        // Every placeholder is present, so a template can never fail at send time - only the value
        // may be empty (e.g. the previous window of a rule that was just created).
        let values = PLACEHOLDERS.iter().map(|name| (*name, String::new())).collect();
        NotificationValues { values }
    }

    pub fn set(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.values.insert(name, value.into());
        self
    }

    fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(|value| value.as_str())
    }
}

/// Checks that a template only uses placeholders that exist. Called at startup for every template of
/// every sink, so a typo is found before the first delivery instead of showing up in a mail.
pub fn validate(template: &str) -> Result<(), TemplateError> {
    render(template, &NotificationValues::new()).map(|_| ())
}

/// Replaces every `{placeholder}` by its value.
pub fn render(template: &str, values: &NotificationValues) -> Result<String, TemplateError> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let afterBrace = &rest[start + 1..];
        let end = afterBrace.find('}').ok_or(TemplateError::UnclosedPlaceholder)?;
        let name = &afterBrace[..end];
        match values.get(name) {
            Some(value) => out.push_str(value),
            None => return Err(TemplateError::UnknownPlaceholder(name.to_string())),
        }
        rest = &afterBrace[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn testValues() -> NotificationValues {
        NotificationValues::new()
            .set("carrier", "Amazon")
            .set("trackingNumber", "999-8887776-6665554")
            .set("deliveryDate", "2026-09-10")
            .set("deliveryWindow", "2026-09-10 13:15 - 2026-09-10 20:15")
            .set("ruleId", "23")
            .set("doorbellServerAddress", "http://localhost:3001")
    }

    #[test]
    fn rendersTheConfiguredSubjectAndBody() {
        // The templates of the test config, verbatim.
        let subject = "[Doorbell TEST] Tracking notification: {carrier} - {trackingNumber}";
        assert_eq!(
            render(subject, &testValues()).unwrap(),
            "[Doorbell TEST] Tracking notification: Amazon - 999-8887776-6665554"
        );

        let body = "Delivery for {carrier} tracking number {trackingNumber} is expected to arrive on \
                    {deliveryDate}\nCreated AutoBuzz-Rule in Doorbell-Server: {doorbellServerAddress}";
        assert_eq!(
            render(body, &testValues()).unwrap(),
            "Delivery for Amazon tracking number 999-8887776-6665554 is expected to arrive on \
             2026-09-10\nCreated AutoBuzz-Rule in Doorbell-Server: http://localhost:3001"
        );
    }

    #[test]
    fn theEventTellsCreateAndMoveApart() {
        // The same template has to read correctly for both, so the verb is a value, not literal text.
        let created = testValues().set("event", "created").set("previousWindowNote", "");
        let moved = testValues()
            .set("event", "moved")
            .set("previousWindowNote", " (previously 2026-09-11 08:00 - 2026-09-11 20:00)");
        let template = "AutoBuzz-Rule {event}{previousWindowNote}";
        assert_eq!(render(template, &created).unwrap(), "AutoBuzz-Rule created");
        assert_eq!(
            render(template, &moved).unwrap(),
            "AutoBuzz-Rule moved (previously 2026-09-11 08:00 - 2026-09-11 20:00)"
        );
    }

    #[test]
    fn aPlaceholderWithoutAValueBecomesEmpty() {
        // The previous window only exists for an update, so on a create it renders as nothing.
        assert_eq!(render("was: {previousWindow}.", &testValues()).unwrap(), "was: .");
    }

    #[test]
    fn textWithoutPlaceholdersIsUnchanged() {
        assert_eq!(render("a plain sentence", &testValues()).unwrap(), "a plain sentence");
        assert_eq!(render("", &testValues()).unwrap(), "");
    }

    #[test]
    fn rejectsATypoInsteadOfLeavingItInTheMessage() {
        let error = validate("arrives {deliveryDat}").unwrap_err();
        assert_eq!(error, TemplateError::UnknownPlaceholder("deliveryDat".to_string()));
        // The message lists what would have been right.
        assert!(error.to_string().contains("{deliveryDate}"), "unhelpful message: {}", error);
    }

    #[test]
    fn rejectsAnUnclosedPlaceholder() {
        assert_eq!(validate("arrives {deliveryDate"), Err(TemplateError::UnclosedPlaceholder));
    }

    #[test]
    fn everyPlaceholderIsAcceptedByTheValidator() {
        for name in PLACEHOLDERS {
            validate(&format!("x {{{}}} y", name)).unwrap_or_else(|e| panic!("{} rejected: {}", name, e));
        }
    }
}
