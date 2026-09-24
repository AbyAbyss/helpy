# Helpy implementation plan

Helpy is a Tauri v2 desktop app (React + TypeScript frontend, Rust backend). This plan covers all nine phases from the brief. Each phase ends in a runnable app. Status is tracked at the top of each phase.

## Architecture at a glance

```
src-tauri/src/
  lib.rs            app setup, plugin wiring, command registration
  settings/         typed schema (single source of truth, exported to TS via ts-rs),
                    persistence, validation, import/export, change events
  overlay/          one click-through window per monitor, cursor tracker, fullscreen detection
  tray/             tray menu + state-driven tray icon
  hotkeys/          global shortcut registration from settings, conflict reporting
  state.rs          shared app state (buddy state, capture paused, agent counts)
  (later) providers/, speech/, capture/, guidance/, agents/, connectors/, usage/

src/
  bindings/         TS types generated from Rust (never edited by hand)
  lib/              IPC wrappers, settings store hook, hotkey helpers
  design/           tokens.css, shared UI primitives
  overlay/          overlay window app (buddy, later annotations)
  buddy/            buddy styles and state animations
  settings/         settings window: registry, sections, search
```

One window bundle, routed by window label: `settings`, `overlay-<n>`, later `panel`, `card-<id>`, `agents`, `inbox`.

**Settings as one schema.** The Rust `Settings` struct owns defaults, serde shape, and validation. `ts-rs` generates `src/bindings/*.ts` from it during `cargo test`, so the frontend never re-declares a setting's type. The frontend has a small registry per setting (label, help text, keywords) used by the UI and search. Adding a setting means: a field + default in Rust, and one registry entry with its control.

## Phase 1: shell, overlay, buddy, tray, hotkeys, settings

Status: done. Verified with Rust unit tests, a Windows target `cargo check`, and a launch on Linux X11 (Xvfb) where the buddy tracked the pointer at the configured offset.

Plan:
1. Scaffold with `npm create tauri-app` (react-ts), identifier `com.helpy.app`.
2. Settings module: `Settings` struct with sections, `#[serde(default)]` everywhere so older files load, JSON file in the app config dir, atomic writes (temp file + rename), `settings://changed` broadcast. Commands: get, patch, reset section, reset all, export, import (import strips anything secret-shaped and re-validates).
3. Overlay: one transparent, borderless, always-on-top, skip-taskbar, content-protected window per monitor, set to ignore cursor events. Built at physical monitor bounds so mixed DPI works. A monitor watcher rebuilds overlays when displays change.
4. Cursor tracker: a Rust thread polls the global cursor at ~120 Hz, emits only when it moves. Each overlay converts physical global coordinates into its own CSS pixels using its monitor origin and scale factor. The buddy uses a frame-rate-independent spring on `requestAnimationFrame` (smoothness 0 means it snaps to the cursor).
5. Buddy: four built-in styles plus custom SVG/PNG upload (validated, stored in app data, served as a data URL). States: idle, listening, thinking, speaking, agent count badge, attention badge.
6. Tray: menu with every item from the brief (items for later phases are present but disabled), tray icon swaps by state.
7. Hotkeys: registered from settings, re-registered on change, failures reported back to the settings page. Escape is only registered while annotations are on screen, so Helpy never swallows Escape from other apps.
8. Settings window: sidebar, search across every setting, instant apply, per-section reset, JSON import/export, inline validation, help text, light/dark/system theme. General, Cursor buddy, and Hotkeys are fully built. Other sections appear in the sidebar marked with the phase that delivers them.

Crates: `tauri` (tray-icon, image-png), `tauri-plugin-global-shortcut`, `tauri-plugin-autostart`, `tauri-plugin-dialog`, `tauri-plugin-single-instance`, `tauri-plugin-log`, `serde`, `serde_json`, `ts-rs`, `thiserror`, `log`, `base64`. Windows only: `windows` (foreground window + monitor queries for fullscreen detection).

npm: `@tauri-apps/api`, `@tauri-apps/plugin-dialog`, `@fontsource-variable/*` (fonts bundled locally, no network).

