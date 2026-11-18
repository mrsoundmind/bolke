//! The voice separator, run in its own process (see `super`).
//!
//! Sortformer hears up to four voices at a time, so the meeting is heard in
//! two-minute windows. Each voice in a window gets a voice fingerprint
//! (TitaNet) from the audio where it speaks alone, and fingerprints that
//! match across windows are joined into one person. Two voices from the same
//! window are never joined, so a meeting can have any number of people.

use super::{Separation, Voice, VoiceSegment, CPU_ONLY_FLAG};
use anyhow::{anyhow, Context, Result};
use sherpa_onnx::{SpeakerEmbeddingExtractor, SpeakerEmbeddingExtractorConfig};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use transcribe_cpp::{BackendMask, DiarizeOptions, Model};

const RATE: usize = 16_000;
/// Two minutes heard best in tests: long enough for each voice to speak
/// alone, short enough that four voices rarely share one.
const WINDOW: usize = 120 * RATE;
/// Fingerprints at least this alike are one person.
const SAME_PERSON: f32 = 0.5;
/// A voice needs this much audio alone to get a fingerprint.
const MIN_SOLO: usize = RATE * 3 / 2;
/// Fingerprints are taken from pieces this long and averaged.
const PIECE: usize = 6 * RATE;
/// A "person" who talked less than this is a split-off bit of someone else.
const MIN_PERSON_MS: i64 = 8_000;

/// Arguments: `<sortformer.gguf> <titanet.onnx> <recording.wav> <out.json>
/// [--cpu-only]`. Prints `progress <done> <total>` as windows finish.
pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(2).collect();
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let cpu_only = args.iter().any(|a| a == CPU_ONLY_FLAG);
    let [sortformer, titanet, wav, out] = positional[..] else {
        eprintln!("usage: <sortformer> <titanet> <wav> <out> [--cpu-only]");
        return 2;
    };
    match separate(sortformer, titanet, wav, cpu_only).and_then(|separation| {
        std::fs::write(out, serde_json::to_vec(&separation)?).context("write result")
    }) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Voice separation failed: {e:#}");
            1
        }
    }
}

struct Local {
    window: usize,
    segments: Vec<(i64, i64)>,
    fingerprint: Option<Vec<f32>>,
    talk_ms: i64,
}

fn separate(sortformer: &str, titanet: &str, wav: &str, cpu_only: bool) -> Result<Separation> {
    if cpu_only {
        transcribe_cpp::init_backends_with(None::<&Path>, BackendMask::CPU)
    } else {
        transcribe_cpp::init_backends_default()
    }
    .map_err(|e| anyhow!("backends: {e}"))?;
    let pcm = crate::audio_toolkit::read_wav_samples(wav)?;
    let model = Model::load(sortformer).map_err(|e| anyhow!("load Sortformer: {e}"))?;
    let mut session = model
        .diarize_session()
        .map_err(|e| anyhow!("diarize session: {e}"))?;
    let threads = std::thread::available_parallelism()
        .map(|n| n.get().min(4))
        .unwrap_or(2) as i32;
    let extractor = SpeakerEmbeddingExtractor::create(&SpeakerEmbeddingExtractorConfig {
        model: Some(titanet.to_string()),
        num_threads: threads,
        ..Default::default()
    })
    .ok_or_else(|| anyhow!("load TitaNet"))?;

    let windows = pcm.len().div_ceil(WINDOW);
    let mut locals: Vec<Local> = Vec::new();
    for (w, chunk) in pcm.chunks(WINDOW).enumerate() {
        if chunk.len() >= RATE {
            hear_window(&mut session, &extractor, w, chunk, &mut locals)?;
        }
        println!("progress {} {}", w + 1, windows);
        let _ = std::io::stdout().flush();
    }

    let mut people = cluster(&locals);
    absorb_small(&mut people, &locals);

    let mut segments = Vec::new();
    let mut voices = Vec::new();
    for (i, person) in people.iter().enumerate() {
        let voice = i as u32 + 1;
        for &m in &person.members {
            segments.extend(
                locals[m]
                    .segments
                    .iter()
                    .map(|&(start_ms, end_ms)| VoiceSegment {
                        start_ms,
                        end_ms,
                        voice,
                    }),
            );
        }
        voices.push(Voice {
            voice,
            fingerprint: person.fingerprint.clone(),
        });
    }
    segments.sort_by_key(|s| s.start_ms);
    Ok(Separation { segments, voices })
}

