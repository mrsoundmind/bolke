//! Where a call is (design/wireframes/MeetingStartEnd.dc.html): the service
//! (Google Meet, Zoom, Teams…), the app or browser it's in, and, in a
//! browser, the tab and its title.
//!
//! Only the browser's tab addresses and titles are read, never a page's
//! contents. On a Mac the browser is asked through Apple Events (macOS asks
//! the user once per browser); on Windows and Linux the browser's window
//! titles name the tab that's showing.

use serde::Serialize;
use specta::Type;

/// Where a call is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Type)]
pub struct CallPlace {
    /// The app holding the microphone: "Dia", "Zoom".
    pub app: String,
    /// The meeting service, when Bolke can tell: "Google Meet".
    pub service: Option<String>,
    /// The meeting's own name from the tab or window, when it has one.
    pub title: Option<String>,
    /// The call tab's address, in a browser that tells it.
    #[serde(skip)]
    pub tab: Option<String>,
}

/// Desktop apps that are a service in themselves.
const SERVICE_APPS: &[&str] = &[
    "Zoom",
    "Microsoft Teams",
    "Slack",
    "Webex",
    "FaceTime",
    "Discord",
    "Skype",
    "WhatsApp",
    "Telegram",
    "Signal",
];

/// Meeting services by the address of their call pages.
const SERVICE_HOSTS: &[(&str, &str)] = &[
    ("meet.google.com", "Google Meet"),
    ("zoom.us", "Zoom"),
    ("zoom.com", "Zoom"),
    ("teams.microsoft.com", "Microsoft Teams"),
    ("teams.live.com", "Microsoft Teams"),
    ("teams.cloud.microsoft", "Microsoft Teams"),
    ("webex.com", "Webex"),
    ("app.slack.com", "Slack huddle"),
    ("discord.com", "Discord"),
    ("web.whatsapp.com", "WhatsApp"),
    ("meet.jit.si", "Jitsi"),
    ("8x8.vc", "Jitsi"),
    ("whereby.com", "Whereby"),
    ("app.goto.com", "GoTo Meeting"),
    ("gotomeeting.com", "GoTo Meeting"),
    ("meeting.zoho", "Zoho Meeting"),
    ("around.co", "Around"),
    ("gather.town", "Gather"),
    ("riverside.fm", "Riverside"),
    ("streamyard.com", "StreamYard"),
    ("web.telegram.org", "Telegram"),
    ("web.skype.com", "Skype"),
    ("facetime.apple.com", "FaceTime"),
    ("chime.aws", "Amazon Chime"),
    ("bluejeans.com", "BlueJeans"),
];

/// Meeting services by words in a window or tab title, for when there's no
/// address to go by (Windows, Linux, Firefox).
const SERVICE_TITLES: &[(&str, &str)] = &[
    ("meet - ", "Google Meet"),
    ("meet – ", "Google Meet"),
    ("google meet", "Google Meet"),
    ("microsoft teams", "Microsoft Teams"),
    ("zoom meeting", "Zoom"),
    ("zoom workplace", "Zoom"),
    ("webex", "Webex"),
    ("huddle", "Slack huddle"),
    ("discord", "Discord"),
    ("whatsapp", "WhatsApp"),
    ("jitsi meet", "Jitsi"),
    ("whereby", "Whereby"),
    ("goto meeting", "GoTo Meeting"),
    ("zoho meeting", "Zoho Meeting"),
    ("riverside", "Riverside"),
    ("streamyard", "StreamYard"),
    ("amazon chime", "Amazon Chime"),
];

/// Browser names a window title can end with ("… - Google Chrome").
const BROWSER_SUFFIXES: &[&str] = &[
    "google chrome",
    "chrome",
    "microsoft edge",
    "edge",
    "mozilla firefox",
    "firefox",
    "brave",
    "vivaldi",
    "opera",
    "chromium",
    "arc",
    "dia",
    "safari",
    "comet",
    "zen browser",
];

/// The service a page address belongs to.
pub fn service_from_url(url: &str) -> Option<&'static str> {
    let rest = url.split("://").nth(1)?;
    let host = rest.split(['/', '?', '#']).next()?.to_lowercase();
    let host = host.split(':').next().unwrap_or("");
    SERVICE_HOSTS
        .iter()
        .find(|(h, _)| host == *h || host.ends_with(&format!(".{h}")))
        .map(|(_, s)| *s)
}