Risks:
- Click-through transparent windows: solid on Windows and macOS. On Linux X11 it needs a compositor for transparency; without one the overlay would be opaque, so Helpy detects missing compositing and disables the overlay with a message (planned hardening in Phase 9). Wayland: layer-shell overlays are not available through GTK3/Tauri; overlays may appear as normal windows, and `set_ignore_cursor_events` is best effort.
- Content protection: Windows uses `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` (Windows 10 2004+; older builds show a black box). macOS `NSWindowSharingNone` is honored by ScreenCaptureKit only when capturing with window exclusion; Phase 2 capture will also explicitly exclude Helpy's windows. Linux has no equivalent, so Phase 2 hides overlays for the capture frame instead.
- macOS: overlay needs `macOSPrivateApi` for transparency, windows must join all Spaces and sit above fullscreen apps. Helpy runs as an accessory app (no Dock icon).
- Fullscreen auto-hide: implemented on Windows in Phase 1. macOS (CGWindowList bounds check) and X11 (`_NET_WM_STATE_FULLSCREEN`) land in Phase 9.
- IPC rate: cursor events at 120 Hz to every overlay are cheap in practice, but if profiling shows lag the fallback is to emit only to the overlay under the cursor.

## Phase 2: providers, retry and budget core, text Q&A with screenshots

Plan: `providers/` trait (`chat`, `stream`, `capabilities`) with Anthropic, OpenAI, Gemini, and an OpenAI-compatible client that covers Ollama, LM Studio, and llama.cpp. A `guard` layer wraps every call: retry classification (retryable vs fatal), exponential backoff with `retry-after`, fallback chain accounting, budget pre-checks. The guard is the same code agents use later, so limits are enforced in one place. Screen capture of the cursor's monitor via `xcap`, resized with `image`, with the resize ratio recorded for coordinate mapping. Text ask panel (hotkey) with follow-ups.

Crates: `reqwest` (rustls, stream), `tokio`, `eventsource-stream`, `keyring`, `xcap`, `image`, `async-trait`, `tokio-util`.

Risks: macOS Screen Recording permission prompts; Wayland capture needs the xdg portal (`ashpd`), one prompt per session. Tool-calling quality on small local models: strict-JSON fallback with schema validation.

## Phase 3: voice input and output

Plan: `cpal` capture with level meter, `nnnoiseless` suppression, silence auto-stop. Local STT via `whisper-rs` (whisper.cpp) with a model manager; cloud STT (OpenAI, Deepgram). TTS through OS voices (`tts` crate), Piper (bundled binary, downloaded voices), or cloud. Listening panel with live waveform and partial transcripts. Push-to-talk uses the shortcut Pressed/Released states.

Risks: whisper.cpp build time and GPU backends per OS; wake word is off by default and deferred until a small local model is picked.

## Phase 4: visual guidance

Plan: drawing action schema (highlight, point, arrow, speak) as tool calls, validated in Rust. Coordinate pipeline: model coords in screenshot space, divided by resize ratio, offset by monitor origin, then converted per overlay. Step card as a small separate focusable window (the overlay stays click-through). Click detection near target via a global mouse hook (`rdev`, listen only) to advance steps.

Risks: global mouse listening needs Accessibility permission on macOS and does not work on Wayland (Next button remains).

## Phase 5: circle to explain

Plan: selection mode flips the overlay to accept mouse input, draws rectangle or lasso, crops the capture, runs explain/OCR/translate/summarize. Diagram labels are placed with a simple force-based layout around the selection, with collision checks so labels never overlap.

## Phase 6: agent core

Plan: `agents/` runtime on tokio with a scheduler (single, parallel with a cap, sequential with reorderable queue), per-agent limits (steps, tool calls, repeats, no-progress, time, token and cost budgets) enforced in the loop, not in prompts. State persisted to SQLite (`rusqlite`) after every step so restarts keep counters. Plan card, agent panel, floating agent cards as separate small windows, notifications (`tauri-plugin-notification`). Basic tools: web search, fetch, files in approved folders with backups, shell under policy.

Tests: fake providers and tools that fail in scripted ways prove every limit holds, including across restart and fallback chains.

## Phase 7: connectors and approvals

Plan: OAuth (PKCE, loopback redirect via `tiny_http`) with tokens in the keychain, Gmail and Notion first, then Calendar, Drive, Microsoft 365, Slack, GitHub. MCP client (`rmcp`) for stdio and HTTP servers. Approval inbox and per-connector action rules checked in Rust before any side-effect tool executes.

Risks: Gmail restricted scopes need Google verification; support user-provided client IDs from day one.

## Phase 8: advanced agents

Chained runs, dependency graphs, orchestrator with depth 1 and a sub-agent cap, voice steering, templates with parameters, triggers (cron via `croner`, file watch via `notify`, polling for email and Notion) with auto-pause and hourly rate limits, isolated headless browser (`chromiumoxide`), external agent runners (Claude Code headless) streaming into the panel.

## Phase 9: precision and polish

Accessibility snapping (UI Automation via `uiautomation`, macOS AX via `accessibility-sys`, AT-SPI via `atspi`), privacy features (app blocklist, password field blur, offline mode), usage tracking and budgets, behavior profiles, onboarding with permission checks, fullscreen detection on macOS and X11.
