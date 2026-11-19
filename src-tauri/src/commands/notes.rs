//! Notes, projects, meeting recording and AI summaries.

use crate::managers::{
    history::HistoryManager,
    meeting::{wav_duration_ms, MeetingRecorder, MeetingStatus, SavedMeeting},
    notes::{
        ActionPoint, Note, NoteChanges, NoteKind, NotePeople, NoteSummary, NotesManager,
        PersonSummary, Project,
    },
    speaker_models::{SpeakerModelManager, SpeakerModels},
    summary_model::{SummaryModel, SummaryModelManager},
    transcription_jobs::TranscriptionJobManager,
};
use crate::settings::{get_settings, AppSettings, APPLE_INTELLIGENCE_PROVIDER_ID};
use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

/// Tells the app's windows the notes changed outside them.
pub fn emit_notes_changed(app: &AppHandle) {
    let _ = app.emit("notes-changed", ());
}

fn err(e: anyhow::Error) -> String {
    e.to_string()
}

// ----- Projects -----

#[tauri::command]
#[specta::specta]
pub fn list_projects(notes: State<'_, Arc<NotesManager>>) -> Result<Vec<Project>, String> {
    notes.list_projects().map_err(err)
}

#[tauri::command]
#[specta::specta]
pub fn create_project(
    notes: State<'_, Arc<NotesManager>>,
    name: String,
) -> Result<Project, String> {
    notes.create_project(&name).map_err(err)
}

#[tauri::command]
#[specta::specta]
pub fn rename_project(
    notes: State<'_, Arc<NotesManager>>,
    id: i64,
    name: String,
) -> Result<(), String> {
    notes.rename_project(id, &name).map_err(err)
}

#[tauri::command]
#[specta::specta]
pub fn delete_project(notes: State<'_, Arc<NotesManager>>, id: i64) -> Result<(), String> {
    notes.delete_project(id).map_err(err)
}

// ----- Notes -----

#[tauri::command]
#[specta::specta]
pub async fn list_notes(
    notes: State<'_, Arc<NotesManager>>,
    project_id: Option<i64>,
) -> Result<Vec<Note>, String> {
    notes.list_notes(project_id).await.map_err(err)
}

#[tauri::command]
#[specta::specta]
pub async fn get_note(notes: State<'_, Arc<NotesManager>>, id: i64) -> Result<Note, String> {
    notes.get_note(id).await.map_err(err)
}

#[tauri::command]
#[specta::specta]
pub async fn create_text_note(
    notes: State<'_, Arc<NotesManager>>,
    title: String,
    project_id: Option<i64>,
) -> Result<Note, String> {
    notes
        .create_text_note(&title, project_id)
        .await
        .map_err(err)
}

#[tauri::command]
#[specta::specta]
pub async fn update_note(
    notes: State<'_, Arc<NotesManager>>,
    id: i64,
    changes: NoteChanges,
) -> Result<Note, String> {
    notes.update_note(id, changes).await.map_err(err)
}

#[tauri::command]
#[specta::specta]
pub async fn move_note(
    notes: State<'_, Arc<NotesManager>>,
    id: i64,
    project_id: Option<i64>,
) -> Result<Note, String> {
    notes.move_note(id, project_id).await.map_err(err)
}

#[tauri::command]
#[specta::specta]
pub async fn set_action_point_done(
    notes: State<'_, Arc<NotesManager>>,
    id: i64,
    index: u32,
    done: bool,
) -> Result<Note, String> {
    notes
        .set_action_point_done(id, index as usize, done)
        .await
        .map_err(err)
}

/// Starts or stops recording an idea, like pressing the idea shortcut.
#[tauri::command]
#[specta::specta]
pub fn toggle_idea_recording(app: AppHandle) {
    crate::signal_handle::send_transcription_input(&app, "record_idea", "notes");
}

/// Deletes the note and its recording.
#[tauri::command]
#[specta::specta]
pub async fn delete_note(
    notes: State<'_, Arc<NotesManager>>,
    history: State<'_, Arc<HistoryManager>>,
    jobs: State<'_, Arc<TranscriptionJobManager>>,
    id: i64,
) -> Result<(), String> {
    if let Some(entry_id) = notes.delete_note(id).map_err(err)? {
        jobs.cancel(entry_id);
        if let Err(e) = history.delete_entry(entry_id).await {
            warn!("Deleted note {id}, but not its recording: {e}");
        }
    }
    Ok(())
}

/// Makes a note from an audio or video file; its text is written out in the
/// background.
#[tauri::command]
#[specta::specta]
pub async fn import_audio_note(
    app: AppHandle,
    path: String,
    project_id: Option<i64>,
) -> Result<Note, String> {
    let path = PathBuf::from(path);
    let title = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let jobs = Arc::clone(&app.state::<Arc<TranscriptionJobManager>>());
    let adding = Arc::clone(&jobs);
    let entry = tauri::async_runtime::spawn_blocking(move || adding.add_file(path))
        .await
        .map_err(|e| format!("Failed to read the file: {}", e))?
        .map_err(err)?;
    let history = app.state::<Arc<HistoryManager>>();
    let duration_ms = wav_duration_ms(&history.get_audio_file_path(&entry.file_name));
    let note = app
        .state::<Arc<NotesManager>>()
        .create_recorded_note(NoteKind::Meeting, &title, project_id, entry.id, duration_ms)
        .await;
    jobs.start(entry.id);
    note.map_err(err)
}

