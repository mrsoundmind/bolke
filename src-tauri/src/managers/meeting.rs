//! Records a meeting (or any long note) straight to a WAV file in the
//! recordings folder, so an hour of audio never sits in memory.
//!
//! The file's header is rewritten every few seconds, so a crash keeps what
//! was recorded up to then. A small `.meeting.json` file beside it says what
//! the recording was for; it is removed when the recording is saved or
//! discarded, so one found at startup is a recording to rescue.

use anyhow::{anyhow, Result};
use hound::{WavSpec, WavWriter};
use log::{error, info, warn};
use recorder::{AudioChunk, Recorder, RecorderConfig, Sink};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

use crate::call_audio::{self, CallCapture};
use crate::managers::history::HistoryManager;
use crate::settings::get_settings;

const SAMPLE_RATE: u32 = 16_000;
/// Samples between header rewrites: what a crash can lose.
const FLUSH_EVERY: u64 = SAMPLE_RATE as u64 * 5;
/// Samples per level sent to the recording screen (100 ms).
const LEVEL_EVERY: usize = 1_600;
const PENDING_SUFFIX: &str = ".meeting.json";

/// What a recording is for, kept beside it until it's saved.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingMeeting {
    /// The WAV's name in the recordings folder.
    pub file_name: String,
    pub title: String,
    pub project_id: Option<i64>,
    /// The call's sound, recorded beside it until it's mixed in.
    #[serde(default)]
    pub call_file: Option<String>,
}

/// The recording under way, for the app's screens.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Type)]
pub struct MeetingStatus {
    pub recording: bool,
    pub title: String,
    pub project_id: Option<i64>,
    pub elapsed_ms: u64,
    /// The call's sound is being recorded too.
    pub call: bool,
}

/// A finished recording, ready to become a note.
pub struct SavedMeeting {
    pub pending: PendingMeeting,
    pub duration_ms: i64,
}

struct WavSink {
    writer: Option<WavWriter<BufWriter<File>>>,
    /// Samples written, shared with the status.
    written: Arc<AtomicU64>,
    unflushed: u64,
    app: AppHandle,
    level_sum: f32,
    level_count: usize,
}

impl WavSink {
    fn write(&mut self, samples: &[f32]) -> Result<(), hound::Error> {
        let Some(writer) = self.writer.as_mut() else {
            return Ok(());
        };
        for &sample in samples {
            writer.write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
        }
        self.written
            .fetch_add(samples.len() as u64, Ordering::Relaxed);
        self.unflushed += samples.len() as u64;
        if self.unflushed >= FLUSH_EVERY {
            self.unflushed = 0;
            writer.flush()?;
        }
        Ok(())
    }

    fn send_level(&mut self, samples: &[f32]) {
        self.level_sum += samples.iter().map(|s| s * s).sum::<f32>();
        self.level_count += samples.len();
        if self.level_count >= LEVEL_EVERY {
            let rms = (self.level_sum / self.level_count as f32).sqrt();
            let _ = self
                .app
                .emit_to("main", "meeting-level", (rms * 5.0).min(1.0));
            self.level_sum = 0.0;
            self.level_count = 0;
        }
    }
}

impl Sink for WavSink {
    fn process_chunk(&mut self, chunk: AudioChunk<'_>) {
        let real = chunk.valid_frames * chunk.channels as usize;
        let samples = &chunk.samples[..real];
        self.send_level(samples);
        if let Err(e) = self.write(samples) {
            // Stop writing; what's on disk so far is kept.
            error!("Failed to write the meeting recording: {e}");
            self.writer = None;
        }
    }
}

struct Active {
    recorder: Recorder<WavSink>,
    pending: PendingMeeting,
    written: Arc<AtomicU64>,
    started: Instant,
    call: Option<CallCapture>,
}

pub struct MeetingRecorder {
    app: AppHandle,
    active: Mutex<Option<Active>>,
}

