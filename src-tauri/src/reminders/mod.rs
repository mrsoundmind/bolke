//! Reminders: "remind me…" said with either shortcut becomes a to-do with a
//! time, and Bolke rings it then, with a notification and the bubble.
//!
//! Dictation is only caught when it starts like a reminder, and the bubble
//! offers to type the words instead, in case they were meant for the page.

pub mod heard;

pub use heard::Heard;
use heard::{Change, SpokenChange};

use crate::managers::notes::NotesManager;
use crate::managers::todos::Todo;
use chrono::{Local, TimeZone};
use log::{error, info, warn};
use serde::Serialize;
use specta::Type;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// How often due reminders are looked for.
const CHECK_EVERY: Duration = Duration::from_secs(15);

/// What the bubble shows about a reminder.
#[derive(Clone, Debug, Serialize, Type)]
pub struct ReminderBubble {
    pub id: i64,
    pub text: String,
    pub due_at: Option<i64>,
    /// It came from dictation, so "Type it instead" is offered.
    pub can_type: bool,
    /// What was just done to it by voice, with Undo; `None` for a new one.
    pub changed: Option<ReminderChange>,
}

/// A change to a reminder said out loud.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ReminderChange {
    Deleted,
    Done,
    Moved,
    /// No reminder matched what was said.
    NotFound,
}

/// What Undo puts back after a change by voice.
enum Undo {
    Deleted(Todo),
    Done(i64),
    Moved(i64, Option<i64>),
}

static LAST_CHANGE: Mutex<Option<Undo>> = Mutex::new(None);

/// Dictated words held back as a reminder, until the bubble goes or they're
/// typed after all.
static HELD_BACK: Mutex<Option<(i64, String)>> = Mutex::new(None);

pub fn emit_changed(app: &AppHandle) {
    let _ = app.emit("todos-changed", ());
}

/// Seconds since the Unix epoch for a local time.
pub fn timestamp(at: chrono::NaiveDateTime) -> Option<i64> {
    Local
        .from_local_datetime(&at)
        .earliest()
        .map(|t| t.timestamp())
}

/// Makes `text` a to-do when it asks for one, and shows it in the bubble.
/// Returns whether it did; the words are then neither typed nor saved as an
/// idea.
pub fn catch(app: &AppHandle, text: &str, heard: Heard) -> bool {
    // "Delete the reminder about Priya" changes one; it doesn't make one.
    if let Some(change) = heard::hear_change(text, heard, Local::now().naive_local()) {
        if apply_change(app, &change, heard) {
            return true;
        }
    }
    let Some(spoken) = heard::hear(text, heard, Local::now().naive_local()) else {
        return false;
    };
    let due_at = spoken.due.and_then(timestamp);
    let notes = app.state::<Arc<NotesManager>>();
    let todo = match notes.create_todo(&spoken.what, due_at, Some(text.trim())) {
        Ok(todo) => todo,
        Err(e) => {
            // Better typed or saved as usual than lost.
            error!("Failed to save a reminder: {e}");
            return false;
        }
    };
    info!(
        "Saved to-do {} ({})",
        todo.id,
        if due_at.is_some() {
            "with a time"
        } else {
            "no time"
        }
    );
    emit_changed(app);
    let can_type = heard == Heard::Dictation;
    if can_type {
        *HELD_BACK.lock().unwrap_or_else(|e| e.into_inner()) = Some((todo.id, text.to_string()));
    }
    show(app, &todo, can_type, crate::overlay::ReminderOverlay::Set);
    true
}

/// How well `about` names `text`: the share of its words found there.
fn match_score(about: &[String], text: &str) -> f32 {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .collect();
    let found = about
        .iter()
        .filter(|a| {
            words.iter().any(|w| {
                w == *a
                    // "calling" names "call", "meds" names "medicine".
                    || (a.len() >= 3 && w.len() >= 3 && (w.starts_with(a.as_str()) || a.starts_with(w.as_str())))
            })
        })
        .count();
    found as f32 / about.len().max(1) as f32
}

