//! A reminder, and the Slack-style time expressions that schedule one.

use std::mem;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;

/// Where `tomorrow` lands when no clock time is given, matching Slack.
const DEFAULT_MORNING: u32 = 9 * 3600;

const DAY: u32 = 24 * 3600;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum State {
    Pending,
    Done,
}

pub struct Reminder {
    /// Seconds since the Unix epoch. Stored as an instant rather than a delay so that a
    /// reminder written before a restart still comes due at the time the user asked for.
    pub due_unix: u64,
    pub state: State,
    /// How many times it has been pushed back. The character has opinions about this.
    pub snoozes: u32,
    pub text: String,
}

impl Reminder {
    pub fn due_in(delay: Duration, text: String) -> Self {
        Self {
            due_unix: now_unix() + delay.as_secs(),
            state: State::Pending,
            snoozes: 0,
            text,
        }
    }

    pub fn is_due(&self, now: u64) -> bool {
        self.state == State::Pending && self.due_unix <= now
    }
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Reads a time expression and answers how long from now it means.
///
/// Relative: `in 40m`, `2h`, `1h30m`, `3 days`. The leading `in` is optional and units may
/// repeat.
///
/// Absolute: `at 15:00`, `at 3pm`, `at 9`, `tomorrow`, `tomorrow at 9:30`. A clock time that has
/// already passed today means tomorrow, and a bare `tomorrow` means 09:00.
///
/// By day name: `friday`, `fri`, `friday at 15:00`, `next tuesday`. A day name means the next
/// one to come round, which is today only when the time has not yet passed. Writing `next` in
/// front of it rules today out and changes nothing else.
///
/// Absolute times are resolved against the local clock as a plain offset from now, so a
/// reminder that crosses a daylight-saving change arrives an hour off. Reminders live for hours
/// rather than months, which makes that rare enough not to carry a time zone library for.
pub fn parse_when(input: &str) -> Option<Duration> {
    parse_when_at(input, local_time_of_day(), local_weekday())
}

/// The same, with the current local time of day and weekday supplied, which is what the tests
/// exercise.
fn parse_when_at(input: &str, now: u32, today: u16) -> Option<Duration> {
    let lowered = input.trim().to_ascii_lowercase();

    if let Some(rest) = lowered.strip_prefix("tomorrow") {
        return Some(delay_until(clock_after_day_word(rest)?, now, true));
    }
    if let Some(rest) = lowered.strip_prefix("today") {
        return Some(delay_until(clock_after_day_word(rest)?, now, false));
    }

    // `next friday` and `friday` take the same clock time after them as `tomorrow` does.
    let (after_next, not_today) = match lowered.strip_prefix("next ") {
        Some(rest) => (rest.trim_start(), true),
        None => (lowered.as_str(), false),
    };
    let (word, rest) = split_first_word(after_next);
    if let Some(wanted) = weekday_number(word) {
        let clock = clock_after_day_word(rest)?;
        return Some(delay_until_weekday(wanted, clock, today, now, not_today));
    }

    if let Some(rest) = lowered.strip_prefix("at ") {
        return Some(delay_until(parse_clock(rest.trim())?, now, false));
    }
    parse_duration(&lowered)
}

/// How long from `now` until `target`, rolling over to the next day when the time has passed.
fn delay_until(target: u32, now: u32, next_day: bool) -> Duration {
    let mut seconds = i64::from(target) - i64::from(now);
    if next_day || seconds <= 0 {
        seconds += i64::from(DAY);
    }
    Duration::from_secs(seconds as u64)
}

/// How long from now until `clock` on the next `wanted` weekday.
///
/// Today counts when the time has not yet passed, which is the same rule a bare clock time
/// follows. `next` rules today out and does nothing else: it never pushes the reminder a further
/// week. The two readings of "next friday" cannot both be served, and of the two mistakes, being
/// reminded a week early is one you can see and snooze, while being reminded a week late is one
/// you cannot.
fn delay_until_weekday(wanted: u16, clock: u32, today: u16, now: u32, not_today: bool) -> Duration {
    let mut days = u32::from((wanted + 7 - today) % 7);
    if days == 0 && (not_today || clock <= now) {
        days = 7;
    }
    let seconds = i64::from(days) * i64::from(DAY) + i64::from(clock) - i64::from(now);
    Duration::from_secs(seconds as u64)
}

/// Sunday is 0, matching the weekday Windows reports in SYSTEMTIME.
fn weekday_number(word: &str) -> Option<u16> {
    Some(match word {
        "sunday" | "sun" => 0,
        "monday" | "mon" => 1,
        "tuesday" | "tue" | "tues" => 2,
        "wednesday" | "wed" => 3,
        "thursday" | "thu" | "thur" | "thurs" => 4,
        "friday" | "fri" => 5,
        "saturday" | "sat" => 6,
        _ => return None,
    })
}

fn split_first_word(text: &str) -> (&str, &str) {
    match text.split_once(' ') {
        Some((word, rest)) => (word, rest),
        None => (text, ""),
    }
}

/// The clock time following `tomorrow` or `today`, which may be absent, bare, or after `at`.
fn clock_after_day_word(rest: &str) -> Option<u32> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Some(DEFAULT_MORNING);
    }
    let clock = rest.strip_prefix("at").unwrap_or(rest).trim();
    parse_clock(clock)
}

