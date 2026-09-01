//! A page listing the reminders, served on the loopback address.
//!
//! It runs on a thread of its own. The message loop answers the shortcut and paints the
//! notification, and a browser asking for the page must never be able to make it wait, so the
//! two share the store through a lock that is held for a few microseconds at a time and never
//! across an open window.
//!
//! The HTTP here is only as much as a browser on the same machine needs: one request per
//! connection, no keep-alive, no compression, no ranges. It is deliberately small rather than
//! general, and it listens on 127.0.0.1 so nothing outside this machine can reach it.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_NULL};

use crate::log;
use crate::reminder::{self, State};
use crate::store::Store;

/// Asked for first, so the address stays the same between runs and can be bookmarked.
const PREFERRED_PORT: u16 = 7654;

/// The page itself, built into the executable so there is still only one file to download.
const PAGE: &str = include_str!("dashboard.html");

/// Serves the dashboard until the process ends. Returns the address it ended up on.
pub fn start(store: Arc<Mutex<Store>>, wake: u32) -> io::Result<String> {
    let listener = bind()?;
    let address = format!("http://{}", listener.local_addr()?);

    thread::spawn(move || {
        for connection in listener.incoming() {
            match connection {
                Ok(stream) => serve(stream, &store, wake),
                Err(error) => log::problem(&format!("dashboard connection failed: {error}")),
            }
        }
    });

    Ok(address)
}

/// The preferred port, or any free one when something else already has it.
fn bind() -> io::Result<TcpListener> {
    let preferred = SocketAddr::from((Ipv4Addr::LOCALHOST, PREFERRED_PORT));
    match TcpListener::bind(preferred) {
        Ok(listener) => Ok(listener),
        Err(_) => TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))),
    }
}

fn serve(mut stream: TcpStream, store: &Arc<Mutex<Store>>, wake: u32) {
    let Some(request) = read_request_line(&stream) else {
        return;
    };
    let mut words = request.split_whitespace();
    let (Some(method), Some(target)) = (words.next(), words.next()) else {
        return;
    };

    let answer = route(method, target, store, wake);
    if let Err(error) = answer.write_to(&mut stream) {
        // A browser that navigated away mid-response is not worth reporting.
        if error.kind() != io::ErrorKind::BrokenPipe {
            log::problem(&format!("dashboard reply failed: {error}"));
        }
    }
}

/// The first line, with the rest of the headers read and dropped so the browser is not left
/// writing into a socket nobody is reading.
fn read_request_line(stream: &TcpStream) -> Option<String> {
    let mut reader = BufReader::new(stream);
    let mut first = String::new();
    if reader.read_line(&mut first).ok()? == 0 {
        return None;
    }

    let mut header = String::new();
    loop {
        header.clear();
        match reader.read_line(&mut header) {
            Ok(0) => break,
            Ok(_) if header.trim().is_empty() => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    Some(first)
}

struct Answer {
    status: &'static str,
    kind: &'static str,
    body: String,
}

impl Answer {
    fn html(body: &str) -> Self {
        Self {
            status: "200 OK",
            kind: "text/html; charset=utf-8",
            body: body.to_string(),
        }
    }

    fn json(body: String) -> Self {
        Self {
            status: "200 OK",
            kind: "application/json; charset=utf-8",
            body,
        }
    }

    fn missing() -> Self {
        Self {
            status: "404 Not Found",
            kind: "text/plain; charset=utf-8",
            body: "no such thing here\n".to_string(),
        }
    }

    fn write_to(&self, stream: &mut TcpStream) -> io::Result<()> {
        // Closing after each reply is what keeps a browser's idle connection from holding up
        // the next request, which matters when only one is served at a time.
        write!(
            stream,
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            self.status,
            self.kind,
            self.body.len(),
            self.body
        )?;
        stream.flush()
    }
}

fn route(method: &str, target: &str, store: &Arc<Mutex<Store>>, wake: u32) -> Answer {
    match (method, target) {
        ("GET", "/") => Answer::html(PAGE),
        ("GET", "/reminders") => Answer::json(list(store)),
        ("POST", _) => match act(target, store) {
            Some(body) => {
                // The message loop may be asleep with nothing due for an hour. Nudging it makes
                // it work out again when the next reminder is, now that one has moved.
                unsafe { PostThreadMessageW(wake, WM_NULL, 0, 0) };
                Answer::json(body)
            }
            None => Answer::missing(),
        },
        _ => Answer::missing(),
    }
}

/// Every reminder, as the page wants them. The time is sent as an instant and turned into
/// something readable in the browser, which already knows the reader's clock and language.
fn list(store: &Arc<Mutex<Store>>) -> String {
    let Ok(store) = store.lock() else {
        return "[]".to_string();
    };

    let mut json = String::from("[");
    for (index, reminder) in store.reminders.iter().enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            "{{\"index\":{},\"due\":{},\"snoozes\":{},\"done\":{},\"text\":\"{}\"}}",
            index,
            reminder.due_unix,
            reminder.snoozes,
            reminder.state == State::Done,
            escape(&reminder.text)
        ));
    }
    json.push(']');
    json
}

