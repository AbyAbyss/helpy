# Helpy

Helpy is a desktop AI companion that lives next to your mouse cursor. You ask it something out loud, it looks at your screen, and it answers by drawing highlights, pointers and arrows on top of your real apps. It can also run AI agents in the background that you start just by talking.

Built with Tauri v2 (Rust) and React + TypeScript. Windows and macOS are first class, Linux X11 is supported, Linux Wayland is best effort.

The full phase plan, crate list and platform risks are in [docs/PLAN.md](docs/PLAN.md).

## What works today (Phases 1 to 6)

- **Cursor buddy.** A small character follows the pointer on every monitor, with per-monitor DPI handled in Rust. Three built-in styles (Pip, Spark, Dot) or your own SVG/PNG. Size, opacity, offset and follow smoothness are adjustable. It auto-hides in fullscreen apps and, optionally, when the mouse rests.
- **Overlays.** One transparent, always-on-top, click-through window per monitor. They're hidden from the taskbar and excluded from screen capture (`WDA_EXCLUDEFROMCAPTURE` on Windows, `NSWindowSharingNone` on macOS). They are rebuilt when monitors are plugged in or removed.
- **Tray.** Toggle the buddy, toggle voice guidance, pause screen capture, circle to explain, open the agent panel, switch behavior profile, open settings, quit. The icon changes when capture is paused. The approval inbox is shown but disabled until Phase 7.
- **Global hotkeys.** All nine actions can be rebound. Duplicates are rejected, and combinations the OS already uses get a warning. The hotkeys for actions that exist now (voice ask, text ask, circle to explain, open agent panel, pause all agents, open settings, pause capture, clear annotations) are registered with the OS. The others are saved and marked "Not active yet".
- **Settings window.** Search (Ctrl/Cmd+F), instant apply, per-section reset, JSON import/export, inline validation, and light/dark/system theme. Sections: General, Cursor buddy (with a live preview that follows your mouse), Hotkeys, AI providers, Answer style, Visual guidance (with a live preview), Agents, Circle to explain, Voice input, Voice output.
- **AI providers.** Anthropic, OpenAI, Google Gemini, Ollama, LM Studio, llama.cpp server and any OpenAI-compatible endpoint, all streaming. Add a provider from a preset, find local models with one click, load a provider's model list, test the connection, and choose which model each feature uses. API keys go in the OS keychain.
- **Text questions (Alt+Shift+T).** A panel opens next to the cursor. Answers stream in, and follow-ups keep the context until you close it (Esc). Answer style decides whether Helpy sends a screenshot with every question, asks you first, or lets the model decide. Screenshots are of the monitor under the cursor, downscaled to 1568 px, and never include Helpy's own windows.
- **Voice (Alt+Shift+Space).** Hold to talk, or press to start and stop. A small glowing waveform appears by the cursor, shows what you're saying, then a short caption of the answer; click it for the full conversation. Speech recognition runs on your computer with Whisper, or through OpenAI or Deepgram. Answers are read aloud sentence by sentence as they arrive, in a system voice, a Piper voice or an OpenAI voice. Esc stops listening, answering and speaking. Optional wake word ("hey helpy"), noise suppression, auto-stop on silence, and a microphone picker with a live level meter.
- **Visual guidance.** Ask "where is…" or "how do I…" about the app in front of you and Helpy shows you instead of only telling you: it highlights the control (optionally dimming the rest of the screen), points at it with a labelled pointer, or draws an arrow, all on the click-through overlay. Walkthroughs go one step at a time. A small step card ("Step 2 of 4", Next, Repeat, Stop) is the only clickable part. Click the highlighted spot (or press Next) and Helpy takes a fresh screenshot to check the result before showing the next step. Esc or Stop ends it. Steps are read aloud when voice guidance is on, which you can switch from the card. Models with tool calling use a `show_step` tool; others reply with a JSON step, which is validated the same way. Coordinates outside the screenshot are sent back to the model to correct. A walkthrough stops after 12 steps by default.
- **Circle to explain (Alt+Shift+C).** The screen freezes under a light dim, and you drag a box or draw freehand around anything. Helpy explains what's in it. For diagrams, pictures and busy interfaces it also labels the parts, textbook style, with labels placed around the selection that never overlap each other or the card; click a label for more about that part. The other actions are one click away: copy the text in it (read by the vision model, straight to the clipboard), translate it, summarize it, or ask your own follow-up questions about it. Only while this is open does the overlay take the mouse, and only on that monitor; Esc, the close button or a click outside gives every click back to your apps. It uses the model routed to Circle to explain, or the questions model.
- **Agents.** Ask for background work in your own words ("clean up my desktop", "remind me about the dentist tomorrow at 3", "research standing desks under £400"), by voice or text, from the agent panel, or from a circled part of the screen ("To agent"). Helpy plans one to five agents and shows a plan card first: each agent's goal and tools, what it will ask you before doing, and any folder it needs. Start with Enter, a click, or by saying "yes, go". Several agents run all at once or one after another, under a running limit.
  - Agents can search the web (DuckDuckGo by default, rate limited; Brave with an API key; or your own SearXNG), read public web pages (never local or private addresses), work with files only in folders you approved, run shell commands under your shell policy, and add reminders and calendar events: Reminders and Calendar on macOS, Outlook on Windows when it's installed, Helpy notifications otherwise. Tools a computer can't support are never offered.
  - Every file change is backed up and a whole run can be undone in one click. File changes go ahead by default because they're undoable; deletes, reminders and commands ask first. Approvals, questions and "retry, skip or cancel?" come to you on the agent's card; a rejected action never runs.
  - Limits are enforced in Rust and can't be talked around: steps and tool calls per agent, a time limit, per-agent, per-batch and daily token and cost budgets checked before every call (failed attempts count), and stuck detection (the same action or reply repeated, or several steps with nothing new). Long-running agents summarize older work to stay within their model. Agents are saved after every step, so an agent running when Helpy quits comes back paused with its limits where they were.
  - **The dock:** a glowing chip per agent at the screen edge. Blue is working, green done, yellow a question, red needs permission or went wrong. Hover a chip for its card: what it's doing in plain words, the last command, progress, and the buttons it needs (approve, answer, retry, raise a budget once, undo). Finished open-ended agents (an app, a site) stay ready for changes; follow up by text or voice and the same agent carries on. When a spoken request becomes an agent, the voice waveform glides into the dock and becomes its chip.
  - **The agent panel (Alt+Shift+A):** every agent grouped by request, with its live status, full result, a timeline of what it did, tokens, cost and time, and pause, resume, cancel, retry, run again, rename, remove, export to Markdown and undo.
