//! Reading a reminder out of what was said: "remind me to call Priya tomorrow
//! at 5" becomes "Call Priya", due tomorrow at 5:00 pm. English and Hinglish
//! ("kal subah 9 baje yaad dilana"), on this computer, with no model.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime, Weekday};

/// Which way the words arrived. Dictation is typed unless it starts like a
/// reminder, so it's caught only then; an idea is caught more freely.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heard {
    Idea,
    Dictation,
    /// Typed into To-do: always a to-do, its time read if there is one.
    Typed,
}

/// A to-do read from speech.
#[derive(Clone, Debug, PartialEq)]
pub struct Spoken {
    pub what: String,
    pub due: Option<NaiveDateTime>,
}

struct Tok {
    orig: String,
    norm: String,
    used: bool,
}

fn norm(word: &str) -> String {
    word.trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
        .replace('’', "'")
}

/// Words said before the request that aren't part of it.
const FILLERS: &[&str] = &[
    "hey", "hi", "ok", "okay", "please", "bolke", "so", "um", "uh", "and",
];

/// How a reminder starts in English; longest first.
const STARTS: &[&[&str]] = &[
    &["set", "a", "reminder", "to"],
    &["set", "a", "reminder", "for"],
    &["set", "a", "reminder"],
    &["create", "a", "reminder", "to"],
    &["remind", "me", "to"],
    &["remind", "me", "that"],
    &["remind", "me", "about"],
    &["remind", "me", "of"],
    &["remind", "me"],
    &["reminder", "to"],
    &["reminder"],
];

/// How a to-do starts, for an idea or typed text only.
const TODO_STARTS: &[&[&str]] = &[
    &["dont", "forget", "to"],
    &["don't", "forget", "to"],
    &["do", "not", "forget", "to"],
    &["to-do"],
    &["todo"],
];

/// "Remind me" in Hindi, which comes anywhere in the sentence.
const HINDI: &[&[&str]] = &[
    &["yaad", "dila", "dena"],
    &["yaad", "dila", "do"],
    &["yaad", "dilana"],
    &["yaad", "dilaana"],
    &["yad", "dilana"],
    &["yaad", "dila"],
    &["याद", "दिला", "देना"],
    &["याद", "दिलाना"],
    &["याद", "दिला"],
];

/// "...to my to-do list" at the end of "add milk to my to-do list".
const LIST_ENDS: &[&[&str]] = &[
    &["to", "my", "to-do", "list"],
    &["to", "my", "todo", "list"],
    &["to", "my", "to", "do", "list"],
    &["on", "my", "to-do", "list"],
    &["on", "my", "todo", "list"],
    &["to", "the", "to-do", "list"],
    &["to", "the", "todo", "list"],
    &["to", "my", "list"],
    &["to", "my", "to-dos"],
    &["to", "my", "todos"],
];

/// Words left hanging at either end once the time and request are taken out.
const LOOSE: &[&str] = &[
    "to",
    "that",
    "about",
    "of",
    "at",
    "on",
    "by",
    "and",
    "for",
    "ki",
    "ko",
    "ke",
    "mujhe",
    "मुझे",
    "please",
    "then",
    "in",
];

impl Tok {
    fn new(orig: &str) -> Self {
        Self {
            orig: orig.to_string(),
            norm: norm(orig),
            used: false,
        }
    }
}

fn matches_at(toks: &[Tok], at: usize, phrase: &[&str]) -> bool {
    phrase.len() + at <= toks.len()
        && phrase
            .iter()
            .enumerate()
            .all(|(k, w)| !toks[at + k].used && toks[at + k].norm == *w)
}

fn take(toks: &mut [Tok], at: usize, len: usize) {
    for t in &mut toks[at..at + len] {
        t.used = true;
    }
}

/// The first word that isn't filler.
fn first_word(toks: &mut [Tok]) -> usize {
    let mut i = 0;
    while i < toks.len() && FILLERS.contains(&toks[i].norm.as_str()) {
        i += 1;
    }
    i
}

