//! A call's sound: what the computer plays (the other people on a video
//! call), recorded beside the microphone and mixed in when the meeting
//! stops, so their voices are in the transcript even through headphones.
//!
//! On Linux it's read from the sound server's monitor of the speakers, with
//! the tools every desktop has: `parec` (PulseAudio, and PipeWire's Pulse
//! layer) or else `pw-record` (PipeWire). Other systems don't have it yet.

use anyhow::{anyhow, Result};
use hound::{WavSpec, WavWriter};
use log::{error, info, warn};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Instant;

const RATE: u32 = 16_000;
/// Samples between header rewrites: what a crash can lose.
const FLUSH_EVERY: usize = RATE as usize * 5;

/// The ways of reading the speakers' sound, tried in order.
fn commands() -> Vec<Command> {
    let mut list = Vec::new();
    if cfg!(target_os = "linux") {
        let mut parec = Command::new("parec");
        parec.args([
            "--device=@DEFAULT_MONITOR@",
            "--format=float32le",
            "--rate=16000",
            "--channels=1",
            "--raw",
            "--latency-msec=100",
        ]);
        list.push(parec);
        let mut pw = Command::new("pw-record");
        pw.args([
            "-P",
            "{ stream.capture.sink = true }",
            "--rate",
            "16000",
            "--channels",
            "1",
            "--format",
            "f32",
            "-",
        ]);
        list.push(pw);
    }
    list
}

fn found(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

/// Whether this computer can record a call's sound.
pub fn available() -> bool {
    commands()
        .iter()
        .any(|c| found(&c.get_program().to_string_lossy()))
}

/// The call's sound being written to a WAV.
pub struct CallCapture {
    child: Child,
    /// Writes silence while set, so the recording stays in time.
    muted: Arc<AtomicBool>,
    writer: JoinHandle<Result<u64>>,
    /// When its first sound arrived, to line it up with the microphone.
    pub started: Instant,
}

impl CallCapture {
    /// Starts writing the speakers' sound to `path`.
    pub fn start(path: &Path) -> Result<Self> {
        let mut last_error = anyhow!("No way to record the call's sound on this computer");
        for command in commands() {
            if !found(&command.get_program().to_string_lossy()) {
                continue;
            }
            match Self::start_with(command, path) {
                Ok(capture) => return Ok(capture),
                Err(e) => last_error = e,
            }
        }
        Err(last_error)
    }

    fn start_with(mut command: Command, path: &Path) -> Result<Self> {
        let program = command.get_program().to_string_lossy().into_owned();
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let started = Instant::now();
        // A tool that can't reach the sound server exits at once.
        std::thread::sleep(std::time::Duration::from_millis(150));
        if let Some(status) = child.try_wait()? {
            return Err(anyhow!("{program} stopped at once ({status})"));
        }
        let path = path.to_path_buf();
        let muted = Arc::new(AtomicBool::new(false));
        let quiet = Arc::clone(&muted);
        let writer = std::thread::Builder::new()
            .name("call-audio".into())
            .spawn(move || write_wav(stdout, &path, &quiet))?;
        info!("Recording the call's sound with {program}");
        Ok(Self {
            child,
            muted,
            writer,
            started,
        })
    }

    /// Turns the call's sound off (silence is written) or back on.
    pub fn set_on(&self, on: bool) {
        self.muted.store(!on, Ordering::Relaxed);
    }

    pub fn is_on(&self) -> bool {
        !self.muted.load(Ordering::Relaxed)
    }

    /// Stops and finishes the file; returns how many samples it holds.
    pub fn stop(mut self) -> u64 {
        let _ = self.child.kill();
        let _ = self.child.wait();
        match self.writer.join() {
            Ok(Ok(samples)) => samples,
            Ok(Err(e)) => {
                error!("Failed to write the call's sound: {e}");
                0
            }
            Err(_) => 0,
        }
    }
}

fn spec() -> WavSpec {
    WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    }
}

