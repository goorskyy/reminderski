//! The settings file, kept beside the reminders.
//!
//! One `name = value` per line, `#` starts a comment. Only the two shortcuts live here so far,
//! which is why there is a parser for shortcuts and barely one for the file around them.
//!
//! A setting that cannot be read falls back to what it was before anybody edited the file. A
//! typed shortcut is the one thing here somebody will get wrong, and refusing to start over it
//! would take away the shortcut they were trying to fix.

use std::fs;
use std::io;
use std::path::Path;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
};

use crate::log;

/// What the file says when it is not there, and what it is written with the first time.
const DEFAULT_FILE: &str = "\
# Reminderski settings. Restart Reminderski after changing anything here.
#
# A shortcut is its modifiers and one key, joined by +. The modifiers are ctrl, alt, shift and
# win, and at least one of them is required: a global shortcut with no modifier would swallow
# that key everywhere.
#
# The key may be a letter, a digit, a function key or the space bar: r, 7, f9, space.
#
# capture  opens the form, with whatever text is selected already in it
# answer   brings the oldest reminder on screen to the front

capture = ctrl+alt+r
answer = ctrl+alt+a
";

/// A global shortcut, in the two pieces RegisterHotKey asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Shortcut {
    pub modifiers: u32,
    pub key: u32,
}

pub struct Settings {
    pub capture: Shortcut,
    pub answer: Shortcut,
}

impl Default for Settings {
    /// What the application had before any of this was configurable. NOREPEAT belongs here for
    /// the same reason parse_shortcut adds it: held down, the combination would otherwise open a
    /// form for every repeat the keyboard sends.
    fn default() -> Self {
        let held = MOD_CONTROL | MOD_ALT | MOD_NOREPEAT;
        Self {
            capture: Shortcut {
                modifiers: held,
                key: b'R'.into(),
            },
            answer: Shortcut {
                modifiers: held,
                key: b'A'.into(),
            },
        }
    }
}

/// Reads the file, writing it out first if it is not there yet.
///
/// The file is written so that the menu has something to open and so the comments in it are the
/// documentation. Failing to write it is not worth refusing to start over: the defaults are the
/// same either way, and the log says what happened.
pub fn load(path: &Path) -> Settings {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if let Err(error) = write_default(path) {
                log::problem(&format!("could not write {}: {error}", path.display()));
            }
            DEFAULT_FILE.to_string()
        }
        Err(error) => {
            log::problem(&format!("could not read {}: {error}", path.display()));
            return Settings::default();
        }
    };

    let mut settings = Settings::default();
    for (number, line) in contents.lines().enumerate() {
        let Some((name, value)) = setting_on(line) else {
            continue;
        };
        let target = match name {
            "capture" => &mut settings.capture,
            "answer" => &mut settings.answer,
            _ => {
                log::problem(&format!(
                    "{}: line {}: no setting called {name}",
                    path.display(),
                    number + 1
                ));
                continue;
            }
        };
        match parse_shortcut(value) {
            Some(shortcut) => *target = shortcut,
            // Named rather than counted, because the person who mistyped it is reading the log
            // to find out why their shortcut does nothing.
            None => log::problem(&format!(
                "{}: line {}: {name} is not a shortcut this understands: {value}",
                path.display(),
                number + 1
            )),
        }
    }
    settings
}

fn write_default(path: &Path) -> io::Result<()> {
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }
    fs::write(path, DEFAULT_FILE)
}

/// Splits one line into a setting and its value, or nothing if it carries neither.
fn setting_on(line: &str) -> Option<(&str, &str)> {
    let line = line.split('#').next().unwrap_or("").trim();
    if line.is_empty() {
        return None;
    }
    let (name, value) = line.split_once('=')?;
    Some((name.trim(), value.trim()))
}

/// Reads `ctrl+alt+r` and the like. Case and spacing are not worth being strict about.
///
/// NOREPEAT is added to whatever is asked for rather than being offered as a choice: without it,
/// holding the combination down opens a form for every repeat the keyboard sends.
pub fn parse_shortcut(text: &str) -> Option<Shortcut> {
    let mut modifiers = 0;
    let mut key = None;

    for part in text.split('+') {
        let part = part.trim().to_ascii_lowercase();
        match part.as_str() {
            "" => return None,
            "ctrl" | "control" => modifiers |= MOD_CONTROL,
            "alt" => modifiers |= MOD_ALT,
            "shift" => modifiers |= MOD_SHIFT,
            "win" => modifiers |= MOD_WIN,
            // Two keys is as wrong as none, and silently taking the last would register a
            // shortcut nobody asked for.
            _ if key.is_some() => return None,
            _ => key = Some(virtual_key(&part)?),
        }
    }

    if modifiers == 0 {
        return None;
    }
    Some(Shortcut {
        modifiers: modifiers | MOD_NOREPEAT,
        key: key?,
    })
}

/// The keys worth naming: the ones somebody would actually reach for with three fingers already
/// occupied. Anything else is a virtual key code nobody wants to look up.
fn virtual_key(name: &str) -> Option<u32> {
    if name == "space" {
        return Some(0x20);
    }
    if let Some(number) = name.strip_prefix('f')
        && let Ok(number) = number.parse::<u32>()
        && (1..=24).contains(&number)
    {
        return Some(0x70 + number - 1);
    }

    let mut characters = name.chars();
    let single = characters.next()?;
    if characters.next().is_some() {
        return None;
    }
    match single {
        'a'..='z' => Some(single.to_ascii_uppercase() as u32),
        '0'..='9' => Some(single as u32),
        _ => None,
    }
}

