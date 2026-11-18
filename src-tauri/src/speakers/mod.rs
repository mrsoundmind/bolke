//! Who said what in a meeting: the recording is split into voices, then each
//! stretch is transcribed on its own, so every line has its speaker.
//!
//! The voice separator runs in a child process (the Bolke executable started
//! again with [`WORKER_FLAG`]), like the speech engine, so a GPU fault there
//! can't take the app down. A crash is tried again on the CPU alone.

mod turns;
#[cfg(not(target_os = "windows"))]
mod worker;

pub use turns::{turns, Turn, VoiceSegment};

use anyhow::{anyhow, Result};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Hidden first argument that turns the executable into the voice separator.
pub const WORKER_FLAG: &str = "--speakers-worker";
const CPU_ONLY_FLAG: &str = "--cpu-only";

/// Whether this build can tell voices apart. Windows needs the separator's
/// libraries built to match Bolke's runtime first.
pub const AVAILABLE: bool = cfg!(not(target_os = "windows"));

/// One person's voice fingerprint, for knowing them again later.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Voice {
    pub voice: u32,
    pub fingerprint: Vec<f32>,
}

/// What the separator heard.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Separation {
    pub segments: Vec<VoiceSegment>,
    pub voices: Vec<Voice>,
}

pub fn is_worker_invocation() -> bool {
    std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == WORKER_FLAG)
}

/// Runs the separator in this process. Returns the exit code.
pub fn run_worker() -> i32 {
    #[cfg(not(target_os = "windows"))]
    return worker::run();
    #[cfg(target_os = "windows")]
    2
}

/// How alike two voice fingerprints are, from -1 to 1.
pub fn similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (na * nb).max(1e-9)
}

/// The two model files the separator needs.
pub struct SeparatorModels {
    pub sortformer: PathBuf,
    pub titanet: PathBuf,
}

/// Separates the voices in a 16 kHz mono WAV. `progress` gets 0.0 to 1.0;
/// `cancelled` stops it between windows.
pub fn separate(
    models: &SeparatorModels,
    wav: &Path,
    progress: &dyn Fn(f32),
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<Separation>> {
    if !AVAILABLE {
        return Err(anyhow!(
            "Telling voices apart isn't available on this system yet"
        ));
    }
    match run_child(models, wav, false, progress, cancelled) {
        Err(e) => {
            warn!("Voice separation failed ({e}); trying again on the CPU");
            run_child(models, wav, true, progress, cancelled)
        }
        ok => ok,
    }
}

fn run_child(
    models: &SeparatorModels,
    wav: &Path,
    cpu_only: bool,
    progress: &dyn Fn(f32),
    cancelled: &dyn Fn() -> bool,
) -> Result<Option<Separation>> {
    let out = std::env::temp_dir().join(format!(
        "bolke-voices-{}-{}.json",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    ));
    #[cfg(target_os = "linux")]
    let exe = PathBuf::from("/proc/self/exe");
    #[cfg(not(target_os = "linux"))]
    let exe = std::env::current_exe()?;
    let mut command = Command::new(exe);
    command
        .arg(WORKER_FLAG)
        .arg(&models.sortformer)
        .arg(&models.titanet)
        .arg(wav)
        .arg(&out)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if cpu_only {
        command.arg(CPU_ONLY_FLAG);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let started = std::time::Instant::now();
    let mut child = command.spawn()?;
    let stderr = child.stderr.take().expect("stderr is piped");
    let errors = std::thread::spawn(move || {
        let mut last = Vec::new();
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            log::debug!("voice separator: {line}");
            last.push(line);
            if last.len() > 20 {
                last.remove(0);
            }
        }
        last.join("\n")
    });
    let stdout = child.stdout.take().expect("stdout is piped");
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(&out);
            return Ok(None);
        }
        let mut parts = line.split_whitespace();
        if parts.next() == Some("progress") {
            let done: f32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
            let total: f32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(1.0);
            progress((done / total.max(1.0)).min(1.0));
        }
    }
    let status = child.wait()?;
    let errors = errors.join().unwrap_or_default();
    if !status.success() {
        let _ = std::fs::remove_file(&out);
        return Err(anyhow!("the voice separator stopped ({status}): {errors}"));
    }
    let separation: Separation = serde_json::from_slice(&std::fs::read(&out)?)?;
    let _ = std::fs::remove_file(&out);
    info!(
        "Separated {} voices in {:.1}s{}",
        separation.voices.len(),
        started.elapsed().as_secs_f32(),
        if cpu_only { " on the CPU" } else { "" }
    );
    Ok(Some(separation))
}
