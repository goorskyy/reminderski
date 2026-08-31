//! The reminder file, kept under the user's roaming profile.
//!
//! One reminder per line, `<due>\t<state>\t<text>`, with the text escaped so that a multiline
//! reminder still occupies a single line. The whole file is rewritten on every change, which is
//! honest at this size and keeps one code path instead of two; the replacement goes through a
//! temporary file so an interrupted write cannot leave a half-written store behind.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::reminder::{Reminder, State};

pub struct Store {
    path: PathBuf,
    pub reminders: Vec<Reminder>,
    /// Lines this version could not read. They are written back untouched, because rewriting
    /// the file must never be what destroys them.
    unreadable: Vec<String>,
}

pub fn default_path() -> io::Result<PathBuf> {
    let roaming = env::var("APPDATA")
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, "APPDATA is not set"))?;
    Ok(Path::new(&roaming)
        .join("Reminderski")
        .join("reminders.txt"))
}

impl Store {
    /// Reads the stored reminders. A file that does not exist yet simply holds none.
    pub fn open(path: PathBuf) -> io::Result<Self> {
        let contents = match fs::read_to_string(&path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error),
        };

        let mut reminders = Vec::new();
        let mut unreadable = Vec::new();
        // A byte order mark survives being opened and saved in Notepad, and would otherwise
        // make the first reminder unreadable.
        for (index, line) in contents.trim_start_matches('\u{feff}').lines().enumerate() {
            if line.is_empty() {
                continue;
            }
            match decode(line) {
                Some(reminder) => reminders.push(reminder),
                // Only this application writes the file, so an unreadable line means it was
                // damaged. Say so rather than passing over it in silence.
                None => {
                    eprintln!("{}: line {} is unreadable", path.display(), index + 1);
                    unreadable.push(line.to_string());
                }
            }
        }

        Ok(Self {
            path,
            reminders,
            unreadable,
        })
    }

    pub fn save(&self) -> io::Result<()> {
        if let Some(directory) = self.path.parent() {
            fs::create_dir_all(directory)?;
        }

        let mut contents = String::new();
        for reminder in &self.reminders {
            contents.push_str(&encode(reminder));
            contents.push('\n');
        }
        for line in &self.unreadable {
            contents.push_str(line);
            contents.push('\n');
        }

        // Rename replaces the destination on Windows, so the store is never briefly absent.
        let temporary = self.path.with_extension("tmp");
        fs::write(&temporary, contents)?;
        fs::rename(&temporary, &self.path)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn pending(&self) -> usize {
        self.reminders
            .iter()
            .filter(|reminder| reminder.state == State::Pending)
            .count()
    }
}

fn encode(reminder: &Reminder) -> String {
    let mut line = reminder.due_unix.to_string();
    line.push('\t');
    line.push_str(state_word(reminder.state));
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
    let (due, rest) = line.split_once('\t')?;
    let due_unix = due.parse().ok()?;

    // Reminders written before states existed have no state column and are all pending.
    let (state, escaped) = match rest.split_once('\t') {
        Some((word, remainder)) => match parse_state(word) {
            Some(state) => (state, remainder),
            None => (State::Pending, rest),
        },
        None => (State::Pending, rest),
    };

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
    Some(Reminder {
        due_unix,
        state,
        text,
    })
}

fn state_word(state: State) -> &'static str {
    match state {
        State::Pending => "pending",
        State::Done => "done",
    }
}

fn parse_state(word: &str) -> Option<State> {
    match word {
        "pending" => Some(State::Pending),
        "done" => Some(State::Done),
        _ => None,
    }
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

    fn reminder(due_unix: u64, state: State, text: &str) -> Reminder {
        Reminder {
            due_unix,
            state,
            text: text.to_string(),
        }
    }

    /// Saves the given reminders, reads the file back, and cleans up after itself.
    fn round_trip(reminders: Vec<Reminder>) -> Vec<Reminder> {
        let path = temporary_path();
        let mut store = Store::open(path.clone()).unwrap();
        store.reminders = reminders;
        store.save().unwrap();

        let loaded = Store::open(path.clone()).unwrap().reminders;
        fs::remove_file(&path).unwrap();
        loaded
    }

    #[test]
    fn stores_and_reads_back_awkward_text() {
        let text = "Zażółć gęślą jaźń\r\nsecond line\ttabbed\\escaped 🎉";
        let loaded = round_trip(vec![reminder(1_800_000_000, State::Pending, text)]);

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].due_unix, 1_800_000_000);
        assert_eq!(loaded[0].text, text);
    }

    #[test]
    fn keeps_every_reminder_and_its_state() {
        let loaded = round_trip(vec![
            reminder(1, State::Done, "finished"),
            reminder(2, State::Pending, "waiting"),
        ]);

        assert_eq!(loaded.len(), 2);
        assert!(loaded[0].state == State::Done);
        assert!(loaded[1].state == State::Pending);
        assert_eq!(loaded[1].text, "waiting");
    }

    #[test]
    fn saving_replaces_the_previous_contents() {
        let path = temporary_path();
        let mut store = Store::open(path.clone()).unwrap();
        store.reminders.push(reminder(1, State::Pending, "first"));
        store.save().unwrap();
        store.reminders.clear();
        store.reminders.push(reminder(2, State::Pending, "second"));
        store.save().unwrap();

        let loaded = Store::open(path.clone()).unwrap().reminders;
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].text, "second");

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn reads_reminders_written_before_states_existed() {
        let path = temporary_path();
        fs::write(&path, "1800000000\tbuy milk\n").unwrap();

        let store = Store::open(path.clone()).unwrap();
        assert_eq!(store.reminders.len(), 1);
        assert_eq!(store.reminders[0].text, "buy milk");
        assert!(store.reminders[0].state == State::Pending);

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn reads_a_file_that_was_saved_with_a_byte_order_mark() {
        let path = temporary_path();
        fs::write(&path, "\u{feff}1800000000\tpending\tbuy milk\n").unwrap();

        let store = Store::open(path.clone()).unwrap();
        assert_eq!(store.reminders.len(), 1);
        assert_eq!(store.reminders[0].text, "buy milk");

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_missing_file_holds_no_reminders() {
        assert!(Store::open(temporary_path()).unwrap().reminders.is_empty());
    }

    #[test]
    fn saving_does_not_destroy_a_line_it_could_not_read() {
        let path = temporary_path();
        fs::write(&path, "not a reminder\n1800000000\tpending\tkept\n").unwrap();

        let mut store = Store::open(path.clone()).unwrap();
        assert_eq!(store.reminders.len(), 1);
        assert_eq!(store.reminders[0].text, "kept");

        store.reminders[0].state = State::Done;
        store.save().unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("not a reminder"));
        assert!(written.contains("done\tkept"));

        fs::remove_file(&path).unwrap();
    }
}
