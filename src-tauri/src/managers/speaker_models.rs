//! The two small models that tell voices apart in a meeting, downloaded once
//! together (see `crate::speakers`).

use crate::managers::model::{DownloadProgress, ModelManager};
use crate::speakers::SeparatorModels;
use anyhow::{anyhow, Result};
use hf_hub::api::tokio::CancellationToken;
use log::info;
use serde::Serialize;
use specta::Type;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Emitter};

struct File {
    id: &'static str,
    url: &'static str,
    name: &'static str,
    size: u64,
    sha256: &'static str,
}

/// Sortformer hears who is talking when (NVIDIA Open Model License);
/// TitaNet tells whether two voices are the same person (CC-BY-4.0). Both
/// pinned, so the checksums always match.
const FILES: &[File] = &[
    File {
        id: "sortformer",
        url: "https://huggingface.co/handy-computer/diar_streaming_sortformer_4spk-v2.1-gguf/resolve/60ebab3ef7cae9869be0abcde13360347e96ba03/diar_streaming_sortformer_4spk-v2.1-Q8_0.gguf",
        name: "diar_streaming_sortformer_4spk-v2.1-Q8_0.gguf",
        size: 139_310_336,
        sha256: "a5dacdc650790266c7a362e54e6bf51952015487edaa606c4e11632bc32442a9",
    },
    File {
        id: "titanet",
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/nemo_en_titanet_small.onnx",
        name: "nemo_en_titanet_small.onnx",
        size: 40_257_283,
        sha256: "ad4a1802485d8b34c722d2a9d04249662f2ece5d28a7a039063ca22f515a789e",
    },
];

/// The voice models as the app shows them.
#[derive(Clone, Debug, Serialize, Type)]
pub struct SpeakerModels {
    /// This system can tell voices apart at all.
    pub available: bool,
    pub downloaded: bool,
    pub downloading: bool,
    pub size_bytes: u64,
}

/// Sent as "speaker-models-progress" while they download.
#[derive(Clone, Debug, Serialize, Type)]
pub struct SpeakerModelsProgress {
    pub downloaded: u64,
    pub total: u64,
}

pub struct SpeakerModelManager {
    app: AppHandle,
    dir: PathBuf,
    download: Mutex<Option<CancellationToken>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

fn total_size() -> u64 {
    FILES.iter().map(|f| f.size).sum()
}

impl SpeakerModelManager {
    pub fn new(app: &AppHandle) -> Result<Self> {
        let dir = crate::portable::app_data_dir(app)
            .map_err(|e| anyhow!("Failed to get app data dir: {e}"))?
            .join("models")
            .join("speakers");
        fs::create_dir_all(&dir)?;
        Ok(Self {
            app: app.clone(),
            dir,
            download: Mutex::new(None),
        })
    }

    fn path(&self, file: &File) -> PathBuf {
        self.dir.join(file.name)
    }

    /// The models, once both are downloaded.
    pub fn ready(&self) -> Option<SeparatorModels> {
        if !crate::speakers::AVAILABLE || !FILES.iter().all(|f| self.path(f).is_file()) {
            return None;
        }
        Some(SeparatorModels {
            sortformer: self.path(&FILES[0]),
            titanet: self.path(&FILES[1]),
        })
    }

    pub fn status(&self) -> SpeakerModels {
        SpeakerModels {
            available: crate::speakers::AVAILABLE,
            downloaded: self.ready().is_some(),
            downloading: lock(&self.download).is_some(),
            size_bytes: total_size(),
        }
    }

    fn changed(&self) {
        let _ = self.app.emit("speaker-models-changed", ());
    }

    /// Downloads both, resuming a partial one. Returns whether they're ready.
    pub async fn download(&self) -> Result<bool> {
        if !crate::speakers::AVAILABLE {
            return Err(anyhow!(
                "Telling voices apart isn't available on this system yet"
            ));
        }
        let token = {
            let mut download = lock(&self.download);
            if download.is_some() {
                return Ok(false);
            }
            let token = CancellationToken::new();
            *download = Some(token.clone());
            token
        };
        self.changed();
        let result = self.download_files(&token).await;
        *lock(&self.download) = None;
        self.changed();
        let ready = result?;
        if ready {
            info!("Voice models downloaded");
        }
        Ok(ready)
    }

    async fn download_files(&self, token: &CancellationToken) -> Result<bool> {
        let total = total_size();
        let mut before = 0;
        for file in FILES {
            let path = self.path(file);
            if !path.is_file() {
                let app = self.app.clone();
                let on_progress = move |p: &DownloadProgress| {
                    let _ = app.emit(
                        "speaker-models-progress",
                        SpeakerModelsProgress {
                            downloaded: before + p.downloaded,
                            total,
                        },
                    );
                };
                let partial = self.dir.join(format!("{}.partial", file.name));
                let finished = ModelManager::download_file_resumable(
                    file.id,
                    file.url,
                    &partial,
                    file.size,
                    file.sha256,
                    token,
                    &on_progress,
                )
                .await?;
                if !finished {
                    return Ok(false);
                }
                fs::rename(&partial, &path)?;
            }
            before += file.size;
        }
        Ok(true)
    }

    pub fn cancel_download(&self) {
        if let Some(token) = lock(&self.download).as_ref() {
            token.cancel();
        }
    }
}
