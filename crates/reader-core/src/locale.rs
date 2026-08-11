//! Locale policy: English UI collation, Brazilian Portuguese relative time.
//!
//! v1 delegated both to `Intl`. v2 implements them directly so the binary needs
//! no ICU data. The observable contract is what the v1 tests assert:
//!
//! * collation ignores case and accents and orders embedded numbers by value;
//! * relative time is `agora`, `há N minuto(s)`, `há N hora(s)`, `há N dia(s)`.

use std::cmp::Ordering;

use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

/// Locale of all user-facing UI text.
pub const APP_LOCALE: &str = "en";
/// Locale of relative timestamps in the library.
pub const RELATIVE_TIME_LOCALE: &str = "pt-BR";

/// Primary weight classes, ordered the way the CLDR root collation orders them:
/// whitespace, then punctuation and symbols, then digits, then letters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum CharClass {
    Whitespace,
    Punctuation,
    Digit,
    Letter,
}

fn class_of(value: char) -> CharClass {
    if value.is_whitespace() {
        CharClass::Whitespace
    } else if value.is_ascii_digit() {
        CharClass::Digit
    } else if value.is_alphanumeric() {
        CharClass::Letter
    } else {
        CharClass::Punctuation
    }
}

/// One comparable element: a numeric run compares by value, everything else by
/// its case- and accent-folded character.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Unit {
    Number(String),
    Char(CharClass, char),
}

impl Unit {
    const fn class(&self) -> CharClass {
        match self {
            Self::Number(_) => CharClass::Digit,
            Self::Char(class, _) => *class,
        }
    }
}

impl Ord for Unit {
    fn cmp(&self, other: &Self) -> Ordering {
        let class = self.class().cmp(&other.class());
        if class != Ordering::Equal {
            return class;
        }
        match (self, other) {
            (Self::Number(left), Self::Number(right)) => left
                .len()
                .cmp(&right.len())
                .then_with(|| left.as_str().cmp(right.as_str())),
            (Self::Char(_, left), Self::Char(_, right)) => left.cmp(right),
            // Same class with different shapes only happens for digits, which
            // are always collected into `Number`.
            (Self::Number(_), Self::Char(..)) => Ordering::Less,
            (Self::Char(..), Self::Number(_)) => Ordering::Greater,
        }
    }
}

impl PartialOrd for Unit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Fold a string into comparable units: strip diacritics, lowercase, and group
/// digit runs so `Book 2` sorts before `Book 10`.
fn collation_key(text: &str) -> Vec<Unit> {
    let folded: Vec<char> = text
        .nfd()
        .filter(|value| !is_combining_mark(*value))
        .flat_map(char::to_lowercase)
        .collect();

    let mut units = Vec::with_capacity(folded.len());
    let mut index = 0;
    while index < folded.len() {
        let value = folded[index];
        if value.is_ascii_digit() {
            let start = index;
            while index < folded.len() && folded[index].is_ascii_digit() {
                index += 1;
            }
            let digits: String = folded[start..index].iter().collect();
            let trimmed = digits.trim_start_matches('0');
            units.push(Unit::Number(if trimmed.is_empty() {
                "0".to_owned()
            } else {
                trimmed.to_owned()
            }));
            continue;
        }
        units.push(Unit::Char(class_of(value), value));
        index += 1;
    }
    units
}

/// Compare two UI strings with English sort collation.
///
/// Case and accents are ignored, so `compare_text("e", "é")` is
/// [`Ordering::Equal`]; callers relying on a total order should fall back to a
/// stable sort, as v1 did.
#[must_use]
pub fn compare_text(left: &str, right: &str) -> Ordering {
    collation_key(left).cmp(&collation_key(right))
}

/// Sort strings in place with [`compare_text`], keeping equal entries stable.
pub fn sort_text<T, F>(items: &mut [T], text_of: F)
where
    F: Fn(&T) -> String,
{
    items.sort_by(|left, right| compare_text(&text_of(left), &text_of(right)));
}

const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 60 * MINUTE_MS;
const DAY_MS: i64 = 24 * HOUR_MS;

/// Format `timestamp` relative to `now`, both in epoch milliseconds.
///
/// Future timestamps are clamped to `agora`, matching v1's `Math.max(0, …)`.
#[must_use]
pub fn format_relative_time(timestamp: i64, now: i64) -> String {
    let elapsed = (now - timestamp).max(0);
    if elapsed < MINUTE_MS {
        return "agora".to_owned();
    }
    let (amount, singular, plural) = if elapsed < HOUR_MS {
        (elapsed / MINUTE_MS, "minuto", "minutos")
    } else if elapsed < DAY_MS {
        (elapsed / HOUR_MS, "hora", "horas")
    } else {
        (elapsed / DAY_MS, "dia", "dias")
    };
    let unit = if amount == 1 { singular } else { plural };
    format!("há {amount} {unit}")
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::{APP_LOCALE, RELATIVE_TIME_LOCALE, compare_text, format_relative_time, sort_text};

    fn sorted(mut items: Vec<&str>) -> Vec<&str> {
        sort_text(&mut items, |item| (*item).to_owned());
        items
    }

    #[test]
    fn locale_policy_keeps_ui_english_and_relative_time_portuguese() {
        assert_eq!(APP_LOCALE, "en");
        assert_eq!(RELATIVE_TIME_LOCALE, "pt-BR");
        assert_eq!(format_relative_time(1_000_000, 1_000_000), "agora");
        assert_eq!(
            format_relative_time(1_000_000 - 2 * 60_000, 1_000_000),
            "há 2 minutos"
        );
        assert_eq!(
            format_relative_time(1_000_000 - 3 * 60 * 60_000, 1_000_000),
            "há 3 horas"
        );
        assert_eq!(
            format_relative_time(1_000_000 - 2 * 24 * 60 * 60_000, 1_000_000),
            "há 2 dias"
        );
    }

    #[test]
    fn singular_units_drop_the_plural_s() {
        assert_eq!(format_relative_time(0, 60_000), "há 1 minuto");
        assert_eq!(format_relative_time(0, 60 * 60_000), "há 1 hora");
        assert_eq!(format_relative_time(0, 24 * 60 * 60_000), "há 1 dia");
    }

    #[test]
    fn future_timestamps_read_as_now() {
        assert_eq!(format_relative_time(2_000_000, 1_000_000), "agora");
    }

    #[test]
    fn collation_is_numeric_aware() {
        assert_eq!(
            sorted(vec!["Book 10", "Book 2", "apple"]),
            vec!["apple", "Book 2", "Book 10"]
        );
        assert_eq!(
            sorted(vec!["page-10.jpg", "page-0002.jpg", "Page-1.jpg"]),
            vec!["Page-1.jpg", "page-0002.jpg", "page-10.jpg"]
        );
    }

    #[test]
    fn collation_ignores_case_and_accents() {
        assert_eq!(compare_text("e", "é"), Ordering::Equal);
        assert_eq!(compare_text("abc", "ÁBC"), Ordering::Equal);
        assert_eq!(compare_text("Zebra", "apple"), Ordering::Greater);
    }

    #[test]
    fn punctuation_and_digits_sort_before_letters() {
        assert_eq!(
            sorted(vec!["zulu", "10", "_hidden", "alpha"]),
            vec!["_hidden", "10", "alpha", "zulu"]
        );
    }

    #[test]
    fn leading_zeros_do_not_change_numeric_order() {
        assert_eq!(compare_text("ch007", "ch7"), Ordering::Equal);
        assert_eq!(compare_text("ch007", "ch8"), Ordering::Less);
    }
}
