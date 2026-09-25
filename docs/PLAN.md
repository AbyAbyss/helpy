# Helpy implementation plan

Helpy is built phase by phase. Each phase ends with an app that runs (`npm run tauri dev`) and does something useful on its own. This file lists what each phase delivers, the crates and plugins it pulls in, and the platform risks that could change the design.

Status: **Phases 1 to 5 are implemented.** Phases 6 to 9 are planned, plus the additional requirements R1 to R7 below.

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

## Phase 2: providers and text Q&A (done)

- Provider-neutral conversation types (`ai/types.rs`) with one adapter per API: Anthropic Messages, OpenAI chat completions (also used for Ollama, LM Studio, llama.cpp and custom endpoints) and Gemini. All stream through one SSE reader that applies the idle timeout and cancellation. Enum dispatch instead of a trait object, since the set of wire formats is closed.
- `ai/limits.rs`: every model call goes through `run_step`, which enforces the retry, backoff, fallback and budget rules. Tests drive it with a scripted failing provider under a paused clock.
- `ai/ledger.rs`: daily tokens and cost, written to disk after every call. Failed attempts that may have been billed (timeouts, 5xx, garbled output) are charged their estimated input.
- Screen capture of the cursor's monitor via `xcap`, downscaled to 1568 px on the long edge and sent as JPEG. `CaptureMeta` maps model coordinates back through the resize and DPI (tested now, used by Phase 4).
- The model decides whether to look: models with tool calling get a `view_screen` tool; models without it are asked to reply `VIEW_SCREEN` (held back from the panel while it streams). Only the newest screenshot stays in the conversation.
- API keys in the OS keychain via `keyring`.
- Anthropic specifics: sampling parameters are left out (current Claude models reject them), thinking blocks are passed back unchanged, and `claude-opus-5` / `claude-fable-5-1` opt into server-side refusal fallback (`fallbacks: "default"`), with the documented echo rules after a mid-output fallback.

Crates: `reqwest` 0.12 (rustls with ring, which cross-compiles without extra tooling), `tokio`, `tokio-util`, `futures-util`, `bytes`, `keyring` 3, `xcap`, `image`, `thiserror`, `chrono`. npm: `react-markdown` (escapes raw HTML in answers).

Not in Phase 2: per-agent and per-batch budgets (Phase 6), provider usage charts (Phase 9), voice (Phase 3).

## Phase 3: voice (done)

- Audio in: cpal (through rodio) on its own thread per capture; `voice/dsp.rs` mixes to mono, resamples to 16 kHz, runs RNNoise (`nnnoiseless`, pure Rust) when noise suppression is on, meters the level, and detects the end of speech against a learned noise floor.
- Speech to text: local Whisper (`whisper-rs`) with live partial transcripts while speaking, or OpenAI / Deepgram through the same retry rules as other provider calls (not counted in the token budget, since they bill per minute of audio). The model manager reads each model's real size from the server.
- Text to speech: OS voices (`tts` crate), Piper (release binary and voices downloaded on demand; the option is hidden where Piper has no build), and OpenAI voices. Answers are split into sentences as they stream and spoken in order; a retry or Esc drops everything queued.
- Voice flow: push-to-talk or toggle on the voice hotkey, and an optional wake word matched with local Whisper. The compact waveform pill (R6) sits beside the cursor, sizes its window to its content so it blocks no clicks around it, and opens the full panel on click. Esc is captured only while listening, answering or speaking.
- Typed and spoken questions share one conversation: answers go out as `ask://event` to every window.

Crates: `rodio` (playback, and its cpal for capture), `whisper-rs`, `nnnoiseless`, `tts`, `flate2`, `tar`, `zip`, `reqwest` multipart.

## Phase 4: visual guidance (done)