/// How to write a shortcut back to somebody, in the order the modifiers are usually said.
pub fn describe(shortcut: Shortcut) -> String {
    let mut written = String::new();
    for (flag, name) in [
        (MOD_CONTROL, "Ctrl"),
        (MOD_ALT, "Alt"),
        (MOD_SHIFT, "Shift"),
        (MOD_WIN, "Win"),
    ] {
        if shortcut.modifiers & flag != 0 {
            written.push_str(name);
            written.push('+');
        }
    }
    written.push_str(&key_name(shortcut.key));
    written
}

fn key_name(key: u32) -> String {
    match key {
        0x20 => "Space".to_string(),
        0x70..=0x87 => format!("F{}", key - 0x70 + 1),
        _ => char::from_u32(key).map_or_else(|| format!("{key:#04x}"), String::from),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A file of this test's own, so the tests do not disturb each other or the real settings.
    fn temporary_path() -> std::path::PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            "reminderski-settings-{}-{}.txt",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        std::env::temp_dir().join(name)
    }

    #[test]
    fn a_first_run_writes_the_file_and_keeps_the_defaults() {
        let path = temporary_path();
        let settings = load(&path);

        assert_eq!(settings.capture, Settings::default().capture);
        assert_eq!(
            fs::read_to_string(&path).expect("the file should have been written"),
            DEFAULT_FILE
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn an_edited_file_is_what_the_shortcuts_come_from() {
        let path = temporary_path();
        fs::write(&path, "# mine\ncapture = win+shift+f9\n").expect("could not write");

        let settings = load(&path);
        assert_eq!(settings.capture, shortcut("win+shift+f9"));
        // Left out of the file, so it stays what it has always been.
        assert_eq!(settings.answer, Settings::default().answer);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn a_shortcut_it_cannot_read_leaves_the_one_that_worked() {
        let path = temporary_path();
        fs::write(&path, "capture = ctrl+nonsense\n").expect("could not write");

        // Refusing to start would take away the shortcut they were editing it to fix.
        assert_eq!(load(&path).capture, Settings::default().capture);
        let _ = fs::remove_file(&path);
    }

    fn shortcut(text: &str) -> Shortcut {
        parse_shortcut(text).expect("should have read a shortcut")
    }

    #[test]
    fn reads_the_shortcut_the_application_has_always_had() {
        assert_eq!(shortcut("ctrl+alt+r"), Settings::default().capture);
    }

    #[test]
    fn case_and_spacing_are_not_worth_being_strict_about() {
        assert_eq!(shortcut("  CTRL + Alt +R "), shortcut("ctrl+alt+r"));
    }

    #[test]
    fn function_keys_and_the_space_bar_can_be_asked_for() {
        assert_eq!(shortcut("ctrl+f9").key, 0x78);
        assert_eq!(shortcut("ctrl+shift+space").key, 0x20);
    }

    #[test]
    fn every_shortcut_stops_the_keyboard_repeating_it() {
        assert!(shortcut("win+k").modifiers & MOD_NOREPEAT != 0);
    }

    #[test]
    fn a_shortcut_without_a_modifier_is_refused() {
        // It would take that key away from every other application on the machine.
        assert_eq!(parse_shortcut("r"), None);
    }

    #[test]
    fn what_it_cannot_read_is_refused_rather_than_guessed() {
        for text in [
            "", "ctrl+", "ctrl+alt", "ctrl+rr", "ctrl+f0", "ctrl+f25", "ctrl+a+b",
        ] {
            assert_eq!(parse_shortcut(text), None, "{text} should not have read");
        }
    }

    #[test]
    fn a_shortcut_is_written_back_the_way_it_is_said() {
        assert_eq!(describe(shortcut("alt+ctrl+r")), "Ctrl+Alt+R");
        assert_eq!(describe(shortcut("ctrl+shift+space")), "Ctrl+Shift+Space");
        assert_eq!(describe(shortcut("win+f12")), "Win+F12");
    }

    #[test]
    fn a_line_carries_a_setting_a_comment_or_neither() {
        assert_eq!(
            setting_on("capture = ctrl+alt+r"),
            Some(("capture", "ctrl+alt+r"))
        );
        assert_eq!(
            setting_on("  answer=win+k  # mine"),
            Some(("answer", "win+k"))
        );
        assert_eq!(setting_on("# just a comment"), None);
        assert_eq!(setting_on("   "), None);
        assert_eq!(setting_on("nonsense"), None);
    }

    #[test]
    fn the_file_written_on_a_first_run_says_what_the_defaults_are() {
        let mut settings = Settings::default();
        for line in DEFAULT_FILE.lines() {
            if let Some((name, value)) = setting_on(line)
                && let Some(parsed) = parse_shortcut(value)
            {
                match name {
                    "capture" => settings.capture = parsed,
                    "answer" => settings.answer = parsed,
                    _ => panic!("the default file sets {name}, which load does not read"),
                }
            }
        }
        assert_eq!(settings.capture, Settings::default().capture);
        assert_eq!(settings.answer, Settings::default().answer);
    }
}
