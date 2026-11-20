//! The free summary models: small AI models, downloaded like the speech
//! models, that write a note's summary and action points on this computer.
//!
//! They run in `bolke-llama-server` (llama.cpp's `llama-server`), a helper
//! program shipped beside Bolke (see `scripts/build-llama-server.sh`). It runs as its own process
//! because Bolke's speech engine links its own copy of ggml, and a second copy
//! in the same program would clash. Bolke starts it when a summary is asked
//! for, talks to it over HTTP on 127.0.0.1, and stops it after a few idle
//! minutes so its memory comes back.

use crate::managers::model::{DownloadProgress, ModelManager};
use crate::settings::{get_settings, write_settings};
use anyhow::{anyhow, Result};
use hf_hub::api::tokio::CancellationToken;
use log::{debug, info, warn};
use serde::Serialize;
use specta::Type;
use std::collections::HashMap;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// One downloadable summary model.
struct Catalog {
    id: &'static str,
    name: &'static str,
    url: &'static str,
    file: &'static str,
    size: u64,
    sha256: &'static str,
    recommended: bool,
    /// Qwen3's small models think out loud first unless told not to.
    no_thinking: bool,
}

/// Both Apache-2.0, pinned to one revision so the checksum always matches.
/// The smaller one is recommended: half the download, and fast enough on
/// older computers. The 4B writes better summaries for those who want it.
const CATALOG: &[Catalog] = &[
    Catalog {
        id: "qwen3-1.7b",
        name: "Qwen3 1.7B",
        url: "https://huggingface.co/unsloth/Qwen3-1.7B-GGUF/resolve/d7f544eead698dbd1f15126ef60b45a1e1933222/Qwen3-1.7B-Q4_K_M.gguf",
        file: "Qwen3-1.7B-Q4_K_M.gguf",
        size: 1_107_409_472,
        sha256: "b139949c5bd74937ad8ed8c8cf3d9ffb1e99c866c823204dc42c0d91fa181897",
        recommended: true,
        no_thinking: true,
    },
    Catalog {
        id: "qwen3-4b-instruct-2507",
        name: "Qwen3 4B",
        url: "https://huggingface.co/unsloth/Qwen3-4B-Instruct-2507-GGUF/resolve/a06e946bb6b655725eafa393f4a9745d460374c9/Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
        file: "Qwen3-4B-Instruct-2507-Q4_K_M.gguf",
        size: 2_497_281_120,
        sha256: "3605803b982cb64aead44f6c1b2ae36e3acdb41d8e46c8a94c6533bc4c67e597",
        recommended: false,
        no_thinking: false,
    },
];

/// The helper reads this many tokens at once. A note longer than that is
/// summarized in parts (see `commands::notes`), so 8K keeps memory modest:
/// about 1.2 GB beside the 4B model's 2.5 GB.
const CONTEXT_TOKENS: u32 = 8192;
/// The helper stops after this long without a summary or answer. Long
/// enough that follow-up questions in Ask don't reload the model.
const IDLE_STOP: Duration = Duration::from_secs(600);
/// Loading a model from a slow disk can take a while.
const START_TIMEOUT: Duration = Duration::from_secs(180);

/// A summary model as the app shows it.
#[derive(Clone, Debug, Serialize, Type)]
pub struct SummaryModel {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
    pub recommended: bool,
    pub downloaded: bool,
    pub downloading: bool,
    /// Waiting for the speech model to finish downloading first (setup).
    pub queued: bool,
    /// Bytes of an unfinished download already on disk.
    pub partial_bytes: u64,
    /// The one Bolke uses (the chosen one, else the first downloaded).
    pub active: bool,
}

/// Sent as "summary-model-progress" while a model downloads.
#[derive(Clone, Debug, Serialize, Type)]
pub struct SummaryModelProgress {
    pub id: String,
    pub downloaded: u64,
    pub total: u64,
}

/// The model the helper uses for a summary.
#[derive(Clone, Debug)]
pub struct ActiveSummaryModel {
    pub id: String,
    pub path: PathBuf,
    pub no_thinking: bool,
}