/// Writes 32-bit float samples from `input` until it ends.
fn write_wav(mut input: impl Read, path: &Path, muted: &AtomicBool) -> Result<u64> {
    let mut writer = WavWriter::create(path, spec())?;
    let mut buf = [0u8; 4096];
    let mut carry: Vec<u8> = Vec::new();
    let mut written = 0u64;
    let mut unflushed = 0usize;
    loop {
        let n = match input.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        carry.extend_from_slice(&buf[..n]);
        let whole = carry.len() / 4 * 4;
        let quiet = muted.load(Ordering::Relaxed);
        for b in carry[..whole].chunks_exact(4) {
            let sample = if quiet {
                0.0
            } else {
                f32::from_le_bytes([b[0], b[1], b[2], b[3]])
            };
            writer.write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
        }
        written += (whole / 4) as u64;
        unflushed += whole / 4;
        carry.drain(..whole);
        if unflushed >= FLUSH_EVERY {
            unflushed = 0;
            writer.flush()?;
        }
    }
    writer.finalize()?;
    Ok(written)
}

/// Mixes the call's sound into the microphone recording, in place. `delay_ms`
/// is how long after the microphone the call's sound started. The call file
/// is removed once mixed.
pub fn mix_into(mic: &Path, call: &Path, delay_ms: i64) -> Result<()> {
    let read = |p: &Path| -> Result<Vec<i16>> {
        let mut reader = hound::WavReader::open(p)?;
        Ok(reader.samples::<i16>().collect::<Result<Vec<_>, _>>()?)
    };
    let voice = read(mic)?;
    let call_sound = match read(call) {
        Ok(s) => s,
        Err(e) => {
            warn!("The call's sound couldn't be read, keeping the microphone only: {e}");
            return Ok(());
        }
    };
    let offset = (delay_ms.max(0) as usize) * (RATE as usize / 1000);
    let len = voice.len().max(offset + call_sound.len());
    let mixed = PathBuf::from(format!("{}.mixing", mic.display()));
    let mut writer = WavWriter::create(&mixed, spec())?;
    for i in 0..len {
        let a = voice.get(i).copied().unwrap_or(0) as i32;
        let b = i
            .checked_sub(offset)
            .and_then(|j| call_sound.get(j))
            .copied()
            .unwrap_or(0) as i32;
        writer.write_sample((a + b).clamp(i16::MIN as i32, i16::MAX as i32) as i16)?;
    }
    writer.finalize()?;
    std::fs::rename(&mixed, mic)?;
    let _ = std::fs::remove_file(call);
    info!("Mixed the call's sound into {}", mic.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(path: &Path, samples: &[i16]) {
        let mut w = WavWriter::create(path, spec()).unwrap();
        for &s in samples {
            w.write_sample(s).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn writes_float_samples_split_across_reads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("call.wav");
        let bytes: Vec<u8> = [0.5f32, -0.5, 0.25]
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect();
        // A reader that hands out 3 bytes at a time.
        struct Slow(Vec<u8>);
        impl Read for Slow {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                let n = self.0.len().min(3).min(buf.len());
                buf[..n].copy_from_slice(&self.0[..n]);
                self.0.drain(..n);
                Ok(n)
            }
        }
        assert_eq!(
            write_wav(Slow(bytes), &path, &AtomicBool::new(false)).unwrap(),
            3
        );
        let got: Vec<i16> = hound::WavReader::open(&path)
            .unwrap()
            .samples::<i16>()
            .map(|s| s.unwrap())
            .collect();
        assert_eq!(got, vec![16383, -16383, 8191]);
    }

    #[cfg(unix)]
    #[test]
    fn records_what_the_tool_plays_until_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("call.wav");
        // One second of silence, then waiting to be stopped.
        let mut tool = Command::new("sh");
        tool.args(["-c", "head -c 64000 /dev/zero; exec sleep 30"]);
        let capture = CallCapture::start_with(tool, &path).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert_eq!(capture.stop(), 16_000);
        assert_eq!(hound::WavReader::open(&path).unwrap().duration(), 16_000);
    }

    #[test]
    fn mixes_with_delay_and_keeps_the_longer_one() {
        let dir = tempfile::tempdir().unwrap();
        let mic = dir.path().join("m.wav");
        let call = dir.path().join("c.wav");
        wav(&mic, &[100; 32]);
        wav(&call, &[30_000; 32]);
        // 1 ms = 16 samples late.
        mix_into(&mic, &call, 1).unwrap();
        let got: Vec<i16> = hound::WavReader::open(&mic)
            .unwrap()
            .samples::<i16>()
            .map(|s| s.unwrap())
            .collect();
        assert_eq!(got.len(), 48);
        assert_eq!(got[0], 100);
        assert_eq!(got[20], 30_100);
        assert_eq!(got[40], 30_000);
        assert!(!call.exists());
    }
}
