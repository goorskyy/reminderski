//! The reminder file, kept under the user's roaming profile.
//!
//! One reminder per line, `<id>\t<due>\t<state>\t<snoozes>\t<text>`, with the text escaped so a
//! multiline reminder still occupies a single line. The whole file is rewritten on every change,
//! which is honest at this size and keeps one code path instead of two; the replacement goes
//! through a temporary file so an interrupted write cannot leave a half-written store behind.

use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::reminder::{Id, Reminder, State};

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
        let mut handed_out_ids = false;
        // A byte order mark survives being opened and saved in Notepad, and would otherwise
        // make the first reminder unreadable.
        for (index, line) in contents.trim_start_matches('\u{feff}').lines().enumerate() {
            if line.is_empty() {
                continue;
            }
            match decode(line) {
                Some((reminder, given_an_id)) => {
                    handed_out_ids |= given_an_id;
                    reminders.push(reminder);
                }
                // Only this application writes the file, so an unreadable line means it was
                // damaged. Say so rather than passing over it in silence.
                None => {
                    crate::log::problem(&format!(
                        "{}: line {} is unreadable",
                        path.display(),
                        index + 1
                    ));
                    unreadable.push(line.to_string());
                }
            }
        }

        let store = Self {
            path,
            reminders,
            unreadable,
        };

        // An ID handed to a line from an older version means nothing until it is written down:
        // read again after a restart, the same line would be handed a different one. Failing to
        // write it loses nothing, since the line is still there to be given one next time.
        if handed_out_ids && let Err(error) = store.save() {
            crate::log::problem(&format!(
                "{}: could not write the new reminder IDs: {error}",
                store.path.display()
            ));
        }
        Ok(store)
    }

    /// Never a deleted reminder, so nothing can answer, snooze or bring one back.
    pub fn find(&mut self, id: Id) -> Option<&mut Reminder> {
        self.reminders
            .iter_mut()
            .find(|reminder| reminder.id == id && reminder.state != State::Deleted)
    }

    /// Whether this reminder is still there and still waiting to be answered at `now`.
    pub fn is_due(&self, id: Id, now: u64) -> bool {
        self.reminders
            .iter()
            .any(|reminder| reminder.id == id && reminder.is_due(now))
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
}

fn encode(reminder: &Reminder) -> String {
    let mut line = reminder.id.to_string();
    line.push('\t');
    line.push_str(&reminder.due_unix.to_string());
    line.push('\t');
    line.push_str(state_word(reminder.state));
    line.push('\t');
    line.push_str(&reminder.snoozes.to_string());
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

/// Reads one line, and says whether the reminder had to be given an ID because the version that
/// wrote it did not have them yet.
fn decode(line: &str) -> Option<(Reminder, bool)> {
    // A tab inside the text is escaped, so splitting on tabs cannot cut a reminder in half and
    // the number of columns says which version wrote the line.
    let columns: Vec<&str> = line.split('\t').collect();
    let (id, due, state, snoozes, escaped) = match columns[..] {
        [due, text] => (None, due, State::Pending, 0, text),
        [due, state, text] => (None, due, parse_state(state)?, 0, text),
        [due, state, snoozes, text] => {
            (None, due, parse_state(state)?, snoozes.parse().ok()?, text)
        }
        [id, due, state, snoozes, text] => (
            Some(Id::parse(id)?),
            due,
            parse_state(state)?,
            snoozes.parse().ok()?,
            text,
        ),
        _ => return None,
    };
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
    let reminder = Reminder {
        id: id.unwrap_or_else(Id::random),
        due_unix,
        state,
        snoozes,
        text,
    };
    Some((reminder, id.is_none()))
}

fn state_word(state: State) -> &'static str {
    match state {
        State::Pending => "pending",
        State::Done => "done",
        State::Deleted => "deleted",
    }
}

