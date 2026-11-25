//! Notices a call starting and ending: when a call app or a browser starts
//! using the microphone, Bolke offers to record it (design/wireframes/
//! MeetingCapture, "Record this meeting?"), saying where the call is
//! (call_place.rs). "Not now" keeps it quiet until that app stops using the
//! microphone. When the call being recorded ends, Bolke asks "Meeting
//! ended?" (design/wireframes/MeetingStartEnd).
//!
//! Only which apps use the microphone is read, never what they hear.

use log::{debug, info};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Manager};

use crate::call_place::{self, CallPlace};
use crate::managers::meeting::MeetingRecorder;

/// How often the microphone's users are checked.
const POLL: Duration = Duration::from_secs(3);
/// An app has to keep the microphone this many checks in a row before Bolke
/// asks, so a quick voice note or a test of the microphone doesn't.
const POLLS_BEFORE_ASKING: u32 = 2;
/// The app has to let go of the microphone for this many checks before the
/// call counts as over (and a new one may be asked about).
const POLLS_BEFORE_OVER: u32 = 2;
/// While the call's tab is still open on its call page (say, Meet's "You
/// left the meeting"), the end of a recorded call waits this many checks.
const POLLS_WHILE_TAB_OPEN: u32 = 10;

/// Call apps and browsers, matched by lower-case parts of what the system
/// calls them: a Mac bundle id, a Windows program or package, a Linux binary.
const KNOWN_APPS: &[(&[&str], &str)] = &[
    (&["zoom"], "Zoom"),
    (&["teams"], "Microsoft Teams"),
    (&["slack"], "Slack"),
    (&["webex", "cisco"], "Webex"),
    (&["facetime"], "FaceTime"),
    (&["discord"], "Discord"),
    (&["skype"], "Skype"),
    (&["whatsapp"], "WhatsApp"),
    (&["telegram"], "Telegram"),
    (&["signal"], "Signal"),
    (&["google.chrome", "chrome"], "Chrome"),
    (&["microsoft.edge", "msedge"], "Edge"),
    (&["firefox", "mozilla"], "Firefox"),
    (&["thebrowser.dia", "\\dia.exe"], "Dia"),
    (&["thebrowser", "\\arc.exe"], "Arc"),
    (&["perplexity.comet", "\\comet.exe"], "Comet"),
    (&["openai.atlas"], "ChatGPT Atlas"),
    (&["kagi"], "Orion"),
    (&["zen-browser", "zen.exe"], "Zen"),
    (&["chromium"], "Chromium"),
    (&["brave"], "Brave"),
    (&["vivaldi"], "Vivaldi"),
    (&["opera"], "Opera"),
    (&["safari", "com.apple.webkit"], "Safari"),
];

/// Bolke itself, and the tools it records a call's sound with.
const OURS: &[&str] = &["bolke", "parec", "pw-record"];

/// The name to show for an app using the microphone, if it's one that holds
/// calls.
fn call_app_name(id: &str) -> Option<&'static str> {
    let id = id.to_lowercase();
    if OURS.iter().any(|o| id.contains(o)) {
        return None;
    }
    KNOWN_APPS
        .iter()
        .find(|(parts, _)| parts.iter().any(|p| id.contains(p)))
        .map(|(_, name)| *name)
}

/// The first call app using the microphone now: what the system calls it,
/// and its name.
fn call_app_now() -> Option<(String, &'static str)> {
    imp::apps_using_mic()
        .into_iter()
        .find_map(|id| call_app_name(&id).map(|name| (id, name)))
}

/// What to do after a check.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Nothing,
    Ask(&'static str),
    /// The call ended with the question still up.
    Withdraw,
    /// The call being recorded ended: ask "Meeting ended?".
    Ended,
}

#[derive(Default)]
struct Watch {
    app: Option<&'static str>,
    polls: u32,
    gone: u32,
    /// Asked about, recorded or "Not now" for this call.
    handled: bool,
    /// This call was going while a meeting recorded.
    recorded: bool,
    /// "Meeting ended?" was asked for the meeting recording now.
    end_asked: bool,
}

