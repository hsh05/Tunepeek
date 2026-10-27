# Tunepeek

A tiny always-on-top **now-playing flyout** for Windows. Press a hotkey and a small blurred card appears with the current song, artist, album art, a seekable progress bar, previous / play-pause / next, a volume slider, and (optionally) synced lyrics. It never takes keyboard focus, so it won't interrupt what you're typing.

Works with anything that shows up in the Windows media overlay: **Spotify desktop** (free or Premium, with or without Spicetify) and **YouTube Music in a browser** (tested design target: Brave).

## Features

- Global hotkey (default `Ctrl+Alt+M`, changeable in Settings). Can also be a **mouse side button** (Mouse4/Mouse5, with or without modifiers)
- Title, artist, album art, progress bar with click/drag seeking
- Play/pause, previous, next
- Volume slider + mouse wheel anywhere on the card (controls the *app's* volume in the Windows mixer, no Premium or API keys)
- Follows the most recently active player automatically
- Acrylic blur, follows the system light/dark theme, accent colour from the album art, fade in/out
- Synced lyrics via [LRCLIB](https://lrclib.net) (optional)
- Position (any corner / edges / centre), margin, monitor (cursor or primary), auto-hide delay, start with Windows (off by default)
- Closes with the hotkey, `Esc`, a click anywhere outside it, or after N seconds of inactivity (hovering keeps it open)
- Lives in the system tray

## How it works

| Need | Mechanism |
| --- | --- |
| Title / artist / art / position / controls | Windows System Media Transport Controls (SMTC), the same thing behind the volume-flyout media card |
| Volume | Windows audio-session API (`ISimpleAudioVolume`), per process |
| Blur | `window-vibrancy` acrylic |
| UI | Tauri 2 (Rust + WebView2), plain HTML/CSS/JS, no bundler |

Because it reads SMTC, no Spotify API, login, or Premium is needed.

## Install (so you can launch it from Windows search)

```powershell
.\build.cmd
```

Use `build.cmd` from either Command Prompt or PowerShell. Running `build.ps1` directly from Command Prompt opens it in Notepad instead of running it.

This builds `Tunepeek_<version>_x64-setup.exe` in `src-tauri\target\release\bundle\nsis\` and offers to run it. The installer is per-user (no admin prompt), adds a Start menu entry, and appears in *Settings → Apps*. After that, press Start and type "Tunepeek".

- Tunepeek has no main window. It lives in the **system tray** (check the `^` overflow arrow); the settings window opens automatically on first launch.
- Launching it again from search while it's running just opens Settings.
- To remove it: *Settings → Apps → Tunepeek → Uninstall*. Turn off "Start with Windows" in Settings first, so no startup entry is left behind.

## Build for development

Requirements: Windows 10/11, [Rust](https://rustup.rs) with the MSVC toolchain (Visual Studio Build Tools, "Desktop development with C++"), and the WebView2 runtime (already on Windows 11).

```powershell
cargo install tauri-cli --version "^2"
cd src-tauri
cargo tauri dev      # run
cargo tauri build    # installer in src-tauri/target/release/bundle/nsis
```

To use your own icon: `cargo tauri icon path\to\icon.png`.

## Known limitations

- **Volume is per app, not per tab.** Browsers expose one audio session per browser process, so the slider changes Brave's volume as a whole.
- **Source filter.** By default only players whose name contains `spotify` or `brave` are shown. If a YouTube *video* is playing in Brave it appears too; add/remove names in Settings.
- **Esc** works as a global hotkey only while the flyout is open (the flyout has no keyboard focus).
- **Exclusive-fullscreen games** can't be drawn over by any normal window. Borderless/windowed fullscreen is fine.
- Lyrics depend on LRCLIB having the track; if not, the line stays empty.
- Windows 10 draws square corners; Windows 11 rounds them.

## License

MIT. Tunepeek is not affiliated with Spotify, Google, or Brave.