impl MeetingRecorder {
    pub fn new(app: &AppHandle) -> Self {
        Self {
            app: app.clone(),
            active: Mutex::new(None),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Option<Active>> {
        self.active
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn recordings_dir(&self) -> PathBuf {
        self.app
            .state::<Arc<HistoryManager>>()
            .recordings_dir()
            .to_path_buf()
    }

    pub fn status(&self) -> MeetingStatus {
        match self.lock().as_ref() {
            Some(active) => MeetingStatus {
                recording: true,
                title: active.pending.title.clone(),
                project_id: active.pending.project_id,
                elapsed_ms: active.started.elapsed().as_millis() as u64,
                call: active.call.as_ref().is_some_and(CallCapture::is_on),
            },
            None => MeetingStatus::default(),
        }
    }

    /// Opens the microphone chosen in Settings and starts writing. With
    /// `call`, the computer's sound (the others on a video call) is recorded
    /// too, when this computer can.
    pub fn start(
        &self,
        title: String,
        project_id: Option<i64>,
        call: bool,
    ) -> Result<MeetingStatus> {
        let mut active = self.lock();
        if active.is_some() {
            return Err(anyhow!("A recording is already under way"));
        }

        let dir = self.recordings_dir();
        std::fs::create_dir_all(&dir)?;
        let file_name = format!(
            "bolke-meeting-{}.wav",
            chrono::Utc::now().timestamp_millis()
        );
        let path = dir.join(&file_name);
        let call_file = call.then(|| file_name.replace(".wav", ".call.wav"));
        let mut pending = PendingMeeting {
            file_name,
            title,
            project_id,
            call_file,
        };
        write_pending(&dir, &pending)?;

        let writer = WavWriter::create(
            &path,
            WavSpec {
                channels: 1,
                sample_rate: SAMPLE_RATE,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        );
        let started = writer.map_err(anyhow::Error::from).and_then(|writer| {
            let written = Arc::new(AtomicU64::new(0));
            let recorder = Recorder::open_with_failure_handler(
                RecorderConfig {
                    device: self.selected_device(),
                    ..RecorderConfig::speech()
                },
                |e| error!("The meeting microphone failed: {e}"),
            )?;
            recorder
                .start(WavSink {
                    writer: Some(writer),
                    written: Arc::clone(&written),
                    unflushed: 0,
                    app: self.app.clone(),
                    level_sum: 0.0,
                    level_count: 0,
                })
                .map_err(recorder::Error::from)?;
            Ok((recorder, written))
        });
        let (recorder, written) = match started {
            Ok(started) => started,
            Err(e) => {
                remove_quietly(&path);
                remove_quietly(&pending_path(&dir, &pending.file_name));
                return Err(e);
            }
        };

        let started = Instant::now();
        let call = match pending.call_file.as_ref() {
            Some(call_file) => match CallCapture::start(&dir.join(call_file)) {
                Ok(capture) => Some(capture),
                Err(e) => {
                    // The meeting goes on with the microphone alone.
                    warn!("Couldn't record the call's sound: {e}");
                    pending.call_file = None;
                    write_pending(&dir, &pending)?;
                    None
                }
            },
            None => None,
        };
        crate::call_names::begin(&path, chrono::Utc::now().timestamp_millis());
        if call.is_some() {
            crate::call_screen::watch();
        }
        info!("Meeting recording started: {}", pending.file_name);
        *active = Some(Active {
            recorder,
            pending,
            written,
            started,
            call,
        });
        drop(active);
        crate::tray::update_tray_menu(&self.app);
        Ok(self.status())
    }

    /// Turns the call's sound on or off while recording.
    pub fn set_call(&self, on: bool) {
        if let Some(capture) = self.lock().as_ref().and_then(|a| a.call.as_ref()) {
            capture.set_on(on);
        }
    }

    /// Changes the title or project while recording.
    pub fn update(&self, title: String, project_id: Option<i64>) -> Result<()> {
        let mut active = self.lock();
        let Some(active) = active.as_mut() else {
            return Ok(());
        };
        active.pending.title = title;
        active.pending.project_id = project_id;
        write_pending(&self.recordings_dir(), &active.pending)
    }

    /// Ends the recording. `None` if nothing was recording. The caller turns
    /// it into a note, then calls `forget`.
    pub fn stop(&self) -> Result<Option<SavedMeeting>> {
        let Some(active) = self.lock().take() else {
            return Ok(None);
        };
        crate::tray::update_tray_menu(&self.app);
        crate::call_names::end();
        let call = active.call.map(|capture| {
            let delay_ms = capture.started.duration_since(active.started).as_millis() as i64;
            capture.stop();
            delay_ms
        });
        let stopped = active.recorder.stop();
        let written = active.written.load(Ordering::Relaxed);
        match stopped {
            Ok(stopped) => {
                if !stopped.is_complete() {
                    warn!(
                        "Meeting recording ended by {:?}, {} frames dropped",
                        stopped.end_reason, stopped.dropped_frames
                    );
                }
                if let Some(writer) = stopped.sink.writer {
                    writer.finalize()?;
                }
            }
            // The sink is lost, but the file holds everything up to its last
            // header rewrite.
            Err(e) => error!("Failed to stop the meeting recording cleanly: {e}"),
        }
        info!(
            "Meeting recording stopped: {} ({} s)",
            active.pending.file_name,
            written / SAMPLE_RATE as u64
        );
        let dir = self.recordings_dir();
        let wav = dir.join(&active.pending.file_name);
        if let (Some(call_file), Some(delay_ms)) = (active.pending.call_file.as_ref(), call) {
            if let Err(e) = call_audio::mix_into(&wav, &dir.join(call_file), delay_ms) {
                error!("Couldn't mix in the call's sound: {e}");
            }
        }
        Ok(Some(SavedMeeting {
            duration_ms: wav_duration_ms(&wav)
                .unwrap_or((written * 1000 / SAMPLE_RATE as u64) as i64),
            pending: active.pending,
        }))
    }

    /// Ends the recording and deletes it.
    pub fn discard(&self) -> Result<()> {
        if let Some(saved) = self.stop()? {
            let dir = self.recordings_dir();
            remove_quietly(&dir.join(&saved.pending.file_name));
            crate::call_names::remove(&dir.join(&saved.pending.file_name));
            if let Some(call_file) = &saved.pending.call_file {
                remove_quietly(&dir.join(call_file));
            }
            self.forget(&saved.pending);
        }
        Ok(())
    }

    /// The recording became a note; it no longer needs rescuing.
    pub fn forget(&self, pending: &PendingMeeting) {
        remove_quietly(&pending_path(&self.recordings_dir(), &pending.file_name));
    }

    /// Recordings left by a crash or quit. Empty ones are deleted.
    pub fn leftovers(&self) -> Vec<SavedMeeting> {
        let dir = self.recordings_dir();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let is_pending = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(PENDING_SUFFIX));
            if !is_pending {
                continue;
            }
            let pending: Option<PendingMeeting> = std::fs::read_to_string(&path)
                .ok()
                .and_then(|json| serde_json::from_str(&json).ok());
            let Some(pending) = pending else {
                warn!("Removing unreadable {}", path.display());
                remove_quietly(&path);
                continue;
            };
            let wav = dir.join(&pending.file_name);
            if let Some(call_file) = &pending.call_file {
                let call = dir.join(call_file);
                if call.exists() && wav.exists() {
                    if let Err(e) = call_audio::mix_into(&wav, &call, 0) {
                        error!("Couldn't mix in a rescued call's sound: {e}");
                    }
                }
            }
            match wav_duration_ms(&wav) {
                Some(duration_ms) if duration_ms > 0 => found.push(SavedMeeting {
                    pending,
                    duration_ms,
                }),
                _ => {
                    remove_quietly(&wav);
                    remove_quietly(&path);
                }
            }
        }
        found
    }

    fn selected_device(&self) -> Option<String> {
        let name = get_settings(&self.app).selected_microphone?;
        match recorder::list_input_devices() {
            Ok(devices) => devices.into_iter().find(|d| d.name == name).map(|d| d.id),
            Err(e) => {
                warn!("Failed to list microphones, using the default: {e}");
                None
            }
        }
    }
}

fn pending_path(dir: &Path, file_name: &str) -> PathBuf {
    dir.join(format!("{file_name}{PENDING_SUFFIX}"))
}

fn write_pending(dir: &Path, pending: &PendingMeeting) -> Result<()> {
    std::fs::write(
        pending_path(dir, &pending.file_name),
        serde_json::to_string(pending)?,
    )?;
    Ok(())
}

/// The length of a 16 kHz WAV, if it can be read.
pub fn wav_duration_ms(path: &Path) -> Option<i64> {
    let reader = hound::WavReader::open(path).ok()?;
    let spec = reader.spec();
    let frames = reader.duration() as i64;
    Some(frames * 1000 / spec.sample_rate.max(1) as i64)
}

fn remove_quietly(path: &Path) {
    if let Err(e) = std::fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            error!("Failed to remove {}: {}", path.display(), e);
        }
    }
}
