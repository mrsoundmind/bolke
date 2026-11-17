# Bolke

**Bolke** (बोलके, "by speaking") is private voice dictation for your computer. Hold a shortcut, speak, and your words are typed into whatever app you're using.

Everything runs on your own computer. Your voice never leaves it: no account, no cloud.

## How it works

1. **Hold** the shortcut (⌥ Space on Mac, Ctrl + Space on Windows) and speak.
2. **Let go.** Bolke turns your speech into text on your computer.
3. **Done.** The text is typed where your cursor is.

## Platforms

- **macOS:** in development.
- **Windows:** ready to try (Windows 10 and 11, 64-bit). See below.

## Install on Windows

1. Sign in to GitHub and open the [Windows Build](../../actions/workflows/windows-build.yml) page.
2. Click the newest run that has a green tick.
3. Scroll down to **Artifacts** and click **bolke-x86_64-pc-windows-msvc** to download it.
4. Open the downloaded zip and drag its contents out to a folder.
5. Double-click the installer that ends in **`-setup.exe`**. (The `.msi` file is the same app, for computers managed by an office or school.)
6. Windows may say **"Windows protected your PC"**. This is because Bolke isn't code-signed yet. Click **More info**, then **Run anyway**.
7. Follow the installer, then open **Bolke** from the Start menu.
8. The first time, Bolke asks for microphone access and downloads a speech model. After that it works offline.

To dictate, hold **Ctrl + Space**, speak, and let go. You can change the shortcut in Bolke's settings. Your words are typed where your cursor is.

To uninstall, go to **Settings → Apps → Installed apps**, find **Bolke** and click **Uninstall**.

## Building it yourself

You need [Rust](https://rustup.rs/) and [Bun](https://bun.sh/).

```bash
bun install
bun run tauri dev
```

For platform-specific setup, see [BUILD.md](BUILD.md).

## License

MIT. See [LICENSE](LICENSE).
