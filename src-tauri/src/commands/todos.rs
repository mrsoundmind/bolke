//! To-dos and reminders.

use crate::managers::notes::NotesManager;
use crate::managers::todos::{Todo, TodoList};
use crate::reminders::{self, Heard};
use chrono::Local;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

fn err(e: anyhow::Error) -> String {
    e.to_string()
}

#[tauri::command]
#[specta::specta]
pub fn list_todos(notes: State<'_, Arc<NotesManager>>) -> Result<TodoList, String> {
    notes.list_todos().map_err(err)
}

/// Adds a typed to-do; a time in it ("pay rent on Friday") sets a reminder.
#[tauri::command]
#[specta::specta]
pub fn add_todo(
    app: AppHandle,
    notes: State<'_, Arc<NotesManager>>,
    text: String,
) -> Result<Todo, String> {
    let heard = reminders::heard::hear(&text, Heard::Typed, Local::now().naive_local())
        .ok_or_else(|| "A to-do needs some words".to_string())?;
    let due_at = heard.due.and_then(reminders::timestamp);
    let todo = notes.create_todo(&heard.what, due_at, None).map_err(err)?;
    reminders::emit_changed(&app);
    Ok(todo)
}

#[tauri::command]
#[specta::specta]
pub fn set_todo_text(
    app: AppHandle,
    notes: State<'_, Arc<NotesManager>>,
    id: i64,
    text: String,
) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("A to-do needs some words".to_string());
    }
    notes.set_todo_text(id, &text).map_err(err)?;
    reminders::emit_changed(&app);
    Ok(())
}

/// Sets when to remind, in seconds since the Unix epoch, or clears it.
#[tauri::command]
#[specta::specta]
pub fn set_todo_due(
    app: AppHandle,
    notes: State<'_, Arc<NotesManager>>,
    id: i64,
    due_at: Option<i64>,
) -> Result<(), String> {
    notes.set_todo_due(id, due_at).map_err(err)?;
    reminders::emit_changed(&app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn set_todo_done(
    app: AppHandle,
    notes: State<'_, Arc<NotesManager>>,
    id: i64,
    done: bool,
) -> Result<(), String> {
    notes.set_todo_done(id, done).map_err(err)?;
    reminders::emit_changed(&app);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn delete_todo(
    app: AppHandle,
    notes: State<'_, Arc<NotesManager>>,
    id: i64,
) -> Result<(), String> {
    notes.delete_todo(id).map_err(err)?;
    reminders::emit_changed(&app);
    Ok(())
}

/// Types the dictated words the bubble took as a reminder, and drops it.
#[tauri::command]
#[specta::specta]
pub fn type_reminder_instead(app: AppHandle, id: i64) {
    reminders::type_instead(&app, id);
}

/// Closes the reminder bubble.
#[tauri::command]
#[specta::specta]
pub fn dismiss_reminder(app: AppHandle) {
    reminders::dismiss(&app);
}

/// Undo in the bubble, after a reminder was changed by voice.
#[tauri::command]
#[specta::specta]
pub fn undo_reminder_change(app: AppHandle) {
    reminders::undo_change(&app);
}

/// Opens To-do in the main window, from the bubble.
#[tauri::command]
#[specta::specta]
pub fn open_todo(app: AppHandle, id: i64) {
    reminders::dismiss(&app);
    crate::show_main_window(&app);
    let _ = app.emit("open-todo", id);
}
