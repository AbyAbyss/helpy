# Demo video narration

`helpy-demo.mp4` runs 2:52. Each scene plays up to 1.35 times faster where it was waiting, and only as fast as its narration allows. The title card now leads with "Works with any model" (Anthropic, OpenAI, Gemini, Ollama, LM Studio, any OpenAI-compatible API) and then names the open models this demo runs on. The cut adds two new scenes (builder agents, skills and triggers after the approval inbox; memory after Settings), a new narration and a new music track. The title card and the two static slides near the end hold a still frame while their lines finish.

The voice is synthesized (OpenAI `tts-1-hd`, voice "fable", speed 1.2), every line with the same voice and the same fixed gain. The music is synthesized in code to the exact length of the cut, with a swell into each scene, so neither needs a licence. It sits about 16 dB under the voice and dips a little further while the voice talks. The mix is -17 LUFS with a true peak of -1.5 dBTP. The agent run, the plan cards and the memory notes are staged with sample data; the UI is Helpy's own React components.

## Title

This is Helpy: an AI companion next to your cursor, or in the notch. It looks at your screen, points at the answer, draws on your apps, and runs agents in the background. Bring any model you like: Anthropic, OpenAI, Gemini, or one running on your own computer. This demo runs entirely on open models, from Nebius Token Factory with NVIDIA Nemotron.

## Cursor buddy, voice and visual guidance

Pip follows your pointer on every monitor. Ask out loud, and Helpy screenshots the monitor under your cursor and highlights the real control. Walkthroughs go a step at a time, and Do it clicks for you after you confirm.

## The notch

Or let Helpy live in the notch. It hides until you hover it. Ask out loud right there, and while agents work it tells you what each one is doing.

## Explaining on screen: shapes, labels, a picture and a chart

Ask it to explain something and it teaches on your screen: shapes in any size and direction, labels and formulas, a reference picture, and a chart of the numbers, all on a click-through overlay while Pip traces along.

## Circle to explain

Circle to explain freezes the screen. Draw around anything and Helpy explains it, labels the parts, copies the text, or translates it.

## Agents: the plan card

Background agents: say what you want done. Nemotron Ultra plans it and shows a card first: each agent, its tools, and what it will ask before acting.

## Agents: Tavily and the Playwright browser at work

Press Start and a copy of the buddy flies to the dock. The agent searches with Tavily, drives a real browser through the Playwright MCP server, clicks through listings and fills in forms. Nemotron Super runs each step.

## Agents: approvals

Typing into a website waits for your OK. Approve it or reject it, and a rejected action never runs.

## Agents: the result

When it finishes you get the result, a chart, the CSV it saved, and next steps. Follow up by text or voice, and undo every file change in one click.

## The agent panel and the approval inbox

The agent panel shows every agent with its timeline, tokens and cost, and the approval inbox gathers everything waiting for you, editable before you approve.

## Builder agents, skills and triggers (new)

Agents build, too. "Build me a pomodoro timer" makes the app in its own folder and opens it, handing the coding to Claude Code or Codex when they're installed. Save any finished request as a skill, and schedule one with a sentence: every weekday at eight.

## Settings

Settings are searchable and instant. Pick where the buddy lives, beside the cursor or in the notch, with a live preview. Add Nebius Token Factory, load its models, and press Use for everything. Connectors reach Gmail, Notion, Slack, GitHub and any MCP server.

## Memory (new)

And it remembers you. Mention your name or the apps you use, and Helpy keeps a short note it brings into later conversations, listed in Settings, where you can edit or forget any.

## Limits

Limits live in Rust, not in the prompt: steps, tool calls, time, and token and cost budgets. Agents are saved after every step, so they survive a restart.

## Nemotron on Nebius

Under the hood, Nemotron 3 Super answers and runs the agents, Nemotron 3 Ultra plans and splits the work, and Kimi K3 reads the screen: twelve targets out of twelve in our grounding benchmark. Helpy also takes Anthropic, OpenAI, Gemini or a local model, but everything you just saw ran on open models.

## Outro

Helpy is open source under the MIT licence, built with Tauri, Rust and React, for Windows, macOS and Linux.