impl Watch {
    /// `busy`: a meeting is recording, so this call is handled already.
    /// `held`: the overlay is busy with something else; ask after it.
    /// `tab_open`: the call's tab is still on its call page.
    fn step(&mut self, now: Option<&'static str>, busy: bool, held: bool, tab_open: bool) -> Step {
        if !busy {
            self.end_asked = false;
        }
        let Some(app) = now else {
            self.gone += 1;
            let needed = if tab_open {
                POLLS_WHILE_TAB_OPEN
            } else {
                POLLS_BEFORE_OVER
            };
            if self.gone >= needed && self.app.is_some() {
                let asked = self.handled && !busy;
                let ended = busy && self.recorded && !self.end_asked;
                *self = Watch {
                    end_asked: self.end_asked || ended,
                    ..Watch::default()
                };
                if ended {
                    return Step::Ended;
                }
                if asked {
                    return Step::Withdraw;
                }
            }
            return Step::Nothing;
        };
        self.gone = 0;
        if self.app != Some(app) {
            *self = Watch {
                app: Some(app),
                end_asked: self.end_asked,
                ..Watch::default()
            };
        }
        self.polls += 1;
        if busy {
            self.handled = true;
            self.recorded = true;
        }
        if self.handled || held || self.polls < POLLS_BEFORE_ASKING {
            return Step::Nothing;
        }
        self.handled = true;
        Step::Ask(app)
    }
}

/// Starts checking in the background, for as long as Bolke runs.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::Builder::new()
        .name("call-watch".into())
        .spawn(move || {
            let mut watch = Watch::default();
            // The call app as the system calls it, and where its call is.
            let mut id = String::new();
            let mut place: Option<CallPlace> = None;
            loop {
                std::thread::sleep(POLL);
                let meeting = app.state::<Arc<MeetingRecorder>>();
                let status = meeting.status();
                let busy = status.recording;
                let held = crate::overlay::is_session_showing()
                    || crate::overlay::is_reminder_showing()
                    || crate::overlay::is_call_end_showing();
                let found = call_app_now();
                if let Some((now_id, name)) = &found {
                    if *now_id != id {
                        id = now_id.clone();
                        place = None;
                    }
                    // A meeting started by hand during a call: learn where.
                    if busy && place.is_none() {
                        place = Some(call_place::find(&id, name));
                    }
                }
                let tab_open = found.is_none()
                    && busy
                    && place
                        .as_ref()
                        .and_then(|p| call_place::tab_still_open(&id, p))
                        .unwrap_or(false);
                match watch.step(found.as_ref().map(|f| f.1), busy, held, tab_open) {
                    Step::Nothing => {}
                    Step::Ask(name) => {
                        let here = call_place::find(&id, name);
                        info!(
                            "{name} started using the microphone ({}); offering to record",
                            here.service.as_deref().unwrap_or("no known service")
                        );
                        crate::overlay::show_call_prompt(&app, &here);
                        place = Some(here);
                    }
                    Step::Withdraw => {
                        debug!("The call ended before an answer");
                        crate::overlay::hide_call_prompt(&app);
                        place = None;
                    }
                    Step::Ended => {
                        let here = place.take().unwrap_or_default();
                        info!("The recorded call ended; asking whether the meeting did");
                        crate::overlay::show_call_end(&app, &here, status.elapsed_ms);
                    }
                }
            }
        })
        .ok();
}

#[cfg(target_os = "macos")]
mod imp {
    //! Core Audio's process objects (macOS 14+): each app using audio, and
    //! whether it's taking input now.
    use objc2::rc::Retained;
    use objc2_core_audio::{
        kAudioHardwarePropertyProcessObjectList, kAudioObjectPropertyElementMain,
        kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, kAudioProcessPropertyBundleID,
        kAudioProcessPropertyIsRunningInput, AudioObjectGetPropertyData,
        AudioObjectGetPropertyDataSize, AudioObjectID, AudioObjectPropertyAddress,
    };
    use objc2_foundation::NSString;
    use std::ffi::c_void;
    use std::ptr::NonNull;

