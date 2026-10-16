# Demo video narration

The script read over `helpy-demo.mp4`, scene by scene. The voice in the file is synthesized (espeak-ng with an MBROLA voice); re-record it in your own voice over the same picture if you prefer.

## Title

Meet Helpy: an AI companion that lives next to your cursor. Ask out loud, and it looks at your screen and points at the answer. It's our entry for the Nebius and NVIDIA hackathon, and with one click it runs entirely on open models from Nebius Token Factory, with Nemotron doing the reasoning.

## Cursor buddy, voice and visual guidance

Pip, the cursor buddy, follows your pointer on every monitor. Hold a hotkey and talk. Helpy screenshots the monitor under your cursor, never including its own windows, and highlights the right control on top of your real app. Walkthroughs go one step at a time, highlights snap onto the real button, and Do it clicks for you after you confirm that one click.

## Circle to explain

Circle to explain freezes the screen. Draw around anything, and Helpy explains it, labels the parts of a diagram, copies the text, or translates it.

## Agents: the plan card

Now the part we're proudest of: background agents. Just say what you want done. Helpy plans it with Nemotron Ultra and shows a plan card first: each agent, its tools, and what it will ask you before doing.

## Agents: the Playwright browser at work

Press Start, and a copy of the buddy flies to the dock. The agent drives a real browser through the Playwright MCP server. It opens pages, reads each one as text with every button and field, clicks through listings, fills in search boxes, and scrolls where it has to. Nemotron Super runs these steps, because its tool calls are fast and reliable.

## Agents: approvals

Typing into a website waits for your OK. Approve it on the card, or reject it. A rejected action never runs.

## Agents: the result

When it's done, the card shows the result, the CSV it saved, and a few next steps. Follow up by text or voice, and every file change can be undone in one click.

## Limits and privacy

Limits live in Rust, not in the prompt: steps, tool calls, time, and token and cost budgets per agent, per batch and per day, checked before every call. Agents are saved after every step, so they come back after a restart. Password managers are blanked out of every screenshot, and offline mode keeps everything on your own machine.

## Nemotron on Nebius

Under the hood: Nemotron 3 Super answers questions and runs the agents. Nemotron 3 Ultra plans and splits the work, where the strongest reasoning pays off. Kimi K3 reads the screen, because in our grounding benchmark on Token Factory it hit all twelve targets with a median error of one pixel. And Tavily gives agents search, page extraction and deep research.

## And more

There's more: an agent panel with timelines, spending and undo. Templates and triggers for work that repeats. Connectors for Gmail, Calendar, Notion, Slack and GitHub, plus any MCP server, each action with its own Allow, Ask or Never rule.

## Outro

Helpy is open source under the MIT license, built with Tauri, Rust and React, for Windows, macOS and Linux. Thanks for watching.