// ----- Meeting recording -----

#[tauri::command]
#[specta::specta]
pub fn get_meeting_status(meeting: State<'_, Arc<MeetingRecorder>>) -> MeetingStatus {
    meeting.status()
}

#[tauri::command]
#[specta::specta]
pub fn start_meeting(
    meeting: State<'_, Arc<MeetingRecorder>>,
    title: String,
    project_id: Option<i64>,
    call: bool,
) -> Result<MeetingStatus, String> {
    meeting.start(title, project_id, call).map_err(err)
}

/// "Record" in the "Record this meeting?" bubble: starts the meeting, with
/// the call's sound when it can, and opens it in Meetings.
#[tauri::command]
#[specta::specta]
pub fn record_call(app: AppHandle, title: String) -> Result<(), String> {
    crate::overlay::hide_call_prompt(&app);
    let meeting = app.state::<Arc<MeetingRecorder>>();
    if !meeting.status().recording {
        meeting
            .start(title, None, crate::call_audio::available())
            .map_err(err)?;
    }
    crate::show_main_window(&app);
    let _ = app.emit("open-meetings", ());
    Ok(())
}

/// "Not now" in the "Record this meeting?" bubble.
#[tauri::command]
#[specta::specta]
pub fn dismiss_call_prompt(app: AppHandle) {
    crate::overlay::hide_call_prompt(&app);
}

/// Turns the call's sound on or off during a meeting.
#[tauri::command]
#[specta::specta]
pub fn set_meeting_call(meeting: State<'_, Arc<MeetingRecorder>>, on: bool) {
    meeting.set_call(on);
}

/// Whether a meeting can record the call's sound on this computer.
#[tauri::command]
#[specta::specta]
pub fn can_record_calls() -> bool {
    crate::call_audio::available()
}

#[tauri::command]
#[specta::specta]
pub fn update_meeting(
    meeting: State<'_, Arc<MeetingRecorder>>,
    title: String,
    project_id: Option<i64>,
) -> Result<(), String> {
    meeting.update(title, project_id).map_err(err)
}

/// Stops and saves the recording as a note; its text is written out in the
/// background. `None` if nothing was recording.
#[tauri::command]
#[specta::specta]
pub async fn stop_meeting(app: AppHandle) -> Result<Option<Note>, String> {
    let meeting = Arc::clone(&app.state::<Arc<MeetingRecorder>>());
    let Some(saved) = meeting.stop().map_err(err)? else {
        return Ok(None);
    };
    save_meeting(&app, saved).await.map(Some).map_err(err)
}

#[tauri::command]
#[specta::specta]
pub fn discard_meeting(meeting: State<'_, Arc<MeetingRecorder>>) -> Result<(), String> {
    meeting.discard().map_err(err)
}

/// Adds the recording to History, queues its transcript and makes its note.
async fn save_meeting(app: &AppHandle, saved: SavedMeeting) -> Result<Note> {
    let SavedMeeting {
        pending,
        duration_ms,
    } = saved;
    let jobs = app.state::<Arc<TranscriptionJobManager>>();
    let entry = jobs.add_recording(pending.file_name.clone(), pending.title.clone())?;
    // The note comes first, so the job knows it's a meeting.
    let note = app
        .state::<Arc<NotesManager>>()
        .create_recorded_note(
            NoteKind::Meeting,
            &pending.title,
            pending.project_id,
            entry.id,
            Some(duration_ms),
        )
        .await;
    jobs.start(entry.id);
    let note = note?;
    app.state::<Arc<MeetingRecorder>>().forget(&pending);
    emit_notes_changed(app);
    Ok(note)
}

/// Saves recordings a crash or quit left unsaved. Run once at startup.
pub async fn rescue_meetings(app: AppHandle) {
    let leftovers = app.state::<Arc<MeetingRecorder>>().leftovers();
    for saved in leftovers {
        let file_name = saved.pending.file_name.clone();
        match save_meeting(&app, saved).await {
            Ok(note) => info!("Rescued meeting recording {file_name} as note {}", note.id),
            Err(e) => error!("Failed to rescue meeting recording {file_name}: {e}"),
        }
    }
}

// ----- AI summary -----

/// Writes the note's summary and action points (see [`SummarySource`] for
/// what writes them).
#[tauri::command]
#[specta::specta]
pub async fn summarize_note(app: AppHandle, id: i64) -> Result<Note, String> {
    let notes = Arc::clone(&app.state::<Arc<NotesManager>>());
    let note = notes.get_note(id).await.map_err(err)?;
    if note.text.trim().is_empty() {
        return Err("This note has no text to summarize yet".into());
    }
    let summary = summarize(&app, id, &note.text, note.people.as_ref())
        .await
        .map_err(err)?;
    notes.set_summary(id, Some(&summary)).map_err(err)?;
    notes.get_note(id).await.map_err(err)
}