/// The reminder that `change` is about, if exactly one fits.
fn find_todo<'a>(todos: &'a [Todo], change: &SpokenChange) -> Option<&'a Todo> {
    let open: Vec<&Todo> = todos.iter().filter(|t| t.done_at.is_none()).collect();
    let at = change.at.and_then(timestamp);
    let same_time = |t: &Todo| match (at, t.due_at) {
        (Some(a), Some(d)) => (a - d).abs() < 60,
        _ => false,
    };
    if change.about.is_empty() {
        // "Delete my 5 pm reminder", or "that reminder": the newest.
        return match at {
            Some(_) => {
                let hits: Vec<&&Todo> = open.iter().filter(|t| same_time(t)).collect();
                (hits.len() == 1).then(|| *hits[0])
            }
            None => open.iter().max_by_key(|t| t.created_at).copied(),
        };
    }
    let mut scored: Vec<(f32, &Todo)> = open
        .iter()
        .map(|t| {
            let bonus = if same_time(t) { 0.5 } else { 0.0 };
            (match_score(&change.about, &t.text) + bonus, *t)
        })
        .filter(|(score, _)| *score >= 0.5)
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    match scored.as_slice() {
        [] => None,
        [(_, only)] => Some(only),
        [(best, todo), (next, _), ..] if best > next => Some(todo),
        _ => None,
    }
}

/// Does what was said to a reminder. False when nothing was done and the
/// words should go on as usual (dictation that named no reminder).
fn apply_change(app: &AppHandle, change: &SpokenChange, heard: Heard) -> bool {
    let notes = app.state::<Arc<NotesManager>>();
    let todos = match notes.list_todos() {
        Ok(list) => list.todos,
        Err(e) => {
            error!("Failed to read the to-dos: {e}");
            return false;
        }
    };
    let Some(todo) = find_todo(&todos, change).cloned() else {
        if heard == Heard::Dictation {
            return false;
        }
        info!("No reminder matched a change said out loud");
        let bubble = ReminderBubble {
            id: 0,
            text: change.about.join(" "),
            due_at: None,
            can_type: false,
            changed: Some(ReminderChange::NotFound),
        };
        crate::overlay::show_reminder_overlay(app, &bubble, crate::overlay::ReminderOverlay::Set);
        return true;
    };
    let (result, undo, kind, due_at) = match change.change {
        Change::Delete => (
            notes.delete_todo(todo.id),
            Undo::Deleted(todo.clone()),
            ReminderChange::Deleted,
            todo.due_at,
        ),
        Change::Done => (
            notes.set_todo_done(todo.id, true),
            Undo::Done(todo.id),
            ReminderChange::Done,
            todo.due_at,
        ),
        Change::Move(at) => {
            let due = timestamp(at);
            (
                notes.set_todo_due(todo.id, due),
                Undo::Moved(todo.id, todo.due_at),
                ReminderChange::Moved,
                due,
            )
        }
    };
    if let Err(e) = result {
        error!("Failed to change reminder {}: {e}", todo.id);
        return false;
    }
    info!("Reminder {} changed by voice: {kind:?}", todo.id);
    *LAST_CHANGE.lock().unwrap_or_else(|e| e.into_inner()) = Some(undo);
    emit_changed(app);
    let bubble = ReminderBubble {
        id: todo.id,
        text: todo.text.clone(),
        due_at,
        can_type: false,
        changed: Some(kind),
    };
    crate::overlay::show_reminder_overlay(app, &bubble, crate::overlay::ReminderOverlay::Set);
    true
}

/// Undo in the bubble: puts back the reminder last changed by voice.
pub fn undo_change(app: &AppHandle) {
    let Some(undo) = LAST_CHANGE.lock().unwrap_or_else(|e| e.into_inner()).take() else {
        return;
    };
    let notes = app.state::<Arc<NotesManager>>();
    let result = match undo {
        Undo::Deleted(todo) => notes
            .create_todo(&todo.text, todo.due_at, todo.spoken.as_deref())
            .map(|_| ()),
        Undo::Done(id) => notes.set_todo_done(id, false),
        Undo::Moved(id, due) => notes.set_todo_due(id, due),
    };
    if let Err(e) = result {
        error!("Failed to undo a reminder change: {e}");
    }
    emit_changed(app);
    dismiss(app);
}

fn show(app: &AppHandle, todo: &Todo, can_type: bool, kind: crate::overlay::ReminderOverlay) {
    let bubble = ReminderBubble {
        id: todo.id,
        text: todo.text.clone(),
        due_at: todo.due_at,
        can_type,
        changed: None,
    };
    crate::overlay::show_reminder_overlay(app, &bubble, kind);
}