/// Finds and takes the words that ask for a reminder or to-do. Returns
/// whether there were any.
fn take_request(toks: &mut [Tok], heard: Heard) -> bool {
    let start = first_word(toks);
    let mut found = false;
    for phrase in STARTS {
        if matches_at(toks, start, phrase) {
            take(toks, 0, start + phrase.len());
            found = true;
            break;
        }
    }
    if !found && heard != Heard::Dictation {
        for phrase in TODO_STARTS {
            if matches_at(toks, start, phrase) {
                take(toks, 0, start + phrase.len());
                found = true;
                break;
            }
        }
        // "Add milk to my to-do list".
        if !found {
            for phrase in LIST_ENDS {
                let Some(at) = (0..toks.len()).find(|&i| matches_at(toks, i, phrase)) else {
                    continue;
                };
                take(toks, at, phrase.len());
                if matches!(toks[start].norm.as_str(), "add" | "put" | "write") {
                    take(toks, 0, start + 1);
                }
                found = true;
                break;
            }
        }
        // "...then remind me to call her" in the middle of an idea.
        if !found && heard == Heard::Idea {
            for phrase in &STARTS[4..9] {
                if let Some(at) = (0..toks.len()).find(|&i| matches_at(toks, i, phrase)) {
                    take(toks, at, phrase.len());
                    found = true;
                    break;
                }
            }
        }
    }
    // Hindi asks anywhere, from either shortcut.
    for phrase in HINDI {
        if let Some(at) = (0..toks.len()).find(|&i| matches_at(toks, i, phrase)) {
            take(toks, at, phrase.len());
            found = true;
            break;
        }
    }
    found
}

// ----- Time -----

#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    Morning,
    Noon,
    Afternoon,
    Evening,
    Night,
}

impl Part {
    fn default_time(self) -> (u32, u32) {
        match self {
            Part::Morning => (9, 0),
            Part::Noon => (12, 0),
            Part::Afternoon => (14, 0),
            Part::Evening => (18, 0),
            Part::Night => (21, 0),
        }
    }
}

#[derive(Default)]
struct When {
    /// Days from today.
    day: Option<i64>,
    /// Today when it's still ahead, else the next one.
    weekday: Option<(Weekday, bool)>,
    part: Option<Part>,
    /// Hour, minute, and whether it's pm (`None`: not said).
    clock: Option<(u32, u32, Option<bool>)>,
    after: Option<Duration>,
}

fn number(word: &str) -> Option<u32> {
    if let Ok(n) = word.parse::<u32>() {
        return Some(n);
    }
    Some(match word {
        "a" | "an" | "one" | "ek" => 1,
        "two" | "do" => 2,
        "three" | "teen" => 3,
        "four" | "char" | "chaar" => 4,
        "five" | "paanch" | "panch" => 5,
        "six" | "chhe" | "chhah" | "che" => 6,
        "seven" | "saat" => 7,
        "eight" | "aath" => 8,
        "nine" | "nau" => 9,
        "ten" | "das" => 10,
        "eleven" | "gyarah" => 11,
        "twelve" | "barah" | "baarah" => 12,
        "fifteen" | "pandrah" => 15,
        "twenty" | "bees" => 20,
        "thirty" | "tees" => 30,
        "forty" => 40,
        "forty-five" => 45,
        "fifty" | "pachaas" => 50,
        "sixty" => 60,
        "ninety" => 90,
        _ => return None,
    })
}

fn unit(word: &str) -> Option<Duration> {
    Some(match word {
        "minute" | "minutes" | "min" | "mins" | "minat" | "minute's" => Duration::minutes(1),
        "hour" | "hours" | "hr" | "hrs" | "ghanta" | "ghante" | "ghanton" => Duration::hours(1),
        "day" | "days" | "din" => Duration::days(1),
        "week" | "weeks" | "hafte" | "hafta" => Duration::weeks(1),
        _ => return None,
    })
}

