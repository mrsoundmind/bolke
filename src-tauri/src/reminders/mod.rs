//! Reminders: "remind me…" said with either shortcut becomes a to-do with a
//! time, and Bolke rings it then, with a notification and the bubble.
//!
//! Dictation is only caught when it starts like a reminder, and the bubble
//! offers to type the words instead, in case they were meant for the page.

pub mod heard;

pub use heard::Heard;

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
}

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

fn show(app: &AppHandle, todo: &Todo, can_type: bool, kind: crate::overlay::ReminderOverlay) {
    let bubble = ReminderBubble {
        id: todo.id,
        text: todo.text.clone(),
        due_at: todo.due_at,
        can_type,
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
