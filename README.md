# Helpy

Helpy is a desktop AI companion that lives next to your mouse cursor. You ask it something out loud, it looks at your screen, and it answers by drawing highlights, pointers and arrows on top of your real apps. It can also run AI agents in the background that you start just by talking.

Built with Tauri v2 (Rust) and React + TypeScript. Windows and macOS are first class, Linux X11 is supported, Linux Wayland is best effort.

The full phase plan, crate list and platform risks are in [docs/PLAN.md](docs/PLAN.md).

## What works today (Phases 1 to 4)

- **Cursor buddy.** A small character follows the pointer on every monitor, with per-monitor DPI handled in Rust. Three built-in styles (Pip, Spark, Dot) or your own SVG/PNG. Size, opacity, offset and follow smoothness are adjustable. It auto-hides in fullscreen apps and, optionally, when the mouse rests.
- **Overlays.** One transparent, always-on-top, click-through window per monitor. They're hidden from the taskbar and excluded from screen capture (`WDA_EXCLUDEFROMCAPTURE` on Windows, `NSWindowSharingNone` on macOS). They are rebuilt when monitors are plugged in or removed.
- **Tray.** Toggle the buddy, toggle voice guidance, pause screen capture, switch behavior profile, open settings, quit. The icon changes when capture is paused. Agent panel and approval inbox are shown but disabled until their phase.
- **Global hotkeys.** All nine actions can be rebound. Duplicates are rejected, and combinations the OS already uses get a warning. The hotkeys for actions that exist now (voice ask, text ask, open settings, pause capture, clear annotations) are registered with the OS. The others are saved and marked "Not active yet".
- **Settings window.** Search (Ctrl/Cmd+F), instant apply, per-section reset, JSON import/export, inline validation, and light/dark/system theme. Sections: General, Cursor buddy (with a live preview that follows your mouse), Hotkeys, AI providers, Answer style, Visual guidance (with a live preview), Voice input, Voice output.
- **AI providers.** Anthropic, OpenAI, Google Gemini, Ollama, LM Studio, llama.cpp server and any OpenAI-compatible endpoint, all streaming. Add a provider from a preset, find local models with one click, load a provider's model list, test the connection, and choose which model each feature uses. API keys go in the OS keychain.
- **Text questions (Alt+Shift+T).** A panel opens next to the cursor. Answers stream in, and follow-ups keep the context until you close it (Esc). Answer style decides whether Helpy sends a screenshot with every question, asks you first, or lets the model decide. Screenshots are of the monitor under the cursor, downscaled to 1568 px, and never include Helpy's own windows.
- **Voice (Alt+Shift+Space).** Hold to talk, or press to start and stop. A small glowing waveform appears by the cursor, shows what you're saying, then a short caption of the answer; click it for the full conversation. Speech recognition runs on your computer with Whisper, or through OpenAI or Deepgram. Answers are read aloud sentence by sentence as they arrive, in a system voice, a Piper voice or an OpenAI voice. Esc stops listening, answering and speaking. Optional wake word ("hey helpy"), noise suppression, auto-stop on silence, and a microphone picker with a live level meter.
- **Visual guidance.** Ask "where is…" or "how do I…" about the app in front of you and Helpy shows you instead of only telling you: it highlights the control (optionally dimming the rest of the screen), points at it with a labelled pointer, or draws an arrow, all on the click-through overlay. Walkthroughs go one step at a time. A small step card ("Step 2 of 4", Next, Repeat, Stop) is the only clickable part. Click the highlighted spot (or press Next) and Helpy takes a fresh screenshot to check the result before showing the next step. Esc or Stop ends it. Steps are read aloud when voice guidance is on, which you can switch from the card. Models with tool calling use a `show_step` tool; others reply with a JSON step, which is validated the same way. Coordinates outside the screenshot are sent back to the model to correct. A walkthrough stops after 12 steps by default.
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
                                  # and guidance steps (validation, overlay mapping, click hit-testing)
```

## Project layout

```
src-tauri/src/
  settings/     typed schema (source of truth), validation, persistence, commands
  ai/           provider adapters (anthropic, openai, gemini), SSE, limits, ledger,
                keychain secrets, and the ask flow
  capture.rs    screenshots of the cursor's monitor, and model-to-screen coordinate mapping
  guide/        guidance steps (schema, validation, mapping), the step card, click detection
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
  settings/     settings window app; registry.ts lists every setting's label and control
design/         source SVGs for the app and tray icons
```

### Adding a setting

1. Add the field and its default in `src-tauri/src/settings/schema.rs`, and any range check in `validate.rs`.
2. Run `npm run bindings` to regenerate the TypeScript types.
3. Add one entry to `FIELDS` in `src/settings/registry.ts`. A test fails if a setting in a shown section has no entry.

## Platform notes

- **macOS:** transparent windows need `macOSPrivateApi`, which rules out the Mac App Store. Direct downloads are fine. Screenshots need the Screen Recording permission (onboarding for it arrives in Phase 9; until then macOS asks on first capture).
- **Clicks during walkthroughs** are observed with a listen-only mouse hook (`rdev`); the click still reaches your app. On macOS this needs the Accessibility permission, and on Wayland it isn't available. Without it, the step card asks you to press Next instead.
- **Linux:** overlays need a compositing window manager to be transparent; the settings page warns when none is running. On Wayland, Helpy runs through XWayland when it can. The General section lists what won't work in your session.
