//! Questions about one note or meeting, answered from its own words
//! (design/wireframes/MeetingPage.dc.html, the Ask tab).
//!
//! A long meeting doesn't fit what a small model reads at once, so Bolke
//! cuts it into short passages, picks the ones that share the most words
//! with the question (and their neighbours, for context), and sends those
//! with the meeting's summary. The answer names the lines it came from,
//! which the page shows as quotes.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::Result;
use log::debug;
use tauri::{AppHandle, Manager};

use super::notes::{about_the_call, json_body, plain_words, with_writer, Writer};
use crate::managers::notes::{AnswerCheck, Note, NoteAnswer, NoteQuote, NotesManager};

const ASK_PROMPT: &str = "You answer a question about one meeting or note, \
using only the text in the user message. It has a short summary, then \
numbered lines, each with who said it. Answer the question at the end in one \
to three short sentences, in the same language as the question. Name who \
said or decided things. If the text doesn't answer the question, \
say it didn't come up. Never make anything up. lines: the numbers of up to three \
lines that back up the answer. Reply only with JSON with the keys answer (a \
string) and lines (a list of numbers).";

/// A passage longer than this is cut at sentences.
const PASSAGE_CHARS: usize = 320;
/// Room kept for the prompt, the summary, earlier questions and the reply.
const RESERVED_CHARS: usize = 2_500;
/// How much of the summary goes in.
const SUMMARY_CHARS: usize = 1_500;
/// Earlier questions sent along, so "and what did she say?" makes sense.
const HISTORY: usize = 2;
/// Quotes shown under an answer.
const MAX_QUOTES: usize = 3;
const QUOTE_CHARS: usize = 200;
const MIN_QUOTE_WORDS: usize = 6;

/// A short piece of a note: one turn of a meeting, or part of one.
#[derive(Clone, Debug, PartialEq)]
struct Passage {
    person: Option<u32>,
    start_ms: Option<i64>,
    text: String,
}