/// Long transcripts are cut to this many characters before summarizing.
const MAX_SUMMARY_INPUT: usize = 60_000;

const SUMMARY_PROMPT: &str = "You summarize notes and meeting transcripts. \
Write in the same language as the text. Give a short summary of two to four \
sentences, and a list of action points: things someone said they or others \
will do. Each action point is one short sentence. If there are none, give an \
empty list. Reply only with JSON: {\"summary\": \"...\", \"action_points\": [\"...\"]}";

/// First pass over one person's words, part by part: short notes.
const PERSON_NOTES_PROMPT: &str = "The user message is what one person said in \
a meeting. Take short notes on it in your own words, in the same language. \
Never copy their sentences. Never write \"I\" or a name: start each note with \
a verb such as Wants, Thinks, Asked, Explained, Worried. said: up to 4 main \
points, each under 15 words. agreed: what they agreed to, or an empty list. \
will_do: tasks they took on, each under 12 words, or an empty list. Reply \
only with JSON with the keys said, agreed and will_do, each a list of strings.";

/// Second pass: the notes boiled down to a card.
const PERSON_CARD_PROMPT: &str = "The user message is notes on what one person \
said in a meeting. Combine them into a short card, in the same language. \
gist: one sentence under 30 words on what this person mainly cared about or \
argued. said: the 4 most important points, each under 15 words, none \
repeating. agreed: up to 3. will_do: up to 3 tasks. Never write \"I\" or a \
name: start with a verb. Reply only with JSON with the keys gist (a string), \
said, agreed and will_do (lists of strings).";

/// How much of each a card keeps, however much comes back.
const MAX_SAID: usize = 4;
const MAX_AGREED: usize = 3;
const MAX_WILL_DO: usize = 3;
/// Longer than this isn't a note; it's their words copied.
const MAX_ITEM_CHARS: usize = 140;
const MAX_GIST_CHARS: usize = 260;

/// People who talked less than this get no card of their own.
const MIN_PERSON_TALK_MS: i64 = 15_000;

/// What writes a note's summary, in the order Bolke tries them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SummarySource {
    /// The AI service set up in Post Process settings.
    Service,
    /// A free summary model downloaded to this computer.
    LocalModel,
    /// Apple Intelligence, on this Mac, when neither of those is there.
    AppleIntelligence,
    /// Nothing yet: the note offers to download a summary model.
    None,
}

/// The post-process provider and model, when the user has set one up.
/// Apple Intelligence picked there counts only where it's available.
fn configured_service(
    settings: &AppSettings,
) -> Option<(crate::settings::PostProcessProvider, String)> {
    let provider = settings.active_post_process_provider()?.clone();
    if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID && !apple_intelligence_ready() {
        return None;
    }
    let model = settings
        .post_process_models
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    (!model.trim().is_empty()).then_some((provider, model))
}

fn apple_intelligence_ready() -> bool {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return crate::apple_intelligence::check_apple_intelligence_availability();
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    false
}

fn summary_source(app: &AppHandle) -> SummarySource {
    if configured_service(&get_settings(app)).is_some() {
        SummarySource::Service
    } else if app.state::<Arc<SummaryModelManager>>().active().is_some() {
        SummarySource::LocalModel
    } else if apple_intelligence_ready() {
        SummarySource::AppleIntelligence
    } else {
        SummarySource::None
    }
}

/// What will write a summary, so the note can say so before the user asks.
#[tauri::command]
#[specta::specta]
pub fn get_summary_source(app: AppHandle) -> SummarySource {
    summary_source(&app)
}

/// Sent as "summary-progress" while a long note is summarized in parts.
#[derive(Clone, Debug, serde::Serialize, specta::Type)]
pub struct SummaryProgress {
    pub note_id: i64,
    pub part: u32,
    pub parts: u32,
}

/// What writes one part, with its settings.
enum Writer {
    Service {
        provider: crate::settings::PostProcessProvider,
        model: String,
        api_key: String,
    },
    /// The downloaded model, served by the helper at `base_url`.
    Local {
        base_url: String,
    },
    AppleIntelligence {
        token_limit: i32,
    },
}

impl Writer {
    /// The most text one request takes. Apple's model reads about 4,000
    /// tokens with its reply; the helper 8,192. Services take it all.
    fn part_chars(&self) -> usize {
        match self {
            Writer::Service { .. } => MAX_SUMMARY_INPUT,
            Writer::Local { .. } => 10_000,
            Writer::AppleIntelligence { .. } => 4_000,
        }
    }

    async fn write(&self, text: String) -> Result<NoteSummary> {
        let reply = self.ask(SUMMARY_PROMPT, summary_schema(), text).await?;
        let mut summary = parse_summary_reply(&reply)?;
        if let Writer::AppleIntelligence { token_limit } = self {
            // A word limit set for Apple Intelligence in Post Process applies
            // to the summary, not the action points.
            if *token_limit > 0 {
                let limit = *token_limit as usize;
                let words: Vec<&str> = summary.summary.split_whitespace().collect();
                if words.len() > limit {
                    summary.summary = words[..limit].join(" ");
                }
            }
        }
        Ok(summary)
    }