- Structured actions (highlight, point, arrow, speak) as a tool schema; strict-JSON fallback with validation for models without reliable tool calling.
- Annotations render in the overlay. The step card is the only clickable element, done by making a small separate window for it (click-through is per window, not per pixel, on most platforms).
- Click detection near the target without stealing clicks: a low-level mouse hook (`rdev` listen-only) on Windows/macOS/X11.
- Escape to clear is registered as a global shortcut only while annotations are visible, so it never steals Escape from other apps otherwise.
- As built: guidance lives inside the ask flow. Once the model has seen the screen it can call `show_step`, which returns only after the user has done the step, with a fresh screenshot. So a walkthrough is the ordinary tool loop, capped by the max-steps setting on top of the usual call limit. The visual guidance model, if routed, takes over from the second step. The ask panel and voice pill hide during a walkthrough and come back after it. "Do it for me" is deferred to Phase 9 (R7), and precision snapping stays in Phase 9 as planned.

Crates: `rdev`.

## Phase 5: circle to explain (done)

- Selection mode flips the overlay of the cursor's monitor to capture input. Rectangle and freehand lasso, crop via the Phase 2 capture module.
- Label placement around the selection: greedy angular placement with collision checks, leader lines drawn in SVG.
- OCR via the vision model (no separate OCR engine in v1).
- As built: the screen is captured at full resolution when selection starts, so the selection is made on a still frame and cropped from it. Diagram parts come back after a `PARTS:` line as JSON in crop pixels, which works the same for models with and without tool calling. Actions: explain, copy text, translate (into a set language), summarize, and follow-up questions, all in one conversation per selection. The overlay takes the mouse and keyboard only while Circle to explain is open. "Send to agent as task context" arrives with agents in **Phase 6**.
- Every model call (questions, walkthroughs, circle to explain) now goes through one function, `ai::call::stream`, so the retry, fallback and budget limits can't be skipped by a new feature.

## Phase 6: agent core

- Circle to explain gets its "Send to agent" action (and the matching default-action option) once agents exist.

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

Accessibility snapping (UI Automation via `uiautomation`, macOS AX via `accessibility-sys`, AT-SPI via `atspi`), privacy features (blocklist, password field blur, offline mode), usage tracking, behavior profiles, onboarding, and "Do it for me" guidance (R7).

## Additional requirements (added by the user after Phase 2)

These come on top of the original brief. Each one names the phase that builds it, and those phases must not ship without it. Reference screenshots of the intended look were shared in the conversation; the descriptions below capture them.

### R1. Agents that act on the computer

- "My desktop looks cluttered, can you clean it up?" spawns a file-organizer agent that sorts the Desktop into folders. It works only in folders the user approved (the Desktop is offered on first use), keeps a backup of every move so the whole cleanup can be undone in one click, and shows its plan before moving anything. **Phase 6** (file tools, backups) with the template in **Phase 8**.
- "I have a meeting tomorrow at 3, remind me" creates a real reminder or calendar event in the OS:
  - macOS: Reminders and Calendar through AppleScript/JXA (`osascript`), which asks for Automation permission the first time.
  - Windows: an Outlook event through its COM interface when Outlook desktop is installed; otherwise a Helpy-scheduled reminder with a Windows notification.
  - Linux: a Helpy-scheduled reminder with a desktop notification (no standard reminders app to write to).
  - **Phase 6**, as OS-action tools behind the normal approval rules.
- **Capability registry:** every OS action declares which operating systems support it. Actions that can't work on the current OS are hidden everywhere (plan cards, templates, settings, suggestions) instead of failing later. **Phase 6.**

### R2. Agent dock and hover cards (refines 7.6 floating cards)

- Running agents show as a vertical stack of small rounded chips at the screen edge. Each chip holds a pointer-shaped mark and glows in its status colour, with a small dot when there's something unseen.
- Status colours: **blue** working, **green** done, **yellow** has a question or needs a choice (with the choices as buttons), **red** error, warning or needs permission.
- Hovering a chip slides out a dark card beside it, pointing at the chip:
  - While running: title in caps, status pill, the current step or command in monospace with a terminal icon, and a thin progress bar.
  - When done: a one-paragraph result, "Suggested next" action chips (for example "Open the app folder", "Show tomorrow's reminders"), and "Follow up" with Text and Voice buttons.
- Only the chips and the open card capture the mouse; everything around them stays clickable. **Phase 6.**

### R3. Extract data from a web page into a CSV

