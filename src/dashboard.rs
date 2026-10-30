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

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_NULL};

use crate::log;
use crate::reminder::{self, Id, State};
use crate::store::Store;

/// Asked for first, so the address stays the same between runs and can be bookmarked.
const PREFERRED_PORT: u16 = 7654;

/// The page itself, built into the executable so there is still only one file to download.
const PAGE: &str = include_str!("dashboard.html");

/// More than any reminder anybody writes by hand, and little enough to hold in memory without a
/// second thought.
const LONGEST_BODY: usize = 64 * 1024;

/// Serves the dashboard until the process ends. Returns the address it ended up on.
pub fn start(store: Arc<Mutex<Store>>, wake: u32) -> io::Result<String> {
    let listener = bind()?;
    let address = format!("http://{}", listener.local_addr()?);
    let port = listener.local_addr()?.port();

    thread::spawn(move || {
        for connection in listener.incoming() {
            match connection {
                Ok(stream) => serve(stream, &store, wake, port),
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

fn serve(mut stream: TcpStream, store: &Arc<Mutex<Store>>, wake: u32, port: u16) {
    // Only one connection is served at a time, so one that goes quiet halfway through a request
    // would otherwise hold up the page for good.
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));

    let Some(request) = read_request(&stream) else {
        return;
    };
    let mut words = request.line.split_whitespace();
    let (Some(method), Some(target)) = (words.next(), words.next()) else {
        return;
    };

    let answer = if trusted(&request, method, port) {
        route(method, target, &request.body, store, wake)
    } else {
        Answer::forbidden()
    };
    if let Err(error) = answer.write_to(&mut stream) {
        // A browser that navigated away mid-response is not worth reporting.
        if error.kind() != io::ErrorKind::BrokenPipe {
            log::problem(&format!("dashboard reply failed: {error}"));
        }
    }
}

struct Request {
    line: String,
    /// The name the browser used to reach this server, port included.
    host: String,
    /// Whether the request carries the header only this server's own page sends.
    from_the_page: bool,
    body: String,
}

/// Sent by the page with every change. The value means nothing; the header being there at all
/// is what counts.
const PAGE_HEADER: &str = "x-reminderski";

/// Whether a request can be answered: whether it came from this server's own page, rather than
/// from some other page open in the same browser.
///
/// Any page on any site can send requests to 127.0.0.1, and two things keep the ones from
/// elsewhere from doing anything. The host must be this machine's own name, because a site can
/// point its own name at 127.0.0.1, and the browser would then treat this server as part of that
/// site and let it read the answers. And a change must carry PAGE_HEADER, because a page cannot
/// add a header of its own to a request for another site without first asking that site's
/// permission, which this server never gives. A page elsewhere can still ask for the list, but
/// the browser will not let it read the answer.
fn trusted(request: &Request, method: &str, port: u16) -> bool {
    let here = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    let host_is_here = here
        .iter()
        .any(|name| request.host.eq_ignore_ascii_case(name));
    host_is_here && (method == "GET" || request.from_the_page)
}

/// The first line, the body, and the two headers worth knowing. The rest are read and dropped,
/// so that the browser is not left writing into a socket nobody is reading.
fn read_request(stream: &TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }

    let mut length = 0;
    let mut host = String::new();
    let mut from_the_page = false;
    let mut header = String::new();
    loop {
        header.clear();
        match reader.read_line(&mut header) {
            Ok(0) => break,
            Ok(_) if header.trim().is_empty() => break,
            Ok(_) => {
                let Some((name, value)) = header.split_once(':') else {
                    continue;
                };
                let (name, value) = (name.trim().to_ascii_lowercase(), value.trim());
                match name.as_str() {
                    "content-length" => length = value.parse().ok()?,
                    "host" => host = value.to_string(),
                    PAGE_HEADER => from_the_page = true,
                    _ => {}
                }
            }
            Err(_) => break,
        }
    }

    if length > LONGEST_BODY {
        return None;
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    let body = String::from_utf8(body).ok()?;
    Some(Request {
        line,
        host,
        from_the_page,
        body,
    })
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

    fn forbidden() -> Self {
        Self {
            status: "403 Forbidden",
            kind: "text/plain; charset=utf-8",
            body: "only the dashboard's own page may ask\n".to_string(),
        }
    }

    fn missing() -> Self {
        Self {
            status: "404 Not Found",
            kind: "text/plain; charset=utf-8",
            body: "no such thing here\n".to_string(),
        }
    }

    /// Said in words, because the page shows it to the reader as it is.
    fn refused(reason: &str) -> Self {
        Self {
            status: "400 Bad Request",
            kind: "text/plain; charset=utf-8",
            body: reason.to_string(),
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

fn route(method: &str, target: &str, body: &str, store: &Arc<Mutex<Store>>, wake: u32) -> Answer {
    match (method, target) {
        ("GET", "/") => Answer::html(PAGE),
        ("GET", "/reminders") => Answer::json(list(store)),
        ("POST", _) if target.ends_with("/edit") => match edit(target, body, store) {
            Some(Ok(())) => {
                wake_the_loop(wake);
                Answer::json("{\"ok\":true}".to_string())
            }
            Some(Err(reason)) => Answer::refused(reason),
            None => Answer::missing(),
        },
        ("POST", _) => match act(target, store) {
            Some(body) => {
                wake_the_loop(wake);
                Answer::json(body)
            }
            None => Answer::missing(),
        },
        _ => Answer::missing(),
    }
}

/// The message loop may be asleep with nothing due for an hour. Nudging it makes it work out
/// again when the next reminder is, now that one has moved.
fn wake_the_loop(wake: u32) {
    unsafe { PostThreadMessageW(wake, WM_NULL, 0, 0) };
}

/// Every reminder, as the page wants them. The time is sent as an instant and turned into
/// something readable in the browser, which already knows the reader's clock and language.
fn list(store: &Arc<Mutex<Store>>) -> String {
    let Ok(store) = store.lock() else {
        return "[]".to_string();
    };

    let shown = store
        .reminders
        .iter()
        .filter(|reminder| reminder.state != State::Deleted);

    let mut json = String::from("[");
    for (index, reminder) in shown.enumerate() {
        if index > 0 {
            json.push(',');
        }
        json.push_str(&format!(
            "{{\"id\":\"{}\",\"due\":{},\"snoozes\":{},\"done\":{},\"text\":\"{}\"}}",
            reminder.id,
            reminder.due_unix,
            reminder.snoozes,
            reminder.state == State::Done,
            escape(&reminder.text)
        ));
    }
    json.push(']');
    json
}

/// `/reminders/<id>/done`, or `/reminders/<id>/snooze/30` for thirty minutes.
fn act(target: &str, store: &Arc<Mutex<Store>>) -> Option<String> {
    let mut parts = target.trim_start_matches('/').split('/');
    if parts.next()? != "reminders" {
        return None;
    }
    let id = Id::parse(parts.next()?)?;
    let action = parts.next()?;

    let mut store = store.lock().ok()?;
    let reminder = store.find(id)?;

    match action {
        "done" => reminder.state = State::Done,
        "pending" => reminder.state = State::Pending,
        "delete" => reminder.state = State::Deleted,
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

/// `/reminders/<id>/edit`. The body is the new time on its first line, written the way the form
/// takes it, and the new text after it. A blank time keeps the one the reminder has.
///
/// None when there is no such waiting reminder; a reason, for the reader, when what was asked
/// for cannot be done.
fn edit(target: &str, body: &str, store: &Arc<Mutex<Store>>) -> Option<Result<(), &'static str>> {
    let id = target.strip_prefix("/reminders/")?.strip_suffix("/edit")?;
    let id = Id::parse(id)?;

    let (when, text) = body.split_once('\n').unwrap_or((body, ""));
    if text.trim().is_empty() {
        return Some(Err("A reminder needs something to remind you of."));
    }
    let delay = match when.trim() {
        "" => None,
        when => match reminder::parse_when(when) {
            Some(delay) => Some(delay),
            None => return Some(Err("That time cannot be read.")),
        },
    };

    let mut store = store.lock().ok()?;
    let reminder = store.find(id)?;
    if reminder.state != State::Pending {
        return None;
    }
    reminder.text = text.to_string();
    if let Some(delay) = delay {
        reminder.due_unix = reminder::now_unix() + delay.as_secs();
    }

    if let Err(error) = store.save() {
        log::problem(&format!("dashboard could not save: {error}"));
    }
    Some(Ok(()))
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

    /// Where the dashboard sends an action for the reminder at this position.
    fn target(store: &Arc<Mutex<Store>>, position: usize, action: &str) -> String {
        let id = store.lock().unwrap().reminders[position].id;
        format!("/reminders/{id}/{action}")
    }

    /// What the dashboard reads from a browser that sends exactly these bytes and then stops.
    fn request_from(raw: &[u8]) -> Option<Request> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let mut browser = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        browser.write_all(raw).unwrap();
        browser.shutdown(std::net::Shutdown::Write).unwrap();
        let (stream, _) = listener.accept().unwrap();
        read_request(&stream)
    }

    #[test]
    fn reads_the_body_a_request_says_it_has() {
        let body = "in 2h\nZażółć gęślą jaźń";
        let raw = format!(
            "POST /x HTTP/1.1\r\nHost: here\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );

        let request = request_from(raw.as_bytes()).unwrap();
        assert!(request.line.starts_with("POST /x "));
        assert_eq!(request.body, body);
    }

    #[test]
    fn reads_who_a_request_was_for_and_whether_the_page_sent_it() {
        let request =
            request_from(b"POST /x HTTP/1.1\r\nHost: 127.0.0.1:7654\r\nX-Reminderski: 1\r\n\r\n")
                .unwrap();
        assert_eq!(request.host, "127.0.0.1:7654");
        assert!(request.from_the_page);

        let request = request_from(b"POST /x HTTP/1.1\r\nHost: 127.0.0.1:7654\r\n\r\n").unwrap();
        assert!(!request.from_the_page);
    }

    fn asked(host: &str, from_the_page: bool) -> Request {
        Request {
            line: String::new(),
            host: host.to_string(),
            from_the_page,
            body: String::new(),
        }
    }

    #[test]
    fn the_page_itself_may_read_and_change() {
        assert!(trusted(&asked("127.0.0.1:7654", false), "GET", 7654));
        assert!(trusted(&asked("127.0.0.1:7654", true), "POST", 7654));
        assert!(trusted(&asked("LOCALHOST:7654", true), "POST", 7654));
    }

    #[test]
    fn another_site_may_not_change_anything() {
        // A plain form or fetch from another page cannot carry the header.
        assert!(!trusted(&asked("127.0.0.1:7654", false), "POST", 7654));
    }

    #[test]
    fn a_site_that_points_its_own_name_here_is_turned_away() {
        assert!(!trusted(&asked("evil.example:7654", false), "GET", 7654));
        assert!(!trusted(&asked("evil.example:7654", true), "POST", 7654));
        assert!(!trusted(&asked("", false), "GET", 7654));
    }

    #[test]
    fn the_port_has_to_be_the_one_it_is_listening_on() {
        // It falls back to another port when 7654 is taken.
        assert!(!trusted(&asked("127.0.0.1:7654", false), "GET", 50123));
        assert!(trusted(&asked("127.0.0.1:50123", false), "GET", 50123));
    }

    /// The status line a running dashboard answers these bytes with.
    fn status_of(address: &str, raw: &str) -> String {
        let mut connection = TcpStream::connect(address.trim_start_matches("http://")).unwrap();
        connection.write_all(raw.as_bytes()).unwrap();
        let mut answer = String::new();
        BufReader::new(connection).read_line(&mut answer).unwrap();
        answer.trim().to_string()
    }

    #[test]
    fn a_running_dashboard_only_takes_changes_from_its_own_page() {
        let (store, path) = store_with(&["water the plants"]);
        let id = store.lock().unwrap().reminders[0].id;
        // A thread that does not exist: nudging the message loop simply goes nowhere.
        let address = start(store.clone(), 0).unwrap();
        let host = address.trim_start_matches("http://");
        let done = format!("POST /reminders/{id}/done HTTP/1.1\r\nHost: {host}\r\n");

        let from_elsewhere = status_of(&address, &format!("{done}\r\n"));
        assert!(from_elsewhere.contains("403"), "{from_elsewhere}");
        assert!(store.lock().unwrap().reminders[0].state == State::Pending);

        let rebound = "GET /reminders HTTP/1.1\r\nHost: evil.example\r\n\r\n";
        assert!(status_of(&address, rebound).contains("403"));

        let from_the_page = status_of(&address, &format!("{done}X-Reminderski: 1\r\n\r\n"));
        assert!(from_the_page.contains("200"), "{from_the_page}");
        assert!(store.lock().unwrap().reminders[0].state == State::Done);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_request_without_a_length_has_no_body() {
        let request = request_from(b"GET / HTTP/1.1\r\nHost: here\r\n\r\n").unwrap();
        assert_eq!(request.body, "");
    }

    #[test]
    fn refuses_a_body_too_long_or_shorter_than_it_claims() {
        let huge = format!(
            "POST /x HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            LONGEST_BODY + 1
        );
        assert!(request_from(huge.as_bytes()).is_none());
        assert!(request_from(b"POST /x HTTP/1.1\r\nContent-Length: 10\r\n\r\nshort").is_none());
    }

    #[test]
    fn editing_changes_the_text_and_the_time() {
        let (store, path) = store_with(&["water the plants"]);
        let target = target(&store, 0, "edit");

        assert_eq!(
            edit(&target, "in 2h\nwater the cactus", &store),
            Some(Ok(()))
        );
        let store_guard = store.lock().unwrap();
        let reminder = &store_guard.reminders[0];
        assert_eq!(reminder.text, "water the cactus");
        let due_in = reminder.due_unix - reminder::now_unix();
        assert!((7199..=7201).contains(&due_in), "due in {due_in}s");
        drop(store_guard);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn editing_with_a_blank_time_keeps_the_time() {
        let (store, path) = store_with(&["water the plants"]);
        let due = store.lock().unwrap().reminders[0].due_unix;

        let target = target(&store, 0, "edit");
        assert_eq!(
            edit(&target, "  \nfirst line\nsecond line", &store),
            Some(Ok(()))
        );
        let store_guard = store.lock().unwrap();
        assert_eq!(store_guard.reminders[0].text, "first line\nsecond line");
        assert_eq!(store_guard.reminders[0].due_unix, due);
        drop(store_guard);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_edit_that_cannot_be_read_changes_nothing() {
        let (store, path) = store_with(&["water the plants"]);
        let due = store.lock().unwrap().reminders[0].due_unix;
        let target = target(&store, 0, "edit");

        assert!(matches!(
            edit(&target, "whenever\nnew text", &store),
            Some(Err(_))
        ));
        assert!(matches!(edit(&target, "in 2h\n   ", &store), Some(Err(_))));
        assert!(matches!(edit(&target, "in 2h", &store), Some(Err(_))));

        let store_guard = store.lock().unwrap();
        assert_eq!(store_guard.reminders[0].text, "water the plants");
        assert_eq!(store_guard.reminders[0].due_unix, due);
        drop(store_guard);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn only_a_waiting_reminder_can_be_edited() {
        let (store, path) = store_with(&["water the plants"]);
        let target = target(&store, 0, "edit");
        store.lock().unwrap().reminders[0].state = State::Done;

        assert_eq!(edit(&target, "in 2h\nnew text", &store), None);
        let unknown = format!("/reminders/{}/edit", Id::random());
        assert_eq!(edit(&unknown, "in 2h\nnew text", &store), None);
        assert_eq!(edit("/reminders/0/edit", "in 2h\nnew text", &store), None);
        assert_eq!(store.lock().unwrap().reminders[0].text, "water the plants");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn lists_every_reminder_with_its_id() {
        let (store, path) = store_with(&["water the plants", "call the dentist"]);
        let ids: Vec<Id> = (0..2)
            .map(|position| store.lock().unwrap().reminders[position].id)
            .collect();
        let json = list(&store);

        assert!(json.starts_with('['));
        assert!(json.contains(&format!("\"id\":\"{}\"", ids[0])));
        assert!(json.contains("\"text\":\"water the plants\""));
        assert!(json.contains(&format!("\"id\":\"{}\"", ids[1])));
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
        assert!(act(&target(&store, 0, "done"), &store).is_some());
        assert!(store.lock().unwrap().reminders[0].state == State::Done);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn snoozing_moves_it_and_counts() {
        let (store, path) = store_with(&["water the plants"]);
        assert!(act(&target(&store, 0, "snooze/30"), &store).is_some());

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
        assert!(act(&target(&store, 0, "explode"), &store).is_none());
        assert!(act(&format!("/reminders/{}/done", Id::random()), &store).is_none());
        assert!(act("/reminders/0/done", &store).is_none());
        assert!(act("/nonsense", &store).is_none());
        assert!(act(&target(&store, 0, "snooze"), &store).is_none());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_deleted_reminder_is_no_longer_listed() {
        let (store, path) = store_with(&["water the plants", "call the dentist"]);
        assert!(act(&target(&store, 0, "delete"), &store).is_some());

        let json = list(&store);
        assert!(!json.contains("water the plants"));
        assert!(json.contains("call the dentist"));
        assert!(store.lock().unwrap().reminders[0].state == State::Deleted);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_deleted_reminder_cannot_be_brought_back() {
        let (store, path) = store_with(&["water the plants"]);
        assert!(act(&target(&store, 0, "delete"), &store).is_some());

        assert!(act(&target(&store, 0, "pending"), &store).is_none());
        assert!(act(&target(&store, 0, "done"), &store).is_none());
        assert!(act(&target(&store, 0, "snooze/30"), &store).is_none());
        assert!(act(&target(&store, 0, "delete"), &store).is_none());
        assert!(store.lock().unwrap().reminders[0].state == State::Deleted);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_action_reaches_the_same_reminder_after_others_are_added_before_it() {
        let (store, path) = store_with(&["water the plants"]);
        let wanted = target(&store, 0, "done");
        store.lock().unwrap().reminders.insert(
            0,
            crate::reminder::Reminder::due_in(
                std::time::Duration::from_secs(600),
                "call the dentist".to_string(),
            ),
        );

        assert!(act(&wanted, &store).is_some());
        let store_guard = store.lock().unwrap();
        assert!(store_guard.reminders[0].state == State::Pending);
        assert!(store_guard.reminders[1].state == State::Done);
        drop(store_guard);
        let _ = std::fs::remove_file(&path);
    }
}