/// The service a window or tab title names.
pub fn service_from_title(title: &str) -> Option<&'static str> {
    let lower = title.to_lowercase();
    SERVICE_TITLES
        .iter()
        .find(|(w, _)| lower.starts_with(w) || lower.contains(&format!(" {w}")))
        .map(|(_, s)| *s)
}

/// The meeting's own name in a tab or window title, without the service,
/// the browser or a bare meeting code.
pub fn meeting_name(title: &str, service: Option<&str>) -> Option<String> {
    let mut name = title.trim().to_string();
    // "(3) Meet – …": a count of unread messages.
    if name.starts_with('(') {
        if let Some(end) = name.find(") ") {
            if name[1..end].chars().all(|c| c.is_ascii_digit()) {
                name = name[end + 2..].to_string();
            }
        }
    }
    let separators = [" - ", " – ", " — ", " | "];
    // Browser and service names at either end.
    let mut drop: Vec<String> = BROWSER_SUFFIXES.iter().map(|s| s.to_string()).collect();
    if let Some(s) = service {
        drop.push(s.to_lowercase());
    }
    drop.extend(["meet", "zoom", "teams", "microsoft teams"].map(String::from));
    loop {
        let before = name.clone();
        for sep in separators {
            if let Some((head, tail)) = name.rsplit_once(sep) {
                if drop.contains(&tail.trim().to_lowercase()) {
                    name = head.trim().to_string();
                }
            }
            if let Some((head, tail)) = name.split_once(sep) {
                if drop.contains(&head.trim().to_lowercase()) {
                    name = tail.trim().to_string();
                }
            }
        }
        if name == before {
            break;
        }
    }
    let lower = name.to_lowercase();
    let generic = [
        "",
        "meet",
        "google meet",
        "zoom",
        "zoom meeting",
        "zoom workplace",
        "microsoft teams",
        "meeting",
        "calls",
        "call",
        "webex",
        "slack",
        "huddle",
        "discord",
        "whatsapp",
    ];
    if generic.contains(&lower.as_str()) || drop.contains(&lower) || is_meeting_code(&lower) {
        return None;
    }
    Some(name)
}

/// "abc-defg-hij": a Meet code, not a name.
fn is_meeting_code(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    parts.len() == 3
        && parts.iter().map(|p| p.len()).collect::<Vec<_>>() == [3, 4, 3]
        && s.chars().all(|c| c.is_ascii_lowercase() || c == '-')
}

/// One tab, or window title, of the browser holding the call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tab {
    pub url: Option<String>,
    pub title: String,
}

/// Picks the call among a browser's tabs: the first one on a meeting
/// service.
pub fn place_in_tabs(app: &str, tabs: &[Tab]) -> CallPlace {
    for tab in tabs {
        let service = tab
            .url
            .as_deref()
            .and_then(service_from_url)
            .or_else(|| service_from_title(&tab.title));
        if let Some(service) = service {
            return CallPlace {
                app: app.to_string(),
                service: Some(service.to_string()),
                title: meeting_name(&tab.title, Some(service)),
                tab: tab.url.clone(),
            };
        }
    }
    CallPlace {
        app: app.to_string(),
        ..CallPlace::default()
    }
}

/// Where the call held by `app` (named by call_watch; `id` is what the
/// system calls it) is now.
pub fn find(id: &str, app: &str) -> CallPlace {
    if SERVICE_APPS.contains(&app) {
        return CallPlace {
            app: app.to_string(),
            service: Some(app.to_string()),
            ..CallPlace::default()
        };
    }
    place_in_tabs(app, &imp::tabs(id))
}

/// Whether the call's tab is still open on its call page.
pub fn tab_still_open(id: &str, place: &CallPlace) -> Option<bool> {
    let url = place.tab.as_deref()?;
    let tabs = imp::tabs(id);
    if tabs.is_empty() {
        // The browser quit, or said nothing.
        return Some(false);
    }
    Some(tabs.iter().any(|t| t.url.as_deref() == Some(url)))
}

#[cfg(target_os = "macos")]
mod imp {
    //! Asks the browser for its tabs through Apple Events.
    use super::Tab;
    use std::process::Command;

