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

use crate::call_names::CallNames;

use crate::managers::history::HistoryManager;

static MIGRATIONS: &[M] = &[
    M::up(
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
    ),
    // Who said what in a meeting, and the voices Bolke knows by name.
    M::up(
        "ALTER TABLE notes ADD COLUMN people TEXT;
    ALTER TABLE notes ADD COLUMN people_state TEXT;
    CREATE TABLE IF NOT EXISTS note_voices (
        note_id INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
        person INTEGER NOT NULL,
        fingerprint BLOB NOT NULL,
        PRIMARY KEY (note_id, person)
    );
    CREATE TABLE IF NOT EXISTS voices (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        name TEXT NOT NULL UNIQUE COLLATE NOCASE,
        fingerprint BLOB NOT NULL,
        heard INTEGER NOT NULL DEFAULT 1,
        updated_at INTEGER NOT NULL
    );",
    ),
    // To-dos and reminders (see `managers::todos`).
    M::up(
        "CREATE TABLE IF NOT EXISTS todos (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        text TEXT NOT NULL,
        due_at INTEGER,
        done_at INTEGER,
        rung_at INTEGER,
        spoken TEXT,
        created_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS todos_due ON todos(due_at);",
    ),
    // Questions asked about a note, with their answers (`commands::ask`).
    M::up(
        "CREATE TABLE IF NOT EXISTS note_answers (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        note_id INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
        question TEXT NOT NULL,
        answer TEXT NOT NULL,
        quotes TEXT NOT NULL,
        asked_at INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS note_answers_note ON note_answers(note_id);",
    ),
    // How well the meeting backs each answer up.
    M::up("ALTER TABLE note_answers ADD COLUMN checked TEXT;"),
];

/// A known voice this alike is taken to be that person.
const KNOWN_VOICE: f32 = 0.6;

const NOTE_COLUMNS: &str = "id, project_id, kind, title, text, text_pending, summary, entry_id, \
    duration_ms, created_at, updated_at, people, people_state";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum NoteKind {
    /// A quick spoken idea.
    Idea,
    /// A meeting, call or lecture, recorded or added from a file.
    Meeting,
    /// Typed by hand (older ones were made from an audio file).
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
    /// A meeting's summary for each person who spoke; empty for other notes.
    #[serde(default)]
    pub people: Vec<PersonSummary>,
}

/// What one person in a meeting said, agreed to and will do.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct PersonSummary {
    /// The person's id in the note's `people`.
    pub person: u32,
    /// One sentence on what they mainly cared about or argued.
    #[serde(default)]
    pub gist: String,
    #[serde(default)]
    pub said: Vec<String>,
    #[serde(default)]
    pub agreed: Vec<String>,
    #[serde(default)]
    pub will_do: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct ActionPoint {
    pub text: String,
    /// Ticked off by the user.
    #[serde(default)]
    pub done: bool,
}

/// Who spoke in a meeting, and each of their lines in order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct NotePeople {
    pub people: Vec<Person>,
    pub lines: Vec<NoteLine>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct Person {
    /// 1-based, in the order people first spoke.
    pub id: u32,
    /// `None` until Bolke knows their voice or the user names them.
    pub name: Option<String>,
    /// How long they talked.
    pub talk_ms: i64,
    /// Named because the voice matched one Bolke knows, not by the user.
    #[serde(default)]
    pub guessed: bool,
    /// Named from the call (Google Meet showed them talking at those times).
    #[serde(default)]
    pub from_call: bool,
    /// Names from the call this voice might be, when Bolke isn't sure.
    #[serde(default)]
    pub choices: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct NoteLine {
    pub start_ms: i64,
    pub end_ms: i64,
    pub person: u32,
    pub text: String,
}

/// A question asked about a note, and Bolke's answer from its words.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct NoteAnswer {
    pub id: i64,
    pub question: String,
    pub answer: String,
    /// The lines the answer came from, in the order they were said.
    pub quotes: Vec<NoteQuote>,
    /// Seconds since the Unix epoch.
    pub asked_at: i64,
    pub check: AnswerCheck,
}

/// How well a note backs up an answer, checked word by word against what
/// the AI was given to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AnswerCheck {
    /// Its words are in the meeting, and it has quotes.
    Backed,
    /// The answer says the meeting doesn't answer it.
    NotInMeeting,
    /// Some of it isn't in the meeting: check the quotes.
    Unsure,
}