fn hear_window(
    session: &mut transcribe_cpp::DiarizeSession,
    extractor: &SpeakerEmbeddingExtractor,
    window: usize,
    chunk: &[f32],
    locals: &mut Vec<Local>,
) -> Result<()> {
    let offset_ms = (window * WINDOW / (RATE / 1000)) as i64;
    let found = session
        .run(chunk, &DiarizeOptions::default())
        .map_err(|e| anyhow!("diarize: {e}"))?;
    // Who is talking in each 10 ms frame.
    let frames = chunk.len() / 160;
    let mut talking: Vec<Vec<i32>> = vec![Vec::new(); frames];
    for s in &found {
        let first = (s.t0_ms / 10).max(0) as usize;
        let last = ((s.t1_ms / 10).max(0) as usize).min(frames);
        for f in talking.iter_mut().take(last).skip(first) {
            f.push(s.speaker_id);
        }
    }
    let ids: BTreeSet<i32> = found.iter().map(|s| s.speaker_id).collect();
    for id in ids {
        let mut solo: Vec<f32> = Vec::new();
        for (f, who) in talking.iter().enumerate() {
            if who.len() == 1 && who[0] == id {
                solo.extend_from_slice(&chunk[f * 160..(f + 1) * 160]);
            }
        }
        let fingerprint = if solo.len() >= MIN_SOLO {
            fingerprint(extractor, &solo)
        } else {
            None
        };
        let segments: Vec<(i64, i64)> = found
            .iter()
            .filter(|s| s.speaker_id == id && s.t1_ms > s.t0_ms)
            .map(|s| (offset_ms + s.t0_ms, offset_ms + s.t1_ms))
            .collect();
        let talk_ms = segments.iter().map(|(a, b)| b - a).sum();
        locals.push(Local {
            window,
            segments,
            fingerprint,
            talk_ms,
        });
    }
    Ok(())
}

fn fingerprint(extractor: &SpeakerEmbeddingExtractor, solo: &[f32]) -> Option<Vec<f32>> {
    let mut sum: Option<Vec<f32>> = None;
    for piece in solo.chunks(PIECE).filter(|p| p.len() >= RATE) {
        let stream = extractor.create_stream()?;
        stream.accept_waveform(RATE as i32, piece);
        stream.input_finished();
        if let Some(mut e) = extractor.compute(&stream) {
            normalize(&mut e);
            match &mut sum {
                None => sum = Some(e),
                Some(s) => s.iter_mut().zip(&e).for_each(|(x, y)| *x += y),
            }
        }
    }
    sum.map(|mut e| {
        normalize(&mut e);
        e
    })
}

/// Test-only override of a tuning value.
fn tuned(name: &str, default: f32) -> f32 {
    std::env::var(format!("BOLKE_VOICES_{name}"))
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

pub(super) fn normalize(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
    v.iter_mut().for_each(|x| *x /= n);
}

pub(super) fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

struct Person {
    members: Vec<usize>,
    fingerprint: Vec<f32>,
    talk_ms: i64,
}

/// Average-linkage joining of fingerprints; voices that shared a window stay
/// apart. Voices with no fingerprint (they hardly spoke alone) are left out.
fn cluster(locals: &[Local]) -> Vec<Person> {
    let mut people: Vec<Person> = locals
        .iter()
        .enumerate()
        .filter_map(|(i, l)| {
            l.fingerprint.clone().map(|fingerprint| Person {
                members: vec![i],
                fingerprint,
                talk_ms: l.talk_ms,
            })
        })
        .collect();
    loop {
        let mut best: Option<(f32, usize, usize)> = None;
        for i in 0..people.len() {
            for j in i + 1..people.len() {
                let shared = people[i].members.iter().any(|&a| {
                    people[j]
                        .members
                        .iter()
                        .any(|&b| locals[a].window == locals[b].window)
                });
                if shared {
                    continue;
                }
                let s = cosine(&people[i].fingerprint, &people[j].fingerprint);
                if best.is_none_or(|(b, _, _)| s > b) {
                    best = Some((s, i, j));
                }
            }
        }
        match best {
            Some((s, i, j)) if s >= tuned("SAME", SAME_PERSON) => merge(&mut people, i, j),
            _ => break,
        }
    }
    people
}

/// Joins person `j` into person `i`.
fn merge(people: &mut Vec<Person>, i: usize, j: usize) {
    let other = people.remove(j);
    let i = if j < i { i - 1 } else { i };
    let a = people[i].members.len() as f32;
    let b = other.members.len() as f32;
    people[i]
        .fingerprint
        .iter_mut()
        .zip(&other.fingerprint)
        .for_each(|(x, y)| *x = (*x * a + y * b) / (a + b));
    normalize(&mut people[i].fingerprint);
    people[i].members.extend(other.members);
    people[i].talk_ms += other.talk_ms;
}

/// A few seconds of "person" is a bit of someone else that came out
/// different: it joins whoever it sounds most like.
fn absorb_small(people: &mut Vec<Person>, _locals: &[Local]) {
    loop {
        if people.len() < 2 {
            return;
        }
        let Some(small) = (0..people.len())
            .filter(|&i| people[i].talk_ms < tuned("MIN_MS", MIN_PERSON_MS as f32) as i64)
            .min_by_key(|&i| people[i].talk_ms)
        else {
            return;
        };
        let nearest = (0..people.len())
            .filter(|&i| i != small)
            .max_by(|&a, &b| {
                cosine(&people[small].fingerprint, &people[a].fingerprint)
                    .total_cmp(&cosine(&people[small].fingerprint, &people[b].fingerprint))
            })
            .expect("at least two people");
        merge(people, nearest, small);
    }
}
