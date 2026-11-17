<p align="center">
  <img src=".github/assets/banner.png" alt="Bolke: speak, and it's typed. Private voice dictation that runs entirely on your computer." width="100%">
</p>

<p align="center">
  <a href="../../actions/workflows/windows-build.yml"><img src="https://img.shields.io/github/actions/workflow/status/mrsoundmind/bolke/windows-build.yml?branch=main&label=Windows&logo=windows&style=flat-square&color=2F6BFF" alt="Windows build"></a>
  <a href="../../actions/workflows/linux-build.yml"><img src="https://img.shields.io/github/actions/workflow/status/mrsoundmind/bolke/linux-build.yml?branch=main&label=Linux&logo=linux&logoColor=white&style=flat-square&color=2F6BFF" alt="Linux build"></a>
  <a href="../../actions/workflows/macos-build.yml"><img src="https://img.shields.io/github/actions/workflow/status/mrsoundmind/bolke/macos-build.yml?branch=main&label=macOS&logo=apple&style=flat-square&color=2F6BFF" alt="macOS build"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-2F6BFF?style=flat-square" alt="MIT license"></a>
  <img src="https://img.shields.io/badge/runs-100%25%20on--device-1E46C8?style=flat-square" alt="Runs 100% on-device">
  <a href="https://github.com/mrsoundmind"><img src="https://img.shields.io/badge/created%20by-Shashank%20Rai%20(Mr.%20Soundmind)-0E142C?style=flat-square&logo=github" alt="Created by Shashank Rai (Mr. Soundmind)"></a>
</p>

<p align="center">
  <a href="../../releases/latest"><img src="https://img.shields.io/github/v/release/mrsoundmind/bolke?label=Download&style=for-the-badge&color=2F6BFF" alt="Download Bolke"></a>
</p>

<p align="center">
  <a href="#install-on-mac"><b>Mac</b></a> &nbsp;·&nbsp;
  <a href="#install-on-windows"><b>Windows</b></a> &nbsp;·&nbsp;
  <a href="#install-on-linux"><b>Linux</b></a> &nbsp;·&nbsp;
  <a href="#features"><b>Features</b></a> &nbsp;·&nbsp;
  <a href="#screenshots"><b>Screenshots</b></a> &nbsp;·&nbsp;
  <a href="#logo-and-colours"><b>Logo</b></a> &nbsp;·&nbsp;
  <a href="#building-it-yourself"><b>Build it</b></a>
</p>

<br>

**Bolke** (बोलके, "by speaking") is private voice dictation for your computer. Hold a shortcut, speak, and your words are typed into whatever app you're using.

Everything runs on your own computer. Your voice never leaves it: no account, no cloud.

<p align="center">
  <img src=".github/assets/in-action.png" alt="Bolke typing a dictated message into a notes window, with its live recording bubble at the bottom of the screen" width="100%">
</p>

## How it works

1. 🎙️ **Hold** the shortcut and speak.
2. ✋ **Let go.** Bolke turns your speech into text, on your computer.
3. ⌨️ **Done.** The text is typed where your cursor is.

|     Mac     |     Windows      |         Linux          |
| :---------: | :--------------: | :--------------------: |
| **⌥ Space** | **Ctrl + Space** | **Ctrl + Alt + Space** |

## Features

- 🔒 **Private.** Speech recognition runs on your computer. Nothing you say is uploaded.
- ✈️ **Works offline.** After the first model download, no internet is needed.
- 🌍 **Many languages.** Choose from over 20 speech models, from small and fast to large and accurate.
- ⌨️ **Types anywhere.** Email, chat, documents, the browser: wherever your cursor is.
- 🕘 **History.** Look back at what you dictated, copy it again, or replay the recording.
- 🎛️ **Your shortcut.** Hold to talk, or press once to start and again to stop.
- 💸 **Free and open source.** No account, no subscription, MIT licensed.

## Screenshots

<p align="center">
  <img src=".github/assets/screenshot-home.png" alt="Bolke's home screen" width="85%">
  <br><sub><b>Home:</b> your shortcut, your model and recent dictations at a glance</sub>
</p>