/// Types the dictated words that were taken as a reminder, and drops the
/// reminder.
pub fn type_instead(app: &AppHandle, id: i64) {
    let held = HELD_BACK.lock().unwrap_or_else(|e| e.into_inner()).take();
    let Some((held_id, text)) = held.filter(|(held_id, _)| *held_id == id) else {
        return;
    };
    if let Err(e) = app.state::<Arc<NotesManager>>().delete_todo(held_id) {
        warn!("Failed to drop the reminder: {e}");
    }
    emit_changed(app);
    crate::utils::hide_recording_overlay(app);
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("paste".into())
        .spawn(move || {
            // Let the bubble's click settle before typing.
            std::thread::sleep(Duration::from_millis(150));
            if let Err(e) = crate::clipboard::paste(text, app) {
                error!("Failed to type the words: {e}");
            }
        });
}

/// Closes the reminder bubble.
pub fn dismiss(app: &AppHandle) {
    HELD_BACK.lock().unwrap_or_else(|e| e.into_inner()).take();
    if crate::overlay::is_reminder_showing() {
        crate::utils::hide_recording_overlay(app);
    }
}

/// Shows a reminder's time has come: a notification, and the bubble unless
/// it's busy with a recording.
fn ring(app: &AppHandle, todo: &Todo) {
    info!("Reminder {} is due", todo.id);
    notify(&todo.text);
    if !crate::overlay::is_session_showing() {
        show(app, todo, false, crate::overlay::ReminderOverlay::Due);
    }
}

/// A notification from the system, with a sound.
fn notify(text: &str) {
    #[cfg(target_os = "macos")]
    {
        let quoted = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
        let script = format!(
            "display notification {} with title \"Bolke\" sound name \"Glass\"",
            quoted(text)
        );
        if let Err(e) = std::process::Command::new("osascript")
            .args(["-e", &script])
            .spawn()
        {
            warn!("Failed to show the reminder notification: {e}");
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Err(e) = std::process::Command::new("notify-send")
            .args(["--app-name=Bolke", "--urgency=critical", "Bolke", text])
            .spawn()
        {
            warn!("Failed to show the reminder notification: {e}");
        }
        let _ = std::process::Command::new("canberra-gtk-play")
            .args(["-i", "message-new-instant"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    #[cfg(target_os = "windows")]
    let _ = text;
}

/// Looks for due reminders every few seconds, for as long as Bolke runs.
/// Ones missed while Bolke was closed ring as soon as it starts.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("reminders".into())
        .spawn(move || loop {
            match app.state::<Arc<NotesManager>>().take_due_todos() {
                Ok(due) if !due.is_empty() => {
                    for todo in &due {
                        ring(&app, todo);
                    }
                    emit_changed(&app);
                }
                Ok(_) => {}
                Err(e) => error!("Failed to look for due reminders: {e}"),
            }
            std::thread::sleep(CHECK_EVERY);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn todo(id: i64, text: &str, due_at: Option<i64>) -> Todo {
        Todo {
            id,
            text: text.into(),
            due_at,
            done_at: None,
            rung_at: None,
            spoken: None,
            created_at: id,
        }
    }

    fn about(words: &[&str]) -> SpokenChange {
        SpokenChange {
            change: Change::Delete,
            about: words.iter().map(|w| w.to_string()).collect(),
            at: None,
        }
    }

    #[test]
    fn the_right_reminder_is_found_from_a_few_words() {
        let todos = vec![
            todo(1, "Call Priya about the deck", Some(1000)),
            todo(2, "Buy milk", None),
            todo(3, "Call the dentist", Some(5000)),
        ];
        let id = |c: &SpokenChange| find_todo(&todos, c).map(|t| t.id);
        assert_eq!(id(&about(&["calling", "priya"])), Some(1));
        assert_eq!(id(&about(&["milk"])), Some(2));
        assert_eq!(id(&about(&["dentist"])), Some(3));
        // Two calls: not sure which.
        assert_eq!(id(&about(&["call"])), None);
        assert_eq!(id(&about(&["rahul"])), None);
        // "That reminder" is the newest.
        assert_eq!(id(&about(&[])), Some(3));
    }
}