struct Helper {
    child: Child,
    port: u16,
    model_id: String,
}

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub struct SummaryModelManager {
    app: AppHandle,
    dir: PathBuf,
    downloads: Mutex<HashMap<String, CancellationToken>>,
    helper: Mutex<Option<Helper>>,
    /// Serializes starting the helper and the summaries it writes.
    busy: tokio::sync::Mutex<()>,
    /// Bumped on every use; an idle timer stops the helper only if nothing
    /// used it since the timer was set.
    uses: AtomicU64,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl SummaryModelManager {
    pub fn new(app: &AppHandle) -> Result<Self> {
        let dir = crate::portable::app_data_dir(app)
            .map_err(|e| anyhow!("Failed to get app data dir: {e}"))?
            .join("models")
            .join("summary");
        fs::create_dir_all(&dir)?;
        let manager = Self {
            app: app.clone(),
            dir,
            downloads: Mutex::new(HashMap::new()),
            helper: Mutex::new(None),
            busy: tokio::sync::Mutex::new(()),
            uses: AtomicU64::new(0),
        };
        manager.stop_leftover_helper();
        Ok(manager)
    }

    fn path(&self, entry: &Catalog) -> PathBuf {
        self.dir.join(entry.file)
    }

    fn partial_path(&self, entry: &Catalog) -> PathBuf {
        self.dir.join(format!("{}.partial", entry.file))
    }

    fn entry(id: &str) -> Result<&'static Catalog> {
        CATALOG
            .iter()
            .find(|e| e.id == id)
            .ok_or_else(|| anyhow!("Unknown summary model: {id}"))
    }

    fn is_downloaded(&self, entry: &Catalog) -> bool {
        self.path(entry).is_file()
    }

    /// The model in use: the one chosen in settings if it's downloaded,
    /// else the first downloaded one, recommended first.
    pub fn active(&self) -> Option<ActiveSummaryModel> {
        let chosen = get_settings(&self.app).summary_model;
        let entry = chosen
            .as_deref()
            .and_then(|id| CATALOG.iter().find(|e| e.id == id))
            .filter(|e| self.is_downloaded(e))
            .or_else(|| CATALOG.iter().find(|e| self.is_downloaded(e)))?;
        Some(ActiveSummaryModel {
            id: entry.id.to_string(),
            path: self.path(entry),
            no_thinking: entry.no_thinking,
        })
    }

    pub fn list(&self) -> Vec<SummaryModel> {
        let active = self.active().map(|a| a.id);
        let queued = get_settings(&self.app).summary_model_queued;
        let downloads = lock(&self.downloads);
        CATALOG
            .iter()
            .map(|e| SummaryModel {
                id: e.id.to_string(),
                name: e.name.to_string(),
                size_bytes: e.size,
                recommended: e.recommended,
                downloaded: self.is_downloaded(e),
                downloading: downloads.contains_key(e.id),
                queued: queued.as_deref() == Some(e.id) && !downloads.contains_key(e.id),
                partial_bytes: fs::metadata(self.partial_path(e))
                    .map(|m| m.len())
                    .unwrap_or(0),
                active: active.as_deref() == Some(e.id),
            })
            .collect()
    }

    fn changed(&self) {
        let _ = self.app.emit("summary-models-changed", ());
    }

    /// Downloads a model, resuming a partial one. Returns once it's done,
    /// failed or was cancelled; progress goes out as "summary-model-progress".
    pub async fn download(&self, id: &str) -> Result<()> {
        let entry = Self::entry(id)?;
        if self.is_downloaded(entry) {
            return Ok(());
        }
        let token = CancellationToken::new();
        {
            let mut downloads = lock(&self.downloads);
            if downloads.contains_key(id) {
                return Ok(());
            }
            downloads.insert(id.to_string(), token.clone());
        }
        self.changed();
        let app = self.app.clone();
        let id_owned = id.to_string();
        let on_progress = move |p: &DownloadProgress| {
            let _ = app.emit(
                "summary-model-progress",
                SummaryModelProgress {
                    id: id_owned.clone(),
                    downloaded: p.downloaded,
                    total: p.total,
                },
            );
        };
        let partial = self.partial_path(entry);
        let result = ModelManager::download_file_resumable(
            entry.id,
            entry.url,
            &partial,
            entry.size,
            entry.sha256,
            &token,
            &on_progress,
        )
        .await;
        lock(&self.downloads).remove(id);
        let finished = match result {
            Ok(true) => fs::rename(&partial, self.path(entry)).map(|_| true),
            Ok(false) => Ok(false),
            Err(e) => {
                self.changed();
                return Err(e);
            }
        };
        self.changed();
        if finished? {
            info!("Summary model {id} downloaded");
            // The first model downloaded becomes the one in use.
            let mut settings = get_settings(&self.app);
            if settings.summary_model.is_none() {
                settings.summary_model = Some(id.to_string());
            }
            if settings.summary_model_queued.as_deref() == Some(id) {
                settings.summary_model_queued = None;
            }
            write_settings(&self.app, settings);
        } else {
            info!("Summary model {id} download cancelled");
        }
        Ok(())
    }

    pub fn cancel_download(&self, id: &str) {
        self.unqueue(id);
        if let Some(token) = lock(&self.downloads).get(id) {
            token.cancel();
        }
    }

    /// Downloads `id` once no speech model is downloading, so the speech
    /// model picked in setup comes first. Remembered across a quit until it's
    /// downloaded or cancelled.
    pub fn queue_download(self: &Arc<Self>, id: &str) -> Result<()> {
        let entry = Self::entry(id)?;
        if self.is_downloaded(entry) {
            return Ok(());
        }
        let mut settings = get_settings(&self.app);
        settings.summary_model_queued = Some(id.to_string());
        write_settings(&self.app, settings);
        self.changed();
        self.start_queued();
        Ok(())
    }

    /// Starts the queued download, if any, once speech downloads are done.
    pub fn start_queued(self: &Arc<Self>) {
        let Some(id) = get_settings(&self.app).summary_model_queued else {
            return;
        };
        let this = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            // Give a speech download started just before a moment to show up.
            tokio::time::sleep(Duration::from_secs(3)).await;
            while this
                .app
                .try_state::<Arc<ModelManager>>()
                .is_some_and(|m| m.is_downloading_any())
            {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            // Cancelled or replaced while waiting.
            if get_settings(&this.app).summary_model_queued.as_deref() != Some(id.as_str()) {
                return;
            }
            if let Err(e) = this.download(&id).await {
                // Stays queued: the next launch tries again.
                warn!("Queued summary model download failed: {e}");
            }
        });
    }

    fn unqueue(&self, id: &str) {
        let mut settings = get_settings(&self.app);
        if settings.summary_model_queued.as_deref() == Some(id) {
            settings.summary_model_queued = None;
            write_settings(&self.app, settings);
            self.changed();
        }
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let entry = Self::entry(id)?;
        self.cancel_download(id);
        {
            let mut helper = lock(&self.helper);
            if helper.as_ref().is_some_and(|h| h.model_id == id) {
                *helper = None;
            }
        }
        for path in [self.path(entry), self.partial_path(entry)] {
            if path.exists() {
                fs::remove_file(&path)?;
            }
        }
        let mut settings = get_settings(&self.app);
        if settings.summary_model.as_deref() == Some(id) {
            settings.summary_model = None;
            write_settings(&self.app, settings);
        }
        self.changed();
        Ok(())
    }

    pub fn select(&self, id: &str) -> Result<()> {
        let entry = Self::entry(id)?;
        if !self.is_downloaded(entry) {
            return Err(anyhow!("Download {} first", entry.name));
        }
        let mut settings = get_settings(&self.app);
        settings.summary_model = Some(id.to_string());
        write_settings(&self.app, settings);
        self.changed();
        Ok(())
    }

    /// Runs `work` with the helper's address (`http://127.0.0.1:<port>/v1`),
    /// starting the helper with the active model if it isn't running. One
    /// summary at a time; the helper stops after a few idle minutes.
    pub async fn with_helper<T, F, Fut>(self: &Arc<Self>, work: F) -> Result<T>
    where
        F: FnOnce(String, ActiveSummaryModel) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let model = self
            .active()
            .ok_or_else(|| anyhow!("No summary model is downloaded"))?;
        let _busy = self.busy.lock().await;
        self.uses.fetch_add(1, Ordering::SeqCst);
        let port = self.ensure_helper(&model).await?;
        let result = work(format!("http://127.0.0.1:{port}/v1"), model).await;
        self.stop_when_idle();
        result
    }

    async fn ensure_helper(&self, model: &ActiveSummaryModel) -> Result<u16> {
        {
            let mut helper = lock(&self.helper);
            if let Some(h) = helper.as_mut() {
                let alive = matches!(h.child.try_wait(), Ok(None));
                if alive && h.model_id == model.id {
                    return Ok(h.port);
                }
                *helper = None;
            }
        }
        let program = helper_path()?;
        let port = free_port()?;
        let log_path = crate::portable::app_log_dir(&self.app)
            .map(|d| d.join("summary-helper.log"))
            .ok();
        let log = log_path
            .as_ref()
            .and_then(|p| fs::File::create(p).ok())
            .map(Stdio::from)
            .unwrap_or_else(Stdio::null);
        let log_err = log_path
            .as_ref()
            .and_then(|p| fs::OpenOptions::new().append(true).open(p).ok())
            .map(Stdio::from)
            .unwrap_or_else(Stdio::null);

        let mut command = Command::new(&program);
        command
            .arg("--model")
            .arg(&model.path)
            .args(["--host", "127.0.0.1", "--port", &port.to_string()])
            .args(["--ctx-size", &CONTEXT_TOKENS.to_string()])
            .args(["--parallel", "1", "--gpu-layers", "99", "--no-ui"])
            .stdin(Stdio::null())
            .stdout(log)
            .stderr(log_err);
        if model.no_thinking {
            command.args(["--chat-template-kwargs", r#"{"enable_thinking":false}"#]);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        info!(
            "Starting the summary helper with {} on port {port}",
            model.id
        );
        let child = command
            .spawn()
            .map_err(|e| anyhow!("Couldn't start the summary helper: {e}"))?;
        self.write_pid(child.id());
        *lock(&self.helper) = Some(Helper {
            child,
            port,
            model_id: model.id.clone(),
        });

        // Ready when /health answers 200; 503 means still loading.
        let client = reqwest::Client::new();
        let health = format!("http://127.0.0.1:{port}/health");
        let started = std::time::Instant::now();
        loop {
            {
                let mut helper = lock(&self.helper);
                let Some(h) = helper.as_mut() else {
                    return Err(anyhow!("The summary helper was stopped"));
                };
                if let Ok(Some(status)) = h.child.try_wait() {
                    *helper = None;
                    return Err(anyhow!(
                        "The summary helper stopped while loading the model ({status}). \
                         There may not be enough memory."
                    ));
                }
            }
            if let Ok(response) = client
                .get(&health)
                .timeout(Duration::from_secs(2))
                .send()
                .await
            {
                if response.status().is_success() {
                    debug!("Summary helper ready after {:?}", started.elapsed());
                    return Ok(port);
                }
            }
            if started.elapsed() > START_TIMEOUT {
                *lock(&self.helper) = None;
                return Err(anyhow!("The summary helper took too long to start"));
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }

    fn stop_when_idle(self: &Arc<Self>) {
        let uses = self.uses.load(Ordering::SeqCst);
        let me = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(IDLE_STOP).await;
            if me.uses.load(Ordering::SeqCst) == uses {
                if let Ok(_busy) = me.busy.try_lock() {
                    if lock(&me.helper).take().is_some() {
                        info!("Stopped the idle summary helper");
                    }
                    me.clear_pid();
                }
            }
        });
    }

    /// Stops the helper now (on quit).
    pub fn stop(&self) {
        lock(&self.helper).take();
        self.clear_pid();
    }

    fn pid_path(&self) -> PathBuf {
        self.dir.join("helper.pid")
    }

    fn write_pid(&self, pid: u32) {
        let _ = fs::write(self.pid_path(), pid.to_string());
    }

    fn clear_pid(&self) {
        let _ = fs::remove_file(self.pid_path());
    }

    /// A helper left running by a crash would hold gigabytes of memory.
    fn stop_leftover_helper(&self) {
        let Ok(text) = fs::read_to_string(self.pid_path()) else {
            return;
        };
        self.clear_pid();
        let Ok(pid) = text.trim().parse::<u32>() else {
            return;
        };
        if is_helper_process(pid) {
            warn!("Stopping a summary helper left running (pid {pid})");
            kill_process(pid);
        }
    }
}

/// Where the helper program is: beside Bolke in an installed app, or in
/// src-tauri/binaries in a development build.
fn helper_path() -> Result<PathBuf> {
    let name = if cfg!(windows) {
        "bolke-llama-server.exe"
    } else {
        "bolke-llama-server"
    };
    let exe = std::env::current_exe()?;
    if let Some(dir) = exe.parent() {
        let beside = dir.join(name);
        if beside.is_file() {
            return Ok(beside);
        }
    }
    if cfg!(debug_assertions) {
        let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries");
        if let Ok(entries) = fs::read_dir(&dev) {
            for entry in entries.flatten() {
                let file = entry.file_name();
                if file.to_string_lossy().starts_with("bolke-llama-server-") {
                    return Ok(entry.path());
                }
            }
        }
    }
    Err(anyhow!(
        "The summary helper isn't installed with this copy of Bolke"
    ))
}

fn free_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

#[cfg(unix)]
fn is_helper_process(pid: u32) -> bool {
    Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("llama-server"))
        .unwrap_or(false)
}

#[cfg(windows)]
fn is_helper_process(pid: u32) -> bool {
    use std::os::windows::process::CommandExt;
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("llama-server"))
        .unwrap_or(false)
}

#[cfg(unix)]
fn kill_process(pid: u32) {
    let _ = Command::new("kill").arg(pid.to_string()).status();
}

#[cfg(windows)]
fn kill_process(pid: u32) {
    use std::os::windows::process::CommandExt;
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .creation_flags(0x0800_0000)
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique_and_one_is_recommended() {
        let mut ids: Vec<_> = CATALOG.iter().map(|e| e.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len());
        assert_eq!(CATALOG.iter().filter(|e| e.recommended).count(), 1);
        for e in CATALOG {
            assert_eq!(e.sha256.len(), 64);
            assert!(e.url.ends_with(e.file));
        }
    }
}
