//! A reminder, and the Slack-style time expressions that schedule one.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct Reminder {
    /// Seconds since the Unix epoch. Stored as an instant rather than a delay so that a
    /// reminder written before a restart still comes due at the time the user asked for.
    pub due_unix: u64,
    pub text: String,
}

impl Reminder {
    pub fn due_in(delay: Duration, text: String) -> Self {
        let due = SystemTime::now() + delay;
        Self {
            due_unix: due.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            text,
        }
    }
}

/// Reads a relative time expression such as `in 40m`, `2h`, `1h30m` or `3 days`.
///
/// The leading `in` is optional, units may repeat, and spaces anywhere are ignored. Returns
/// `None` for anything else, including absolute times such as `at 15:00`, which this version
/// does not understand.
pub fn parse_when(input: &str) -> Option<Duration> {
    let lowered = input.trim().to_ascii_lowercase();
    let expression = lowered.strip_prefix("in ").unwrap_or(&lowered).trim_start();

    let bytes = expression.as_bytes();
    let mut position = 0;
    let mut seconds: u64 = 0;
    let mut read_a_group = false;

    while position < bytes.len() {
        position = skip_spaces(bytes, position);
        if position == bytes.len() {
            break;
        }

        let amount_start = position;
        while position < bytes.len() && bytes[position].is_ascii_digit() {
            position += 1;
        }
        let amount: u64 = expression[amount_start..position].parse().ok()?;

        position = skip_spaces(bytes, position);
        let unit_start = position;
        while position < bytes.len() && bytes[position].is_ascii_alphabetic() {
            position += 1;
        }

        // An empty unit fails here, so a bare number is rejected rather than guessed at.
        seconds = seconds
            .checked_add(amount.checked_mul(unit_seconds(&expression[unit_start..position])?)?)?;
        read_a_group = true;
    }

    read_a_group.then(|| Duration::from_secs(seconds))
}

fn skip_spaces(bytes: &[u8], mut position: usize) -> usize {
    while position < bytes.len() && bytes[position] == b' ' {
        position += 1;
    }
    position
}

fn unit_seconds(unit: &str) -> Option<u64> {
    Some(match unit {
        "s" | "sec" | "secs" | "second" | "seconds" => 1,
        "m" | "min" | "mins" | "minute" | "minutes" => 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3600,
        "d" | "day" | "days" => 86400,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seconds(input: &str) -> Option<u64> {
        parse_when(input).map(|delay| delay.as_secs())
    }

    #[test]
    fn reads_the_headline_expression() {
        assert_eq!(seconds("in 40m"), Some(40 * 60));
    }

    #[test]
    fn the_leading_in_and_the_spacing_are_optional() {
        assert_eq!(seconds("40m"), Some(40 * 60));
        assert_eq!(seconds("  IN   40 minutes "), Some(40 * 60));
    }

    #[test]
    fn adds_up_repeated_units() {
        assert_eq!(seconds("in 1h30m"), Some(90 * 60));
        assert_eq!(seconds("2 days 3 hours"), Some(2 * 86400 + 3 * 3600));
    }

    #[test]
    fn accepts_every_unit_spelling() {
        assert_eq!(seconds("90s"), Some(90));
        assert_eq!(seconds("2hrs"), Some(2 * 3600));
        assert_eq!(seconds("3 days"), Some(3 * 86400));
    }

    #[test]
    fn rejects_what_it_cannot_schedule() {
        assert_eq!(seconds(""), None);
        assert_eq!(seconds("in"), None);
        assert_eq!(seconds("40"), None);
        assert_eq!(seconds("in40m"), None);
        assert_eq!(seconds("40x"), None);
        assert_eq!(seconds("tomorrow"), None);
        assert_eq!(seconds("at 15:00"), None);
    }

    #[test]
    fn rejects_a_delay_too_large_to_represent() {
        assert_eq!(seconds("99999999999999999999d"), None);
        assert_eq!(seconds("1000000000000000d"), None);
    }

    #[test]
    fn due_in_is_the_delay_from_now() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let reminder = Reminder::due_in(Duration::from_secs(600), "tea".to_string());
        assert!(reminder.due_unix >= now + 600 && reminder.due_unix <= now + 602);
    }
}
