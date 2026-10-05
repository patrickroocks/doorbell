use regex::Regex;
use std::sync::LazyLock;

/// Quoted-printable soft line break: a "=" directly in front of a line end.
static QP_SOFT_BREAK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"=\r?\n").unwrap());

/// A quoted-printable escape of one byte, e.g. "=C2" or "=A0".
static QP_ESCAPE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"=[0-9A-Fa-f]{2}").unwrap());

/// Any run of whitespace, including the Unicode spaces mails like to use.
static WHITESPACE_RUN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\s\u{a0}\u{2000}-\u{200a}\u{202f}\u{205f}\u{3000}]+").unwrap());

/// Reduces mail text to the plain sentence that a date pattern can be matched against.
///
/// The announcement of the delivery day is broken up by the mail formatting - bold markers, line
/// breaks and a non-breaking space encoded as "=C2=A0" sit in the middle of it:
///
/// ```text
/// am Mittwoch, den 12.08.
/// =C2=A0**
///
/// zugestellt.
/// ```
///
/// becomes "am Mittwoch, den 12.08. zugestellt.".
///
/// This is only used for the date patterns. The delivery id is searched in the untouched text,
/// where the "=XX" escapes still carry their meaning.
pub fn normalizeForParsing(text: &str) -> String {
    // Soft line breaks first: they only continue a line and must not leave a space behind.
    let withoutSoftBreaks = QP_SOFT_BREAK.replace_all(text, "");

    // Every remaining escape stands for a byte of some character that is not part of an
    // announcement's wording - a space is the right replacement ("=C2=A0" is a non-breaking space).
    let withoutEscapes = QP_ESCAPE.replace_all(&withoutSoftBreaks, " ");

    // The plain text part of a mail marks bold text up with stars, in the middle of the sentence.
    let withoutStars = withoutEscapes.replace('*', "");

    WHITESPACE_RUN.replace_all(&withoutStars, " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizesFutureAnnouncementOfRealMail() {
        // Exactly as it appears in the raw source of the DHL mail.
        let raw = "voraussichtlich\r\n\r\n**\r\n\r\n\r\n\r\nam Mittwoch, den 12.08.\r\n=C2=A0**\r\n\r\nzugestellt.";
        assert_eq!(normalizeForParsing(raw), "voraussichtlich am Mittwoch, den 12.08. zugestellt.");
    }

    #[test]
    fn normalizesTodayAnnouncement() {
        let raw = "**heute zwischen 13:45=C2=A0 - 15:15 Uhr=C2=A0**\r\ndurch Ihre Paketzustellkraft zugestellt.";
        assert_eq!(
            normalizeForParsing(raw),
            "heute zwischen 13:45 - 15:15 Uhr durch Ihre Paketzustellkraft zugestellt."
        );
    }

    #[test]
    fn joinsSoftLineBreaksWithoutSpace() {
        // A soft break continues the word, so "piececode=\r\n=3D123" must not gain a space in the
        // middle - the same mechanism that splits the delivery ids.
        assert_eq!(normalizeForParsing("piececode=\r\n=3D123"), "piececode 123");
    }

    #[test]
    fn normalizesAlreadyDecodedText() {
        // Text coming from a decoded MIME part has real non-breaking spaces instead of "=C2=A0".
        let decoded = "am Mittwoch, den 12.08.\r\n\u{a0}**\r\n\r\nzugestellt.";
        assert_eq!(normalizeForParsing(decoded), "am Mittwoch, den 12.08. zugestellt.");
    }
}