#[tauri::command]
#[specta::specta]
pub fn list_note_answers(app: AppHandle, id: i64) -> Result<Vec<NoteAnswer>, String> {
    let notes = app.state::<Arc<NotesManager>>();
    notes.list_answers(id).map_err(|e| e.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn clear_note_answers(app: AppHandle, id: i64) -> Result<(), String> {
    let notes = app.state::<Arc<NotesManager>>();
    notes.clear_answers(id).map_err(|e| e.to_string())
}

/// Answers `question` from the note's words and keeps it with the note.
#[tauri::command]
#[specta::specta]
pub async fn ask_note(app: AppHandle, id: i64, question: String) -> Result<NoteAnswer, String> {
    let question = question.trim().to_string();
    if question.is_empty() {
        return Err("Type a question first".into());
    }
    let notes = Arc::clone(&app.state::<Arc<NotesManager>>());
    let note = notes.get_note(id).await.map_err(|e| e.to_string())?;
    let earlier = notes.list_answers(id).map_err(|e| e.to_string())?;
    let passages = passages(&note);
    if passages.is_empty() {
        return Err("This note has no words to ask about yet".into());
    }
    let checked = with_writer(&app, |writer| {
        let question = question.clone();
        async move { answer(&writer, &note, &passages, &earlier, &question).await }
    })
    .await
    .map_err(|e| e.to_string())?;
    notes
        .add_answer(
            id,
            &question,
            &checked.answer,
            &checked.quotes,
            checked.check,
        )
        .map_err(|e| e.to_string())
}

/// An answer, its quotes, and how well the meeting backs it up.
struct Checked {
    answer: String,
    quotes: Vec<NoteQuote>,
    check: AnswerCheck,
}

async fn answer(
    writer: &Writer,
    note: &Note,
    passages: &[Passage],
    earlier: &[NoteAnswer],
    question: &str,
) -> Result<Checked> {
    let names = names(note);
    let budget = writer
        .part_chars()
        .saturating_sub(RESERVED_CHARS)
        .max(1_500);
    let picked = pick(passages, question, &names, budget);
    debug!(
        "Answering from {} of {} passages",
        picked.len(),
        passages.len()
    );
    let text = context(note, passages, &picked, &names, earlier, question);
    let reply = writer.ask(ASK_PROMPT, schema(), text.clone()).await?;
    let (answer, lines) = read_reply(&reply, passages, &picked);
    let check = check(&answer, &text, &lines);
    debug!("Answer checked: {check:?}");
    Ok(Checked {
        answer,
        quotes: quotes_of(passages, &lines),
        check,
    })
}

/// "It didn't come up", in the words a model uses for it.
fn didnt_come_up(answer: &str) -> bool {
    let a = answer.to_lowercase();
    [
        "didn't come up",
        "did not come up",
        "not discussed",
        "wasn't discussed",
        "was not discussed",
        "weren't discussed",
        "not mentioned",
        "wasn't mentioned",
        "was not mentioned",
        "no mention",
        "not covered",
        "doesn't say",
        "does not say",
        "no information",
        "not specified",
        "wasn't specified",
        "not explicitly",
        "no specific",
        "nothing about",
        "does not mention",
        "doesn't mention",
        "not come up",
        "no discussion",
    ]
    .iter()
    .any(|t| a.contains(t))
        || a.trim_start().starts_with("no,")
        || a.trim_start().starts_with("no.")
        || a.trim() == "no"
}

/// Share of an answer's key words that must be in what the model read for
/// the answer to count as backed.
const BACKED_SHARE: f32 = 0.8;

/// Checks an answer against the text the model was given: an answer whose
/// key words aren't in the meeting was made up, at least in part.
fn check(answer: &str, sent: &str, quotes: &[usize]) -> AnswerCheck {
    if didnt_come_up(answer) {
        return AnswerCheck::NotInMeeting;
    }
    let sent: HashSet<String> = key_stems(sent).into_iter().collect();
    let words = key_stems(answer);
    if words.is_empty() {
        return AnswerCheck::Unsure;
    }
    let found = words.iter().filter(|w| sent.contains(*w)).count();
    let share = found as f32 / words.len() as f32;
    if share >= BACKED_SHARE && !quotes.is_empty() {
        AnswerCheck::Backed
    } else {
        AnswerCheck::Unsure
    }
}

/// The note in short passages, in the order they were said.
fn passages(note: &Note) -> Vec<Passage> {
    let mut out = Vec::new();
    match &note.people {
        Some(people) if !people.lines.is_empty() => {
            for line in &people.lines {
                let pieces = cut(&line.text);
                let total: usize = pieces
                    .iter()
                    .map(|p| p.chars().count())
                    .sum::<usize>()
                    .max(1);
                let span = (line.end_ms - line.start_ms).max(0);
                let mut done = 0;
                for piece in pieces {
                    // Where in the turn this piece starts, roughly.
                    let at = line.start_ms + span * done as i64 / total as i64;
                    done += piece.chars().count();
                    out.push(Passage {
                        person: Some(line.person),
                        start_ms: Some(at),
                        text: piece,
                    });
                }
            }
        }
        _ => {
            for piece in cut(&note.text) {
                out.push(Passage {
                    person: None,
                    start_ms: None,
                    text: piece,
                });
            }
        }
    }
    out
}

/// `text` in pieces of about [`PASSAGE_CHARS`], cut after sentences.
fn cut(text: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut piece = String::new();
    for word in text.split_whitespace() {
        if !piece.is_empty() {
            piece.push(' ');
        }
        piece.push_str(word);
        let len = piece.chars().count();
        let ends_sentence = word.ends_with(['.', '?', '!', '।', '。']);
        if (ends_sentence && len >= PASSAGE_CHARS / 2) || len >= PASSAGE_CHARS {
            pieces.push(std::mem::take(&mut piece));
        }
    }
    if !piece.is_empty() {
        pieces.push(piece);
    }
    pieces
}

/// Each person's name as the page shows it.
fn names(note: &Note) -> HashMap<u32, String> {
    note.people
        .as_ref()
        .map(|p| {
            p.people
                .iter()
                .map(|person| {
                    let name = person
                        .name
                        .clone()
                        .unwrap_or_else(|| format!("Person {}", person.id));
                    (person.id, name)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Words too common to tell passages apart.
const COMMON: [&str; 60] = [
    "the",
    "and",
    "for",
    "are",
    "was",
    "were",
    "with",
    "that",
    "this",
    "what",
    "when",
    "where",
    "which",
    "who",
    "whom",
    "why",
    "how",
    "did",
    "does",
    "do",
    "about",
    "from",
    "into",
    "they",
    "them",
    "their",
    "there",
    "then",
    "than",
    "have",
    "has",
    "had",
    "will",
    "would",
    "should",
    "could",
    "can",
    "you",
    "your",
    "our",
    "his",
    "her",
    "she",
    "him",
    "its",
    "say",
    "said",
    "tell",
    "told",
    "meeting",
    "anything",
    "something",
    "any",
    "some",
    "all",
    "not",
    "but",
    "just",
    "also",
    "get",
];

/// A word's first letters, so "decide", "decided" and "decision" match.
fn stem(word: &str) -> String {
    word.chars().take(5).collect()
}

fn key_stems(text: &str) -> Vec<String> {
    plain_words(text)
        .split(' ')
        .filter(|w| w.chars().count() > 2 && !COMMON.contains(w))
        .map(stem)
        .collect()
}

/// The passages to send, in the order they were said: all of them when
/// they fit, else the ones sharing the most (rare) words with the question,
/// each with the passage before and after it.
fn pick(
    passages: &[Passage],
    question: &str,
    names: &HashMap<u32, String>,
    budget: usize,
) -> Vec<usize> {
    let size = |i: usize| passages[i].text.chars().count() + 24;
    let total: usize = (0..passages.len()).map(size).sum();
    if total <= budget {
        return (0..passages.len()).collect();
    }
    // People the question names, by name or as "person 3".
    let asked = plain_words(question);
    let about: HashSet<u32> = names
        .iter()
        .filter(|(_, name)| {
            let name = plain_words(name);
            !name.is_empty() && format!(" {asked} ").contains(&format!(" {name} "))
        })
        .map(|(id, _)| *id)
        .collect();
    let wanted: HashSet<String> = key_stems(question)
        .into_iter()
        .filter(|w| !about.iter().any(|id| stem(&plain_words(&names[id])) == *w))
        .collect();
    let stems: Vec<HashSet<String>> = passages
        .iter()
        .map(|p| key_stems(&p.text).into_iter().collect())
        .collect();
    // Rare words count for more.
    let n = passages.len() as f32;
    let weight: HashMap<&String, f32> = wanted
        .iter()
        .map(|w| {
            let df = stems.iter().filter(|s| s.contains(w)).count() as f32;
            (w, (n / (df + 1.0)).ln() + 1.0)
        })
        .collect();
    // Small talk about the call crowds out the meeting, unless that's what
    // the question is about.
    let call_question = about_the_call(&asked)
        || ["call", "interrupt", "connect"]
            .iter()
            .any(|w| asked.split(' ').any(|a| a.starts_with(w)));
    let part = when_in_meeting(&asked);
    let mut scored: Vec<(f32, usize)> = passages
        .iter()
        .enumerate()
        .filter(|(_, p)| call_question || !about_the_call(&plain_words(&p.text)))
        .map(|(i, p)| {
            let mut score: f32 = wanted
                .iter()
                .filter(|w| stems[i].contains(*w))
                .map(|w| weight[w])
                .sum();
            if p.person.is_some_and(|id| about.contains(&id)) {
                score += 1.5;
            }
            if call_question && about_the_call(&plain_words(&p.text)) {
                score += 2.0;
            }
            // "At the start", "after the meeting": the first or last part.
            let at = i as f32 / n;
            match part {
                Some(Part::Start) if at < 0.15 => score += 2.0,
                Some(Part::End) if at > 0.85 => score += 2.0,
                _ => {}
            }
            (score, i)
        })
        .filter(|(score, _)| *score > 0.0)
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut chosen: Vec<usize> = Vec::new();
    let mut used = 0;
    'best: for (_, i) in scored {
        for j in [i, i.saturating_sub(1), (i + 1).min(passages.len() - 1)] {
            if chosen.contains(&j)
                || (!call_question && about_the_call(&plain_words(&passages[j].text)))
            {
                continue;
            }
            if used + size(j) > budget {
                break 'best;
            }
            used += size(j);
            chosen.push(j);
        }
    }
    // Nothing matched ("what happened?"): the summary does most of the
    // work, with passages spread over the whole meeting.
    if chosen.is_empty() {
        let step = (total / budget).max(1) + 1;
        for i in (0..passages.len()).step_by(step) {
            if used + size(i) > budget {
                break;
            }
            used += size(i);
            chosen.push(i);
        }
    }
    chosen.sort_unstable();
    chosen
}

/// A part of the meeting a question points at.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    Start,
    End,
}

fn when_in_meeting(asked: &str) -> Option<Part> {
    let has = |phrases: &[&str]| {
        phrases
            .iter()
            .any(|p| format!(" {asked} ").contains(&format!(" {p} ")))
    };
    if has(&[
        "after the meeting",
        "next step",
        "next steps",
        "action item",
        "action items",
        "follow up",
        "at the end",
        "end of the meeting",
        "do next",
        "wrap up",
    ]) {
        Some(Part::End)
    } else if has(&[
        "at the start",
        "start of the meeting",
        "at the beginning",
        "beginning of the meeting",
        "first thing",
        "opened with",
    ]) {
        Some(Part::Start)
    } else {
        None
    }
}

fn clock(ms: i64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// What the model reads: the summary, the picked lines numbered, the last
/// couple of questions, then the question.
fn context(
    note: &Note,
    passages: &[Passage],
    picked: &[usize],
    names: &HashMap<u32, String>,
    earlier: &[NoteAnswer],
    question: &str,
) -> String {
    let mut text = format!("Title: {}\n", note.title);
    if let Some(summary) = &note.summary {
        let mut short = summary.summary.clone();
        for point in &summary.action_points {
            short.push_str(&format!("\n- To do: {}", point.text));
        }
        for card in &summary.people {
            if !card.gist.is_empty() {
                let name = names.get(&card.person).cloned().unwrap_or_default();
                short.push_str(&format!("\n- {name}: {}", card.gist));
            }
        }
        let short: String = short.chars().take(SUMMARY_CHARS).collect();
        text.push_str(&format!("\nSummary:\n{short}\n"));
    }
    text.push_str("\nLines:\n");
    for &i in picked {
        let p = &passages[i];
        let who = p
            .person
            .and_then(|id| names.get(&id).cloned())
            .map(|name| match p.start_ms {
                Some(ms) => format!("{name} ({})", clock(ms)),
                None => name,
            });
        match who {
            Some(who) => text.push_str(&format!("[{}] {who}: {}\n", i + 1, p.text)),
            None => text.push_str(&format!("[{}] {}\n", i + 1, p.text)),
        }
    }
    let recent = &earlier[earlier.len().saturating_sub(HISTORY)..];
    if !recent.is_empty() {
        text.push_str("\nEarlier questions:\n");
        for a in recent {
            text.push_str(&format!("Q: {}\nA: {}\n", a.question, a.answer));
        }
    }
    text.push_str(&format!("\nQuestion: {}", open_form(question)));
    text
}

/// Words that start a yes or no question.
const YES_NO: [&str; 14] = [
    "does", "did", "do", "is", "was", "are", "were", "will", "would", "can", "could", "should",
    "has", "have",
];

/// A yes or no question asked as an open one. A small model often gets the
/// yes or no backwards ("Does the deadline matter?" "Yes" when they said it doesn't),
/// but says what was said about it correctly.
fn open_form(question: &str) -> String {
    let q = question.trim();
    let first = q.split_whitespace().next().unwrap_or("").to_lowercase();
    if !YES_NO.contains(&first.as_str()) {
        return q.to_string();
    }
    let rest = q[q.find(char::is_whitespace).unwrap_or(q.len())..]
        .trim()
        .trim_end_matches('?');
    format!("What was said about this: {rest}?")
}

fn schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "answer": { "type": "string" },
            "lines": { "type": "array", "items": { "type": "integer" } }
        },
        "required": ["answer", "lines"],
        "additionalProperties": false
    })
}

/// The answer and its quotes. Only lines that were sent can be quoted; a
/// reply that isn't JSON is the answer as it came.
fn read_reply(reply: &str, passages: &[Passage], picked: &[usize]) -> (String, Vec<usize>) {
    #[derive(serde::Deserialize)]
    struct Reply {
        answer: String,
        #[serde(default)]
        lines: Vec<serde_json::Value>,
    }
    let Ok(parsed) = serde_json::from_str::<Reply>(json_body(reply)) else {
        let text = match reply.find("</think>") {
            Some(end) => &reply[end + "</think>".len()..],
            None => reply,
        };
        return (text.trim().to_string(), Vec::new());
    };
    let mut lines: Vec<usize> = parsed
        .lines
        .iter()
        .filter_map(|v| match v {
            serde_json::Value::Number(n) => n.as_u64().map(|n| n as usize),
            serde_json::Value::String(s) => s.trim_matches(['[', ']', ' ']).parse().ok(),
            _ => None,
        })
        .filter_map(|n| n.checked_sub(1))
        .filter(|i| picked.contains(i))
        // "Exactly. Okay, so" backs up nothing.
        .filter(|&i| passages[i].text.split_whitespace().count() >= MIN_QUOTE_WORDS)
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines.truncate(MAX_QUOTES);
    (parsed.answer.trim().to_string(), lines)
}

/// The passages at `lines`, as the page shows them.
fn quotes_of(passages: &[Passage], lines: &[usize]) -> Vec<NoteQuote> {
    lines
        .iter()
        .map(|&i| {
            let p = &passages[i];
            NoteQuote {
                person: p.person,
                start_ms: p.start_ms,
                text: shortened(&p.text),
            }
        })
        .collect()
}

fn shortened(text: &str) -> String {
    if text.chars().count() <= QUOTE_CHARS {
        return text.to_string();
    }
    let head: String = text.chars().take(QUOTE_CHARS).collect();
    let cut = head.rfind(' ').unwrap_or(head.len());
    format!("{}…", head[..cut].trim_end_matches([',', ';', ' ']))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn passage(person: u32, text: &str) -> Passage {
        Passage {
            person: Some(person),
            start_ms: Some(0),
            text: text.into(),
        }
    }

    #[test]
    fn long_turns_are_cut_after_sentences() {
        let text = "One two three. ".repeat(40);
        let pieces = cut(&text);
        assert!(pieces.len() > 1);
        assert!(pieces.iter().all(|p| p.chars().count() <= PASSAGE_CHARS));
        assert!(pieces.iter().all(|p| p.ends_with('.')));
        assert_eq!(cut("  "), Vec::<String>::new());
    }

    #[test]
    fn the_passages_that_match_the_question_are_picked_with_neighbours() {
        let mut passages: Vec<Passage> = (0..40)
            .map(|i| {
                passage(
                    1 + i % 2,
                    &format!("filler words number {i} about nothing much"),
                )
            })
            .collect();
        passages[20] = passage(2, "The budget for the checkout redesign is ten lakh.");
        let names = HashMap::from([(1, "Priya".to_string()), (2, "Rahul".to_string())]);
        let picked = pick(&passages, "What budget did we set?", &names, 250);
        assert!(picked.contains(&20));
        assert!(picked.contains(&19) && picked.contains(&21));
        assert!(picked.windows(2).all(|w| w[0] < w[1]));
        // Everything is sent when it fits.
        assert_eq!(pick(&passages, "budget", &names, 100_000).len(), 40);
        // A question about a person leans to their lines.
        let picked = pick(&passages, "What did Rahul want?", &names, 70);
        assert_eq!(picked, vec![1]);
        assert_eq!(passages[1].person, Some(2));
        // Nothing in common still sends something.
        assert!(!pick(&passages, "zzz?", &names, 200).is_empty());
    }

    #[test]
    fn replies_quote_only_lines_that_were_sent() {
        let passages = vec![
            passage(1, "We ship it to everyone on Friday."),
            passage(2, "I will test it with five people."),
            passage(1, "Not this."),
        ];
        let reply = r#"```json
{"answer":" Friday, and Rahul tests it. ","lines":[2, "1", 3, 9, 2]}
```"#;
        let (answer, lines) = read_reply(reply, &passages, &[0, 1, 2]);
        let quotes = quotes_of(&passages, &lines);
        assert_eq!(answer, "Friday, and Rahul tests it.");
        assert_eq!(
            quotes.iter().map(|q| q.text.as_str()).collect::<Vec<_>>(),
            vec![
                "We ship it to everyone on Friday.",
                "I will test it with five people."
            ]
        );
        let (answer, lines) = read_reply("<think>x</think> It didn't come up.", &passages, &[0]);
        let quotes = quotes_of(&passages, &lines);
        assert_eq!(answer, "It didn't come up.");
        assert!(quotes.is_empty());
    }

    #[test]
    fn yes_or_no_questions_are_asked_as_open_ones() {
        assert_eq!(
            open_form("Does the deadline matter, according to Priya?"),
            "What was said about this: the deadline matter, according to Priya?"
        );
        assert_eq!(open_form("What was decided?"), "What was decided?");
        assert!(didnt_come_up("The text does not come up."));
        assert!(didnt_come_up("No"));
        assert!(!didnt_come_up("Nobody objected to Drupal."));
    }

    #[test]
    fn long_quotes_are_cut_at_a_word() {
        let long = "word ".repeat(100);
        let short = shortened(&long);
        assert!(short.ends_with('…'));
        assert!(short.chars().count() <= QUOTE_CHARS + 1);
        assert_eq!(clock(65_000), "1:05");
        assert_eq!(clock(3_725_000), "1:02:05");
    }
}

/// Asks a question about a real note with a running summary helper:
/// `BOLKE_HELPER=http://127.0.0.1:PORT/v1 BOLKE_NOTE=12 BOLKE_ASK="..." cargo test
/// --lib ask_live -- --ignored --nocapture`
#[cfg(test)]
mod ask_live {
    use super::*;

    pub(super) fn load(id: i64) -> Note {
        let db = std::path::PathBuf::from(std::env::var("HOME").unwrap())
            .join("Library/Application Support/app.bolke.desktop/notes.db");
        let conn =
            rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .unwrap();
        let (title, text, summary, people): (String, String, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT title, text, summary, people FROM notes WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        let note = Note {
            id,
            project_id: None,
            kind: crate::managers::notes::NoteKind::Meeting,
            title,
            text,
            text_pending: false,
            summary: summary.and_then(|s| serde_json::from_str(&s).ok()),
            entry_id: None,
            audio_file: None,
            duration_ms: None,
            created_at: 0,
            updated_at: 0,
            people: people.and_then(|p| serde_json::from_str(&p).ok()),
            people_state: None,
        };
        note
    }

    #[test]
    #[ignore]
    fn ask_live() {
        let base_url = std::env::var("BOLKE_HELPER").unwrap();
        let id: i64 = std::env::var("BOLKE_NOTE").unwrap().parse().unwrap();
        let questions = std::env::var("BOLKE_ASK").unwrap();
        let note = load(id);
        let writer = Writer::Local { base_url };
        let passages = passages(&note);
        tauri::async_runtime::block_on(async {
            for q in questions.split('|') {
                let started = std::time::Instant::now();
                let c = answer(&writer, &note, &passages, &[], q).await.unwrap();
                println!(
                    "\nQ: {q}\nA: {}  ({:.1}s, {:?})",
                    c.answer,
                    started.elapsed().as_secs_f32(),
                    c.check
                );
                for quote in c.quotes {
                    println!(
                        "   > {:?} {:?}: {}",
                        quote.person, quote.start_ms, quote.text
                    );
                }
            }
        });
    }
}

/// Grades answers against questions with known answers:
/// `BOLKE_HELPER=... BOLKE_NOTE=12 BOLKE_CASES=cases.json cargo test --lib
/// ask_eval -- --ignored --nocapture`. Each case has `q`, `truth`, and
/// either words the answer must have (`any` / `all`) or `trap: true` for a
/// question the meeting never answers.
#[cfg(test)]
mod ask_eval {
    use super::*;

    #[derive(serde::Deserialize)]
    struct Case {
        q: String,
        truth: String,
        #[serde(default)]
        any: Vec<String>,
        #[serde(default)]
        all: Vec<String>,
        #[serde(default)]
        trap: bool,
    }

    #[test]
    #[ignore]
    fn ask_eval() {
        let base_url = std::env::var("BOLKE_HELPER").unwrap();
        let id: i64 = std::env::var("BOLKE_NOTE").unwrap().parse().unwrap();
        let cases: Vec<Case> = serde_json::from_str(
            &std::fs::read_to_string(std::env::var("BOLKE_CASES").unwrap()).unwrap(),
        )
        .unwrap();
        let note = super::ask_live::load(id);
        let writer = Writer::Local { base_url };
        let passages = passages(&note);
        let (mut right, mut sure_and_wrong) = (0, 0);
        tauri::async_runtime::block_on(async {
            for case in &cases {
                let c = answer(&writer, &note, &passages, &[], &case.q)
                    .await
                    .unwrap();
                let a = c.answer.to_lowercase();
                let ok = if case.trap {
                    didnt_come_up(&a)
                } else {
                    (case.any.is_empty() || case.any.iter().any(|w| a.contains(w)))
                        && case.all.iter().all(|w| a.contains(w))
                };
                right += ok as usize;
                if !ok && c.check == AnswerCheck::Backed {
                    sure_and_wrong += 1;
                }
                println!(
                    "\n[{}] {}\n  A: {}\n  check: {:?}  quotes: {}\n  truth: {}",
                    if ok { "RIGHT" } else { "WRONG" },
                    case.q,
                    c.answer,
                    c.check,
                    c.quotes.len(),
                    case.truth
                );
            }
        });
        println!(
            "\nSCORE {right}/{} right; {sure_and_wrong} wrong but shown as backed",
            cases.len()
        );
    }
}
