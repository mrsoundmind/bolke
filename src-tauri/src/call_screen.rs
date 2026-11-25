//! Learns who's who from the call window, for the first minutes of a call.
//!
//! While a call is recorded, Bolke looks at the call window (Meet in a
//! browser, Zoom, Teams) twice a second and notes whose tile has the
//! "talking" ring (swift/call_screen.swift). Once everyone it has seen has
//! talked enough to be recognised, it stops looking: the voices are matched
//! to those names after the meeting and remembered from then on. Mac only
//! for now; elsewhere the browser add-on does this job.
//!
//! The looking happens in a small helper process (Bolke started with
//! [`WORKER_FLAG`]). macOS decides once per process whether it may see other
//! windows, so a Bolke that was open before Screen Recording was allowed
//! would never see them; a fresh helper does, without reopening Bolke.

use tauri::AppHandle;

pub const WORKER_FLAG: &str = "--call-screen-worker";

/// Whether this process was started as the call-window helper.
pub fn is_worker_invocation() -> bool {
    std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == WORKER_FLAG)
}

/// Runs the call-window helper: one line of JSON per look, on stdout, until
/// Bolke stops reading or Screen Recording isn't allowed.
pub fn run_worker() -> i32 {
    #[cfg(target_os = "macos")]
    return mac::run_worker();
    #[cfg(not(target_os = "macos"))]
    2
}

/// Starts learning names from the call window, for the meeting just started.
pub fn watch(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    mac::watch(app.clone());
    #[cfg(not(target_os = "macos"))]
    let _ = app;
}

