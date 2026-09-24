# Helpy implementation plan

Helpy is built phase by phase. Each phase ends with an app that runs (`npm run tauri dev`) and does something useful on its own. This file lists what each phase delivers, the crates and plugins it pulls in, and the platform risks that could change the design.

Status: **Phase 1 is implemented.** Phases 2 to 9 are planned.

## Architecture in one paragraph

A Rust backend owns every piece of state that matters (settings, cursor tracking, windows, hotkeys, later agents and budgets) and pushes changes to the frontend with Tauri events. The React frontend is one Vite bundle with one entry per window kind (`overlay.html`, `settings.html`, later `panel.html`, `card.html`), so each window loads only what it needs. Settings live in one typed Rust schema (`src-tauri/src/settings/schema.rs`); `ts-rs` generates the matching TypeScript types into `src/bindings/`, so the compiler catches drift between the two sides.

## Module map

| Module | Rust | Frontend |
|---|---|---|
| Settings | `settings/` schema, defaults, load/save, import/export, validation | `settings/` window, field registry, search |
| Overlay | `overlay.rs` one window per monitor, click-through, capture exclusion | `overlay/` buddy, later annotations |
| Cursor | `cursor.rs` polling thread, per-monitor routing | `overlay/useCursor.ts` smoothing |
| Fullscreen detection | `fullscreen/` per-OS | none |
| Tray | `tray.rs` menu, state icons | none |
| Hotkeys | `hotkeys.rs` registration from settings | `settings/HotkeyInput.tsx` capture, conflicts |
| Providers (P2) | `providers/` trait + adapters, `limits/` retry, backoff, budgets | |
| Speech (P3) | `speech/` stt, tts | `panel/` waveform |
| Capture (P2) | `capture/` per-monitor screenshots | |
| Agents (P6+) | `agents/` scheduler, run modes, persistence, limits | `agents/` panel, cards |
| Connectors (P7) | `connectors/` OAuth, keychain, MCP | |

## Phase 1: shell (done)

Delivers: scaffold, one overlay per monitor, cursor buddy, tray, global hotkeys, settings window with persistence, search, import/export, and the General, Cursor buddy and Hotkeys sections.

- Overlay: transparent, undecorated, always on top, skipped in the taskbar, `set_ignore_cursor_events(true)` and `set_content_protected(true)`. Sized to each monitor's physical bounds. Rebuilt when the monitor layout changes (checked every 2 s).
- Cursor: a Rust thread polls `AppHandle::cursor_position()` every 8 ms and emits only when the position changes, already converted to the target monitor's logical pixels. The frontend applies an exponential smoothing factor (the "follow smoothness" setting) inside `requestAnimationFrame`. Smoothness 0 means the buddy snaps to the raw position every frame.
- Fullscreen auto-hide: Windows uses `SHQueryUserNotificationState` plus a foreground-window-covers-monitor check; macOS compares the on-screen window list against display bounds; Linux X11 reads `_NET_WM_STATE_FULLSCREEN` on the active window.
- Settings: stored as JSON in the app config dir, written atomically (temp file + rename). Unknown or missing fields fall back to defaults so older files keep loading. Every change is broadcast as `settings://changed` so all windows update instantly.
- Hotkeys: `tauri-plugin-global-shortcut`. Actions that exist in Phase 1 (open settings, pause screen capture, clear annotations) are registered. The rest are stored, validated and conflict-checked now, and get wired in their phase.
- Tray: every menu item from the brief is present. Agent panel and approval inbox are disabled until Phase 6/7. Profile switching lists the three built-in profiles; the profile contents arrive in Phase 9.

Crates: `tauri` (tray-icon, image-png, macos-private-api), `tauri-plugin-global-shortcut`, `tauri-plugin-autostart`, `tauri-plugin-dialog`, `tauri-plugin-single-instance`, `serde`, `serde_json`, `ts-rs`, `windows` (Windows), `core-graphics` + `core-foundation` (macOS), `x11rb` (Linux).
npm: `@tauri-apps/api`, the matching plugin packages, `@fontsource-variable/*` for bundled fonts (no network at runtime).

## Phase 2: providers and text Q&A

- `Provider` trait: `chat(request) -> stream of events`, `capabilities() -> {vision, tools, streaming}`. Adapters: Anthropic, OpenAI, Gemini, and one OpenAI-compatible adapter that covers Ollama, LM Studio, llama.cpp and custom URLs.
- `limits/` module: retry classifier (retryable vs terminal errors), exponential backoff honoring `retry-after`, budget ledger. Every provider call goes through it, including non-agent calls. Unit tests with a scripted failing provider.
- Screen capture of the cursor's monitor via `xcap`. Screenshots record `{monitor origin, scale factor, resize ratio}` so model coordinates map back exactly.
- API keys in the OS keychain via `keyring`.
- Text ask panel (second hotkey), follow-up context until dismissed.

Crates: `reqwest` (rustls, stream), `tokio`, `eventsource-stream`, `keyring`, `xcap`, `image`, `thiserror`.

