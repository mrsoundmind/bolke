//! Names from the call (design/wireframes/MeetingNames.dc.html).
//!
//! The Bolke browser add-on watches a Google Meet call and tells Bolke who is
//! in it and whose tile lights up while they talk. Bolke listens for that on
//! 127.0.0.1 only, keeps it while a meeting is recording, and saves it beside
//! the recording. After the meeting, each voice gets the name whose talking
//! lines up with it (see `managers::notes::name_from_call`).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use log::{debug, info, warn};
use serde::{Deserialize, Serialize};

/// The add-on sends its news here.
pub const PORT: u16 = 47_213;
/// Nothing the add-on sends is anywhere near this big.
const MAX_BODY: usize = 256 * 1024;
const SIDECAR_SUFFIX: &str = ".call-names.json";

/// One stretch of someone talking, in ms from the recording's start.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Speech {
    pub name: String,
    pub start_ms: i64,
    pub end_ms: i64,
}

/// What the call showed during one meeting.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CallNames {
    /// "meet" for Google Meet.
    pub platform: String,
    /// Everyone the call listed, in the order first seen.
    pub people: Vec<String>,
    /// The name the call shows for this computer's user.
    pub self_name: Option<String>,
    pub speaking: Vec<Speech>,
}

/// What the add-on sends, every couple of seconds during a call.
#[derive(Debug, Deserialize)]
struct Report {
    platform: String,
    #[serde(default)]
    people: Vec<String>,
    #[serde(default, rename = "self")]
    self_name: Option<String>,
    /// Finished stretches of talking, in ms since the Unix epoch.
    #[serde(default)]
    speaking: Vec<ReportSpeech>,
}

#[derive(Debug, Deserialize)]
struct ReportSpeech {
    name: String,
    start: i64,
    end: i64,
}

struct Collecting {
    /// Unix ms when the recording started.
    started_ms: i64,
    sidecar: PathBuf,
    names: CallNames,
}

static COLLECTING: Mutex<Option<Collecting>> = Mutex::new(None);

fn collecting() -> std::sync::MutexGuard<'static, Option<Collecting>> {
    COLLECTING.lock().unwrap_or_else(|e| e.into_inner())
}

/// Where a recording's names are kept.
pub fn sidecar_for(wav: &Path) -> PathBuf {
    let name = wav
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    wav.with_file_name(format!("{name}{SIDECAR_SUFFIX}"))
}

/// Starts keeping what the add-on reports, for the recording at `wav`.
pub fn begin(wav: &Path, started_ms: i64) {
    *collecting() = Some(Collecting {
        started_ms,
        sidecar: sidecar_for(wav),
        names: CallNames::default(),
    });
}

/// Stops keeping reports. What came in is already on disk.
pub fn end() {
    if let Some(c) = collecting().take() {
        if !c.names.people.is_empty() {
            info!(
                "Names from the call: {} people, {} stretches of talking",
                c.names.people.len(),
                c.names.speaking.len()
            );
        }
    }
}

/// The names saved beside a recording, if the add-on sent any.
pub fn load(wav: &Path) -> Option<CallNames> {
    let text = std::fs::read_to_string(sidecar_for(wav)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Deletes the names saved beside a recording.
pub fn remove(wav: &Path) {
    let _ = std::fs::remove_file(sidecar_for(wav));
}

/// Whether a meeting is recording and nothing but the screen has told
/// Bolke names yet (the add-on, when there, is better).
pub fn wants_screen() -> bool {
    collecting()
        .as_ref()
        .is_some_and(|c| c.names.platform.is_empty() || c.names.platform.starts_with("screen"))
}

/// Adds what Bolke saw in the call window: `speaking` holds stretches of
/// talking in ms since the Unix epoch.
pub fn add_from_screen(app: &str, people: Vec<String>, speaking: Vec<(String, i64, i64)>) -> bool {
    take_report(Report {
        platform: format!("screen-{app}"),
        people,
        self_name: None,
        speaking: speaking
            .into_iter()
            .map(|(name, start, end)| ReportSpeech { name, start, end })
            .collect(),
    })
}

/// Adds a report to the meeting being recorded. Returns whether one is.
fn take_report(report: Report) -> bool {
    let mut guard = collecting();
    let Some(c) = guard.as_mut() else {
        return false;
    };
    let names = &mut c.names;
    names.platform = report.platform;
    if let Some(me) = report.self_name.filter(|n| !n.trim().is_empty()) {
        names.self_name = Some(me);
    }
    for person in report.people {
        let person = person.trim().to_string();
        if !person.is_empty() && !names.people.contains(&person) {
            names.people.push(person);
        }
    }
    for s in report.speaking {
        let start_ms = s.start - c.started_ms;
        let end_ms = s.end - c.started_ms;
        // Talk from before the recording started doesn't count.
        if end_ms <= 0 || end_ms <= start_ms || s.name.trim().is_empty() {
            continue;
        }
        let name = s.name.trim().to_string();
        if !names.people.contains(&name) {
            names.people.push(name.clone());
        }
        names.speaking.push(Speech {
            name,
            start_ms: start_ms.max(0),
            end_ms,
        });
    }
    match serde_json::to_string(names) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&c.sidecar, json) {
                warn!("Couldn't save the names from the call: {e}");
            }
        }
        Err(e) => warn!("Couldn't save the names from the call: {e}"),
    }
    true
}

