# Demo video narration

Two cuts of the same picture: `helpy-demo.mp4` has this narration over a quiet music bed, `helpy-demo-music.mp4` has the music alone. Both run 2:58.

The voice is synthesized (Kokoro, British English) and the music is a lo-fi bed rendered from a MIDI score with a General MIDI soundfont, so neither needs a licence. The agent run is staged with sample data and fictional shops.

## Title

This is Helpy: an AI companion that lives next to your cursor. It looks at your screen, points at the answer, draws on your apps and runs agents in the background, all on open models from Nebius Token Factory, with NVIDIA Nemotron doing the thinking.

## Cursor buddy, voice and visual guidance

Pip follows your pointer on every monitor. Ask out loud, and Helpy screenshots the monitor under your cursor, never its own windows, and highlights the real control. Walkthroughs go a step at a time, and Do it clicks for you after you confirm.

## Explaining on screen: shapes, labels, a picture and a chart

Ask it to explain something and it teaches on your screen: it draws shapes in any size and direction, writes labels and formulas, drops in a reference picture, and charts the numbers, all on a click-through overlay while Pip traces along.

## Circle to explain

Circle to explain freezes the screen. Draw around anything and Helpy explains it, labels the parts, copies the text or translates it.

## Agents: the plan card

Background agents. Say what you want done. Nemotron Ultra plans it and shows a card first: each agent, its tools, and what it will ask before acting.

## Agents: Tavily and the Playwright browser at work

Press Start and a copy of the buddy flies to the dock. The agent searches with Tavily, drives a real browser through the Playwright MCP server, reads every page as text, clicks through listings and fills in forms. Nemotron Super runs each step.

## Agents: approvals

Typing into a website waits for your OK. Approve it, or reject it, and a rejected action never runs.

## Agents: the result

When it finishes you get the result, a chart, the CSV it saved and next steps. Follow up by text or voice, and undo every file change in one click.

## The agent panel and the approval inbox

The agent panel shows every agent with its timeline, tokens and cost, and the approval inbox gathers everything waiting for you, editable before you approve.

## Settings

Settings are searchable and instant. Add Nebius Token Factory, load its models, and press Use for everything. Agents search with Tavily, work only in folders you approve, and connect to Gmail, Notion, Slack, GitHub and any MCP server, each action set to allow, ask or never. Privacy blanks password managers out of every screenshot, and the memory page shows what Helpy remembers.

## Limits

Limits live in Rust, not in the prompt: steps, tool calls, time, and token and cost budgets. Agents are saved after every step, so they survive a restart.

## Nemotron on Nebius

Under the hood, Nemotron 3 Super answers and runs the agents, Nemotron 3 Ultra plans and splits the work, and Kimi K3 reads the screen: the most accurate pointer in our grounding benchmark, twelve targets out of twelve.

## Outro

Helpy is open source under the MIT license, built with Tauri, Rust and React for Windows, macOS and Linux.