fn parse_state(word: &str) -> Option<State> {
    match word {
        "pending" => Some(State::Pending),
        "done" => Some(State::Done),
        "deleted" => Some(State::Deleted),
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
            id: Id::random(),
            due_unix,
            state,
            snoozes: 0,
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
    fn remembers_how_many_times_a_reminder_was_pushed_back() {
        let mut pushed = reminder(1, State::Pending, "again");
        pushed.snoozes = 3;

        let loaded = round_trip(vec![pushed]);
        assert_eq!(loaded[0].snoozes, 3);
    }

    #[test]
    fn reads_reminders_written_before_snoozes_were_counted() {
        let path = temporary_path();
        fs::write(&path, "1800000000\tpending\tbuy milk\n").unwrap();

        let store = Store::open(path.clone()).unwrap();
        assert_eq!(store.reminders.len(), 1);
        assert_eq!(store.reminders[0].text, "buy milk");
        assert_eq!(store.reminders[0].snoozes, 0);

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn keeps_each_reminder_its_id() {
        let original = reminder(1, State::Pending, "tea");
        let id = original.id;

        let loaded = round_trip(vec![original]);
        assert_eq!(loaded[0].id, id);
    }

    #[test]
    fn a_file_written_before_ids_is_given_them_for_good() {
        let path = temporary_path();
        fs::write(&path, "1800000000\tpending\t2\tbuy milk\n1\tdone\t0\ttea\n").unwrap();

        let first = Store::open(path.clone()).unwrap();
        let ids: Vec<Id> = first.reminders.iter().map(|reminder| reminder.id).collect();
        assert_ne!(ids[0], ids[1]);

        // Read again, as after a restart: the same IDs, and nothing else about them changed.
        let second = Store::open(path.clone()).unwrap();
        assert_eq!(second.reminders.len(), 2);
        assert_eq!(second.reminders[0].id, ids[0]);
        assert_eq!(second.reminders[1].id, ids[1]);
        assert_eq!(second.reminders[0].due_unix, 1_800_000_000);
        assert_eq!(second.reminders[0].snoozes, 2);
        assert_eq!(second.reminders[0].text, "buy milk");
        assert!(second.reminders[1].state == State::Done);

        let written = fs::read_to_string(&path).unwrap();
        assert!(written.starts_with(&format!("{}\t1800000000\tpending\t2\tbuy milk\n", ids[0])));

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn a_line_whose_id_cannot_be_read_is_kept_rather_than_given_a_new_one() {
        let path = temporary_path();
        fs::write(&path, "not-an-id\t1800000000\tpending\t0\tbuy milk\n").unwrap();

        let mut store = Store::open(path.clone()).unwrap();
        assert!(store.reminders.is_empty());

        store.reminders.push(reminder(1, State::Pending, "tea"));
        store.save().unwrap();
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("not-an-id\t1800000000\tpending\t0\tbuy milk"));

        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn finds_a_reminder_by_its_id() {
        let path = temporary_path();
        let mut store = Store::open(path).unwrap();
        store.reminders.push(reminder(1, State::Pending, "first"));
        store.reminders.push(reminder(2, State::Pending, "second"));
        let wanted = store.reminders[1].id;

        assert_eq!(store.find(wanted).unwrap().text, "second");
        assert!(store.find(Id::random()).is_none());
    }

    #[test]
    fn a_deleted_reminder_stays_in_the_file_as_deleted() {
        let loaded = round_trip(vec![reminder(1, State::Deleted, "gone")]);

        assert_eq!(loaded.len(), 1);
        assert!(loaded[0].state == State::Deleted);
        assert_eq!(loaded[0].text, "gone");
    }

    #[test]
    fn a_deleted_reminder_cannot_be_found() {
        let path = temporary_path();
        let mut store = Store::open(path).unwrap();
        store.reminders.push(reminder(1, State::Deleted, "gone"));
        let id = store.reminders[0].id;

        assert!(store.find(id).is_none());
        assert!(!store.is_due(id, u64::MAX));
    }

    #[test]
    fn a_reminder_is_due_only_while_it_waits_and_its_time_has_come() {
        let path = temporary_path();
        let mut store = Store::open(path).unwrap();
        store.reminders.push(reminder(100, State::Pending, "tea"));
        let id = store.reminders[0].id;

        assert!(!store.is_due(id, 99));
        assert!(store.is_due(id, 100));
        store.reminders[0].state = State::Done;
        assert!(!store.is_due(id, 100));
        assert!(!store.is_due(Id::random(), 100));
    }

    #[test]
    fn a_missing_file_holds_no_reminders() {
        assert!(Store::open(temporary_path()).unwrap().reminders.is_empty());
    }

    #[test]
    fn saving_does_not_destroy_a_line_it_could_not_read() {
        let path = temporary_path();
        fs::write(
            &path,
            "not a reminder
1800000000	pending	0	kept
",
        )
        .unwrap();

        let mut store = Store::open(path.clone()).unwrap();
        assert_eq!(store.reminders.len(), 1);
        assert_eq!(store.reminders[0].text, "kept");

        store.reminders[0].state = State::Done;
        store.save().unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains("not a reminder"));
        assert!(written.contains("done	0	kept"));

        fs::remove_file(&path).unwrap();
    }
}