    /// Sends `text` with a prompt that asks for JSON shaped like `schema`;
    /// returns the reply as it came.
    async fn ask(&self, prompt: &str, schema: serde_json::Value, text: String) -> Result<String> {
        match self {
            Writer::Service {
                provider,
                model,
                api_key,
            } => {
                let disable_reasoning = matches!(provider.id.as_str(), "custom" | "openrouter");
                ask_chat_service(
                    provider,
                    api_key.clone(),
                    model,
                    prompt,
                    schema,
                    text,
                    disable_reasoning,
                )
                .await
            }
            Writer::Local { base_url } => {
                let provider = crate::settings::PostProcessProvider {
                    id: "bolke-summary-model".into(),
                    label: "Bolke summary model".into(),
                    base_url: base_url.clone(),
                    allow_base_url_edit: false,
                    models_endpoint: None,
                    supports_structured_output: true,
                    api_format: Default::default(),
                };
                ask_chat_service(
                    &provider,
                    String::new(),
                    "summary",
                    prompt,
                    schema,
                    text,
                    false,
                )
                .await
            }
            Writer::AppleIntelligence { .. } => ask_on_device(prompt, text).await,
        }
    }
}

async fn summarize(
    app: &AppHandle,
    note_id: i64,
    text: &str,
    people: Option<&NotePeople>,
) -> Result<NoteSummary> {
    let people = people.cloned();
    let text: String = text.chars().take(MAX_SUMMARY_INPUT).collect();
    let settings = get_settings(app);
    if let Some((provider, model)) = configured_service(&settings) {
        debug!("Summarizing a note with provider '{}'", provider.id);
        let writer = if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
            Writer::AppleIntelligence {
                token_limit: model.trim().parse::<i32>().unwrap_or(0),
            }
        } else {
            let api_key = settings
                .post_process_api_keys
                .get(&provider.id)
                .cloned()
                .unwrap_or_default();
            Writer::Service {
                provider,
                model,
                api_key,
            }
        };
        return summarize_with_people(app, note_id, &writer, text, people).await;
    }
    let models = Arc::clone(&app.state::<Arc<SummaryModelManager>>());
    if models.active().is_some() {
        let app = app.clone();
        return models
            .with_helper(|base_url, model| async move {
                debug!("Summarizing a note with the summary model {}", model.id);
                let writer = Writer::Local { base_url };
                summarize_with_people(&app, note_id, &writer, text, people).await
            })
            .await;
    }
    if apple_intelligence_ready() {
        debug!("Summarizing a note with Apple Intelligence");
        let writer = Writer::AppleIntelligence { token_limit: 0 };
        return summarize_with_people(app, note_id, &writer, text, people).await;
    }
    Err(anyhow!("no_ai_provider"))
}

/// The whole note's summary, then a card for each person who said enough.
async fn summarize_with_people(
    app: &AppHandle,
    note_id: i64,
    writer: &Writer,
    text: String,
    people: Option<NotePeople>,
) -> Result<NoteSummary> {
    let mut summary = summarize_in_parts(app, note_id, writer, text).await?;
    let Some(people) = people else {
        return Ok(summary);
    };
    let speakers = people_to_summarize(&people);
    if speakers.len() < 2 {
        return Ok(summary);
    }
    // Someone who talked a long time is read in parts, like a long note.
    let room = writer.part_chars().saturating_sub(400).max(1_000);
    let jobs: Vec<(u32, String, Vec<String>)> = speakers
        .into_iter()
        .map(|(id, words)| {
            let parts = split_into_parts(&words, room);
            (id, words, parts)
        })
        .collect();
    // Each part, then one pass per person to boil it down.
    let count: u32 = jobs
        .iter()
        .map(|(_, _, parts)| parts.len() as u32 + 1)
        .sum();
    let mut done = 0;
    let mut progress = |app: &AppHandle| {
        done += 1;
        let _ = app.emit(
            "summary-progress",
            SummaryProgress {
                note_id,
                part: done,
                parts: count,
            },
        );
    };
    for (id, words, parts) in jobs {
        if let Some(card) = write_person_card(writer, id, &words, parts, || progress(app)).await {
            summary.people.push(card);
        }
    }
    Ok(summary)
}

