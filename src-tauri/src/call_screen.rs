//! Learns who's who from the call window, for the first minutes of a call.
//!
//! While a call is recorded, Bolke looks at the call window (Meet in a
//! browser, Zoom, Teams) twice a second and notes whose tile has the
//! "talking" ring (swift/call_screen.swift). Once everyone it has seen has
//! talked enough to be recognised, it stops looking: the voices are matched
//! to those names after the meeting and remembered from then on. Mac only
//! for now; elsewhere the browser add-on does this job.

#[cfg(target_os = "macos")]
mod mac {
    use std::collections::HashMap;
    use std::ffi::{c_char, CStr};
    use std::time::{Duration, Instant};

    use log::{debug, info, warn};
    use serde::Deserialize;

    use crate::call_names;

    extern "C" {
        fn bolke_call_frame() -> *mut c_char;
        fn bolke_free_string(s: *mut c_char);
        fn bolke_screen_access(ask: i32) -> i32;
    }

    const LOOK_EVERY: Duration = Duration::from_millis(500);
    /// A pause shorter than this is the same stretch of talking.
    const GAP_MS: i64 = 900;
    /// Looking stops after this long whatever happens...
    const AT_MOST: Duration = Duration::from_secs(15 * 60);
    /// ...or, after this long, once nobody new has shown up for a while and
    /// everyone seen has talked enough for their voice to be matched.
    const AT_LEAST: Duration = Duration::from_secs(4 * 60);
    const NOBODY_NEW: Duration = Duration::from_secs(3 * 60);
    const ENOUGH_TALK_MS: i64 = 15_000;
    /// The window must stay out of view this long before Bolke gives up.
    const NO_WINDOW_FOR: Duration = Duration::from_secs(5 * 60);

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

    fn frame() -> Option<Frame> {
        // SAFETY: the bridge returns a strdup'd C string (or null), freed here.
        unsafe {
            let raw = bolke_call_frame();
            if raw.is_null() {
                return None;
            }
            let text = CStr::from_ptr(raw).to_string_lossy().into_owned();
            bolke_free_string(raw);
            serde_json::from_str(&text).ok()
        }
    }

    fn now_ms() -> i64 {
        chrono::Utc::now().timestamp_millis()
    }

    pub fn watch() {
        std::thread::spawn(|| {
            // SAFETY: plain C call into the Swift bridge.
            if unsafe { bolke_screen_access(1) } == 0 {
                info!("Names from the call window are off: Screen Recording isn't allowed");
                return;
            }
            let started = Instant::now();
            let mut last_new = Instant::now();
            let mut last_window = Instant::now();
            let mut app = String::new();
            // name -> (start, last seen) while talking.
            let mut open: HashMap<String, (i64, i64)> = HashMap::new();
            let mut talk: HashMap<String, i64> = HashMap::new();
            let mut people: Vec<String> = Vec::new();
            let mut finished: Vec<(String, i64, i64)> = Vec::new();
            let mut last_send = Instant::now();

            let why = loop {
                std::thread::sleep(LOOK_EVERY);
                if !call_names::wants_screen() {
                    break "the meeting ended or the add-on took over";
                }
                let now = now_ms();
                match frame() {
                    Some(f) if f.ok => {
                        last_window = Instant::now();
                        app = f.app;
                        for name in f.speaking {
                            if !people.contains(&name) {
                                debug!("Saw a new person talking in the call window");
                                people.push(name.clone());
                                last_new = Instant::now();
                            }
                            let entry = open.entry(name).or_insert((now, now));
                            entry.1 = now;
                        }
                    }
                    Some(f) if f.error == "no_permission" => {
                        break "Screen Recording was turned off"
                    }
                    _ => {}
                }
                // Stretches that went quiet are finished.
                open.retain(|name, (start, last)| {
                    if now - *last > GAP_MS {
                        *talk.entry(name.clone()).or_default() += *last - *start;
                        finished.push((name.clone(), *start, *last));
                        false
                    } else {
                        true
                    }
                });
                if last_send.elapsed() >= Duration::from_secs(2) && !finished.is_empty() {
                    call_names::add_from_screen(
                        &app,
                        people.clone(),
                        std::mem::take(&mut finished),
                    );
                    last_send = Instant::now();
                }

                let elapsed = started.elapsed();
                if elapsed >= AT_MOST {
                    break "15 minutes passed";
                }
                if last_window.elapsed() >= NO_WINDOW_FOR {
                    break "no call window in view";
                }
                let everyone_heard = !people.is_empty()
                    && people
                        .iter()
                        .all(|p| talk.get(p).copied().unwrap_or(0) >= ENOUGH_TALK_MS);
                if elapsed >= AT_LEAST && last_new.elapsed() >= NOBODY_NEW && everyone_heard {
                    break "everyone has been heard";
                }
            };
            for (name, (start, last)) in open.drain() {
                finished.push((name, start, last));
            }
            if !finished.is_empty() {
                call_names::add_from_screen(&app, people.clone(), finished);
            }
            if people.is_empty() {
                warn!("Stopped looking at the call window ({why}); saw nobody talking");
            } else {
                info!(
                    "Stopped looking at the call window ({why}); saw {} people talking",
                    people.len()
                );
            }
        });
    }
}

/// Starts learning names from the call window, for the meeting just started.
pub fn watch() {
    #[cfg(target_os = "macos")]
    mac::watch();
}