<table>
  <tr>
    <td width="50%"><img src=".github/assets/screenshot-welcome.png" alt="First-run welcome screen, choosing languages"></td>
    <td width="50%"><img src=".github/assets/screenshot-models.png" alt="The Models screen, with speed, accuracy and size for each model"></td>
  </tr>
  <tr>
    <td align="center"><sub><b>Welcome:</b> pick the languages you speak</sub></td>
    <td align="center"><sub><b>Models:</b> compare accuracy, speed and size</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src=".github/assets/screenshot-history.png" alt="History of past dictations"></td>
    <td width="50%"><img src=".github/assets/screenshot-settings.png" alt="Bolke's settings"></td>
  </tr>
  <tr>
    <td align="center"><sub><b>History:</b> everything you've dictated</sub></td>
    <td align="center"><sub><b>Settings:</b> shortcut, microphone and sounds</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src=".github/assets/screenshot-advanced.png" alt="Advanced settings"></td>
    <td width="50%" align="center">
      <img src=".github/assets/bubble-recording.png" alt="The small recording bubble" width="80%"><br><br>
      <img src=".github/assets/bubble-live.png" alt="The live bubble, showing words as you speak" width="100%">
    </td>
  </tr>
  <tr>
    <td align="center"><sub><b>Advanced:</b> overlay, pasting and more</sub></td>
    <td align="center"><sub><b>Recording bubble:</b> floats over your work while you speak</sub></td>
  </tr>
</table>

## Platforms