    fn address(selector: u32) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        }
    }

    /// Reads a fixed-size property into `out`; false if it can't.
    fn read<T>(object: AudioObjectID, selector: u32, out: &mut T) -> bool {
        let address = address(selector);
        let mut size = std::mem::size_of::<T>() as u32;
        // SAFETY: `out` is a valid, writable `T` of `size` bytes.
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                NonNull::from(&address),
                0,
                std::ptr::null(),
                NonNull::from(&mut size),
                NonNull::from(out).cast::<c_void>(),
            )
        };
        status == 0
    }

    fn processes() -> Vec<AudioObjectID> {
        let system = kAudioObjectSystemObject as AudioObjectID;
        let address = address(kAudioHardwarePropertyProcessObjectList);
        let mut size = 0u32;
        // SAFETY: only writes the size.
        let status = unsafe {
            AudioObjectGetPropertyDataSize(
                system,
                NonNull::from(&address),
                0,
                std::ptr::null(),
                NonNull::from(&mut size),
            )
        };
        let each = std::mem::size_of::<AudioObjectID>();
        if status != 0 || size == 0 {
            return Vec::new();
        }
        let mut ids: Vec<AudioObjectID> = vec![0; size as usize / each];
        // SAFETY: `ids` holds `size` bytes.
        let status = unsafe {
            AudioObjectGetPropertyData(
                system,
                NonNull::from(&address),
                0,
                std::ptr::null(),
                NonNull::from(&mut size),
                NonNull::new_unchecked(ids.as_mut_ptr().cast::<c_void>()),
            )
        };
        if status != 0 {
            return Vec::new();
        }
        ids.truncate(size as usize / each);
        ids
    }

    fn bundle_id(process: AudioObjectID) -> Option<String> {
        let mut value: *mut c_void = std::ptr::null_mut();
        if !read(process, kAudioProcessPropertyBundleID, &mut value) || value.is_null() {
            return None;
        }
        // SAFETY: the property is a CFString we own (+1); CFString and
        // NSString are toll-free bridged, and `Retained` releases it.
        let id = unsafe { Retained::from_raw(value.cast::<NSString>()) }?;
        Some(id.to_string())
    }

    #[cfg(test)]
    pub fn all_apps() -> Vec<(String, bool)> {
        processes()
            .into_iter()
            .filter_map(|p| {
                let mut running: u32 = 0;
                read(p, kAudioProcessPropertyIsRunningInput, &mut running);
                Some((bundle_id(p)?, running != 0))
            })
            .collect()
    }

    pub fn apps_using_mic() -> Vec<String> {
        processes()
            .into_iter()
            .filter(|&p| {
                let mut running: u32 = 0;
                read(p, kAudioProcessPropertyIsRunningInput, &mut running) && running != 0
            })
            .filter_map(bundle_id)
            .collect()
    }
}

#[cfg(windows)]
mod imp {
    //! Windows' privacy settings keep, for each app allowed the microphone,
    //! when it last started and stopped using it. Stopped = 0 means now.
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};

    const MICROPHONE: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

    fn in_use(key: &RegKey) -> bool {
        let start: u64 = key.get_value("LastUsedTimeStart").unwrap_or(0);
        let stop: u64 = key.get_value("LastUsedTimeStop").unwrap_or(1);
        start > 0 && stop == 0
    }

    pub fn apps_using_mic() -> Vec<String> {
        let Ok(root) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(MICROPHONE) else {
            return Vec::new();
        };
        let mut apps = Vec::new();
        for name in root.enum_keys().flatten() {
            let Ok(key) = root.open_subkey(&name) else {
                continue;
            };
            if name == "NonPackaged" {
                // Desktop programs, by path with `#` for `\`.
                for program in key.enum_keys().flatten() {
                    if key.open_subkey(&program).is_ok_and(|k| in_use(&k)) {
                        apps.push(program.replace('#', "\\"));
                    }
                }
            } else if in_use(&key) {
                apps.push(name);
            }
        }
        apps
    }
}

#[cfg(target_os = "linux")]
mod imp {
    //! PulseAudio (or PipeWire's stand-in for it) lists every recording
    //! stream with the program behind it.
    use std::process::Command;