/// Listens for the add-on on 127.0.0.1, for as long as Bolke runs.
pub fn listen() {
    std::thread::spawn(|| {
        let listener = match TcpListener::bind(("127.0.0.1", PORT)) {
            Ok(l) => l,
            Err(e) => {
                warn!("Names from the call are off: can't listen on port {PORT}: {e}");
                return;
            }
        };
        debug!("Listening for the Bolke add-on on 127.0.0.1:{PORT}");
        for stream in listener.incoming().flatten() {
            if let Err(e) = answer(stream) {
                debug!("A request from the add-on failed: {e}");
            }
        }
    });
}

/// Handles one request. Only the Bolke add-on (an extension page, never a
/// website) may report; anyone may ask if Bolke is there.
fn answer(stream: TcpStream) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut length = 0usize;
    let mut origin = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
            break;
        }
        if let Some((key, value)) = line.split_once(':') {
            match key.trim().to_ascii_lowercase().as_str() {
                "content-length" => length = value.trim().parse().unwrap_or(0),
                "origin" => origin = value.trim().to_string(),
                _ => {}
            }
        }
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");
    let from_addon =
        origin.starts_with("chrome-extension://") || origin.starts_with("moz-extension://");

    let (status, body) = match (method, path) {
        ("GET", "/bolke") => ("200 OK", status_json()),
        ("POST", "/call") if !from_addon => ("403 Forbidden", "{}".to_string()),
        ("POST", "/call") if length > MAX_BODY => ("413 Payload Too Large", "{}".to_string()),
        ("POST", "/call") => {
            let mut body = vec![0u8; length];
            reader.read_exact(&mut body)?;
            match serde_json::from_slice::<Report>(&body) {
                Ok(report) => {
                    take_report(report);
                    ("200 OK", status_json())
                }
                Err(_) => ("400 Bad Request", "{}".to_string()),
            }
        }
        _ => ("404 Not Found", "{}".to_string()),
    };
    let mut stream = stream;
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

fn status_json() -> String {
    let recording = collecting().is_some();
    format!(r#"{{"app":"bolke","recording":{recording}}}"#)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_are_kept_relative_to_the_recording() {
        let dir = std::env::temp_dir().join(format!("bolke-call-names-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wav = dir.join("m.wav");
        begin(&wav, 10_000);
        let report: Report = serde_json::from_str(
            r#"{"platform":"meet","self":"Sam","people":["Sam","Priya"],
                "speaking":[{"name":"Priya","start":9000,"end":12000},
                            {"name":"Rahul","start":13000,"end":15000},
                            {"name":"Priya","start":5000,"end":8000}]}"#,
        )
        .unwrap();
        assert!(take_report(report));
        end();
        let names = load(&wav).unwrap();
        assert_eq!(names.self_name.as_deref(), Some("Sam"));
        assert_eq!(names.people, vec!["Sam", "Priya", "Rahul"]);
        assert_eq!(
            names.speaking,
            vec![
                Speech {
                    name: "Priya".into(),
                    start_ms: 0,
                    end_ms: 2000
                },
                Speech {
                    name: "Rahul".into(),
                    start_ms: 3000,
                    end_ms: 5000
                },
            ]
        );
        remove(&wav);
        assert!(load(&wav).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