- "Pull the prices from this page into a CSV" spawns a scraping agent for the page on screen (URL read from the browser through the accessibility tree, or asked for).
- Scraping uses a stealth-capable scraper such as Scrapling or Botasaurus, run in a Python environment Helpy manages, so pages that block plain HTTP clients still load. It only fetches pages the user points it at, rate-limits itself, and the user stays responsible for respecting each site's terms.
- The CSV is saved to the projects folder, previewed in the agent card, and can be opened directly. **Phase 8** (with the headless browser).

### R4. Build apps and sites

- "Build me a Mac app that controls my local Spotify with a custom UI" or "Make a web app and launch it" runs a builder agent: an external coding agent (Claude Code, headless) or Helpy's own tools, working in a new folder inside the projects folder, then launching the result.
- Settings: a default projects folder (for example `~/Helpy Projects`) that the user can change, plus per-task subfolders. **Phase 8** (external runners); the folder setting lands in **Phase 6**.

### R5. Agents that stay open for follow-ups

- Tasks without a natural end (an app, a site) don't disappear when a round finishes. The agent waits in a "ready for changes" state. The user says or types the change and the same agent continues with its context.
- Context stays bounded: older turns and large tool outputs are replaced by rolling summaries, with a per-agent context cap, so a long-lived agent never overflows. Summaries are saved with the agent's state. **Phase 6** (state and summaries), **Phase 8** (builder agents).

### R6. Compact voice UI and the hand-off animation

- The voice hotkey shows only a small glowing waveform beside the cursor, with no panel. Clicking it opens the full ask panel with the conversation. **Phase 3.**
- When a request turns out to be an agent task, the waveform slides to the dock and morphs into that agent's chip, taking on its status glow. **Phase 6.**

### R7. "Do it for me" in visual guidance

Deferred from Phase 4 at the user's request, to be built later. When it's turned on (off by default), the step card gets a "Do it" button. Helpy shows exactly where it will click, waits for the user to confirm, then clicks the target itself. One confirmation per click, never a batch. It needs a mouse-control crate (`enigo`) and Accessibility permission on macOS, and it should click the snapped element rather than raw model coordinates, so it lands after accessibility snapping. **Phase 9.**

## Platform risks

1. **macOS transparency needs `macOSPrivateApi: true`.** That flag blocks Mac App Store distribution. Direct download is fine.
2. **Capture exclusion differs per OS.** Windows: `set_content_protected` uses `SetWindowDisplayAffinity`; `WDA_EXCLUDEFROMCAPTURE` needs Windows 10 2004+, older builds show a black box instead of hiding. macOS: `NSWindowSharingNone` works for `CGWindowList` capture, but ScreenCaptureKit on macOS 15+ may still record it. **Still open:** Phase 2 relies on `NSWindowSharingNone` alone; excluding Helpy's windows by window ID needs a ScreenCaptureKit capture path and must be verified on a real Mac. Linux: no content protection at all, so Helpy hides its overlays and the ask panel for about 120 ms around each capture (verified under X11).
3. **Wayland.** No global cursor position, no always-on-top guarantee, no global shortcuts without the portal, input-region click-through only via layer-shell. Helpy sets `GDK_BACKEND=x11` on Linux so it runs under XWayland when available, and the settings window shows a notice listing what won't work on native Wayland.
4. **Click-through on Linux X11** needs a compositor for transparency. Without one, overlays show as black rectangles; the notice mentions this.
5. **Global hotkeys can collide with the OS.** Registration failures are reported back to the Hotkeys section instead of silently ignored.
6. **Per-monitor DPI.** All positions travel in physical pixels in Rust and convert to logical only at the window that renders them, using that monitor's own scale factor.
7. **Gmail restricted scopes** need Google verification for a public client ID. Phase 7 supports a user-provided client ID.
8. **Linux screen capture needs PipeWire headers** at build time (xcap links it for Wayland capture), and saving API keys needs a running Secret Service.
9. **Fullscreen detection on macOS** is a heuristic (a window covering the full display at the top layer). Presentation apps that use a separate Space are detected; borderless windowed games may not be.