    pub fn apps_using_mic() -> Vec<String> {
        let Ok(out) = Command::new("pactl")
            .args(["list", "source-outputs"])
            .env("LC_ALL", "C")
            .output()
        else {
            return Vec::new();
        };
        super::programs_in_pactl(&String::from_utf8_lossy(&out.stdout))
    }
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
mod imp {
    pub fn apps_using_mic() -> Vec<String> {
        Vec::new()
    }
}

/// The program behind each stream in `pactl list source-outputs`.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn programs_in_pactl(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let value = line
                .strip_prefix("application.process.binary = ")
                .or_else(|| line.strip_prefix("application.name = "))?;
            Some(value.trim_matches('"').to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn call_apps_are_named_and_bolke_is_not() {
        assert_eq!(call_app_name("us.zoom.xos"), Some("Zoom"));
        assert_eq!(call_app_name("company.thebrowser.dia"), Some("Dia"));
        assert_eq!(call_app_name("company.thebrowser.Browser"), Some("Arc"));
        assert_eq!(
            call_app_name("com.microsoft.teams2"),
            Some("Microsoft Teams")
        );
        assert_eq!(
            call_app_name(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
            Some("Chrome")
        );
        assert_eq!(call_app_name("com.google.Chrome.helper"), Some("Chrome"));
        assert_eq!(
            call_app_name("MSTeams_8wekyb3d8bbwe"),
            Some("Microsoft Teams")
        );
        assert_eq!(call_app_name("app.bolke.desktop"), None);
        assert_eq!(call_app_name("parec"), None);
        assert_eq!(call_app_name("com.apple.VoiceMemos"), None);
    }

    #[test]
    fn asks_once_per_call_after_two_checks() {
        let mut w = Watch::default();
        assert_eq!(w.step(Some("Zoom"), false, false, false), Step::Nothing);
        assert_eq!(w.step(Some("Zoom"), false, false, false), Step::Ask("Zoom"));
        assert_eq!(w.step(Some("Zoom"), false, false, false), Step::Nothing);
        // One missed check isn't the end of the call.
        assert_eq!(w.step(None, false, false, false), Step::Nothing);
        assert_eq!(w.step(Some("Zoom"), false, false, false), Step::Nothing);
        assert_eq!(w.step(None, false, false, false), Step::Nothing);
        assert_eq!(w.step(None, false, false, false), Step::Withdraw);
        // A new call is asked about again.
        assert_eq!(w.step(Some("Zoom"), false, false, false), Step::Nothing);
        assert_eq!(w.step(Some("Zoom"), false, false, false), Step::Ask("Zoom"));
    }

    #[test]
    fn waits_for_the_overlay_and_skips_a_recorded_call() {
        let mut w = Watch::default();
        w.step(Some("Chrome"), false, true, false);
        assert_eq!(w.step(Some("Chrome"), false, true, false), Step::Nothing);
        assert_eq!(
            w.step(Some("Chrome"), false, false, false),
            Step::Ask("Chrome")
        );

        let mut w = Watch::default();
        w.step(Some("Zoom"), true, false, false);
        w.step(Some("Zoom"), true, false, false);
        // The meeting stopped while the call goes on: no question.
        assert_eq!(w.step(Some("Zoom"), false, false, false), Step::Nothing);
    }

    #[test]
    fn asks_once_when_the_recorded_call_ends() {
        let mut w = Watch::default();
        w.step(Some("Dia"), false, false, false);
        assert_eq!(w.step(Some("Dia"), false, false, false), Step::Ask("Dia"));
        // Recorded from here on.
        assert_eq!(w.step(Some("Dia"), true, false, false), Step::Nothing);
        assert_eq!(w.step(None, true, false, false), Step::Nothing);
        assert_eq!(w.step(None, true, false, false), Step::Ended);
        // "Keep going": not asked again while that meeting records.
        w.step(Some("Dia"), true, false, false);
        w.step(None, true, false, false);
        assert_eq!(w.step(None, true, false, false), Step::Nothing);
        // A new recording asks again.
        w.step(None, false, false, false);
        w.step(Some("Dia"), true, false, false);
        w.step(None, true, false, false);
        assert_eq!(w.step(None, true, false, false), Step::Ended);
    }

    #[test]
    fn waits_longer_while_the_call_tab_is_open() {
        let mut w = Watch::default();
        w.step(Some("Dia"), true, false, false);
        for _ in 1..POLLS_WHILE_TAB_OPEN {
            assert_eq!(w.step(None, true, false, true), Step::Nothing);
        }
        assert_eq!(w.step(None, true, false, true), Step::Ended);
    }

    #[test]
    fn a_meeting_without_a_call_is_never_asked_about() {
        let mut w = Watch::default();
        for _ in 0..5 {
            assert_eq!(w.step(None, true, false, false), Step::Nothing);
        }
    }

    #[test]
    fn pactl_streams_give_their_programs() {
        let text = "Source Output #12\n\tProperties:\n\t\tapplication.name = \"ZOOM VoiceEngine\"\n\t\tapplication.process.binary = \"zoom\"\n";
        assert_eq!(programs_in_pactl(text), vec!["ZOOM VoiceEngine", "zoom"]);
    }
}

/// Prints what's using the microphone now:
/// `cargo test --lib call_watch::live -- --ignored --nocapture`
#[cfg(test)]
mod live {
    #[test]
    #[ignore]
    fn shows_apps_using_the_microphone() {
        // Hold the microphone ourselves, to see it counted.
        #[cfg(target_os = "macos")]
        let _mic = {
            use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
            let device = cpal::default_host().default_input_device().unwrap();
            let config = device.default_input_config().unwrap();
            let stream = device
                .build_input_stream(
                    config.config(),
                    |_: &[f32], _: &_| {},
                    |e| eprintln!("{e}"),
                    None,
                )
                .unwrap();
            stream.play().unwrap();
            std::thread::sleep(std::time::Duration::from_secs(2));
            stream
        };
        let apps = super::imp::apps_using_mic();
        println!("apps using the microphone: {apps:?}");
        #[cfg(target_os = "macos")]
        println!("all audio apps: {:?}", super::imp::all_apps());
        println!("call app: {:?}", super::call_app_now());
    }
}
