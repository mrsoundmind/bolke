# Bolke

**Bolke** (बोलके, "by speaking") is private voice dictation for your computer. Hold a shortcut, speak, and your words are typed into whatever app you're using.

Everything runs on your own computer. Your voice never leaves it: no account, no cloud.

## How it works

1. **Hold** the shortcut (⌥ Space on Mac) and speak.
2. **Let go.** Bolke turns your speech into text on your computer.
3. **Done.** The text is typed where your cursor is.

## Platforms

- **macOS:** in development.
- **Windows:** built automatically on every change. Download the latest installer from the newest run of the [Windows Build](../../actions/workflows/windows-build.yml) action, under "Artifacts". It isn't code-signed yet, so Windows will show a warning when you install it.

## Building it yourself

You need [Rust](https://rustup.rs/) and [Bun](https://bun.sh/).

```bash
bun install
bun run tauri dev
```

For platform-specific setup, see [BUILD.md](BUILD.md).

## License

MIT. See [LICENSE](LICENSE).