- **Retry and budget limits, enforced in Rust.** Timeouts, network errors, rate limits (honoring `retry-after`), provider 5xx errors and garbled output are retried with exponential backoff, up to 3 times by default. Bad keys, missing models and refusals are never retried on the same model. Fallback models get one attempt each and count toward the retry limit. A daily token limit (and an optional cost limit) is checked before every call, retries included, and spending is saved to disk so it survives restarts.

## Run it

Prerequisites: Node 20+, Rust stable, and the [Tauri system dependencies](https://tauri.app/start/prerequisites/). On Debian/Ubuntu:

```sh
sudo apt install libwebkit2gtk-4.1-dev build-essential libxdo-dev libssl-dev \
  libayatana-appindicator3-dev librsvg2-dev libpipewire-0.3-dev libclang-dev \
  libasound2-dev libspeechd-dev cmake libxtst-dev libgbm-dev
```

PipeWire is used for screen capture on Wayland, ALSA for the microphone, speech-dispatcher for system voices, XTest/XRecord for noticing clicks during walkthroughs, and CMake builds whisper.cpp (on macOS and Windows too). API keys are stored through the Secret Service (GNOME Keyring or KWallet), so one of those needs to be running to save keys on Linux; local providers work without it.

Then:

```sh
npm install
npm run tauri dev
```

Helpy starts in the tray. Open settings from the tray menu or with Alt+Shift+S. To have the window open on launch instead, turn off **General → Start in the tray**.

To ask questions, add a provider under **Settings → AI providers**. The quickest free option is [Ollama](https://ollama.com) with a vision model such as `llama3.2-vision`: start it, click **Add provider → Find models on this computer**, and press Alt+Shift+T anywhere.

## Tests

```sh
npm test                          # frontend: key handling, follow smoothing, settings registry
cd src-tauri && cargo test        # backend: settings, hotkeys, monitor math, provider request mapping,
                                  # SSE parsing, screenshot mapping, the retry/budget limits, and audio
                                  # (resampling, silence detection, noise suppression, sentence splitting),
                                  # guidance steps (validation, overlay mapping, click hit-testing), and
                                  # circle to explain (cropping, the PARTS reply, model choice), and agents:
                                  # a scripted fake provider and fake tools prove every limit (steps, tool
                                  # calls, time, budgets, repeats, no progress, retries, approvals, restarts),
                                  # plus file undo, path escapes, private-address fetches and search parsing
```

## Project layout

```
src-tauri/src/
  settings/     typed schema (source of truth), validation, persistence, commands
  ai/           provider adapters (anthropic, openai, gemini), SSE, limits, ledger,
                keychain secrets, and the ask flow
  capture.rs    screenshots of the cursor's monitor, and model-to-screen coordinate mapping
  guide/        guidance steps (schema, validation, mapping), the step card, click detection
  circle/       circle to explain: selection mode, cropping, the PARTS reply, actions
  agents/       the agent runner and its limits, planner, tools (files with undo, web,
                search adapters, shell, reminders), SQLite store, scheduler, commands
  voice/        microphone, audio processing, Whisper and cloud speech to text, text to speech
                (system, Piper, OpenAI), wake word, and the listening session
  overlay.rs    per-monitor overlay windows
  cursor.rs     cursor polling, per-monitor coordinates, buddy visibility
  fullscreen/   fullscreen-app detection for Windows, macOS and X11
  hotkeys.rs    global hotkey registration and conflict warnings
  tray.rs       tray menu and icon state
src/
  bindings/     TypeScript types generated from the Rust schema (do not edit)
  buddy/        the buddy character and smoothing, shared by overlay and settings
  overlay/      overlay window app
  ask/          the ask panel
  pill/         the compact voice waveform
  guide/        highlights, pointers and arrows, shared by the overlay and the settings preview
  step/         the walkthrough step card
  circle/       selection, the result card, and diagram label placement
  plan/         the agent plan card
  dock/         the agent dock and its hover cards
  agents/       the agent panel
  settings/     settings window app; registry.ts lists every setting's label and control
design/         source SVGs for the app and tray icons
```

### Adding a setting

1. Add the field and its default in `src-tauri/src/settings/schema.rs`, and any range check in `validate.rs`.
2. Run `npm run bindings` to regenerate the TypeScript types.
3. Add one entry to `FIELDS` in `src/settings/registry.ts`. A test fails if a setting in a shown section has no entry.

## Platform notes

- **macOS:** transparent windows need `macOSPrivateApi`, which rules out the Mac App Store. Direct downloads are fine. Screenshots need the Screen Recording permission (onboarding for it arrives in Phase 9; until then macOS asks on first capture).
- **The step card is frosted glass** on macOS (vibrancy) and Windows 11 22H2 or later (Acrylic). On Linux and older Windows it's an opaque dark card.
- **Agent notifications** use the system's notification service (on Linux, a notification daemon has to be running). On macOS, the first reminder or calendar event asks for Automation permission for Reminders or Calendar.
- **Clicks during walkthroughs** are observed with a listen-only mouse hook (`rdev`); the click still reaches your app. On macOS this needs the Accessibility permission, and on Wayland it isn't available. Without it, the step card asks you to press Next instead.
- **Linux:** overlays need a compositing window manager to be transparent; the settings page warns when none is running. On Wayland, Helpy runs through XWayland when it can. The General section lists what won't work in your session.