/// One person's card: notes on each part of their words, then those notes
/// boiled down. `on_step` runs before each call to the AI.
async fn write_person_card(
    writer: &Writer,
    id: u32,
    words: &str,
    parts: Vec<String>,
    mut on_step: impl FnMut(),
) -> Option<PersonSummary> {
    let mut notes = PersonSummary {
        person: id,
        ..Default::default()
    };
    for part in parts {
        on_step();
        match writer
            .ask(PERSON_NOTES_PROMPT, person_schema(false), part)
            .await
        {
            Ok(reply) => match parse_person_reply(id, &reply, words) {
                Some(more) => merge_person(&mut notes, more),
                None => debug!("Part of person {id}'s notes came back empty"),
            },
            // One person failing shouldn't lose the whole summary.
            Err(e) => warn!("Failed to take notes on person {id}'s words: {e}"),
        }
    }
    on_step();
    if notes.said.is_empty() && notes.agreed.is_empty() && notes.will_do.is_empty() {
        return None;
    }
    let card = match writer
        .ask(PERSON_CARD_PROMPT, person_schema(true), notes_text(&notes))
        .await
    {
        Ok(reply) => parse_person_reply(id, &reply, words),
        Err(e) => {
            warn!("Failed to boil down person {id}'s notes: {e}");
            None
        }
    };
    Some(trimmed(card.unwrap_or(notes)))
}

/// The notes as the second pass reads them.
fn notes_text(notes: &PersonSummary) -> String {
    let list = |title: &str, items: &[String]| {
        let mut text = format!("{title}:\n");
        for item in items {
            text.push_str(&format!("- {item}\n"));
        }
        text
    };
    [
        list("said", &notes.said),
        list("agreed", &notes.agreed),
        list("will_do", &notes.will_do),
    ]
    .join("\n")
}

/// A card cut to what fits, whatever came back.
fn trimmed(mut card: PersonSummary) -> PersonSummary {
    card.said.truncate(MAX_SAID);
    card.agreed.truncate(MAX_AGREED);
    card.will_do.truncate(MAX_WILL_DO);
    card
}

/// Lower-case words, for spotting copied sentences.
fn plain_words(text: &str) -> String {
    text.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// One note tidied: written about them, not by them, and not their words
/// copied. `None` when there's nothing usable.
fn tidy_note(item: &str, their_words: &str, limit: usize) -> Option<String> {
    let mut item = item
        .trim()
        .trim_start_matches(['-', '•', '*'])
        .trim()
        .trim_matches('"')
        .trim()
        .to_string();
    // Pieces of JSON a small model left in.
    if item.is_empty() || item.contains(['[', ']', '{', '}']) || item.contains("\":") {
        return None;
    }
    // "I said that X" -> "X"; "I will send it" -> "Send it".
    let lower = item.to_lowercase();
    for start in [
        "i said that ",
        "i mentioned that ",
        "i emphasized that ",
        "i noted that ",
        "i added that ",
        "i highlighted ",
        "i suggested ",
        "i discussed ",
        "i think ",
        "i feel ",
        "i will ",
        "i'll ",
        "we will ",
        "i ",
    ] {
        if lower.starts_with(start) {
            item = item[start.len()..].trim().to_string();
            break;
        }
    }
    let mut chars = item.chars();
    item = match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => return None,
    };
    item = item.trim_end_matches(['.', ';', ',']).to_string();
    if item.chars().count() > limit {
        return None;
    }
    // Eight or more of their words in a row is a quote, not a note.
    let plain = plain_words(&item);
    if plain.split(' ').count() >= 8 && their_words.contains(&plain) {
        return None;
    }
    Some(item)
}

/// Adds a part's points to a person's card, skipping repeats.
fn merge_person(card: &mut PersonSummary, more: PersonSummary) {
    fn add(to: &mut Vec<String>, items: Vec<String>) {
        for item in items {
            if !to.iter().any(|t| t.eq_ignore_ascii_case(&item)) {
                to.push(item);
            }
        }
    }
    add(&mut card.said, more.said);
    add(&mut card.agreed, more.agreed);
    add(&mut card.will_do, more.will_do);
}

/// Each person who talked long enough, with their words in order.
fn people_to_summarize(people: &NotePeople) -> Vec<(u32, String)> {
    people
        .people
        .iter()
        .filter(|p| p.talk_ms >= MIN_PERSON_TALK_MS)
        .map(|p| {
            let words: Vec<&str> = people
                .lines
                .iter()
                .filter(|l| l.person == p.id && !l.text.trim().is_empty())
                .map(|l| l.text.trim())
                .collect();
            (p.id, words.join("\n"))
        })
        .filter(|(_, words)| !words.is_empty())
        .collect()
}

fn summary_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "summary": { "type": "string" },
            "action_points": { "type": "array", "items": { "type": "string" } }
        },
        "required": ["summary", "action_points"],
        "additionalProperties": false
    })
}

fn person_schema(gist: bool) -> serde_json::Value {
    let list = serde_json::json!({ "type": "array", "items": { "type": "string" } });
    if gist {
        serde_json::json!({
            "type": "object",
            "properties": {
                "gist": { "type": "string" },
                "said": list, "agreed": list, "will_do": list
            },
            "required": ["gist", "said", "agreed", "will_do"],
            "additionalProperties": false
        })
    } else {
        serde_json::json!({
            "type": "object",
            "properties": { "said": list, "agreed": list, "will_do": list },
            "required": ["said", "agreed", "will_do"],
            "additionalProperties": false
        })
    }
}

