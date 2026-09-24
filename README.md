# Helpy

Helpy is a desktop AI companion that lives next to your mouse cursor. You ask it something out loud, it looks at your screen, and it answers by drawing highlights, pointers and arrows on top of your real apps. It can also run AI agents in the background that you start just by talking.

Built with Tauri v2 (Rust) and React + TypeScript. Windows and macOS are first class, Linux X11 is supported, Linux Wayland is best effort.

The full phase plan, crate list and platform risks are in [docs/PLAN.md](docs/PLAN.md).

## What works today (Phase 1)

- **Cursor buddy.** A small character follows the pointer on every monitor, with per-monitor DPI handled in Rust. Three built-in styles (Pip, Spark, Dot) or your own SVG/PNG. Size, opacity, offset and follow smoothness are adjustable. It auto-hides in fullscreen apps and, optionally, when the mouse rests.
- **Overlays.** One transparent, always-on-top, click-through window per monitor. They're hidden from the taskbar and excluded from screen capture (`WDA_EXCLUDEFROMCAPTURE` on Windows, `NSWindowSharingNone` on macOS). They are rebuilt when monitors are plugged in or removed.
- **Tray.** Toggle the buddy, toggle voice guidance, pause screen capture, switch behavior profile, open settings, quit. The icon changes when capture is paused. Agent panel and approval inbox are shown but disabled until their phase.
- **Global hotkeys.** All nine actions can be rebound. Duplicates are rejected, and combinations the OS already uses get a warning. The hotkeys for actions that exist now (open settings, pause capture, clear annotations) are registered with the OS. The others are saved and marked "Not active yet".
- **Settings window.** Search (Ctrl/Cmd+F), instant apply, per-section reset, JSON import/export, inline validation, and light/dark/system theme. Sections: General, Cursor buddy (with a live preview that follows your mouse), Hotkeys.

## Run it

Prerequisites: Node 20+, Rust stable, and the [Tauri system dependencies](https://tauri.app/start/prerequisites/). On Debian/Ubuntu:

```sh
sudo apt install libwebkit2gtk-4.1-dev build-essential libxdo-dev libssl-dev \
  libayatana-appindicator3-dev librsvg2-dev
```

Then:

```sh
npm install
npm run tauri dev
```

Helpy starts in the tray. Open settings from the tray menu or with Alt+Shift+S. To have the window open on launch instead, turn off **General → Start in the tray**.

## Tests

```sh
npm test                          # frontend: key handling, follow smoothing, settings registry
cd src-tauri && cargo test        # backend: settings load/repair/validation, hotkeys, monitor math
```

## Project layout

```
src-tauri/src/
  settings/     typed schema (source of truth), validation, persistence, commands
  overlay.rs    per-monitor overlay windows
  cursor.rs     cursor polling, per-monitor coordinates, buddy visibility
  fullscreen/   fullscreen-app detection for Windows, macOS and X11
  hotkeys.rs    global hotkey registration and conflict warnings
  tray.rs       tray menu and icon state
src/
  bindings/     TypeScript types generated from the Rust schema (do not edit)
  buddy/        the buddy character and smoothing, shared by overlay and settings
  overlay/      overlay window app
  settings/     settings window app; registry.ts lists every setting's label and control
design/         source SVGs for the app and tray icons
```

### Adding a setting

1. Add the field and its default in `src-tauri/src/settings/schema.rs`, and any range check in `validate.rs`.
2. Run `npm run bindings` to regenerate the TypeScript types.
3. Add one entry to `FIELDS` in `src/settings/registry.ts`. A test fails if a setting in a shown section has no entry.

## Platform notes

- **macOS:** transparent windows need `macOSPrivateApi`, which rules out the Mac App Store. Direct downloads are fine.
- **Linux:** overlays need a compositing window manager to be transparent; the settings page warns when none is running. On Wayland, Helpy runs through XWayland when it can. The General section lists what won't work in your session.