/// Seconds since midnight for `15:00`, `3pm`, `3:30 pm` or `9`.
fn parse_clock(text: &str) -> Option<u32> {
    let (digits, afternoon) = match text.strip_suffix("pm") {
        Some(rest) => (rest.trim_end(), Some(true)),
        None => match text.strip_suffix("am") {
            Some(rest) => (rest.trim_end(), Some(false)),
            None => (text, None),
        },
    };

    let (hours, minutes) = match digits.split_once(':') {
        Some((hours, minutes)) => (hours.trim(), minutes.trim()),
        None => (digits.trim(), "0"),
    };
    let hour: u32 = hours.parse().ok()?;
    let minute: u32 = minutes.parse().ok()?;
    if minute > 59 {
        return None;
    }

    let hour = match afternoon {
        // On a 12 hour clock noon and midnight are both written 12.
        Some(true) if hour == 12 => 12,
        Some(true) if hour < 12 => hour + 12,
        Some(false) if hour == 12 => 0,
        Some(false) if hour < 12 => hour,
        None if hour < 24 => hour,
        _ => return None,
    };
    Some(hour * 3600 + minute * 60)
}

/// Reads one or more `<amount><unit>` groups, such as `40m` or `1h 30m`.
fn parse_duration(input: &str) -> Option<Duration> {
    let expression = input.strip_prefix("in ").unwrap_or(input).trim_start();

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

/// The local wall clock, which is what the user set their reminder against.
pub fn local_now() -> SYSTEMTIME {
    let mut now: SYSTEMTIME = unsafe { mem::zeroed() };
    unsafe { GetLocalTime(&mut now) };
    now
}

fn local_time_of_day() -> u32 {
    let now = local_now();
    u32::from(now.wHour) * 3600 + u32::from(now.wMinute) * 60 + u32::from(now.wSecond)
}

fn local_weekday() -> u16 {
    local_now().wDayOfWeek
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 10:00 on a Wednesday, the reference "now" for the absolute cases. Midweek, so that a day
    /// name can be tested in both directions.
    const TEN: u32 = 10 * 3600;
    const WEDNESDAY: u16 = 3;

    fn seconds(input: &str) -> Option<u64> {
        parse_when_at(input, TEN, WEDNESDAY).map(|delay| delay.as_secs())
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
    fn reads_a_clock_time_later_today() {
        assert_eq!(seconds("at 15:00"), Some(5 * 3600));
        assert_eq!(seconds("at 15"), Some(5 * 3600));
        assert_eq!(seconds("AT 3PM"), Some(5 * 3600));
        assert_eq!(seconds("at 3:30 pm"), Some(5 * 3600 + 30 * 60));
    }

    #[test]
    fn a_clock_time_already_past_means_tomorrow() {
        assert_eq!(seconds("at 9:00"), Some(23 * 3600));
        assert_eq!(seconds("at 10:00"), Some(24 * 3600));
    }

    #[test]
    fn midnight_and_noon_are_both_written_twelve() {
        assert_eq!(seconds("at 12am"), Some(14 * 3600));
        assert_eq!(seconds("at 12pm"), Some(2 * 3600));
    }

    #[test]
    fn tomorrow_defaults_to_the_morning() {
        assert_eq!(seconds("tomorrow"), Some(23 * 3600));
        assert_eq!(seconds("tomorrow at 9:30"), Some(23 * 3600 + 30 * 60));
        assert_eq!(seconds("tomorrow 15:00"), Some(29 * 3600));
    }

    #[test]
    fn today_stays_on_the_same_day_unless_it_has_passed() {
        assert_eq!(seconds("today at 15:00"), Some(5 * 3600));
        assert_eq!(seconds("today at 9:00"), Some(23 * 3600));
    }

    /// From Wednesday: Friday is two days off, Tuesday is six, because a day name always looks
    /// forward. A bare one means the morning, as `tomorrow` does.
    #[test]
    fn a_day_name_means_the_next_one_to_come_round() {
        assert_eq!(seconds("friday"), Some(2 * 86400 - 3600));
        assert_eq!(seconds("tuesday"), Some(6 * 86400 - 3600));
        assert_eq!(seconds("friday at 15:00"), Some(2 * 86400 + 5 * 3600));
        assert_eq!(seconds("friday 15:00"), Some(2 * 86400 + 5 * 3600));
    }

    #[test]
    fn a_day_name_may_be_written_short() {
        assert_eq!(seconds("fri"), seconds("friday"));
        assert_eq!(seconds("TUES"), seconds("tuesday"));
        assert_eq!(seconds("sun"), seconds("sunday"));
    }

    /// Today's own name is today while the time is still ahead, and a week off once it is not,
    /// which is the rule a bare clock time already follows.
    #[test]
    fn todays_name_stays_today_until_the_time_has_passed() {
        assert_eq!(seconds("wednesday at 15:00"), Some(5 * 3600));
        assert_eq!(seconds("wednesday at 9:00"), Some(7 * 86400 - 3600));
        assert_eq!(seconds("wednesday"), Some(7 * 86400 - 3600));
    }

    /// `next` rules today out. It does not push the reminder a further week, so from Wednesday
    /// `next friday` is the same Friday as `friday`.
    #[test]
    fn next_only_rules_out_today() {
        assert_eq!(
            seconds("next wednesday at 15:00"),
            Some(7 * 86400 + 5 * 3600)
        );
        assert_eq!(seconds("next friday"), seconds("friday"));
        assert_eq!(seconds("next tuesday"), Some(6 * 86400 - 3600));
    }

    #[test]
    fn rejects_what_it_cannot_schedule() {
        assert_eq!(seconds(""), None);
        assert_eq!(seconds("in"), None);
        assert_eq!(seconds("40"), None);
        assert_eq!(seconds("in40m"), None);
        assert_eq!(seconds("40x"), None);
        assert_eq!(seconds("at"), None);
        assert_eq!(seconds("next"), None);
        assert_eq!(seconds("next someday"), None);
        assert_eq!(seconds("fridayish"), None);
        assert_eq!(seconds("friday at 25:00"), None);
        assert_eq!(seconds("at 25:00"), None);
        assert_eq!(seconds("at 12:60"), None);
        assert_eq!(seconds("at 13pm"), None);
        assert_eq!(seconds("tomorrowish"), None);
    }

    #[test]
    fn rejects_a_delay_too_large_to_represent() {
        assert_eq!(seconds("99999999999999999999d"), None);
        assert_eq!(seconds("1000000000000000d"), None);
    }

    #[test]
    fn due_in_is_the_delay_from_now() {
        let now = now_unix();
        let reminder = Reminder::due_in(Duration::from_secs(600), "tea".to_string());
        assert!(reminder.due_unix >= now + 600 && reminder.due_unix <= now + 602);
        assert!(!reminder.is_due(now));
        assert!(reminder.is_due(now + 600));
    }

    #[test]
    fn a_finished_reminder_never_comes_due_again() {
        let mut reminder = Reminder::due_in(Duration::from_secs(0), "tea".to_string());
        assert!(reminder.is_due(now_unix()));
        reminder.state = State::Done;
        assert!(!reminder.is_due(now_unix()));
    }
}
