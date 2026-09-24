# Helpy

An AI screen tutor and assistant that lives next to your mouse cursor. Press a hotkey, ask a question, and Helpy answers by drawing highlights, pointers, and arrows on top of your real apps. Spawn background agents by voice that work with Gmail, Notion, and more.

Built with Tauri v2 (Rust) and React + TypeScript. Windows and macOS are first class, Linux X11 is supported, and Linux Wayland is best effort.

**Status:** Phase 1 of 9. The full plan, crate choices, and platform risks are in [docs/PLAN.md](docs/PLAN.md).

## What works in Phase 1

- **Cursor buddy** that follows the pointer on every monitor, with four built-in characters (Pip, Orbit, Spark, Pebble) or your own SVG/PNG. It shows idle, listening, thinking, and speaking states, an agent count badge, and an attention dot. It flips sides near screen edges, can hide when the mouse is idle, and hides in fullscreen apps on Windows.
- **Overlay windows**: one transparent, click-through, always-on-top window per monitor, excluded from screen capture, rebuilt automatically when displays change. Coordinates are mapped per monitor, so mixed DPI setups line up. Settings → Cursor buddy → *Check overlay alignment* draws a frame on each screen to confirm it.
- **Tray** with every menu entry from the brief (entries for later phases are present but disabled) and an icon that changes for listening, capture paused, agents running, and approval needed.
- **Global hotkeys** for all nine actions, rebindable with a recorder that catches duplicates, combinations another app already owns, and ones the OS reserves. Escape clears annotations, and is only captured while something is on screen.
- **Settings window** with a searchable sidebar, instant apply, per-section reset, JSON import/export (never includes keys or tokens), inline validation, and light/dark/system themes. General, Cursor buddy, and Hotkeys are complete. Every later setting is already listed in its section and in search, tagged with the phase that delivers it.

## Develop

Prerequisites: Node 20+, Rust stable, and the [Tauri system dependencies](https://v2.tauri.app/start/prerequisites/) for your OS.

```sh
npm install
npm run tauri dev
```

Helpy starts in the tray. Click the tray icon, or press `Alt+Shift+,`, to open settings.

To work on the UI in a plain browser (a mock backend fills in), run `npm run dev` and open `http://localhost:1420/?window=settings`.

| Command | What it does |
| --- | --- |
| `npm run tauri build` | Release build and installers |
| `npm run test:rust` | Rust unit tests (settings validation, persistence, import) |
| `npm run bindings` | Regenerates `src/bindings/` TypeScript types from the Rust settings schema |
| `npm run typecheck` | TypeScript check |

## How settings are defined

`src-tauri/src/settings/schema.rs` is the single source of truth. Each field's type and default live there, `ts-rs` exports the types to `src/bindings/`, and the defaults are exported to `src/bindings/defaultSettings.json`. Validation happens in Rust (`validate.rs`), so the frontend shows the same messages the backend enforces. The frontend adds one registry entry per setting in `src/settings/registry.tsx` for its label, help text, search keywords, and control.

Settings are stored as JSON in the OS config folder (for example `%APPDATA%\com.helpy.app\settings.json` on Windows). A corrupt file is kept as `settings.json.bad` and defaults are used.

## Project layout

```
src-tauri/src/
  settings/   schema, validation, persistence, commands
  overlay/    per-monitor overlay windows, cursor tracker, fullscreen detection
  tray/       tray menu and state icons
  hotkeys/    global shortcut registration
  state.rs    shared app state and runtime status
src/
  bindings/   generated from Rust, do not edit
  buddy/      buddy characters and follow physics
  overlay/    overlay window UI
  settings/   settings window, registry, controls
  design/     design tokens
design/       source SVGs for the app icon and mascot
```

## Platform notes

- **macOS**: Helpy runs as a menu bar app with no Dock icon. Transparent overlays use `macOSPrivateApi`, so this build can't go to the Mac App Store. Screen Recording, Microphone, and Accessibility permissions are requested from Phase 2 onward.
- **Windows**: per-monitor DPI aware. Overlays are hidden from screen capture with `WDA_EXCLUDEFROMCAPTURE` (Windows 10 version 2004 or later).
- **Linux X11**: transparent overlays need a compositing window manager. Without one, the overlay area paints black.
- **Linux Wayland**: apps can't read the global pointer position, so the buddy can't follow the cursor, and global hotkeys depend on the desktop. Settings shows a notice in the affected sections.
