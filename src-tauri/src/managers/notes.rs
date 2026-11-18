//! Notes and projects: ideas, meetings and written notes, each with its full
//! text, an optional AI summary and, when recorded, its audio.
//!
//! A recorded note's audio and first transcript live in a History entry
//! (`entry_id`), which background transcription fills in. The note copies
//! that text the first time it's read after the transcript is ready, and from
//! then on keeps its own copy, which the user can edit.

use anyhow::{anyhow, Result};
use chrono::Utc;
use log::{debug, info};
use rusqlite::{params, Connection, OptionalExtension, Row};
use rusqlite_migration::{Migrations, M};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

use crate::managers::history::HistoryManager;

static MIGRATIONS: &[M] = &[M::up(
    "CREATE TABLE IF NOT EXISTS projects (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL,
        created_at INTEGER NOT NULL
    );
    CREATE TABLE IF NOT EXISTS notes (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
        kind TEXT NOT NULL,
        title TEXT NOT NULL,
        text TEXT NOT NULL DEFAULT '',
        text_pending BOOLEAN NOT NULL DEFAULT 0,
        summary TEXT,
        entry_id INTEGER,
        duration_ms INTEGER,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS notes_project ON notes(project_id);",
)];

const NOTE_COLUMNS: &str = "id, project_id, kind, title, text, text_pending, summary, entry_id, \
    duration_ms, created_at, updated_at";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    /// A quick spoken idea.
    Idea,
    /// A long recording of a meeting, call or lecture.
    Meeting,
    /// Typed by hand, or made from an audio file.
    Note,
}