/// Reads a person's JSON card, tidying each note against `their_words`;
/// `None` when nothing usable is in it.
fn parse_person_reply(person: u32, reply: &str, their_words: &str) -> Option<PersonSummary> {
    #[derive(serde::Deserialize)]
    struct Reply {
        #[serde(default)]
        gist: String,
        #[serde(default)]
        said: Vec<String>,
        #[serde(default)]
        agreed: Vec<String>,
        #[serde(default)]
        will_do: Vec<String>,
    }
    let json = json_body(reply);
    let reply: Reply = serde_json::from_str(json).ok()?;
    let their_words = plain_words(their_words);
    let clean = |items: Vec<String>| -> Vec<String> {
        let mut kept: Vec<String> = Vec::new();
        for item in items {
            if let Some(note) = tidy_note(&item, &their_words, MAX_ITEM_CHARS) {
                if !kept.iter().any(|k| k.eq_ignore_ascii_case(&note)) {
                    kept.push(note);
                }
            }
        }
        kept
    };
    let card = PersonSummary {
        person,
        gist: tidy_note(&reply.gist, &their_words, MAX_GIST_CHARS)
            .map(|g| format!("{g}."))
            .unwrap_or_default(),
        said: clean(reply.said),
        agreed: clean(reply.agreed),
        will_do: clean(reply.will_do),
    };
    if card.said.is_empty() && card.agreed.is_empty() && card.will_do.is_empty() {
        return None;
    }
    Some(card)
}

/// The JSON in a reply, past any `<think>` block or code fence.
fn json_body(reply: &str) -> &str {
    let mut body = reply.trim();
    if let Some(end) = body.find("</think>") {
        body = body[end + "</think>".len()..].trim();
    }
    match (body.find('{'), body.rfind('}')) {
        (Some(start), Some(end)) if end > start => &body[start..=end],
        _ => body,
    }
}

/// Splits `text` at spaces into parts of at most `limit` characters.
fn split_into_parts(text: &str, limit: usize) -> Vec<String> {
    let mut parts = Vec::new();
    let mut part = String::new();
    for word in text.split_whitespace() {
        if !part.is_empty() && part.chars().count() + 1 + word.chars().count() > limit {
            parts.push(std::mem::take(&mut part));
        }
        if !part.is_empty() {
            part.push(' ');
        }
        part.push_str(word);
    }
    if !part.is_empty() {
        parts.push(part);
    }
    parts
}

/// A note that fits is summarized at once. A longer one is summarized part
/// by part; the parts' summaries are then summarized together, and every
/// part's action points are kept.
async fn summarize_in_parts(
    app: &AppHandle,
    note_id: i64,
    writer: &Writer,
    text: String,
) -> Result<NoteSummary> {
    let limit = writer.part_chars();
    if text.chars().count() <= limit {
        return writer.write(text).await;
    }
    let parts = split_into_parts(&text, limit);
    let count = parts.len() as u32;
    debug!("Summarizing a long note in {count} parts");
    let mut summaries = Vec::with_capacity(parts.len());
    let mut action_points: Vec<ActionPoint> = Vec::new();
    for (i, part) in parts.into_iter().enumerate() {
        let _ = app.emit(
            "summary-progress",
            SummaryProgress {
                note_id,
                part: i as u32 + 1,
                parts: count,
            },
        );
        let summary = writer.write(part).await?;
        summaries.push(summary.summary);
        for point in summary.action_points {
            if !action_points.iter().any(|p| p.text == point.text) {
                action_points.push(point);
            }
        }
    }
    let joined = summaries.join("\n\n");
    let summary = if joined.chars().count() <= limit {
        writer.write(joined).await?.summary
    } else {
        joined
    };
    Ok(NoteSummary {
        summary,
        action_points,
        people: Vec::new(),
    })
}

/// Asks an OpenAI-style chat endpoint (a service, or the summary helper)
/// for JSON shaped like `schema`.
async fn ask_chat_service(
    provider: &crate::settings::PostProcessProvider,
    api_key: String,
    model: &str,
    prompt: &str,
    schema: serde_json::Value,
    text: String,
    disable_reasoning: bool,
) -> Result<String> {
    if provider.supports_structured_output {
        crate::llm_client::send_chat_completion_with_schema(
            provider,
            api_key,
            model,
            text,
            Some(prompt.to_string()),
            Some(schema),
            disable_reasoning,
        )
        .await
    } else {
        crate::llm_client::send_chat_completion(
            provider,
            api_key,
            model,
            format!("{prompt}\n\nText:\n{text}"),
            disable_reasoning,
        )
        .await
    }
    .map_err(|e| anyhow!("{e}"))?
    .ok_or_else(|| anyhow!("The AI sent back nothing"))
}

/// Asks Apple Intelligence, on this Mac.
async fn ask_on_device(prompt: &str, text: String) -> Result<String> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        if !crate::apple_intelligence::check_apple_intelligence_availability() {
            return Err(anyhow!("Apple Intelligence isn't available on this Mac"));
        }
        let prompt = prompt.to_string();
        tauri::async_runtime::spawn_blocking(move || {
            crate::apple_intelligence::process_text_plain(&prompt, &text)
        })
        .await
        .map_err(|e| anyhow!("{e}"))?
        .map_err(|e| anyhow!("{e}"))
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        let _ = (prompt, text);
        Err(anyhow!("Apple Intelligence isn't available here"))
    }
}