/// `/reminders/3/done`, or `/reminders/3/snooze/30` for thirty minutes.
fn act(target: &str, store: &Arc<Mutex<Store>>) -> Option<String> {
    let mut parts = target.trim_start_matches('/').split('/');
    if parts.next()? != "reminders" {
        return None;
    }
    let index: usize = parts.next()?.parse().ok()?;
    let action = parts.next()?;

    let mut store = store.lock().ok()?;
    let reminder = store.reminders.get_mut(index)?;

    match action {
        "done" => reminder.state = State::Done,
        "pending" => reminder.state = State::Pending,
        "snooze" => {
            let minutes: u64 = parts.next()?.parse().ok()?;
            reminder.due_unix = reminder::now_unix() + minutes * 60;
            reminder.state = State::Pending;
            reminder.snoozes += 1;
        }
        _ => return None,
    }

    if let Err(error) = store.save() {
        log::problem(&format!("dashboard could not save: {error}"));
    }
    Some("{\"ok\":true}".to_string())
}

fn escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            // Anything below a space would end the string early or be read as a control code.
            character if (character as u32) < 0x20 => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A store of its own, and the file it lives in so the test can take it away again.
    fn store_with(texts: &[&str]) -> (Arc<Mutex<Store>>, PathBuf) {
        // Numbered, because the tests run at the same time and would otherwise share a file.
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "reminderski-dashboard-{}-{}.txt",
            std::process::id(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        ));
        let mut store = Store::open(path.clone()).unwrap();
        for text in texts {
            store.reminders.push(crate::reminder::Reminder::due_in(
                std::time::Duration::from_secs(600),
                (*text).to_string(),
            ));
        }
        (Arc::new(Mutex::new(store)), path)
    }

    #[test]
    fn lists_every_reminder_with_its_index() {
        let (store, path) = store_with(&["water the plants", "call the dentist"]);
        let json = list(&store);

        assert!(json.starts_with('['));
        assert!(json.contains("\"index\":0"));
        assert!(json.contains("\"text\":\"water the plants\""));
        assert!(json.contains("\"index\":1"));
        assert!(json.contains("\"done\":false"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn escapes_text_that_would_break_the_json() {
        let (store, path) = store_with(&["say \"hello\"\nand \\ goodbye"]);
        let json = list(&store);

        assert!(json.contains(r#"say \"hello\"\nand \\ goodbye"#));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn marking_done_changes_the_reminder() {
        let (store, path) = store_with(&["water the plants"]);
        assert!(act("/reminders/0/done", &store).is_some());
        assert!(store.lock().unwrap().reminders[0].state == State::Done);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn snoozing_moves_it_and_counts() {
        let (store, path) = store_with(&["water the plants"]);
        assert!(act("/reminders/0/snooze/30", &store).is_some());

        let store_guard = store.lock().unwrap();
        let reminder = &store_guard.reminders[0];
        let due_in = reminder.due_unix - reminder::now_unix();
        assert!((1799..=1801).contains(&due_in), "due in {due_in}s");
        assert_eq!(reminder.snoozes, 1);
        drop(store_guard);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn refuses_anything_it_does_not_recognise() {
        let (store, path) = store_with(&["water the plants"]);
        assert!(act("/reminders/0/explode", &store).is_none());
        assert!(act("/reminders/9/done", &store).is_none());
        assert!(act("/nonsense", &store).is_none());
        assert!(act("/reminders/0/snooze", &store).is_none());
        let _ = std::fs::remove_file(&path);
    }
}