fn weekday(word: &str) -> Option<Weekday> {
    Some(match word {
        "monday" | "somvar" | "somwar" => Weekday::Mon,
        "tuesday" | "mangalvar" | "mangalwar" => Weekday::Tue,
        "wednesday" | "budhvar" | "budhwar" => Weekday::Wed,
        "thursday" | "guruvar" | "guruwar" | "brihaspativar" => Weekday::Thu,
        "friday" | "shukravar" | "shukrawar" => Weekday::Fri,
        "saturday" | "shanivar" | "shaniwar" => Weekday::Sat,
        "sunday" | "ravivar" | "raviwar" | "itvaar" | "itwar" => Weekday::Sun,
        _ => return None,
    })
}

fn part(word: &str) -> Option<Part> {
    Some(match word {
        "morning" | "subah" | "subha" | "savere" => Part::Morning,
        "noon" | "midday" => Part::Noon,
        "afternoon" | "dopahar" | "dopehar" => Part::Afternoon,
        "evening" | "shaam" | "sham" => Part::Evening,
        "night" | "tonight" | "raat" => Part::Night,
        _ => return None,
    })
}

/// "5", "5:30", "17:00", "5pm", "5.30pm" -> hour, minute, pm.
fn clock(word: &str) -> Option<(u32, u32, Option<bool>)> {
    let w = word.replace('.', ":");
    let (digits, pm) = if let Some(d) = w.strip_suffix("pm").or(w.strip_suffix("p:m")) {
        (d, Some(true))
    } else if let Some(d) = w.strip_suffix("am").or(w.strip_suffix("a:m")) {
        (d, Some(false))
    } else {
        (w.as_str(), None)
    };
    let digits = digits.trim_end_matches(':');
    let (h, m) = match digits.split_once(':') {
        Some((h, m)) => (h.parse().ok()?, m.parse().ok()?),
        None => (number(digits)?, 0),
    };
    (h <= 23 && m <= 59).then_some((h, m, pm))
}

fn ampm(word: &str) -> Option<bool> {
    match word.replace('.', "").as_str() {
        "pm" => Some(true),
        "am" => Some(false),
        _ => None,
    }
}

/// The word `at`, unless it's already taken.
fn peek(toks: &[Tok], at: usize) -> String {
    toks.get(at)
        .filter(|t| !t.used)
        .map(|t| t.norm.clone())
        .unwrap_or_default()
}