impl AnswerCheck {
    fn as_str(self) -> &'static str {
        match self {
            AnswerCheck::Backed => "backed",
            AnswerCheck::NotInMeeting => "not_in_meeting",
            AnswerCheck::Unsure => "unsure",
        }
    }

    fn parse(s: Option<String>) -> AnswerCheck {
        match s.as_deref() {
            Some("backed") => AnswerCheck::Backed,
            Some("not_in_meeting") => AnswerCheck::NotInMeeting,
            _ => AnswerCheck::Unsure,
        }
    }
}

/// One line of a note that backs up an answer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct NoteQuote {
    /// Who said it, in a meeting where Bolke found who said what.
    pub person: Option<u32>,
    /// When in the recording it was said.
    pub start_ms: Option<i64>,
    pub text: String,
}

/// Finding who said what.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PeopleState {
    Working,
    Failed,
}

impl PeopleState {
    fn as_str(self) -> &'static str {
        match self {
            PeopleState::Working => "working",
            PeopleState::Failed => "failed",
        }
    }

    fn parse(s: &str) -> Option<PeopleState> {
        match s {
            "working" => Some(PeopleState::Working),
            "failed" => Some(PeopleState::Failed),
            _ => None,
        }
    }
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
    /// Who said what, once found.
    pub people: Option<NotePeople>,
    pub people_state: Option<PeopleState>,
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

    pub(crate) fn connection(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(conn)
    }

    pub(crate) fn now() -> i64 {
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

    /// The questions asked about a note, oldest first.
    pub fn list_answers(&self, note_id: i64) -> Result<Vec<NoteAnswer>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, question, answer, quotes, asked_at, checked FROM note_answers
             WHERE note_id = ?1 ORDER BY id",
        )?;
        let rows = stmt.query_map([note_id], |row| {
            let quotes: String = row.get(3)?;
            Ok(NoteAnswer {
                id: row.get(0)?,
                question: row.get(1)?,
                answer: row.get(2)?,
                quotes: serde_json::from_str(&quotes).unwrap_or_default(),
                asked_at: row.get(4)?,
                check: AnswerCheck::parse(row.get(5)?),
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn add_answer(
        &self,
        note_id: i64,
        question: &str,
        answer: &str,
        quotes: &[NoteQuote],
        check: AnswerCheck,
    ) -> Result<NoteAnswer> {
        let conn = self.connection()?;
        let asked_at = Self::now();
        conn.execute(
            "INSERT INTO note_answers (note_id, question, answer, quotes, asked_at, checked)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                note_id,
                question,
                answer,
                serde_json::to_string(quotes)?,
                asked_at,
                check.as_str()
            ],
        )?;
        Ok(NoteAnswer {
            id: conn.last_insert_rowid(),
            question: question.to_string(),
            answer: answer.to_string(),
            quotes: quotes.to_vec(),
            asked_at,
            check,
        })
    }

    pub fn clear_answers(&self, note_id: i64) -> Result<()> {
        self.connection()?
            .execute("DELETE FROM note_answers WHERE note_id = ?1", [note_id])?;
        Ok(())
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

    /// A recording with no speech in it has nothing to wait for: its note
    /// stops waiting and keeps an empty text the user can type into. Returns
    /// whether a note was waiting on this History entry.
    pub fn stop_waiting(&self, entry_id: i64) -> Result<bool> {
        let changed = self.connection()?.execute(
            "UPDATE notes SET text_pending = 0 WHERE entry_id = ?1 AND text_pending = 1",
            params![entry_id],
        )?;
        Ok(changed > 0)
    }
}

impl NotesManager {
    /// The note made from History entry `entry_id`: its id, kind and
    /// whether it's still waiting for its text.
    pub fn note_for_entry(&self, entry_id: i64) -> Result<Option<(i64, NoteKind, bool)>> {
        Ok(self
            .connection()?
            .query_row(
                "SELECT id, kind, text_pending FROM notes WHERE entry_id = ?1",
                [entry_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        NoteKind::parse(&row.get::<_, String>(1)?),
                        row.get::<_, bool>(2)?,
                    ))
                },
            )
            .optional()?)
    }

    pub fn set_people_state(&self, id: i64, state: Option<PeopleState>) -> Result<()> {
        self.connection()?.execute(
            "UPDATE notes SET people_state = ?1 WHERE id = ?2",
            params![state.map(PeopleState::as_str), id],
        )?;
        Ok(())
    }

    /// Saves who said what, naming people whose voices Bolke knows.
    pub fn set_people(
        &self,
        id: i64,
        mut people: NotePeople,
        fingerprints: &[(u32, Vec<f32>)],
        call: Option<&CallNames>,
    ) -> Result<()> {
        let mut conn = self.connection()?;
        let known = Self::known_voices(&conn)?;
        name_known_voices(&mut people.people, fingerprints, &known);
        if let Some(call) = call {
            let sure = name_from_call(&mut people, call);
            // Voices the call named for sure are known from now on.
            for (person, name) in sure {
                if let Some((_, fingerprint)) = fingerprints.iter().find(|(p, _)| *p == person) {
                    self.remember_voice(&conn, &name, fingerprint)?;
                }
            }
        }
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM note_voices WHERE note_id = ?1", [id])?;
        for (person, fingerprint) in fingerprints {
            tx.execute(
                "INSERT INTO note_voices (note_id, person, fingerprint) VALUES (?1, ?2, ?3)",
                params![id, person, to_blob(fingerprint)],
            )?;
        }
        tx.execute(
            "UPDATE notes SET people = ?1, people_state = NULL WHERE id = ?2",
            params![serde_json::to_string(&people)?, id],
        )?;
        tx.commit()?;
        Ok(())
    }

    fn known_voices(conn: &Connection) -> Result<Vec<(String, Vec<f32>)>> {
        let mut stmt = conn.prepare("SELECT name, fingerprint FROM voices")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                from_blob(&row.get::<_, Vec<u8>>(1)?),
            ))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Names everyone Bolke has been told about, for picking a name.
    pub fn voice_names(&self) -> Result<Vec<String>> {
        let conn = self.connection()?;
        let mut stmt = conn.prepare("SELECT name FROM voices ORDER BY updated_at DESC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Names a person in a meeting, and remembers their voice for the next
    /// one. Giving two people the same name makes them one person. An empty
    /// name forgets the name.
    pub async fn name_person(&self, id: i64, person: u32, name: &str) -> Result<Note> {
        let note = self.get_note(id).await?;
        let mut people = note
            .people
            .ok_or_else(|| anyhow!("Note {id} has no people"))?;
        let name: String = name.trim().chars().take(60).collect();
        let name = (!name.is_empty()).then_some(name);
        let same = name.as_ref().and_then(|n| {
            people
                .people
                .iter()
                .find(|p| {
                    p.id != person && p.name.as_ref().is_some_and(|m| m.eq_ignore_ascii_case(n))
                })
                .map(|p| p.id)
        });
        let conn = self.connection()?;
        if let Some(keep) = same {
            // The same person, heard as two: their lines and talk join up.
            let talk: i64 = people
                .people
                .iter()
                .filter(|p| p.id == person)
                .map(|p| p.talk_ms)
                .sum();
            people.people.retain(|p| p.id != person);
            if let Some(p) = people.people.iter_mut().find(|p| p.id == keep) {
                p.talk_ms += talk;
                p.guessed = false;
            }
            for line in people.lines.iter_mut().filter(|l| l.person == person) {
                line.person = keep;
            }
        } else if let Some(p) = people.people.iter_mut().find(|p| p.id == person) {
            p.name = name.clone();
            p.guessed = false;
            p.from_call = false;
            p.choices.clear();
        }
        conn.execute(
            "UPDATE notes SET people = ?1, updated_at = ?2 WHERE id = ?3",
            params![serde_json::to_string(&people)?, Self::now(), id],
        )?;

        if let Some(name) = &name {
            let fingerprint: Option<Vec<u8>> = conn
                .query_row(
                    "SELECT fingerprint FROM note_voices WHERE note_id = ?1 AND person = ?2",
                    params![id, person],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(fingerprint) = fingerprint.map(|b| from_blob(&b)) {
                self.remember_voice(&conn, name, &fingerprint)?;
            }
        }
        self.get_note(id).await
    }

    /// Adds a voice to what Bolke knows of `name`, averaging it in.
    fn remember_voice(&self, conn: &Connection, name: &str, fingerprint: &[f32]) -> Result<()> {
        let known: Option<(Vec<u8>, i64)> = conn
            .query_row(
                "SELECT fingerprint, heard FROM voices WHERE name = ?1",
                [name],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let (voice, heard) = match known {
            Some((blob, heard)) => {
                let old = from_blob(&blob);
                let w = heard as f32;
                let mut mixed: Vec<f32> = old
                    .iter()
                    .zip(fingerprint)
                    .map(|(a, b)| (a * w + b) / (w + 1.0))
                    .collect();
                let n = mixed.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
                mixed.iter_mut().for_each(|x| *x /= n);
                (mixed, heard + 1)
            }
            None => (fingerprint.to_vec(), 1),
        };
        conn.execute(
            "INSERT INTO voices (name, fingerprint, heard, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(name) DO UPDATE SET fingerprint = ?2, heard = ?3, updated_at = ?4",
            params![name, to_blob(&voice), heard, Self::now()],
        )?;
        Ok(())
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
pub(crate) fn parse_summary(stored: Option<String>) -> Option<NoteSummary> {
    let stored = stored?;
    Some(serde_json::from_str(&stored).unwrap_or(NoteSummary {
        summary: stored,
        ..Default::default()
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
        people: row
            .get::<_, Option<String>>(11)?
            .and_then(|json| serde_json::from_str(&json).ok()),
        people_state: row
            .get::<_, Option<String>>(12)?
            .as_deref()
            .and_then(PeopleState::parse),
    })
}

fn to_blob(fingerprint: &[f32]) -> Vec<u8> {
    fingerprint.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn from_blob(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

/// A voice takes a name from the call when at least this share of its
/// talking lines up with that name's tile lighting up...
const CALL_SURE: f32 = 0.5;
/// ...and this many times more than with anyone else's.
const CALL_AHEAD: f32 = 2.0;
/// Below sure, names lining up at least this much are offered as choices.
const CALL_MAYBE: f32 = 0.15;

/// Names the people a known voice didn't name, from who the call showed
/// talking at the same moments. Each name goes to one voice, the best
/// matches first. Unsure voices get the likeliest names as one-tap choices.
/// Returns the people named for sure.
fn name_from_call(people: &mut NotePeople, call: &CallNames) -> Vec<(u32, String)> {
    let first_word = |n: &str| n.split_whitespace().next().unwrap_or("").to_lowercase();
    // A name already on someone (a known voice) isn't free, nor is one with
    // the same first name ("Sam" for "Sam Lee").
    let taken: Vec<String> = people
        .people
        .iter()
        .filter_map(|p| p.name.as_deref().map(first_word))
        .collect();
    let free: Vec<&String> = call
        .people
        .iter()
        .filter(|n| !taken.contains(&first_word(n)))
        .collect();

    // How long each unnamed voice talked while each name's tile was lit.
    let mut scores: Vec<(f32, u32, usize)> = Vec::new();
    let mut by_person: Vec<(u32, Vec<(f32, usize)>)> = Vec::new();
    for person in people.people.iter().filter(|p| p.name.is_none()) {
        let lines: Vec<&NoteLine> = people
            .lines
            .iter()
            .filter(|l| l.person == person.id)
            .collect();
        let talk: i64 = lines.iter().map(|l| (l.end_ms - l.start_ms).max(0)).sum();
        if talk <= 0 {
            continue;
        }
        let mut shares: Vec<(f32, usize)> = free
            .iter()
            .enumerate()
            .map(|(k, name)| {
                let together: i64 = call
                    .speaking
                    .iter()
                    .filter(|s| &&s.name == name)
                    .map(|s| {
                        lines
                            .iter()
                            .map(|l| (l.end_ms.min(s.end_ms) - l.start_ms.max(s.start_ms)).max(0))
                            .sum::<i64>()
                    })
                    .sum();
                (together as f32 / talk as f32, k)
            })
            .filter(|(share, _)| *share > 0.0)
            .collect();
        shares.sort_by(|a, b| b.0.total_cmp(&a.0));
        if let Some(&(best, k)) = shares.first() {
            let next = shares.get(1).map_or(0.0, |s| s.0);
            if best >= CALL_SURE && best >= next * CALL_AHEAD {
                scores.push((best, person.id, k));
            }
        }
        by_person.push((person.id, shares));
    }

    scores.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut sure: Vec<(u32, String)> = Vec::new();
    let mut used: Vec<usize> = Vec::new();
    for (_, person, k) in scores {
        if used.contains(&k) || sure.iter().any(|(p, _)| *p == person) {
            continue;
        }
        used.push(k);
        sure.push((person, free[k].clone()));
    }
    for p in people.people.iter_mut() {
        if let Some((_, name)) = sure.iter().find(|(id, _)| *id == p.id) {
            p.name = Some(name.clone());
            p.guessed = true;
            p.from_call = true;
        } else if p.name.is_none() {
            p.choices = by_person
                .iter()
                .find(|(id, _)| *id == p.id)
                .map(|(_, shares)| {
                    shares
                        .iter()
                        .filter(|(share, k)| *share >= CALL_MAYBE && !used.contains(k))
                        .take(3)
                        .map(|(_, k)| free[*k].clone())
                        .collect()
                })
                .unwrap_or_default();
        }
    }
    sure
}

/// Gives people the names of known voices they match, each name once, the
/// closest matches first.
fn name_known_voices(
    people: &mut [Person],
    fingerprints: &[(u32, Vec<f32>)],
    known: &[(String, Vec<f32>)],
) {
    let mut pairs: Vec<(f32, u32, usize)> = Vec::new();
    for (person, fingerprint) in fingerprints {
        for (k, (_, voice)) in known.iter().enumerate() {
            let s = crate::speakers::similarity(fingerprint, voice);
            if s >= KNOWN_VOICE {
                pairs.push((s, *person, k));
            }
        }
    }
    pairs.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut used_people = Vec::new();
    let mut used_voices = Vec::new();
    for (_, person, k) in pairs {
        if used_people.contains(&person) || used_voices.contains(&k) {
            continue;
        }
        if let Some(p) = people
            .iter_mut()
            .find(|p| p.id == person && p.name.is_none())
        {
            p.name = Some(known[k].0.clone());
            p.guessed = true;
        }
        used_people.push(person);
        used_voices.push(k);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voices_take_names_from_who_the_call_showed_talking() {
        use crate::call_names::Speech;
        let person = |id, name: Option<&str>| Person {
            id,
            name: name.map(String::from),
            talk_ms: 0,
            guessed: false,
            from_call: false,
            choices: Vec::new(),
        };
        let line = |person, start_ms, end_ms| NoteLine {
            start_ms,
            end_ms,
            person,
            text: String::new(),
        };
        let speech = |name: &str, start_ms, end_ms| Speech {
            name: name.into(),
            start_ms,
            end_ms,
        };
        let mut people = NotePeople {
            people: vec![
                person(1, Some("sam")),
                person(2, None),
                person(3, None),
                person(4, None),
            ],
            lines: vec![
                line(1, 0, 5_000),
                // Voice 2 talks when Anita's tile lights up.
                line(2, 5_000, 9_000),
                line(2, 20_000, 24_000),
                // Voice 3 talks over both Dev and Rahul: not sure.
                line(3, 10_000, 14_000),
                // Voice 4 talks when no tile is lit.
                line(4, 30_000, 31_000),
            ],
        };
        let call = CallNames {
            platform: "meet".into(),
            people: vec![
                "Sam Lee".into(),
                "Anita".into(),
                "Dev".into(),
                "Rahul".into(),
            ],
            self_name: Some("Sam Lee".into()),
            speaking: vec![
                speech("Sam Lee", 0, 5_000),
                speech("Anita", 5_300, 9_000),
                speech("Anita", 20_100, 23_800),
                speech("Dev", 10_000, 12_200),
                speech("Rahul", 11_800, 14_000),
            ],
        };
        let sure = name_from_call(&mut people, &call);
        assert_eq!(sure, vec![(2, "Anita".to_string())]);
        let p = |id: u32| people.people.iter().find(|p| p.id == id).unwrap();
        assert_eq!(p(1).name.as_deref(), Some("sam"));
        assert_eq!(p(2).name.as_deref(), Some("Anita"));
        assert!(p(2).from_call);
        assert_eq!(p(3).name, None);
        assert_eq!(p(3).choices, vec!["Dev".to_string(), "Rahul".to_string()]);
        assert_eq!(p(4).name, None);
        assert!(p(4).choices.is_empty());
    }

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
            ..Default::default()
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
