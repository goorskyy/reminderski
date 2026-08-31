//! Storing reminders in a plain text file under the user's roaming profile.
//!
//! One reminder per line, `<due>\t<text>`, with the text escaped so that a multiline reminder
//! still occupies a single line. Appending is the only write, which keeps a crash from damaging
//! reminders that were already saved.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::reminder::Reminder;

pub fn default_path() -> io::Result<PathBuf> {
    let roaming = env::var("APPDATA")
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "APPDATA is not set"))?;
    Ok(Path::new(&roaming)
        .join("Reminderski")
        .join("reminders.txt"))
}

pub fn append(path: &Path, reminder: &Reminder) -> io::Result<()> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "{}", encode(reminder))
}

/// Reads every stored reminder. A file that does not exist yet simply holds none.
pub fn load(path: &Path) -> io::Result<Vec<Reminder>> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };

    let mut reminders = Vec::new();
    for (index, line) in contents.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        match decode(line) {
            Some(reminder) => reminders.push(reminder),
            // Only this application writes the file, so an unreadable line means it was damaged.
            // Say so rather than dropping it quietly, and keep the reminders still readable.
            None => eprintln!(
                "{}: line {} is unreadable and was skipped",
                path.display(),
                index + 1
            ),
        }
    }
    Ok(reminders)
}

fn encode(reminder: &Reminder) -> String {
    let mut line = reminder.due_unix.to_string();
    line.push('\t');
    for character in reminder.text.chars() {
        match character {
            '\\' => line.push_str("\\\\"),
            '\n' => line.push_str("\\n"),
            '\r' => line.push_str("\\r"),
            '\t' => line.push_str("\\t"),
            _ => line.push(character),
        }
    }
    line
}

fn decode(line: &str) -> Option<Reminder> {
    let (due, escaped) = line.split_once('\t')?;
    let due_unix = due.parse().ok()?;

    let mut text = String::with_capacity(escaped.len());
    let mut characters = escaped.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            text.push(character);
            continue;
        }
        match characters.next()? {
            'n' => text.push('\n'),
            'r' => text.push('\r'),
            't' => text.push('\t'),
            '\\' => text.push('\\'),
            _ => return None,
        }
    }
    Some(Reminder { due_unix, text })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A file of this test's own, so the tests do not disturb each other or the real store.
    fn temporary_path() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            "reminderski-test-{}-{}.txt",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        env::temp_dir().join(name)
    }

    #[test]
    fn stores_and_reads_back_awkward_text() {
        let path = temporary_path();
        let text = "Zażółć gęślą jaźń\r\nsecond line\ttabbed\\escaped 🎉".to_string();
        append(
            &path,
            &Reminder {
                due_unix: 1_800_000_000,
                text: text.clone(),
            },
        )
        .unwrap();

        let loaded = load(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].due_unix, 1_800_000_000);
        assert_eq!(loaded[0].text, text);

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn keeps_every_appended_reminder_in_order() {
        let path = temporary_path();
        for index in 0..3u64 {
            append(
                &path,
                &Reminder {
                    due_unix: index,
                    text: format!("reminder {index}"),
                },
            )
            .unwrap();
        }

        let loaded = load(&path).unwrap();
        assert_eq!(loaded.len(), 3);
        assert_eq!(loaded[2].text, "reminder 2");

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_missing_file_holds_no_reminders() {
        assert!(load(&temporary_path()).unwrap().is_empty());
    }

    #[test]
    fn a_damaged_line_does_not_lose_the_readable_ones() {
        let path = temporary_path();
        fs::write(&path, "not a reminder\n1800000000\tkept\n").unwrap();

        let loaded = load(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].text, "kept");

        fs::remove_file(&path).unwrap();
    }
}