| Platform    | Status                                                                                      |
| ----------- | ------------------------------------------------------------------------------------------- |
| **Windows** | ✅ Ready to try (Windows 10 and 11, 64-bit). [Install](#install-on-windows)                 |
| **Linux**   | ✅ Ready to try (64-bit). [Install](#install-on-linux)                                      |
| **macOS**   | ✅ Ready to try (macOS 10.15 or newer; Apple Silicon and Intel). [Install](#install-on-mac) |

## Install on Mac

1. Download the one for your Mac:
   - **Apple Silicon (M1, M2, M3, M4 and newer):** [Bolke-mac-apple-silicon.dmg](https://github.com/mrsoundmind/bolke/releases/latest/download/Bolke-mac-apple-silicon.dmg)
   - **Intel:** [Bolke-mac-intel.dmg](https://github.com/mrsoundmind/bolke/releases/latest/download/Bolke-mac-intel.dmg)

   Not sure which you have? Click the Apple menu → **About This Mac** and look at **Chip** (Apple M…) or **Processor** (Intel).

2. Double-click the downloaded **`.dmg`** file.
3. Drag **Bolke** into the **Applications** folder.
4. Open **Bolke** from Applications. Because Bolke isn't notarised by Apple yet, macOS says it can't check the app. Click **Done**, then go to **System Settings → Privacy & Security**, scroll down and click **Open Anyway** next to Bolke. You only need to do this once.
5. Bolke asks for two permissions: **Microphone** (to hear you) and **Accessibility** (to type the text and catch your shortcut). Allow both.
6. The first time, Bolke downloads a speech model. After that it works offline.

To dictate, hold **⌥ Space** (Option + Space), speak, and let go.

To uninstall, quit Bolke from the menu bar and drag it from Applications to the Bin.

## Install on Windows

1. Download [Bolke-windows-setup.exe](https://github.com/mrsoundmind/bolke/releases/latest/download/Bolke-windows-setup.exe) and double-click it. ([Bolke-windows.msi](https://github.com/mrsoundmind/bolke/releases/latest/download/Bolke-windows.msi) is the same app, for computers managed by an office or school.)
2. Windows may say **"Windows protected your PC"**. This is because Bolke isn't code-signed yet. Click **More info**, then **Run anyway**.
3. Follow the installer, then open **Bolke** from the Start menu.
4. The first time, Bolke asks for microphone access and downloads a speech model. After that it works offline.

To dictate, hold **Ctrl + Space**, speak, and let go. You can change the shortcut in Bolke's settings. Your words are typed where your cursor is.

To uninstall, go to **Settings → Apps → Installed apps**, find **Bolke** and click **Uninstall**.

## Install on Linux

Tested on clean installs of Ubuntu 22.04 and 24.04, Debian 12, Fedora and openSUSE.

**1. Download**

1. Download the one for your system:
   - **Ubuntu, Debian, Linux Mint, Pop!\_OS, Zorin, elementary:** [Bolke-linux.deb](https://github.com/mrsoundmind/bolke/releases/latest/download/Bolke-linux.deb)
   - **Fedora, openSUSE:** [Bolke-linux.rpm](https://github.com/mrsoundmind/bolke/releases/latest/download/Bolke-linux.rpm)
   - **Bazzite, Fedora Silverblue, SteamOS or anything else:** [Bolke-linux.AppImage](https://github.com/mrsoundmind/bolke/releases/latest/download/Bolke-linux.AppImage)
2. Open your **Downloads** folder, right-click an empty space and choose **Open in Terminal**.

**2. Install: type one command in that Terminal and press Enter**

> [!IMPORTANT]
> Don't double-click the `.deb` or `.rpm` file. That opens the software store, which on some systems fails or asks you to restart without installing. The command below installs Bolke and everything it needs in one go. It asks for your password; nothing shows while you type it, which is normal.

| Your system                                | Command                                                      |
| ------------------------------------------ | ------------------------------------------------------------ |
| Ubuntu, Debian, Mint, Pop!\_OS and similar | `sudo apt install ./Bolke-linux.deb`                         |
| Fedora                                     | `sudo dnf install ./Bolke-linux.rpm`                         |
| openSUSE                                   | `sudo zypper install --allow-unsigned-rpm ./Bolke-linux.rpm` |
| AppImage (any system)                      | `chmod +x Bolke-linux.AppImage && ./Bolke-linux.AppImage`    |

On **Ubuntu 24.04**, an AppImage also needs `sudo apt install libfuse2t64` once. The `.deb` doesn't.

On **Bazzite, Silverblue and other "atomic" systems**, use the AppImage. Installing a `.rpm` there only takes effect after a restart.

**3. Open Bolke** from your apps menu (for the AppImage, double-click the file). The first time, it downloads a speech model. After that it works offline.

To dictate, hold **Ctrl + Alt + Space**, speak, and let go.

To uninstall: `sudo apt remove bolke` (Ubuntu and Debian), `sudo dnf remove Bolke` (Fedora) or `sudo zypper remove Bolke` (openSUSE). For the AppImage, delete the file.

### Linux setup

Shortcuts and typing work differently on each Linux desktop.

- **The shortcut doesn't work (common on Wayland):** add a shortcut in your desktop's keyboard settings that runs `Bolke --toggle-transcription`. Press it once to start and again to stop.
- **Text isn't typed into other apps:** install a typing helper. On Wayland, install `wtype` (or `dotool` or `ydotool` on GNOME and KDE). On X11, install `xdotool`. Then restart Bolke.
- **No microphone:** allow microphone access in your system's sound or privacy settings.

## If the shortcut stops working on Mac

macOS turns on **Secure Input** while you type a password, and some apps (password managers, terminals) leave it on. While it's on, no app can see your shortcut.

- Bolke shows which app is causing it, when it can tell. Quit that app, or close its password field.
- In Terminal or iTerm, open the app menu and turn off **Secure Keyboard Entry**.
- If nothing helps, log out and back in.

## Logo and colours

<p align="center">
  <img src="brand/bolke-brand-sheet.png" alt="Bolke logos on dark and light, app icon, menu bar icon and colours" width="100%">
</p>

The logo, app icon and colours are in the [`brand`](brand) folder, as SVG and PNG.

## Building it yourself

You need [Rust](https://rustup.rs/) and [Bun](https://bun.sh/).

```bash
bun install
bun run tauri dev
```

For platform-specific setup, see [BUILD.md](BUILD.md).

Built with [Tauri](https://tauri.app) (Rust and React).

## License

MIT. See [LICENSE](LICENSE).

Bolke includes its own copies of a few open-source libraries, kept under their original licences: the keyboard, microphone and audio libraries in `src-tauri/crates/` and the model list in `packages/models/`. Each folder has its licence file.

<br>

<p align="center">
  <img src="brand/bolke-app-icon-512.png" alt="" width="72">
  <br>
  <b>Created by Shashank Rai</b> · <a href="https://github.com/mrsoundmind">Mr. Soundmind (@mrsoundmind)</a>
  <br>
  <sub>बोलके, just speak.</sub>
</p>