// ----- Who said what -----

#[tauri::command]
#[specta::specta]
pub fn get_speaker_models(app: AppHandle) -> SpeakerModels {
    app.state::<Arc<SpeakerModelManager>>().status()
}

/// Downloads the voice models; returns whether they're ready (false when
/// cancelled). Progress goes out as "speaker-models-progress".
#[tauri::command]
#[specta::specta]
pub async fn download_speaker_models(app: AppHandle) -> Result<bool, String> {
    let models = Arc::clone(&app.state::<Arc<SpeakerModelManager>>());
    models.download().await.map_err(err)
}

#[tauri::command]
#[specta::specta]
pub fn cancel_speaker_models_download(app: AppHandle) {
    app.state::<Arc<SpeakerModelManager>>().cancel_download();
}

/// Finds who said what in a recorded note, in the background.
#[tauri::command]
#[specta::specta]
pub async fn find_note_people(app: AppHandle, id: i64) -> Result<Note, String> {
    if app.state::<Arc<SpeakerModelManager>>().ready().is_none() {
        return Err("The voice models aren't downloaded".into());
    }
    let notes = Arc::clone(&app.state::<Arc<NotesManager>>());
    let note = notes.get_note(id).await.map_err(err)?;
    let entry_id = note
        .entry_id
        .ok_or_else(|| "This note has no recording".to_string())?;
    notes
        .set_people_state(id, Some(crate::managers::notes::PeopleState::Working))
        .map_err(err)?;
    app.state::<Arc<TranscriptionJobManager>>()
        .find_people(entry_id);
    notes.get_note(id).await.map_err(err)
}

/// Names a person in a meeting (the same name twice makes them one person);
/// Bolke remembers the voice for the next meeting.
#[tauri::command]
#[specta::specta]
pub async fn name_note_person(
    app: AppHandle,
    id: i64,
    person: u32,
    name: String,
) -> Result<Note, String> {
    let notes = Arc::clone(&app.state::<Arc<NotesManager>>());
    notes.name_person(id, person, &name).await.map_err(err)
}

/// Everyone Bolke knows by voice, most recent first.
#[tauri::command]
#[specta::specta]
pub fn get_voice_names(app: AppHandle) -> Result<Vec<String>, String> {
    app.state::<Arc<NotesManager>>().voice_names().map_err(err)
}

// ----- Summary models -----

#[tauri::command]
#[specta::specta]
pub fn get_summary_models(app: AppHandle) -> Vec<SummaryModel> {
    app.state::<Arc<SummaryModelManager>>().list()
}

/// Downloads a summary model; returns when it's done or cancelled.
#[tauri::command]
#[specta::specta]
pub async fn download_summary_model(app: AppHandle, id: String) -> Result<(), String> {
    let models = Arc::clone(&app.state::<Arc<SummaryModelManager>>());
    models.download(&id).await.map_err(err)
}

/// Setup's "Also get free AI summaries": downloads the model once the speech
/// model is done.
#[tauri::command]
#[specta::specta]
pub fn queue_summary_model_download(app: AppHandle, id: String) -> Result<(), String> {
    app.state::<Arc<SummaryModelManager>>()
        .queue_download(&id)
        .map_err(err)
}

#[tauri::command]
#[specta::specta]
pub fn cancel_summary_model_download(app: AppHandle, id: String) {
    app.state::<Arc<SummaryModelManager>>().cancel_download(&id);
}

#[tauri::command]
#[specta::specta]
pub fn delete_summary_model(app: AppHandle, id: String) -> Result<(), String> {
    app.state::<Arc<SummaryModelManager>>()
        .delete(&id)
        .map_err(err)
}

#[tauri::command]
#[specta::specta]
pub fn select_summary_model(app: AppHandle, id: String) -> Result<(), String> {
    app.state::<Arc<SummaryModelManager>>()
        .select(&id)
        .map_err(err)
}

#[derive(serde::Deserialize)]
struct SummaryReply {
    summary: String,
    #[serde(default)]
    action_points: Vec<String>,
}