/// Takes every time word it finds and works out when.
fn take_when(toks: &mut [Tok]) -> When {
    let mut when = When::default();
    let n = toks.len();
    let mut i = 0;
    while i < n {
        if toks[i].used {
            i += 1;
            continue;
        }
        let w = toks[i].norm.clone();
        // "in 20 minutes", "in an hour", "in half an hour".
        if w == "in" && when.after.is_none() {
            if peek(toks, i + 1) == "half"
                && matches!(peek(toks, i + 2).as_str(), "an" | "a")
                && unit(&peek(toks, i + 3)).is_some()
            {
                when.after = Some(unit(&peek(toks, i + 3)).unwrap() / 2);
                take(toks, i, 4);
                i += 4;
                continue;
            }
            if let (Some(k), Some(u)) = (number(&peek(toks, i + 1)), unit(&peek(toks, i + 2))) {
                when.after = Some(u * k as i32);
                take(toks, i, 3);
                i += 3;
                continue;
            }
        }
        // "20 minute baad", "do ghante mein", "aadhe ghante baad".
        if when.after.is_none() {
            let half = matches!(w.as_str(), "aadhe" | "adhe" | "aadha");
            let k = if half { Some(1) } else { number(&w) };
            if let (Some(k), Some(u)) = (k, unit(&peek(toks, i + 1))) {
                if matches!(
                    peek(toks, i + 2).as_str(),
                    "baad" | "bad" | "mein" | "me" | "main"
                ) {
                    let span = if half { u / 2 } else { u * k as i32 };
                    when.after = Some(span);
                    take(toks, i, 3);
                    i += 3;
                    continue;
                }
            }
        }
        // Days.
        let day = match w.as_str() {
            "today" | "aaj" => Some(0),
            "tonight" => {
                when.part.get_or_insert(Part::Night);
                Some(0)
            }
            "tomorrow" | "tmrw" | "kal" => Some(1),
            "parso" | "parson" | "parsoon" => Some(2),
            _ => None,
        };
        if let Some(day) = day {
            when.day = Some(day);
            take(toks, i, 1);
            // "day after tomorrow"
            if w == "tomorrow" && i >= 2 && toks[i - 2].norm == "day" && toks[i - 1].norm == "after"
            {
                when.day = Some(2);
                take(toks, i - 2, 2);
            }
            i += 1;
            continue;
        }
        if let Some(day) = weekday(&w) {
            let next_week = i > 0 && toks[i - 1].norm == "next";
            when.weekday = Some((day, next_week));
            take(toks, i, 1);
            if i > 0 && matches!(toks[i - 1].norm.as_str(), "next" | "this" | "on") {
                take(toks, i - 1, 1);
            }
            if peek(toks, i + 1) == "ko" {
                take(toks, i + 1, 1);
            }
            i += 1;
            continue;
        }
        if let Some(p) = part(&w) {
            when.part = Some(p);
            take(toks, i, 1);
            // "in the morning", "this evening".
            if i >= 2 && toks[i - 2].norm == "in" && toks[i - 1].norm == "the" {
                take(toks, i - 2, 2);
            } else if i > 0 && toks[i - 1].norm == "this" {
                take(toks, i - 1, 1);
            }
            if peek(toks, i + 1) == "ko" || peek(toks, i + 1) == "mein" {
                take(toks, i + 1, 1);
            }
            i += 1;
            continue;
        }
        // Hindi clock: "9 baje", "saade 9 baje", "dhai baje".
        let (offset, skip) = match w.as_str() {
            "saade" | "sade" | "saadhe" => (30, 1),
            "sava" | "savaa" | "sawa" => (15, 1),
            "paune" | "pone" => (-15, 1),
            _ => (0, 0),
        };
        let special = match w.as_str() {
            "dhai" | "dhaai" => Some((2, 30)),
            "dedh" | "derh" => Some((1, 30)),
            _ => None,
        };
        if let Some((h, m)) = special {
            if matches!(peek(toks, i + 1).as_str(), "baje" | "bje") {
                when.clock = Some((h, m, None));
                take(toks, i, 2);
                i += 2;
                continue;
            }
        }
        if let Some(h) = number(&peek(toks, i + skip)) {
            if matches!(peek(toks, i + skip + 1).as_str(), "baje" | "bje" | "baj") && h <= 12 {
                let minutes = h as i32 * 60 + offset;
                when.clock = Some(((minutes / 60) as u32, (minutes % 60) as u32, None));
                take(toks, i, skip + 2);
                i += skip + 2;
                continue;
            }
        }
        // English clock: "at 5", "at 5:30 pm", "5pm", "17:00", "5 o'clock".
        let lead = matches!(w.as_str(), "at" | "by" | "around");
        let at = if lead { i + 1 } else { i };
        if let Some(word) = toks.get(at).filter(|t| !t.used).map(|t| t.norm.clone()) {
            if let Some((h, m, mut pm)) = clock(&word) {
                let mut len = at - i + 1;
                let after = toks.get(at + 1).filter(|t| !t.used).map(|t| t.norm.clone());
                if pm.is_none() {
                    if let Some(p) = after.as_deref().and_then(ampm) {
                        pm = Some(p);
                        len += 1;
                    } else if matches!(after.as_deref(), Some("oclock" | "o'clock")) {
                        len += 1;
                    }
                }
                // A bare number is a time only with "at", am/pm or a colon.
                let clear = lead || pm.is_some() || word.contains(':') || len > 1;
                if clear {
                    when.clock = Some((h, m, pm));
                    take(toks, i, len);
                    i += len;
                    continue;
                }
            }
        }
        i += 1;
    }
    when
}