#[cfg(target_os = "macos")]
mod mac {
    use std::collections::HashMap;
    use std::ffi::{c_char, CStr};
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Child, Command, Stdio};
    use std::time::{Duration, Instant};

    use log::{debug, info, warn};
    use serde::Deserialize;
    use tauri::{AppHandle, Emitter};

    use crate::call_names;
    use crate::notices::{self, Notice, NoticeCode, NoticeSource};

    extern "C" {
        fn bolke_call_frame() -> *mut c_char;
        fn bolke_free_string(s: *mut c_char);
        fn bolke_screen_access(ask: i32) -> i32;
    }

    const LOOK_EVERY: Duration = Duration::from_millis(500);
    /// Slower while no call window is to be found, to spare the battery.
    const LOOK_FOR_WINDOW_EVERY: Duration = Duration::from_secs(2);
    /// How often to check again while Screen Recording isn't allowed.
    const ASK_AGAIN: Duration = Duration::from_secs(5);
    /// A pause shorter than this is the same stretch of talking.
    const GAP_MS: i64 = 900;
    /// Looking stops once the call has been seen this long...
    const AT_MOST: Duration = Duration::from_secs(15 * 60);
    /// ...or, once it's been seen this long, when nobody new has shown up for
    /// a while and everyone seen has talked enough for their voice to be
    /// matched. Until then Bolke keeps looking for the call all meeting: it
    /// may only come into view now and then.
    const AT_LEAST: Duration = Duration::from_secs(4 * 60);
    const NOBODY_NEW: Duration = Duration::from_secs(3 * 60);
    const ENOUGH_TALK_MS: i64 = 15_000;
    /// Out of sight this long, the recording screen says names will come
    /// from voices (design/wireframes/MeetingStartEnd).
    const OUT_OF_SIGHT: Duration = Duration::from_secs(30);

    #[derive(Deserialize)]
    struct Frame {
        ok: bool,
        #[serde(default)]
        app: String,
        #[serde(default)]
        speaking: Vec<String>,
        #[serde(default)]
        error: String,
    }

    impl Frame {
        /// Looking can't work in this process (or on this Mac) at all.
        fn hopeless(&self) -> bool {
            !self.ok && (self.error == "no_permission" || self.error == "old_macos")
        }
    }

    /// One look, as the bridge's JSON text.
    fn look() -> String {
        // SAFETY: the bridge returns a strdup'd C string (or null), freed here.
        unsafe {
            let raw = bolke_call_frame();
            if raw.is_null() {
                return r#"{"ok":false,"error":"none"}"#.to_string();
            }
            let text = CStr::from_ptr(raw).to_string_lossy().into_owned();
            bolke_free_string(raw);
            text
        }
    }

    pub fn run_worker() -> i32 {
        let mut out = std::io::stdout().lock();
        loop {
            let text = look();
            // Bolke stopped reading: it's done with us, or gone.
            if writeln!(out, "{text}").and_then(|_| out.flush()).is_err() {
                return 0;
            }
            let frame = serde_json::from_str::<Frame>(&text).ok();
            if frame.as_ref().is_some_and(|f| f.hopeless()) {
                return 3;
            }
            let seen = frame.is_some_and(|f| f.ok);
            std::thread::sleep(if seen {
                LOOK_EVERY
            } else {
                LOOK_FOR_WINDOW_EVERY
            });
        }
    }

    fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    /// What has been seen so far in this meeting.
    struct Seen {
        /// How long the call window has been in sight.
        seen_for: Duration,
        last_new: Instant,
        app: String,
        /// name -> (start, last seen) while talking.
        open: HashMap<String, (i64, i64)>,
        talk: HashMap<String, i64>,
        people: Vec<String>,
        finished: Vec<(String, i64, i64)>,
        last_send: Instant,
        /// Since when the call window hasn't been found.
        unseen_since: Option<Instant>,
        /// The recording screen says the call is out of sight.
        said_unseen: bool,
    }

    impl Seen {
        fn new() -> Self {
            let now = Instant::now();
            Self {
                seen_for: Duration::ZERO,
                last_new: now,
                app: String::new(),
                open: HashMap::new(),
                talk: HashMap::new(),
                people: Vec::new(),
                finished: Vec::new(),
                last_send: now,
                unseen_since: None,
                said_unseen: false,
            }
        }

        /// Takes in one look. Returns why to stop looking, if it's time.
        fn take(&mut self, frame: Frame) -> Option<&'static str> {
            let now = now_ms();
            if frame.ok {
                self.seen_for += LOOK_EVERY;
                self.app = frame.app;
                for name in frame.speaking {
                    if !self.people.contains(&name) {
                        debug!("Saw a new person talking in the call window");
                        self.people.push(name.clone());
                        self.last_new = Instant::now();
                    }
                    let entry = self.open.entry(name).or_insert((now, now));
                    entry.1 = now;
                }
            }
            // Stretches that went quiet are finished.
            let (talk, finished) = (&mut self.talk, &mut self.finished);
            self.open.retain(|name, (start, last)| {
                if now - *last > GAP_MS {
                    *talk.entry(name.clone()).or_default() += *last - *start;
                    finished.push((name.clone(), *start, *last));
                    false
                } else {
                    true
                }
            });
            if self.last_send.elapsed() >= Duration::from_secs(2) && !self.finished.is_empty() {
                self.send();
            }
            self.time_to_stop()
        }

        fn time_to_stop(&self) -> Option<&'static str> {
            if self.seen_for >= AT_MOST {
                return Some("the call was seen for 15 minutes");
            }
            let everyone_heard = !self.people.is_empty()
                && self
                    .people
                    .iter()
                    .all(|p| self.talk.get(p).copied().unwrap_or(0) >= ENOUGH_TALK_MS);
            if self.seen_for >= AT_LEAST && self.last_new.elapsed() >= NOBODY_NEW && everyone_heard
            {
                return Some("everyone has been heard");
            }
            None
        }

        fn send(&mut self) {
            if !self.finished.is_empty() {
                call_names::add_from_screen(
                    &self.app,
                    self.people.clone(),
                    std::mem::take(&mut self.finished),
                );
            }
            self.last_send = Instant::now();
        }

        fn finish(mut self, why: &str) {
            let open: Vec<_> = self.open.drain().collect();
            for (name, (start, last)) in open {
                self.finished.push((name, start, last));
            }
            self.send();
            if self.people.is_empty() {
                warn!("Stopped looking at the call window ({why}); saw nobody talking");
            } else {
                info!(
                    "Stopped looking at the call window ({why}); saw {} people talking",
                    self.people.len()
                );
            }
        }
    }

    /// How a helper's run ended.
    enum Ended {
        Stop(&'static str),
        NotAllowed,
        Broke,
    }

    fn start_helper() -> std::io::Result<Child> {
        Command::new(std::env::current_exe()?)
            .arg(super::WORKER_FLAG)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
    }

    /// Reads one helper's looks until it's time to stop or the helper ends.
    fn follow(helper: &mut Child, seen: &mut Seen, app: &AppHandle, warned: &mut bool) -> Ended {
        let Some(stdout) = helper.stdout.take() else {
            return Ended::Broke;
        };
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if !call_names::wants_screen() {
                return Ended::Stop("the meeting ended or the add-on took over");
            }
            let Ok(frame) = serde_json::from_str::<Frame>(&line) else {
                continue;
            };
            if frame.hopeless() {
                return Ended::NotAllowed;
            }
            if *warned {
                notices::clear(app, NoticeSource::Meeting);
                *warned = false;
                info!("Screen Recording is allowed now; reading names from the call window");
            }
            // Tell the recording screen when the call stays out of sight.
            if frame.ok {
                seen.unseen_since = None;
                if seen.said_unseen {
                    seen.said_unseen = false;
                    let _ = app.emit("call-in-sight", true);
                }
            } else if frame.error == "no_call_window" {
                let since = *seen.unseen_since.get_or_insert_with(Instant::now);
                if !seen.said_unseen && since.elapsed() >= OUT_OF_SIGHT {
                    seen.said_unseen = true;
                    let _ = app.emit("call-in-sight", false);
                }
            }
            if let Some(why) = seen.take(frame) {
                return Ended::Stop(why);
            }
        }
        Ended::Broke
    }

    /// Waits up to `wait`, unless the meeting ends first.
    fn pause(wait: Duration) -> bool {
        let until = Instant::now() + wait;
        while Instant::now() < until {
            if !call_names::wants_screen() {
                return false;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        true
    }

    pub fn watch(app: AppHandle) {
        std::thread::spawn(move || {
            // Lets macOS ask the user, the first time only. What this process
            // is told doesn't matter: the helper finds out for itself.
            // SAFETY: plain C call into the Swift bridge.
            unsafe { bolke_screen_access(1) };
            let mut seen = Seen::new();
            let mut warned = false;
            let why = loop {
                if !call_names::wants_screen() {
                    break "the meeting ended or the add-on took over";
                }
                let mut helper = match start_helper() {
                    Ok(h) => h,
                    Err(e) => {
                        warn!("Couldn't start looking at the call window: {e}");
                        break "the helper wouldn't start";
                    }
                };
                let ended = follow(&mut helper, &mut seen, &app, &mut warned);
                let _ = helper.kill();
                let _ = helper.wait();
                match ended {
                    Ended::Stop(why) => break why,
                    Ended::NotAllowed => {
                        if !warned {
                            info!("Names from the call window wait for Screen Recording to be allowed");
                            notices::raise(
                                &app,
                                Notice::new(NoticeCode::CallNamesNeedScreen, NoticeSource::Meeting),
                            );
                            warned = true;
                        }
                        if !pause(ASK_AGAIN) {
                            break "the meeting ended";
                        }
                    }
                    Ended::Broke => {
                        if !pause(ASK_AGAIN) {
                            break "the meeting ended";
                        }
                    }
                }
            };
            seen.finish(why);
        });
    }
}
