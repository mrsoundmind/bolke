//! To-dos and reminders, kept beside the notes. A spoken "remind me…" or a
//! typed to-do is a row here; a meeting's action points are read from its
//! summary, so ticking one off ticks it in the meeting too.

use anyhow::Result;
use rusqlite::{params, Row};
use serde::Serialize;
use specta::Type;

use crate::managers::notes::{parse_summary, NotesManager};

/// A to-do, with a reminder when it has a time.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
pub struct Todo {
    pub id: i64,
    pub text: String,
    /// When to remind, in seconds since the Unix epoch.
    pub due_at: Option<i64>,
    pub done_at: Option<i64>,
    /// When the reminder went off; cleared when the time changes.
    pub rung_at: Option<i64>,
    /// The words it was made from, when it was said.
    pub spoken: Option<String>,
    pub created_at: i64,
}

/// An action point from a meeting's summary.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
pub struct MeetingTodo {
    pub note_id: i64,
    pub meeting: String,
    pub index: u32,
    pub text: String,
    pub done: bool,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Type)]
pub struct TodoList {
    pub todos: Vec<Todo>,
    pub from_meetings: Vec<MeetingTodo>,
}

const COLUMNS: &str = "id, text, due_at, done_at, rung_at, spoken, created_at";

fn map_todo(row: &Row) -> rusqlite::Result<Todo> {
    Ok(Todo {
        id: row.get(0)?,
        text: row.get(1)?,
        due_at: row.get(2)?,
        done_at: row.get(3)?,
        rung_at: row.get(4)?,
        spoken: row.get(5)?,
        created_at: row.get(6)?,
    })
}

/// Done ones older than this drop off the list.
const KEEP_DONE_SECS: i64 = 30 * 24 * 3600;

impl NotesManager {
    pub fn list_todos(&self) -> Result<TodoList> {
        let conn = self.connection()?;
        let since = Self::now() - KEEP_DONE_SECS;
        let todos = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM todos
                 WHERE done_at IS NULL OR done_at > ?1
                 ORDER BY due_at IS NULL, due_at, created_at DESC"
            ))?
            .query_map([since], map_todo)?
            .collect::<Result<Vec<_>, _>>()?;
        let mut from_meetings = Vec::new();
        let mut stmt = conn.prepare(
            "SELECT id, title, summary, created_at FROM notes
             WHERE kind = 'meeting' AND summary IS NOT NULL
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        for row in rows {
            let (note_id, meeting, summary, created_at) = row?;
            let Some(summary) = parse_summary(summary) else {
                continue;
            };
            for (index, point) in summary.action_points.into_iter().enumerate() {
                from_meetings.push(MeetingTodo {
                    note_id,
                    meeting: meeting.clone(),
                    index: index as u32,
                    text: point.text,
                    done: point.done,
                    created_at,
                });
            }
        }
        Ok(TodoList {
            todos,
            from_meetings,
        })
    }

    pub fn create_todo(
        &self,
        text: &str,
        due_at: Option<i64>,
        spoken: Option<&str>,
    ) -> Result<Todo> {
        let conn = self.connection()?;
        let now = Self::now();
        conn.execute(
            "INSERT INTO todos (text, due_at, spoken, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![text.trim(), due_at, spoken, now],
        )?;
        Ok(Todo {
            id: conn.last_insert_rowid(),
            text: text.trim().to_string(),
            due_at,
            done_at: None,
            rung_at: None,
            spoken: spoken.map(str::to_string),
            created_at: now,
        })
    }

    pub fn set_todo_text(&self, id: i64, text: &str) -> Result<()> {
        self.connection()?.execute(
            "UPDATE todos SET text = ?1 WHERE id = ?2",
            params![text.trim(), id],
        )?;
        Ok(())
    }

    /// Sets or clears the reminder time; it rings again at the new time.
    pub fn set_todo_due(&self, id: i64, due_at: Option<i64>) -> Result<()> {
        self.connection()?.execute(
            "UPDATE todos SET due_at = ?1, rung_at = NULL WHERE id = ?2",
            params![due_at, id],
        )?;
        Ok(())
    }

    pub fn set_todo_done(&self, id: i64, done: bool) -> Result<()> {
        let done_at = done.then(Self::now);
        self.connection()?.execute(
            "UPDATE todos SET done_at = ?1 WHERE id = ?2",
            params![done_at, id],
        )?;
        Ok(())
    }

    pub fn delete_todo(&self, id: i64) -> Result<()> {
        self.connection()?
            .execute("DELETE FROM todos WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Reminders whose time has come and that haven't gone off, oldest
    /// first; each is marked as gone off.
    pub fn take_due_todos(&self) -> Result<Vec<Todo>> {
        let conn = self.connection()?;
        let now = Self::now();
        let due = conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM todos
                 WHERE due_at <= ?1 AND done_at IS NULL AND rung_at IS NULL
                 ORDER BY due_at"
            ))?
            .query_map([now], map_todo)?
            .collect::<Result<Vec<_>, _>>()?;
        for todo in &due {
            conn.execute(
                "UPDATE todos SET rung_at = ?1 WHERE id = ?2",
                params![now, todo.id],
            )?;
        }
        Ok(due)
    }
}