    /// Safari names its tabs and current tab differently.
    fn script(id: &str) -> String {
        let (title, _) = if id.starts_with("com.apple.Safari") {
            ("name", "current tab")
        } else {
            ("title", "active tab")
        };
        format!(
            r#"tell application id "{id}"
  set out to ""
  repeat with w in windows
    try
      repeat with t in tabs of w
        set out to out & (URL of t) & tab & ({title} of t) & linefeed
      end repeat
    end try
  end repeat
  return out
end tell"#
        )
    }

    /// Browsers that answer Apple Events about their tabs.
    fn scriptable(id: &str) -> bool {
        let id = id.to_lowercase();
        [
            "com.google.chrome",
            "com.microsoft.edgemac",
            "com.brave.browser",
            "company.thebrowser.",
            "com.vivaldi.vivaldi",
            "com.operasoftware.opera",
            "org.chromium.chromium",
            "ai.perplexity.comet",
            "com.apple.safari",
        ]
        .iter()
        .any(|p| id.starts_with(p))
    }

    pub fn tabs(id: &str) -> Vec<Tab> {
        // Only a browser that's running (it holds the microphone), so asking
        // never opens one.
        if !scriptable(id) || id.contains('"') {
            return Vec::new();
        }
        let Ok(out) = Command::new("/usr/bin/osascript")
            .args(["-e", &script(id)])
            .output()
        else {
            return Vec::new();
        };
        if !out.status.success() {
            log::debug!(
                "The browser didn't list its tabs: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            return Vec::new();
        }
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| {
                let (url, title) = line.split_once('\t')?;
                Some(Tab {
                    url: Some(url.to_string()),
                    title: title.to_string(),
                })
            })
            .collect()
    }
}

#[cfg(windows)]
mod imp {
    //! The titles of the browser's windows: each names its showing tab.
    use super::Tab;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    };

    struct Search {
        program: String,
        titles: Vec<String>,
    }

    fn program_of(window: HWND) -> Option<String> {
        let mut pid = 0u32;
        // SAFETY: plain Win32 calls on a window handle from EnumWindows.
        unsafe {
            GetWindowThreadProcessId(window, Some(&mut pid));
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut len,
            )
            .is_ok();
            let _ = CloseHandle(process);
            ok.then(|| String::from_utf16_lossy(&buf[..len as usize]))
        }
    }

    unsafe extern "system" fn each(window: HWND, data: LPARAM) -> BOOL {
        // SAFETY: `data` is the `Search` passed to EnumWindows below.
        let search = unsafe { &mut *(data.0 as *mut Search) };
        // SAFETY: plain Win32 calls on a window handle from EnumWindows.
        if unsafe { IsWindowVisible(window) }.as_bool() {
            if program_of(window).is_some_and(|p| p.eq_ignore_ascii_case(&search.program)) {
                let mut buf = [0u16; 512];
                // SAFETY: as above.
                let len = unsafe { GetWindowTextW(window, &mut buf) };
                if len > 0 {
                    search
                        .titles
                        .push(String::from_utf16_lossy(&buf[..len as usize]));
                }
            }
        }
        BOOL(1)
    }

    pub fn tabs(id: &str) -> Vec<Tab> {
        // Desktop programs come as their full path.
        if !id.contains('\\') {
            return Vec::new();
        }
        let mut search = Search {
            program: id.to_string(),
            titles: Vec::new(),
        };
        // SAFETY: `search` outlives the enumeration, which is synchronous.
        let _ = unsafe { EnumWindows(Some(each), LPARAM(&mut search as *mut Search as isize)) };
        search
            .titles
            .into_iter()
            .map(|title| Tab { url: None, title })
            .collect()
    }
}

#[cfg(target_os = "linux")]
mod imp {
    //! The titles of the browser's windows, from the X server (xprop), on
    //! desktops that have it: each names its showing tab.
    use super::Tab;
    use std::process::Command;

    fn xprop(args: &[&str]) -> Option<String> {
        let out = Command::new("xprop").args(args).output().ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    }