fn hour_of(h: u32, pm: Option<bool>, part: Option<Part>) -> u32 {
    match pm {
        Some(true) if h < 12 => h + 12,
        Some(false) if h == 12 => 0,
        Some(_) => h,
        None if h >= 13 => h,
        None => match part {
            Some(Part::Afternoon | Part::Evening) if h < 12 => h + 12,
            Some(Part::Night) if h == 12 => 0,
            Some(Part::Night) if (5..12).contains(&h) => h + 12,
            Some(Part::Morning) if h == 12 => 0,
            Some(Part::Morning | Part::Noon) => h,
            // "At 5" means 5 pm; "at 9" means 9 am.
            _ if (1..=6).contains(&h) => h + 12,
            _ => h,
        },
    }
}

fn resolve(when: &When, now: NaiveDateTime) -> Option<NaiveDateTime> {
    if let Some(after) = when.after {
        return Some(now + after);
    }
    let time = match (when.clock, when.part) {
        (Some((h, m, pm)), part) => Some((hour_of(h, pm, part), m)),
        (None, Some(p)) => Some(p.default_time()),
        (None, None) => None,
    };
    let at = |date: NaiveDate, (h, m): (u32, u32)| {
        date.and_time(NaiveTime::from_hms_opt(h, m, 0).unwrap_or_default())
    };
    let today = now.date();
    if let Some((day, next_week)) = when.weekday {
        let ahead = (day.num_days_from_monday() as i64
            - today.weekday().num_days_from_monday() as i64)
            .rem_euclid(7);
        let time = time.unwrap_or((9, 0));
        let mut date = today + Duration::days(ahead);
        // "Next Monday", said on a Monday, is a week away.
        if next_week && ahead == 0 {
            date += Duration::days(7);
        }
        if at(date, time) <= now {
            date += Duration::days(7);
        }
        return Some(at(date, time));
    }
    match (when.day, time) {
        (Some(day), Some(time)) => Some(at(today + Duration::days(day), time)),
        (Some(0), None) => {
            // "Today", with no time: this evening, or soon if that's gone.
            let evening = at(today, (18, 0));
            Some(if evening > now {
                evening
            } else {
                now + Duration::hours(1)
            })
        }
        (Some(day), None) => Some(at(today + Duration::days(day), (9, 0))),
        (None, Some(time)) => {
            let due = at(today, time);
            Some(if due > now {
                due
            } else {
                due + Duration::days(1)
            })
        }
        (None, None) => None,
    }
}

