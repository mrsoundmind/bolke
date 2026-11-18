//! Notes, projects, meeting recording and AI summaries.

use crate::managers::{
    history::HistoryManager,
    meeting::{wav_duration_ms, MeetingRecorder, MeetingStatus, SavedMeeting},
    notes::{ActionPoint, Note, NoteChanges, NoteKind, NoteSummary, NotesManager, Project},
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
    let entry = tauri::async_runtime::spawn_blocking(move || jobs.transcribe_file(path))
        .await
        .map_err(|e| format!("Failed to read the file: {}", e))?
        .map_err(err)?;
    let history = app.state::<Arc<HistoryManager>>();
    let duration_ms = wav_duration_ms(&history.get_audio_file_path(&entry.file_name));
    app.state::<Arc<NotesManager>>()
        .create_recorded_note(NoteKind::Note, &title, project_id, entry.id, duration_ms)
        .await
        .map_err(err)
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
) -> Result<MeetingStatus, String> {
    meeting.start(title, project_id).map_err(err)
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
    let entry = jobs.transcribe_recording(pending.file_name.clone(), pending.title.clone())?;
    let note = app
        .state::<Arc<NotesManager>>()
        .create_recorded_note(
            NoteKind::Meeting,
            &pending.title,
            pending.project_id,
            entry.id,
            Some(duration_ms),
        )
        .await?;
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

/// Writes the note's summary and action points with the AI provider set up
/// for post-processing.
#[tauri::command]
#[specta::specta]
pub async fn summarize_note(app: AppHandle, id: i64) -> Result<Note, String> {
    let notes = Arc::clone(&app.state::<Arc<NotesManager>>());
    let note = notes.get_note(id).await.map_err(err)?;
    if note.text.trim().is_empty() {
        return Err("This note has no text to summarize yet".into());
    }
    let summary = summarize(&get_settings(&app), &note.text)
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

/// What writes a note's summary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum SummarySource {
    /// The AI service set up in Post Process settings.
    Service,
    /// Apple Intelligence, on this Mac, when no service is set up.
    AppleIntelligence,
    /// Nothing: the user has to set up a service.
    None,
}

/// The post-process provider and model, when the user has set one up.
fn configured_service(
    settings: &AppSettings,
) -> Option<(crate::settings::PostProcessProvider, String)> {
    let provider = settings.active_post_process_provider()?.clone();
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

fn summary_source(settings: &AppSettings) -> SummarySource {
    match configured_service(settings) {
        // Apple Intelligence picked by hand counts as a service.
        Some(_) => SummarySource::Service,
        None if apple_intelligence_ready() => SummarySource::AppleIntelligence,
        None => SummarySource::None,
    }
}

/// What will write a summary, so the note can say so before the user asks.
#[tauri::command]
#[specta::specta]
pub fn get_summary_source(app: AppHandle) -> SummarySource {
    summary_source(&get_settings(&app))
}

/// Apple's on-device model reads about 4,000 tokens, its reply included, so
/// a longer note is summarized in parts of this many words.
const ON_DEVICE_PART_WORDS: usize = 1_500;

/// Writes the summary with Apple Intelligence, on this Mac. A long note is
/// summarized part by part; the parts' summaries are then summarized again,
/// and all the parts' action points are kept.
async fn summarize_on_device(text: String, token_limit: i32) -> Result<NoteSummary> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() <= ON_DEVICE_PART_WORDS {
        return summarize_part_on_device(text, token_limit).await;
    }
    let parts: Vec<String> = words
        .chunks(ON_DEVICE_PART_WORDS)
        .map(|part| part.join(" "))
        .collect();
    debug!(
        "Summarizing a long note on this Mac in {} parts",
        parts.len()
    );
    let mut summaries = Vec::with_capacity(parts.len());
    let mut action_points: Vec<ActionPoint> = Vec::new();
    for part in parts {
        let summary = summarize_part_on_device(part, token_limit).await?;
        summaries.push(summary.summary);
        for point in summary.action_points {
            if !action_points.iter().any(|p| p.text == point.text) {
                action_points.push(point);
            }
        }
    }
    let joined = summaries.join("\n\n");
    let summary = if joined.split_whitespace().count() <= ON_DEVICE_PART_WORDS {
        summarize_part_on_device(joined, token_limit).await?.summary
    } else {
        joined
    };
    Ok(NoteSummary {
        summary,
        action_points,
    })
}

async fn summarize_part_on_device(text: String, token_limit: i32) -> Result<NoteSummary> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        if !crate::apple_intelligence::check_apple_intelligence_availability() {
            return Err(anyhow!("Apple Intelligence isn't available on this Mac"));
        }
        let reply = tauri::async_runtime::spawn_blocking(move || {
            crate::apple_intelligence::process_text_plain(SUMMARY_PROMPT, &text)
        })
        .await
        .map_err(|e| anyhow!("{e}"))?
        .map_err(|e| anyhow!("{e}"))?;
        let mut summary = parse_summary_reply(&reply)?;
        // A word limit set for Apple Intelligence in Post Process applies to
        // the summary, not the action points.
        if token_limit > 0 {
            let limit = token_limit as usize;
            let words: Vec<&str> = summary.summary.split_whitespace().collect();
            if words.len() > limit {
                summary.summary = words[..limit].join(" ");
            }
        }
        Ok(summary)
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        let _ = (text, token_limit);
        Err(anyhow!("Apple Intelligence isn't available here"))
    }
}

async fn summarize(settings: &AppSettings, text: &str) -> Result<NoteSummary> {
    let text: String = text.chars().take(MAX_SUMMARY_INPUT).collect();
    let Some((provider, model)) = configured_service(settings) else {
        // Nothing set up: Apple Intelligence on the Mac needs no key.
        if apple_intelligence_ready() {
            debug!("Summarizing a note with Apple Intelligence (no service set up)");
            return summarize_on_device(text, 0).await;
        }
        return Err(anyhow!("no_ai_provider"));
    };
    let api_key = settings
        .post_process_api_keys
        .get(&provider.id)
        .cloned()
        .unwrap_or_default();
    let disable_reasoning = matches!(provider.id.as_str(), "custom" | "openrouter");
    debug!("Summarizing a note with provider '{}'", provider.id);

    if provider.id == APPLE_INTELLIGENCE_PROVIDER_ID {
        let token_limit = model.trim().parse::<i32>().unwrap_or(0);
        return summarize_on_device(text, token_limit).await;
    }

    let reply = if provider.supports_structured_output {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "summary": { "type": "string" },
                "action_points": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["summary", "action_points"],
            "additionalProperties": false
        });
        crate::llm_client::send_chat_completion_with_schema(
            &provider,
            api_key,
            &model,
            text,
            Some(SUMMARY_PROMPT.to_string()),
            Some(schema),
            disable_reasoning,
        )
        .await
    } else {
        crate::llm_client::send_chat_completion(
            &provider,
            api_key,
            &model,
            format!("{SUMMARY_PROMPT}\n\nText:\n{text}"),
            disable_reasoning,
        )
        .await
    }
    .map_err(|e| anyhow!("{e}"))?
    .ok_or_else(|| anyhow!("The AI sent back nothing"))?;
    parse_summary_reply(&reply)
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
    let mut body = reply.trim();
    if let Some(end) = body.find("</think>") {
        body = body[end + "</think>".len()..].trim();
    }
    let json = match (body.find('{'), body.rfind('}')) {
        (Some(start), Some(end)) if end > start => &body[start..=end],
        _ => body,
    };
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