impl NoteKind {
    fn as_str(self) -> &'static str {
        match self {
            NoteKind::Idea => "idea",
            NoteKind::Meeting => "meeting",
            NoteKind::Note => "note",
        }
    }

    fn parse(s: &str) -> NoteKind {
        match s {
            "idea" => NoteKind::Idea,
            "meeting" => NoteKind::Meeting,
            _ => NoteKind::Note,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub created_at: i64,
    /// How many notes it holds.
    pub note_count: i64,
}

/// The AI summary of a note.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct NoteSummary {
    pub summary: String,
    /// Things someone has to do; empty when there are none.
    #[serde(default)]
    pub action_points: Vec<ActionPoint>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct ActionPoint {
    pub text: String,
    /// Ticked off by the user.
    #[serde(default)]
    pub done: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct Note {
    pub id: i64,
    pub project_id: Option<i64>,
    pub kind: NoteKind,
    pub title: String,
    /// The full text. Empty while `text_pending`.
    pub text: String,
    /// The recording is still waiting for its transcript.
    pub text_pending: bool,
    pub summary: Option<NoteSummary>,
    /// The History entry holding the recording.
    pub entry_id: Option<i64>,
    /// The recording's file name in the recordings folder, when it has one.
    pub audio_file: Option<String>,
    pub duration_ms: Option<i64>,
    /// Seconds since the Unix epoch.
    pub created_at: i64,
    pub updated_at: i64,
}

/// What a note change sets; `None` leaves a field as it is.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
pub struct NoteChanges {
    pub title: Option<String>,
    pub text: Option<String>,
    pub kind: Option<NoteKind>,
}

pub struct NotesManager {
    app: AppHandle,
    db_path: PathBuf,
}

impl NotesManager {
    pub fn new(app: &AppHandle) -> Result<Self> {
        let app_data_dir = crate::portable::app_data_dir(app)?;
        std::fs::create_dir_all(&app_data_dir)?;
        let manager = Self {
            app: app.clone(),
            db_path: app_data_dir.join("notes.db"),
        };
        let mut conn = manager.connection()?;
        Migrations::from_slice(MIGRATIONS).to_latest(&mut conn)?;
        info!("Notes database ready at {:?}", manager.db_path);
        Ok(manager)
    }

    fn connection(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(conn)
    }

    fn now() -> i64 {
        Utc::now().timestamp()
    }

    // ----- Projects -----

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT p.id, p.name, p.created_at,
                    (SELECT COUNT(*) FROM notes n WHERE n.project_id = p.id)
             FROM projects p ORDER BY p.name COLLATE NOCASE",
        )?;
        let projects = stmt
            .query_map([], |row| {
                Ok(Project {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    created_at: row.get(2)?,
                    note_count: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(projects)
    }

    pub fn create_project(&self, name: &str) -> Result<Project> {
        let name = clean_name(name)?;
        let conn = self.connection()?;
        let created_at = Self::now();
        conn.execute(
            "INSERT INTO projects (name, created_at) VALUES (?1, ?2)",
            params![name, created_at],
        )?;
        Ok(Project {
            id: conn.last_insert_rowid(),
            name,
            created_at,
            note_count: 0,
        })
    }

    pub fn rename_project(&self, id: i64, name: &str) -> Result<()> {
        let name = clean_name(name)?;
        self.connection()?.execute(
            "UPDATE projects SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        Ok(())
    }

    /// Deletes the project. Its notes stay, outside any project.
    pub fn delete_project(&self, id: i64) -> Result<()> {
        let conn = self.connection()?;
        conn.execute(
            "UPDATE notes SET project_id = NULL WHERE project_id = ?1",
            params![id],
        )?;
        conn.execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ----- Notes -----

    /// Newest first. `project_id` limits it to one project.
    pub async fn list_notes(&self, project_id: Option<i64>) -> Result<Vec<Note>> {
        let notes = {
            let conn = self.connection()?;
            let sql = match project_id {
                Some(_) => format!(
                    "SELECT {NOTE_COLUMNS} FROM notes WHERE project_id = ?1 ORDER BY created_at DESC, id DESC"
                ),
                None => format!("SELECT {NOTE_COLUMNS} FROM notes ORDER BY created_at DESC, id DESC"),
            };
            let mut stmt = conn.prepare(&sql)?;
            let rows = match project_id {
                Some(id) => stmt.query_map([id], map_note)?,
                None => stmt.query_map([], map_note)?,
            };
            rows.collect::<Result<Vec<_>, _>>()?
        };
        let mut filled = Vec::with_capacity(notes.len());
        for note in notes {
            filled.push(self.fill(note).await?);
        }
        Ok(filled)
    }

    pub async fn get_note(&self, id: i64) -> Result<Note> {
        let note = self
            .connection()?
            .query_row(
                &format!("SELECT {NOTE_COLUMNS} FROM notes WHERE id = ?1"),
                [id],
                map_note,
            )
            .optional()?
            .ok_or_else(|| anyhow!("Note {id} not found"))?;
        self.fill(note).await
    }

    /// Adds the recording's file name, and copies in its transcript once it
    /// is ready.
    async fn fill(&self, mut note: Note) -> Result<Note> {
        let Some(entry_id) = note.entry_id else {
            return Ok(note);
        };
        let hm = self.app.state::<Arc<HistoryManager>>();
        let Some(entry) = hm.get_entry_by_id(entry_id).await? else {
            // The recording was deleted from History; the note keeps its text.
            return Ok(note);
        };
        note.audio_file = Some(entry.file_name.clone());
        if note.text_pending && !entry.transcription_text.trim().is_empty() {
            let text = entry
                .post_processed_text
                .filter(|t| !t.trim().is_empty())
                .unwrap_or(entry.transcription_text);
            self.connection()?.execute(
                "UPDATE notes SET text = ?1, text_pending = 0 WHERE id = ?2",
                params![text, note.id],
            )?;
            debug!("Note {} has its transcript", note.id);
            note.text = text;
            note.text_pending = false;
        }
        Ok(note)
    }

    /// A note typed by hand.
    pub async fn create_text_note(&self, title: &str, project_id: Option<i64>) -> Result<Note> {
        let id = self.insert(NoteKind::Note, title, "", project_id, None, None, false)?;
        self.get_note(id).await
    }

    /// An idea spoken with the idea shortcut, already transcribed. Its
    /// recording, when saved, is History entry `entry_id`.
    pub async fn create_idea(
        &self,
        text: &str,
        entry_id: Option<i64>,
        duration_ms: Option<i64>,
    ) -> Result<Note> {
        let id = self.insert(
            NoteKind::Idea,
            &title_from_text(text),
            text,
            None,
            entry_id,
            duration_ms,
            false,
        )?;
        self.get_note(id).await
    }

    /// A note whose text comes from the recording in History entry `entry_id`.
    pub async fn create_recorded_note(
        &self,
        kind: NoteKind,
        title: &str,
        project_id: Option<i64>,
        entry_id: i64,
        duration_ms: Option<i64>,
    ) -> Result<Note> {
        let id = self.insert(
            kind,
            title,
            "",
            project_id,
            Some(entry_id),
            duration_ms,
            true,
        )?;
        self.get_note(id).await
    }

    #[allow(clippy::too_many_arguments)]
    fn insert(
        &self,
        kind: NoteKind,
        title: &str,
        text: &str,
        project_id: Option<i64>,
        entry_id: Option<i64>,
        duration_ms: Option<i64>,
        text_pending: bool,
    ) -> Result<i64> {
        let conn = self.connection()?;
        let now = Self::now();
        conn.execute(
            "INSERT INTO notes (project_id, kind, title, text, text_pending, entry_id, duration_ms, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![project_id, kind.as_str(), title.trim(), text, text_pending, entry_id, duration_ms, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub async fn update_note(&self, id: i64, changes: NoteChanges) -> Result<Note> {
        {
            let conn = self.connection()?;
            let now = Self::now();
            if let Some(title) = &changes.title {
                conn.execute(
                    "UPDATE notes SET title = ?1, updated_at = ?2 WHERE id = ?3",
                    params![title.trim(), now, id],
                )?;
            }
            if let Some(text) = &changes.text {
                // Typing over a transcript still on its way keeps what was typed.
                conn.execute(
                    "UPDATE notes SET text = ?1, text_pending = 0, updated_at = ?2 WHERE id = ?3",
                    params![text, now, id],
                )?;
            }
            if let Some(kind) = changes.kind {
                conn.execute(
                    "UPDATE notes SET kind = ?1, updated_at = ?2 WHERE id = ?3",
                    params![kind.as_str(), now, id],
                )?;
            }
        }
        self.get_note(id).await
    }

    /// Moves the note into a project, or out of any with `None`.
    pub async fn move_note(&self, id: i64, project_id: Option<i64>) -> Result<Note> {
        self.connection()?.execute(
            "UPDATE notes SET project_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![project_id, Self::now(), id],
        )?;
        self.get_note(id).await
    }

    pub fn set_summary(&self, id: i64, summary: Option<&NoteSummary>) -> Result<()> {
        let summary = summary.map(serde_json::to_string).transpose()?;
        self.connection()?.execute(
            "UPDATE notes SET summary = ?1, updated_at = ?2 WHERE id = ?3",
            params![summary, Self::now(), id],
        )?;
        Ok(())
    }

    /// Ticks an action point off, or back on.
    pub async fn set_action_point_done(&self, id: i64, index: usize, done: bool) -> Result<Note> {
        let note = self.get_note(id).await?;
        let mut summary = note
            .summary
            .ok_or_else(|| anyhow!("Note {id} has no summary"))?;
        let point = summary
            .action_points
            .get_mut(index)
            .ok_or_else(|| anyhow!("Note {id} has no action point {index}"))?;
        point.done = done;
        self.set_summary(id, Some(&summary))?;
        self.get_note(id).await
    }

    /// Deletes the note; returns the History entry holding its recording, which
    /// the caller deletes too.
    pub fn delete_note(&self, id: i64) -> Result<Option<i64>> {
        let conn = self.connection()?;
        let entry_id: Option<i64> = conn
            .query_row("SELECT entry_id FROM notes WHERE id = ?1", [id], |row| {
                row.get(0)
            })
            .optional()?
            .flatten();
        conn.execute("DELETE FROM notes WHERE id = ?1", params![id])?;
        Ok(entry_id)
    }
}

/// The first few words of an idea, as its title.
fn title_from_text(text: &str) -> String {
    const MAX_WORDS: usize = 8;
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut title = words
        .iter()
        .take(MAX_WORDS)
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    title = title
        .trim_end_matches(|c: char| c.is_ascii_punctuation())
        .to_string();
    if words.len() > MAX_WORDS {
        title.push('…');
    }
    title
}

/// A stored summary: JSON, or plain text from before it had action points.
fn parse_summary(stored: Option<String>) -> Option<NoteSummary> {
    let stored = stored?;
    Some(serde_json::from_str(&stored).unwrap_or(NoteSummary {
        summary: stored,
        action_points: Vec::new(),
    }))
}

fn clean_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(anyhow!("A project needs a name"));
    }
    Ok(name.chars().take(80).collect())
}

fn map_note(row: &Row) -> rusqlite::Result<Note> {
    let kind: String = row.get(2)?;
    Ok(Note {
        id: row.get(0)?,
        project_id: row.get(1)?,
        kind: NoteKind::parse(&kind),
        title: row.get(3)?,
        text: row.get(4)?,
        text_pending: row.get(5)?,
        summary: parse_summary(row.get(6)?),
        entry_id: row.get(7)?,
        audio_file: None,
        duration_ms: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid() {
        Migrations::from_slice(MIGRATIONS).validate().unwrap();
        let mut conn = Connection::open_in_memory().unwrap();
        Migrations::from_slice(MIGRATIONS)
            .to_latest(&mut conn)
            .unwrap();
        conn.execute(
            "INSERT INTO notes (kind, title, created_at, updated_at) VALUES ('idea', 't', 0, 0)",
            [],
        )
        .unwrap();
        let note = conn
            .query_row(&format!("SELECT {NOTE_COLUMNS} FROM notes"), [], map_note)
            .unwrap();
        assert_eq!(note.kind, NoteKind::Idea);
        assert!(!note.text_pending);
    }

    #[test]
    fn idea_titles_are_its_first_words() {
        assert_eq!(title_from_text("Add a dark mode."), "Add a dark mode");
        assert_eq!(
            title_from_text("one two three four five six seven eight nine"),
            "one two three four five six seven eight…"
        );
        assert_eq!(title_from_text("   "), "");
    }

    #[test]
    fn summaries_read_json_or_plain_text() {
        let json = serde_json::to_string(&NoteSummary {
            summary: "s".into(),
            action_points: vec![ActionPoint {
                text: "a".into(),
                done: true,
            }],
        })
        .unwrap();
        let points = parse_summary(Some(json)).unwrap().action_points;
        assert_eq!(points[0].text, "a");
        assert!(points[0].done);
        assert_eq!(
            parse_summary(Some("plain".into())).unwrap().summary,
            "plain"
        );
        assert!(parse_summary(None).is_none());
    }

    #[test]
    fn project_names_are_trimmed_and_required() {
        assert_eq!(clean_name("  Work  ").unwrap(), "Work");
        assert!(clean_name("   ").is_err());
    }
}
