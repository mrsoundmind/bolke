//! Turns a meeting's voice segments into speaker turns that cover the whole
//! recording: one speaker at a time, no gaps, people numbered by when they
//! first speak.

use serde::{Deserialize, Serialize};

/// Where one voice is heard, as the voice separator reports it. Segments of
/// different voices can overlap.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct VoiceSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub voice: u32,
}

/// A stretch of the recording given to one person.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Turn {
    pub start_ms: i64,
    pub end_ms: i64,
    /// 1-based, in the order people first speak.
    pub person: u32,
}

/// Shorter than this is a cough or an "mm", given to the people around it.
const MIN_TURN_MS: i64 = 500;
const FRAME_MS: i64 = 10;

/// The turns, and for each person (index `person - 1`) the separator's voice.
pub fn turns(segments: &[VoiceSegment], total_ms: i64) -> (Vec<Turn>, Vec<u32>) {
    let frames = (total_ms.max(0) / FRAME_MS) as usize + 1;
    // Where voices overlap, the one with the longer stretch holds the floor.
    let mut floor: Vec<Option<(i64, u32)>> = vec![None; frames];
    for s in segments {
        let len = s.end_ms - s.start_ms;
        if len <= 0 {
            continue;
        }
        let first = (s.start_ms.max(0) / FRAME_MS) as usize;
        let last = ((s.end_ms / FRAME_MS) as usize).min(frames);
        for slot in floor.iter_mut().take(last).skip(first) {
            if slot.is_none_or(|(held, _)| len > held) {
                *slot = Some((len, s.voice));
            }
        }
    }

    // Runs of one voice.
    let mut runs: Vec<(i64, i64, u32)> = Vec::new();
    for (f, slot) in floor.iter().enumerate() {
        let Some((_, voice)) = *slot else { continue };
        let t = f as i64 * FRAME_MS;
        match runs.last_mut() {
            Some(run) if run.2 == voice && run.1 == t => run.1 = t + FRAME_MS,
            _ => runs.push((t, t + FRAME_MS, voice)),
        }
    }
    runs.retain(|r| r.1 - r.0 >= MIN_TURN_MS);

    // Join runs of the same voice, and split each silence between the two
    // people either side of it.
    let mut joined: Vec<(i64, i64, u32)> = Vec::new();
    for run in runs {
        match joined.last_mut() {
            Some(last) if last.2 == run.2 => last.1 = run.1,
            Some(last) => {
                let mid = (last.1 + run.0) / 2;
                last.1 = mid;
                joined.push((mid, run.1, run.2));
            }
            None => joined.push(run),
        }
    }
    if let Some(first) = joined.first_mut() {
        first.0 = 0;
    }
    if let Some(last) = joined.last_mut() {
        last.1 = last.1.max(total_ms);
    }

    let mut voices: Vec<u32> = Vec::new();
    let turns = joined
        .into_iter()
        .map(|(start_ms, end_ms, voice)| {
            let person = match voices.iter().position(|&v| v == voice) {
                Some(i) => i + 1,
                None => {
                    voices.push(voice);
                    voices.len()
                }
            };
            Turn {
                start_ms,
                end_ms,
                person: person as u32,
            }
        })
        .collect();
    (turns, voices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start_ms: i64, end_ms: i64, voice: u32) -> VoiceSegment {
        VoiceSegment {
            start_ms,
            end_ms,
            voice,
        }
    }

    #[test]
    fn covers_the_whole_recording_and_numbers_people_in_order() {
        let (turns, voices) = turns(&[seg(1000, 4000, 7), seg(5000, 9000, 3)], 10_000);
        assert_eq!(voices, vec![7, 3]);
        assert_eq!(
            turns,
            vec![
                Turn {
                    start_ms: 0,
                    end_ms: 4500,
                    person: 1
                },
                Turn {
                    start_ms: 4500,
                    end_ms: 10_000,
                    person: 2
                },
            ]
        );
    }

    #[test]
    fn short_sounds_go_to_the_speaker_around_them() {
        let (turns, voices) = turns(
            &[seg(0, 3000, 1), seg(3000, 3200, 2), seg(3200, 6000, 1)],
            6000,
        );
        assert_eq!(voices, vec![1]);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].end_ms, 6000);
    }

    #[test]
    fn the_longer_voice_holds_the_floor_in_overlap() {
        let (turns, _) = turns(&[seg(0, 10_000, 1), seg(4000, 5000, 2)], 10_000);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].person, 1);
    }

    #[test]
    fn nothing_heard_is_no_turns() {
        let (turns, voices) = turns(&[], 5000);
        assert!(turns.is_empty());
        assert!(voices.is_empty());
    }
}