/// Reads the JSON reply, allowing a `<think>` block or code fence around it.
/// A reply that isn't JSON is kept whole as the summary.
fn parse_summary_reply(reply: &str) -> Result<NoteSummary> {
    let body = {
        let b = reply.trim();
        match b.find("</think>") {
            Some(end) => b[end + "</think>".len()..].trim(),
            None => b,
        }
    };
    let json = json_body(reply);
    let reply = serde_json::from_str::<SummaryReply>(json).unwrap_or_else(|_| SummaryReply {
        summary: body.to_string(),
        action_points: Vec::new(),
    });
    let summary = NoteSummary {
        summary: reply.summary.trim().to_string(),
        action_points: reply
            .action_points
            .into_iter()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .map(|text| ActionPoint { text, done: false })
            .collect(),
        people: Vec::new(),
    };
    if summary.summary.is_empty() && summary.action_points.is_empty() {
        return Err(anyhow!("The AI sent back nothing"));
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_splits_at_spaces_within_the_limit() {
        let text = "one two three four five six seven";
        let parts = split_into_parts(text, 10);
        assert_eq!(parts, vec!["one two", "three four", "five six", "seven"]);
        assert!(parts.iter().all(|p| p.chars().count() <= 10));
        assert_eq!(split_into_parts("short", 100), vec!["short"]);
        // A single word longer than the limit stays whole.
        assert_eq!(split_into_parts("abcdefghijkl", 5), vec!["abcdefghijkl"]);
    }

    #[test]
    fn person_cards_are_read_and_empty_ones_dropped() {
        let words = "So the deck is long and we should cut it. I will send the deck by Friday.";
        let reply = r#"<think>x</think>```json
{"gist":"Wants a shorter deck","said":[" Deck is long ","","will_do]: [","I said that the deck needs cutting"],"agreed":[],"will_do":["I will send the deck by Friday"]}
```"#;
        let card = parse_person_reply(2, reply, words).unwrap();
        assert_eq!(card.person, 2);
        assert_eq!(card.gist, "Wants a shorter deck.");
        assert_eq!(card.said, vec!["Deck is long", "The deck needs cutting"]);
        assert_eq!(card.will_do, vec!["Send the deck by Friday"]);
        assert!(parse_person_reply(1, r#"{"said":[],"agreed":[],"will_do":[]}"#, "").is_none());
        assert!(parse_person_reply(1, "not json", "").is_none());
    }

    #[test]
    fn copied_sentences_and_long_items_are_not_notes() {
        let words = plain_words(
            "Unless we write down the whole plan first, we should not start building it, right?",
        );
        assert_eq!(
            tidy_note(
                "Unless we write down the whole plan first we should not start building",
                &words,
                MAX_ITEM_CHARS
            ),
            None
        );
        assert_eq!(
            tidy_note(
                "Wants the whole process mapped first",
                &words,
                MAX_ITEM_CHARS
            ),
            Some("Wants the whole process mapped first".into())
        );
        assert_eq!(tidy_note(&"word ".repeat(40), &words, MAX_ITEM_CHARS), None);
    }

    #[test]
    fn only_people_who_talked_enough_get_a_card() {
        use crate::managers::notes::{NoteLine, Person};
        let person = |id, talk_ms| Person {
            id,
            name: None,
            talk_ms,
            guessed: false,
        };
        let line = |person, text: &str| NoteLine {
            start_ms: 0,
            end_ms: 0,
            person,
            text: text.into(),
        };
        let people = NotePeople {
            people: vec![person(1, 60_000), person(2, 3_000), person(3, 20_000)],
            lines: vec![
                line(1, "hello"),
                line(2, "hi"),
                line(3, "yes"),
                line(1, "bye"),
            ],
        };
        let got = people_to_summarize(&people);
        assert_eq!(
            got,
            vec![(1, "hello\nbye".to_string()), (3, "yes".to_string())]
        );
    }

    #[test]
    fn summary_replies_are_read_with_or_without_wrapping() {
        let plain = r#"{"summary":" Short. ","action_points":["Send logo", " "]}"#;
        let got = parse_summary_reply(plain).unwrap();
        assert_eq!(got.summary, "Short.");
        assert_eq!(got.action_points.len(), 1);
        assert_eq!(got.action_points[0].text, "Send logo");

        let fenced = "<think>hmm</think>\n```json\n{\"summary\":\"S\",\"action_points\":[]}\n```";
        assert_eq!(parse_summary_reply(fenced).unwrap().summary, "S");

        let prose = "Just a sentence.";
        assert_eq!(parse_summary_reply(prose).unwrap().summary, prose);
        assert!(parse_summary_reply("  ").is_err());
    }
}

/// Writes the person cards for a real meeting with a running summary
/// helper: `BOLKE_HELPER=http://127.0.0.1:PORT/v1 BOLKE_NOTE=12 cargo test
/// --lib person_cards_live -- --ignored --nocapture`
#[cfg(test)]
mod person_cards_live {
    use super::*;

    #[test]
    #[ignore]
    fn person_cards_live() {
        let base_url = std::env::var("BOLKE_HELPER").unwrap();
        let note: i64 = std::env::var("BOLKE_NOTE").unwrap().parse().unwrap();
        let db = std::path::PathBuf::from(std::env::var("HOME").unwrap())
            .join("Library/Application Support/app.bolke.desktop/notes.db");
        let conn =
            rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap();
        let people: String = conn
            .query_row("SELECT people FROM notes WHERE id = ?1", [note], |r| {
                r.get(0)
            })
            .unwrap();
        let people: NotePeople = serde_json::from_str(&people).unwrap();
        let writer = Writer::Local { base_url };
        let room = writer.part_chars().saturating_sub(400).max(1_000);
        tauri::async_runtime::block_on(async {
            for (id, words) in people_to_summarize(&people) {
                let parts = split_into_parts(&words, room);
                let card = write_person_card(&writer, id, &words, parts, || {}).await;
                println!("\n== person {id} ==\n{card:#?}");
            }
        });
    }
}