    pub fn tabs(id: &str) -> Vec<Tab> {
        let program = id.rsplit('/').next().unwrap_or(id).to_lowercase();
        let Some(list) = xprop(&["-root", "_NET_CLIENT_LIST"]) else {
            return Vec::new();
        };
        let windows: Vec<&str> = list
            .split(['#', ','])
            .skip(1)
            .map(str::trim)
            .filter(|w| w.starts_with("0x"))
            .collect();
        let mut tabs = Vec::new();
        for w in windows {
            let Some(info) = xprop(&["-id", w, "WM_CLASS", "_NET_WM_NAME"]) else {
                continue;
            };
            let class = info
                .lines()
                .find(|l| l.starts_with("WM_CLASS"))
                .unwrap_or("")
                .to_lowercase();
            let first = program.split(['-', '.']).next().unwrap_or(&program);
            if first.is_empty() || !class.contains(first) {
                continue;
            }
            if let Some(title) = info
                .lines()
                .find(|l| l.starts_with("_NET_WM_NAME"))
                .and_then(|l| l.split_once(" = "))
                .map(|(_, v)| v.trim().trim_matches('"').to_string())
            {
                tabs.push(Tab { url: None, title });
            }
        }
        tabs
    }
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
mod imp {
    pub fn tabs(_id: &str) -> Vec<super::Tab> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(url: &str, title: &str) -> Tab {
        Tab {
            url: Some(url.into()),
            title: title.into(),
        }
    }

    #[test]
    fn services_come_from_addresses() {
        assert_eq!(
            service_from_url("https://meet.google.com/abc-defg-hij?authuser=0"),
            Some("Google Meet")
        );
        assert_eq!(
            service_from_url("https://us02web.zoom.us/wc/123/join"),
            Some("Zoom")
        );
        assert_eq!(
            service_from_url("https://teams.microsoft.com/v2/"),
            Some("Microsoft Teams")
        );
        assert_eq!(
            service_from_url("https://acme.webex.com/meet/sam"),
            Some("Webex")
        );
        assert_eq!(
            service_from_url("https://notmeet.google.com.evil.io/"),
            None
        );
        assert_eq!(service_from_url("http://localhost:3000/"), None);
    }

    #[test]
    fn services_come_from_window_titles() {
        assert_eq!(
            service_from_title("Meet – Weekly sync - Google Chrome"),
            Some("Google Meet")
        );
        assert_eq!(
            service_from_title("Planning | Microsoft Teams"),
            Some("Microsoft Teams")
        );
        assert_eq!(service_from_title("Zoom Meeting"), Some("Zoom"));
        assert_eq!(service_from_title("Meeting notes - Google Docs"), None);
    }

    #[test]
    fn the_meeting_keeps_its_own_name_only() {
        assert_eq!(
            meeting_name("Meet – Weekly sync", Some("Google Meet")).as_deref(),
            Some("Weekly sync")
        );
        assert_eq!(
            meeting_name(
                "(2) Meet - Design review - Google Chrome",
                Some("Google Meet")
            )
            .as_deref(),
            Some("Design review")
        );
        assert_eq!(
            meeting_name("Meet - abc-defg-hij", Some("Google Meet")),
            None
        );
        assert_eq!(
            meeting_name(
                "Planning with Priya | Microsoft Teams",
                Some("Microsoft Teams")
            )
            .as_deref(),
            Some("Planning with Priya")
        );
        assert_eq!(meeting_name("Zoom Meeting", Some("Zoom")), None);
    }

    #[test]
    fn the_call_tab_is_found_among_others() {
        let tabs = [
            tab("http://localhost:3000/", "My app"),
            tab("https://meet.google.com/abc-defg-hij", "Meet – Stand-up"),
        ];
        let place = place_in_tabs("Dia", &tabs);
        assert_eq!(place.service.as_deref(), Some("Google Meet"));
        assert_eq!(place.title.as_deref(), Some("Stand-up"));
        assert_eq!(
            place.tab.as_deref(),
            Some("https://meet.google.com/abc-defg-hij")
        );
        let none = place_in_tabs("Safari", &tabs[..1]);
        assert_eq!(none.service, None);
        assert_eq!(none.app, "Safari");
    }

    #[test]
    fn desktop_call_apps_are_their_own_service() {
        let place = find("us.zoom.xos", "Zoom");
        assert_eq!(place.service.as_deref(), Some("Zoom"));
    }
}

/// Lists the tabs of a running browser:
/// `BOLKE_BROWSER=company.thebrowser.dia cargo test --lib call_place::live -- --ignored --nocapture`
#[cfg(test)]
mod live {
    #[test]
    #[ignore]
    fn shows_where_the_call_is() {
        let id = std::env::var("BOLKE_BROWSER").unwrap_or_else(|_| "com.google.Chrome".into());
        let place = super::find(&id, "Browser");
        println!("{:?}", place.service);
    }
}