## Phase 3: voice

- STT: `whisper-rs` (whisper.cpp) with a model manager that downloads GGML files; cloud STT via OpenAI or Deepgram.
- Audio input with `cpal`, level meter and waveform streamed to the panel as downsampled frames.
- TTS: OS voices (`tts` crate covers SAPI, AVSpeechSynthesizer and speech-dispatcher), Piper as a sidecar binary, cloud voice.
- Push-to-talk needs key-up events, which `tauri-plugin-global-shortcut` provides (`ShortcutState::Released`).

Crates: `cpal`, `whisper-rs`, `tts`, `hound`, `webrtc-vad` or `nnnoiseless` for noise suppression.

## Phase 4: visual guidance

- Structured actions (highlight, point, arrow, speak) as a tool schema; strict-JSON fallback with validation for models without reliable tool calling.
- Annotations render in the overlay. The step card is the only clickable element, done by making a small separate window for it (click-through is per window, not per pixel, on most platforms).
- Click detection near the target without stealing clicks: a low-level mouse hook (`rdev` listen-only) on Windows/macOS/X11.
- Escape to clear is registered as a global shortcut only while annotations are visible, so it never steals Escape from other apps otherwise.

Crates: `rdev`.

## Phase 5: circle to explain

- Selection mode flips the overlay of the cursor's monitor to capture input. Rectangle and freehand lasso, crop via the Phase 2 capture module.
- Label placement around the selection: greedy angular placement with collision checks, leader lines drawn in SVG.
- OCR via the vision model (no separate OCR engine in v1).

## Phase 6: agent core

- Agent runtime on `tokio`. Each agent is a state machine persisted to SQLite after every step (queue, retry counts, budget spent, pending approvals), so restarts resume without resetting limits.
- Limits enforced in Rust: max steps, max tool calls, repeat detection (hash of tool name + args), no-progress detection, per-agent/batch/day budgets checked before each model call.
- Run modes: single, parallel (semaphore), sequential (reorderable queue).
- Floating cards: one small window per visible card, `set_content_protected(true)`, positions remembered.
- Tools: web search, fetch, files in approved folders with backups, shell with policy.
- Test suite: fake provider and fake tools that fail in scripted ways to prove every limit.

Crates: `rusqlite` (bundled), `tokio`, `tauri-plugin-notification`, `sha2`.

## Phase 7: connectors and approvals

- OAuth with PKCE, loopback redirect server on a random port, tokens in the keychain with auto refresh. Gmail and Notion first.
- MCP client for stdio and HTTP servers (`rmcp`).
- Approval inbox and per-connector action rules, checked in the tool dispatcher so no code path can skip them.

Crates: `oauth2`, `rmcp`, `tiny_http` (redirect listener).

## Phase 8: advanced agents

Chained runs, dependency graph, orchestrator with nested sub-agents (max 5, depth 1), voice steering, templates with parameters, triggers (cron + events) with auto-pause after 3 failures and 10 runs/hour cap, headless browser (`chromiumoxide` with its own profile), external CLI agent runners as child processes.

Crates: `cron`, `notify` (folder watch), `chromiumoxide`.

## Phase 9: polish

Accessibility snapping (UI Automation via `uiautomation`, macOS AX via `accessibility-sys`, AT-SPI via `atspi`), privacy features (blocklist, password field blur, offline mode), usage tracking, behavior profiles, onboarding.

## Platform risks

1. **macOS transparency needs `macOSPrivateApi: true`.** That flag blocks Mac App Store distribution. Direct download is fine.
2. **Capture exclusion differs per OS.** Windows: `set_content_protected` uses `SetWindowDisplayAffinity`; `WDA_EXCLUDEFROMCAPTURE` needs Windows 10 2004+, older builds show a black box instead of hiding. macOS: `NSWindowSharingNone` works for `CGWindowList` capture but ScreenCaptureKit on macOS 15+ may still record it; Phase 2 capture will exclude Helpy's windows explicitly by window ID as a second guard. Linux: no content protection at all, so Phase 2 will hide overlays for one frame during capture on Linux.
3. **Wayland.** No global cursor position, no always-on-top guarantee, no global shortcuts without the portal, input-region click-through only via layer-shell. Helpy sets `GDK_BACKEND=x11` on Linux so it runs under XWayland when available, and the settings window shows a notice listing what won't work on native Wayland.
4. **Click-through on Linux X11** needs a compositor for transparency. Without one, overlays show as black rectangles; the notice mentions this.
5. **Global hotkeys can collide with the OS.** Registration failures are reported back to the Hotkeys section instead of silently ignored.
6. **Per-monitor DPI.** All positions travel in physical pixels in Rust and convert to logical only at the window that renders them, using that monitor's own scale factor.
7. **Gmail restricted scopes** need Google verification for a public client ID. Phase 7 supports a user-provided client ID.
8. **Fullscreen detection on macOS** is a heuristic (a window covering the full display at the top layer). Presentation apps that use a separate Space are detected; borderless windowed games may not be.
