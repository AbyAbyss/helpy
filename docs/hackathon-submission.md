# Devpost submission drafts

Drafts to edit and paste into Devpost. Not meant to be committed as-is.

## Track

Personal AI

## Tagline (one line)

Ask out loud, and Helpy points at the answer on your real screen, then runs agents for the rest, on NVIDIA Nemotron via Nebius Token Factory.

## What it does

Helpy is a desktop companion that lives next to your cursor. Ask "how do I turn on dark mode here?" by voice and, instead of a paragraph of instructions, it highlights the right control in the app you are using, step by step, checking your screen after each click. Circle anything on screen to have it explained, labelled, translated or copied. For bigger jobs ("research standing desks under £400", "tidy my Downloads folder", "remind me about the dentist tomorrow at 3") it plans background agents, shows you the plan first, and asks before anything it can't undo.

It's private by design: your keys stay in the OS keychain, its own windows never appear in screenshots, password managers and password fields are blanked out, every file change can be undone, and limits on steps, time and money are enforced in Rust, where a model can't talk its way past them.

## How we built it

- Tauri 2 (Rust) and React + TypeScript; macOS, Windows and Linux.
- **NVIDIA Nemotron on Nebius Token Factory does the reasoning.** Nemotron 3 Super answers questions and runs agents; Nemotron 3 Ultra plans agent teams and splits work among helpers. Helpy routes each job to its own model, so the expensive model is used only where it pays off.
- **The screen is read by an open vision model on the same Token Factory.** No Nemotron model there accepts images yet, so we measured: a pointing test (`scripts/grounding-eval.mjs`) renders 12 real web pages in headless Chrome, where every target's exact box is known, and asks each model where to click. Kimi-K3 hit 12/12 (Claude Sonnet 5 hit 10/12), so it became the "eyes"; GLM-5.3-Flash hit 11/12 at about a twentieth of the price.
- **One-click setup:** Helpy reads Token Factory's detailed model list to fill in which models see images and what they cost, and a "Use for everything" button routes every job.
- **Tavily is the agents' view of the web:** search with the relevant part of each page, page reading through Extract, and a deep-research tool on Tavily Research that returns a cited report (it asks first, since it can use many credits).
- Accessibility APIs (UI Automation, AX, AT-SPI) snap highlights onto the real button, so a few pixels of model error don't matter.

## Challenges

- Pointing precision. A highlight 40 px off is worse than none, so we built a repeatable benchmark before choosing a model, rather than guessing.
- A model that can't see and a model that can: Helpy hands a request to the vision model only when it needs the screen, keeping Nemotron for everything else.
- Safety for agents that act on your computer: approvals, undo, and limits enforced outside the model.

## What's next

- A Nemotron vision model on Token Factory, to run fully on Nemotron.
- Running scheduled agents on Nebius Serverless Jobs while the laptop sleeps.

## Feedback on Nebius Token Factory, NVIDIA models and Tavily

(Things we actually ran into while building.)

- **No NVIDIA model on Token Factory accepts images.** All four Nemotron models answer `400 This model does not support image input`. A Nemotron vision-language model (e.g. Nemotron Nano VL) there would let screen-aware apps run end to end on NVIDIA models.
- **`/v1/models` hides the useful details.** The plain list has only ids; `?verbose=true` adds modality and pricing, which is great but hard to discover. A capability flag for tool calling would help too.
- **Metadata and behaviour disagree for Gemma 3 27B.** It's listed as `text->text` but accepts images.
- **MiniCPM-V 4.5 rejects requests that include tools** with a raw vLLM message about `--enable-auto-tool-choice`. Either support auto tool choice or say plainly that the model doesn't take tools.
- **Nemotron 3.5 Lightning is very fast**: about 0.3 s for a simple tool call, which suits voice.
- **The promo-code form shows only a Submit button** until you're logged in, with no message saying why.
- **Tavily:** Research worked well (a cited report in about 40 s). The `/usage` endpoint still showed 0 right after extract and research calls, so it's hard to see what a call cost.

## Built with

Rust, Tauri, React, TypeScript, NVIDIA Nemotron, Nebius Token Factory, Tavily, Whisper, Kimi-K3

---

# Demo video script (under 3 minutes)

Record on macOS with voice on. Keep the cursor visible.

| Time | On screen | Say |
|---|---|---|
| 0:00–0:15 | Desktop, Helpy's buddy by the cursor | "This is Helpy. You ask out loud, and it shows you the answer on your actual screen." |
| 0:15–0:30 | Settings → AI providers → Nebius → Model routing → **Use for everything** | "It runs on NVIDIA Nemotron through Nebius Token Factory: Super answers, Ultra plans, and an open vision model reads the screen." |
| 0:30–1:15 | Open System Settings. Hold the voice key: "How do I change my wallpaper?" Highlight appears, step card, click, next step | "It highlights the real control, waits for my click, checks the screen, and moves to the next step." |
| 1:15–1:40 | Circle a diagram or chart (Alt+Shift+C); labels appear around it | "Circle anything and it explains it, textbook style." |
| 1:40–2:30 | Voice: "Research standing desks under 400 pounds and make me a shortlist." Plan card → approve the Tavily research → chip in the dock turns green → open the result | "Bigger jobs become agents. Nemotron Ultra plans them, I approve the plan, and they use Tavily to search and do deep research, asking before anything costly." |
| 2:30–2:50 | Agent panel: timeline, cost, undo | "Every step, every cent and every file change is visible, and one click undoes a whole run." |
| 2:50–3:00 | Repo page | "Open source, MIT licensed. Thanks to Nebius, NVIDIA and Tavily." |