/// What's left once the request and time are taken out, tidied.
fn what_is_left(toks: &[Tok]) -> String {
    let left: Vec<&Tok> = toks.iter().filter(|t| !t.used).collect();
    let mut a = 0;
    let mut b = left.len();
    while a < b && (LOOSE.contains(&left[a].norm.as_str()) || left[a].norm.is_empty()) {
        a += 1;
    }
    while b > a && (LOOSE.contains(&left[b - 1].norm.as_str()) || left[b - 1].norm.is_empty()) {
        b -= 1;
    }
    let text = left[a..b]
        .iter()
        .map(|t| t.orig.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let text = text
        .trim_matches(|c: char| c.is_whitespace() || ",;:-–—".contains(c))
        .trim_end_matches(['.', '!', '?'])
        .trim();
    let mut chars = text.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Reads a reminder or to-do out of `text`, or `None` when it isn't one.
pub fn hear(text: &str, heard: Heard, now: NaiveDateTime) -> Option<Spoken> {
    let mut toks: Vec<Tok> = text.split_whitespace().map(Tok::new).collect();
    if toks.is_empty() {
        return None;
    }
    let asked = take_request(&mut toks, heard);
    if !asked && heard != Heard::Typed {
        return None;
    }
    let when = take_when(&mut toks);
    let due = resolve(&when, now);
    let mut what = what_is_left(&toks);
    if what.is_empty() {
        if heard == Heard::Typed && due.is_none() {
            return None;
        }
        what = "Reminder".to_string();
    }
    Some(Spoken { what, due })
}

/// A change to a reminder already in To-do, said out loud: "delete the
/// reminder about Priya", "move my dentist reminder to 6 pm", "mark the milk
/// to-do done", "Priya wala reminder hata do".
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Delete,
    Done,
    Move(NaiveDateTime),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpokenChange {
    pub change: Change,
    /// The words that name the reminder, lower case; empty for "that
    /// reminder", which means the newest.
    pub about: Vec<String>,
    /// A time said to pick it out: "delete my 5 pm reminder".
    pub at: Option<NaiveDateTime>,
}

const DELETE_WORDS: &[&str] = &["delete", "cancel", "remove", "clear", "drop", "scrap"];
const MOVE_WORDS: &[&str] = &["move", "change", "push", "reschedule", "shift", "postpone"];
const DONE_WORDS: &[&str] = &["done", "complete", "completed", "finished"];
const NOUNS: &[&str] = &[
    "reminder",
    "reminders",
    "todo",
    "todos",
    "to-do",
    "to-dos",
    "task",
    "alarm",
];
const HINDI_DELETE: &[&[&str]] = &[
    &["hata", "do"],
    &["hatao"],
    &["mita", "do"],
    &["delete", "kar", "do"],
    &["delete", "karo"],
    &["cancel", "kar", "do"],
    &["cancel", "karo"],
];
const HINDI_DONE: &[&[&str]] = &[&["ho", "gaya"], &["ho", "gya"], &["done", "kar", "do"]];
const HINDI_DO: &[&[&str]] = &[&["kar", "do"], &["karo"]];
/// Words around the name of the reminder that aren't part of it.
const LINKS: &[&str] = &[
    "the", "my", "a", "an", "about", "to", "for", "of", "as", "that", "this", "it", "called",
    "named", "with", "which", "is", "say", "says", "saying", "mark", "tick", "off", "wala", "wali",
    "vala", "wale", "ka", "ki", "ke", "ko", "please", "at", "on", "by", "set", "one",
];

/// Reads a change to an existing reminder out of `text`, or `None` when it
/// isn't one. Dictation needs the word "reminder" or "to-do" in it.
pub fn hear_change(text: &str, heard: Heard, now: NaiveDateTime) -> Option<SpokenChange> {
    if heard == Heard::Typed {
        return None;
    }
    let mut toks: Vec<Tok> = text.split_whitespace().map(Tok::new).collect();
    let start = first_word(&mut toks);
    if start >= toks.len() {
        return None;
    }
    take(&mut toks, 0, start);
    let has_noun = toks.iter().any(|t| NOUNS.contains(&t.norm.as_str()));
    let first = toks[start].norm.clone();
    let at_end = |toks: &[Tok], phrase: &[&str]| {
        toks.len() >= phrase.len() && matches_at(toks, toks.len() - phrase.len(), phrase)
    };
    let take_end = |toks: &mut Vec<Tok>, phrase: &[&str]| {
        let n = toks.len();
        take(toks, n - phrase.len(), phrase.len());
    };
    let last_is_done = toks
        .last()
        .is_some_and(|t| DONE_WORDS.contains(&t.norm.as_str()));

    let change = if DELETE_WORDS.contains(&first.as_str()) && has_noun {
        take(&mut toks, start, 1);
        Change::Delete
    } else if let Some(p) = HINDI_DELETE.iter().find(|p| has_noun && at_end(&toks, p)) {
        take_end(&mut toks, p);
        Change::Delete
    } else if (first == "mark" || first == "tick")
        && (has_noun || heard == Heard::Idea)
        && (last_is_done || toks.iter().any(|t| t.norm == "off"))
    {
        Change::Done
    } else if has_noun && last_is_done {
        Change::Done
    } else if let Some(p) = HINDI_DONE.iter().find(|p| has_noun && at_end(&toks, p)) {
        take_end(&mut toks, p);
        Change::Done
    } else if has_noun
        && (MOVE_WORDS.contains(&first.as_str()) || HINDI_DO.iter().any(|p| at_end(&toks, p)))
    {
        if MOVE_WORDS.contains(&first.as_str()) {
            take(&mut toks, start, 1);
        } else if let Some(p) = HINDI_DO.iter().find(|p| at_end(&toks, p)) {
            take_end(&mut toks, p);
        }
        let when = take_when(&mut toks);
        Change::Move(resolve(&when, now)?)
    } else {
        return None;
    };
    let at = match change {
        Change::Move(_) => None,
        _ => resolve(&take_when(&mut toks), now),
    };
    let about = toks
        .iter()
        .filter(|t| !t.used && !t.norm.is_empty())
        .map(|t| t.norm.clone())
        .filter(|w| {
            !NOUNS.contains(&w.as_str())
                && !DONE_WORDS.contains(&w.as_str())
                && !LINKS.contains(&w.as_str())
        })
        .collect();
    Some(SpokenChange { change, about, at })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wednesday 18 November 2026, 10:00.
    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 18)
            .unwrap()
            .and_hms_opt(10, 0, 0)
            .unwrap()
    }

    fn at(day: u32, h: u32, m: u32) -> Option<NaiveDateTime> {
        Some(
            NaiveDate::from_ymd_opt(2026, 11, day)
                .unwrap()
                .and_hms_opt(h, m, 0)
                .unwrap(),
        )
    }

    fn idea(text: &str) -> Option<(String, Option<NaiveDateTime>)> {
        hear(text, Heard::Idea, now()).map(|s| (s.what, s.due))
    }

    fn dictation(text: &str) -> Option<(String, Option<NaiveDateTime>)> {
        hear(text, Heard::Dictation, now()).map(|s| (s.what, s.due))
    }

    fn change(text: &str) -> Option<(Change, Vec<String>)> {
        hear_change(text, Heard::Dictation, now()).map(|c| (c.change, c.about))
    }

    fn words(w: &[&str]) -> Vec<String> {
        w.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reminders_are_changed_by_voice() {
        assert_eq!(
            change("Delete the reminder about calling Priya."),
            Some((Change::Delete, words(&["calling", "priya"])))
        );
        assert_eq!(
            change("Hey Bolke, cancel that reminder"),
            Some((Change::Delete, vec![]))
        );
        assert_eq!(
            change("Priya wala reminder hata do"),
            Some((Change::Delete, words(&["priya"])))
        );
        assert_eq!(
            change("Mark the milk to-do as done"),
            Some((Change::Done, words(&["milk"])))
        );
        assert_eq!(
            change("move my dentist reminder to 6 pm"),
            Some((Change::Move(at(18, 18, 0).unwrap()), words(&["dentist"])))
        );
        let by_time = hear_change("delete my 5 pm reminder", Heard::Dictation, now()).unwrap();
        assert_eq!(by_time.at, at(18, 17, 0));
        assert!(by_time.about.is_empty());
        // Not a change: ordinary words, or a reminder being made.
        assert_eq!(change("delete the second paragraph"), None);
        assert_eq!(change("remind me to delete the old files"), None);
        assert_eq!(change("mark it done"), None);
        assert_eq!(
            hear_change("mark call Priya done", Heard::Idea, now()).map(|c| c.change),
            Some(Change::Done)
        );
    }

    #[test]
    fn english_reminders() {
        assert_eq!(
            idea("Remind me to call Priya tomorrow at 5."),
            Some(("Call Priya".into(), at(19, 17, 0)))
        );
        assert_eq!(
            idea("remind me in 20 minutes to check the oven"),
            Some(("Check the oven".into(), at(18, 10, 20)))
        );
        assert_eq!(
            idea("Remind me at 9:30 am on Friday to send the invoice"),
            Some(("Send the invoice".into(), at(20, 9, 30)))
        );
        assert_eq!(
            idea("Hey Bolke, remind me to water the plants this evening."),
            Some(("Water the plants".into(), at(18, 18, 0)))
        );
        assert_eq!(
            idea("Remind me next Monday to renew the domain"),
            Some(("Renew the domain".into(), at(23, 9, 0)))
        );
        assert_eq!(
            idea("Set a reminder for the dentist at 4 p.m."),
            Some(("The dentist".into(), at(18, 16, 0)))
        );
        assert_eq!(
            idea("remind me in half an hour to stretch"),
            Some(("Stretch".into(), at(18, 10, 30)))
        );
    }

    #[test]
    fn a_time_already_gone_today_is_tomorrow() {
        assert_eq!(
            idea("Remind me at 8 to go for a walk"),
            Some(("Go for a walk".into(), at(19, 8, 0)))
        );
    }

    #[test]
    fn hindi_reminders() {
        assert_eq!(
            idea("Kal subah 9 baje yaad dilana, domain kharidna hai"),
            Some(("Domain kharidna hai".into(), at(19, 9, 0)))
        );
        assert_eq!(
            idea("Mujhe shaam 7 baje yaad dila dena ki mummy ko call karna hai"),
            Some(("Mummy ko call karna hai".into(), at(18, 19, 0)))
        );
        assert_eq!(
            idea("20 minute baad yaad dilana chai banani hai"),
            Some(("Chai banani hai".into(), at(18, 10, 20)))
        );
        assert_eq!(
            idea("Parso saade 4 baje yaad dilana, bank jaana hai"),
            Some(("Bank jaana hai".into(), at(20, 16, 30)))
        );
    }

    #[test]
    fn to_dos_without_a_time() {
        assert_eq!(
            idea("Add a pop filter to my to-do list"),
            Some(("A pop filter".into(), None))
        );
        assert_eq!(
            idea("Don't forget to book the Goa tickets"),
            Some(("Book the Goa tickets".into(), None))
        );
        assert_eq!(
            idea("Remind me tomorrow at 5"),
            Some(("Reminder".into(), at(19, 17, 0)))
        );
    }

    #[test]
    fn ordinary_ideas_stay_ideas() {
        assert_eq!(idea("The onboarding should ask for the mic later"), None);
        assert_eq!(idea("Call 5 people about the launch"), None);
    }

    #[test]
    fn dictation_only_when_it_starts_like_a_reminder() {
        assert_eq!(dictation("Thanks, I'll remind them tomorrow"), None);
        assert_eq!(dictation("Add milk to my to-do list"), None);
        assert_eq!(dictation("I said remind me to call you"), None);
        assert_eq!(
            dictation("Remind me to call Priya tomorrow at 5"),
            Some(("Call Priya".into(), at(19, 17, 0)))
        );
        assert_eq!(
            dictation("Kal yaad dilana gym jaana hai"),
            Some(("Gym jaana hai".into(), at(19, 9, 0)))
        );
    }

    #[test]
    fn typed_to_dos_keep_their_words() {
        let typed = |t: &str| hear(t, Heard::Typed, now()).map(|s| (s.what, s.due));
        assert_eq!(
            typed("Pay rent on Friday"),
            Some(("Pay rent".into(), at(20, 9, 0)))
        );
        assert_eq!(typed("Buy milk"), Some(("Buy milk".into(), None)));
        assert_eq!(typed("   "), None);
    }
}
