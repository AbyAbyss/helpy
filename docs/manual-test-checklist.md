# Helpy manual test checklist

Go through these one at a time and tick each box when Helpy does what's expected.
Restart `npm run tauri dev` first so the latest code is running.

- **Ask as written.** "Say" means hold the voice key and speak; "Type" means use the Ask panel.
- **Note failures.** If something goes wrong, note the item number and the time, so the log can be checked.
- **Costs.** Most items cost a fraction of a cent. Items marked 💰 use more, and Helpy asks first.

---

## 1. Plain answers (Ask panel)

- [ ] **1.1** Type: "What's the difference between RAM and storage?"
  Expected: a short answer in a few sentences.
- [ ] **1.2** Type: "What's the difference between RAM and storage? Show it as a table."
  Expected: one or two sentences, then a table.
- [ ] **1.3** Type: "Explain the Pythagorean theorem with the formula."
  Expected: the formula is shown as proper math, not as `$` signs.
- [ ] **1.4** Type: "Write a Python function that reverses a string."
  Expected: a code block with coloured syntax.
- [ ] **1.5** Type: "Draw a flowchart of how a website loads."
  Expected: a drawn flowchart, not code.
- [ ] **1.6** Type: "Show a pie chart of a typical day: sleep 8 hours, work 8, free time 8."
  Expected: a drawn pie chart in Helpy's colours.
- [ ] **1.7** Type: "Show a bar chart of these sales: Jan 120, Feb 150, Mar 90."
  Expected: a drawn bar chart, not its source.

## 2. Voice answers

- [ ] **2.1** Say: "What's the capital of Japan?"
  Expected: you hear the answer, and the pill shows a short caption.
- [ ] **2.2** Say: "What's the difference between RAM and storage? Show it as a table."
  Expected: you hear a sentence or two, then the Ask panel opens by itself with the table.
- [ ] **2.3** Say: "Draw a pie chart of sleep 8 hours, work 8, free time 8."
  Expected: you hear something, even if it's only "It's on your screen.", and the panel opens with the chart.
- [ ] **2.4** Say something, then press Escape while Helpy is speaking.
  Expected: the speech stops.

## 3. Questions about your screen

- [ ] **3.1** With any app open, type: "What is this app and what can I do here?"
  Expected: Helpy looks at the screen and describes that app.
- [ ] **3.2** With an error message on screen, type: "What does this error mean and how do I fix it?"
  Expected: it reads the error and explains it.
- [ ] **3.3** Set screen access to "Ask each time" in Settings, then repeat 3.1.
  Expected: Helpy asks permission before looking.

## 4. Walkthroughs (it points at real buttons and waits for your click)

- [ ] **4.1** Open System Settings. Say: "How do I change my wallpaper?"
  - The highlight sits exactly on the right control.
  - A step card shows the instruction.
  - After you click, the next step appears.
- [ ] **4.2** Open Chrome. Say: "How do I clear my browsing history?"
  Expected: a step-by-step walkthrough.
- [ ] **4.3** Open python.org. Say: "How do I download Python from this website?"
  Expected: it points at the Downloads menu or button.
- [ ] **4.4** During a walkthrough, press Escape.
  Expected: the walkthrough stops and the marks disappear.
- [ ] **4.5** During any walkthrough, listen to what's spoken.
  Expected: no "Step 1", "Step 2" is read out.

## 5. Explanations drawn on screen

- [ ] **5.1** Say: "Explain the Pythagorean theorem on my screen with a triangle."
  - A triangle is drawn in empty space.
  - The sides and their numbers are in different colours.
  - The text stays readable where lines cross it.
- [ ] **5.2** Say: "Explain how the heart pumps blood, with a picture."
  - A labelled heart picture appears.
  - Arrows and notes are drawn on it.
  - The picture stays on screen through every step.
- [ ] **5.3** Open a diagram or chart, such as a Wikipedia image. Say: "Explain this diagram to me."
  Expected: marks are drawn over the real diagram, one idea per step.
- [ ] **5.4** During an explanation, say something like "go back" or "what does that mean?"
  Expected: Helpy responds to what you said.

## 6. Web lookups (no agents)

- [ ] **6.1** Say: "Show me a chart of the US dollar to Indian rupee exchange rate over the last 5 years, on my screen."
  - No plan card appears.
  - Helpy searches, draws a line chart on screen, then explains it.
- [ ] **6.2** Type: "What's the weather in London this weekend?"
  Expected: a real forecast from a web search.
- [ ] **6.3** Type: "What's the latest iPhone and how much does it cost in the UK?"
  Expected: current facts with a price.
- [ ] **6.4** Type: "Chart Bitcoin's price over the last 12 months."
  Expected: a chart of real figures. No made-up data and no build job.

## 7. Several requests in one message, links and memory

- [ ] **7.1** Type: "What time is it in Tokyo, what's 15% of 240, and open youtube.com."
  Expected: all three are handled in one reply, and YouTube opens.
- [ ] **7.2** Type: "Open github.com."
  Expected: it opens in your browser.
- [ ] **7.3** Type: "Remember that I live in London and prefer prices in pounds."
  Expected: Helpy says it saved this.
- [ ] **7.4** In a new conversation, type: "What's a good price for a coffee machine?"
  Expected: the answer uses pounds and the UK.

## 8. Circle to explain (Alt+Shift+C)

- [ ] **8.1** Circle a chart or diagram and ask for an explanation.
  Expected: labels appear around it, with an explanation.
- [ ] **8.2** Circle text in another language and ask for a translation.
  Expected: a translation.
- [ ] **8.3** Circle a block of text and ask to copy it as text.
  Expected: the text is on your clipboard.
- [ ] **8.4** Circle a product photo and ask to "find this cheaper".
  Expected: it's handed to an agent, and a plan card appears.

## 9. Agents (a plan card appears; you approve it)

- [ ] **9.1** Say: "Research the best badminton racket for control in the UK under £150."
  - The plan card appears straight away, with no typed answer first.
  - Approve it. A chip appears in the dock, then turns green.
  - The result has a table and a drawn price chart, not chart code.
- [ ] **9.2** On the finished agent from 9.1, press **Show on my screen**.
  Expected: the Ask panel opens with a "Research result" chip, and the shortlist is drawn and explained on screen.
- [ ] **9.3** On the same agent, click one of its suggestion chips, or type "make it shorter".
  Expected: the agent continues with your follow-up.
- [ ] **9.4** Say: "Compare the MacBook Air and the Dell XPS 13 for a student."
  Expected: agents start, and the result has a comparison table.
- [ ] **9.5** Say: "Build me a website for my bakery."
  - The plan card appears straight away.
  - Approve it. It builds the site, asking before each Claude Code step, and opens it.
- [ ] **9.6** Say: "Build me a Pomodoro timer app."
  Expected: same as 9.5.
- [ ] **9.7** Say: "Tidy my Downloads folder into folders by type."
  - The plan card lists the folder.
  - Files are moved.
  - **Undo** in the agent panel puts them back.
- [ ] **9.8** Say: "Remind me to call the dentist tomorrow at 3pm."
  Expected: a reminder is created and shown.
- [ ] **9.9** Say: "Research three standing desks under £400, then write me a one-page summary."
  Expected: a plan with more than one agent, with the writer waiting for the researcher.
- [ ] **9.10** In the agent panel, open any finished agent.
  Expected: the timeline, cost and token counts are visible.
- [ ] **9.11** While an agent is running, press Pause, then Resume, then Cancel.
  Expected: each one works.

## 10. Uses more credits (💰, try once at most)

- [ ] **10.1** 💰 Type: "Do deep research on whether solar panels are worth it in the UK in 2026."
  - Helpy asks before using Tavily Research.
  - The result is a cited report.

## 11. Connected services (only if connected in Settings → Connectors)

- [ ] **11.1** Type: "What's on my calendar today?"
  Expected: your real events.
- [ ] **11.2** Type: "Give me my morning briefing."
  Expected: calendar plus email. This needs Gmail and Calendar.
- [ ] **11.3** Type: "What are my open GitHub issues?"
  Expected: your real issues.

## 12. Attachments (Ask panel)

- [ ] **12.1** Attach a PDF and type: "Summarize this."
  Expected: a summary, and the PDF shows as a chip.
- [ ] **12.2** Attach a photo and type: "What's in this picture?"
  Expected: a description.

## 13. Settings and safety

- [ ] **13.1** Settings → AI providers → Nebius → **Use for everything**.
  Expected: every job is routed to a Nebius model.
- [ ] **13.2** Pause screen capture, then ask 3.1 again.
  Expected: Helpy says it answered without looking.
- [ ] **13.3** Open a password manager or a password field, then ask 3.1.
  Expected: it's blanked out in what Helpy sees.
- [ ] **13.4** Settings → Guidance: change the highlight colour.
  Expected: walkthrough highlights use the new colour.

## 14. Demo video run-through

Practise this order for the video, and tick each one once it runs cleanly:

- [ ] **14.1** Run 13.1, **Use for everything**.
- [ ] **14.2** Run 4.1, the wallpaper walkthrough.
- [ ] **14.3** Run 5.2 or 6.1, the heart picture or the rupee chart.
- [ ] **14.4** Run 8.1, circling a diagram.
- [ ] **14.5** Run 9.1, then 9.2: racket research, then **Show on my screen**.
- [ ] **14.6** Run 9.10: the agent panel's timeline, cost and undo.

## 15. Hackathon checks (Nebius x NVIDIA)

### Showing Nemotron, Token Factory and Tavily in the app

- [ ] **15.1** Type: "What can you help me with?"
  Expected: the model chip at the top of the Ask panel shows `nvidia/nemotron-3-super-120b-a12b`.
- [ ] **15.2** Run any agent, such as 9.1. In the agent panel, look at the plan.
  Expected: Nemotron Ultra planned it and Nemotron Super ran it, as shown in the agent's model and timeline.
- [ ] **15.3** Run a walkthrough, such as 4.1, then check the Ask panel.
  Expected: Nemotron Super took the question and Kimi-K3, an open vision model on Token Factory, read the screen.
- [ ] **15.4** Run 6.1 or 9.1, then open the agent timeline or the answer.
  Expected: the web lookups went through Tavily, shown as Tavily search, extract or research results.
- [ ] **15.5** Settings → Usage.
  Expected: the cost of today's tests is shown per feature and per model.
- [ ] **15.6** In Settings → AI providers, turn off every provider except Nebius, then run 1.1, 4.1 and 9.1.
  Expected: all three still work, so the whole app runs on Token Factory.

### Questions judges might ask (have an answer ready)

- [ ] **15.7** "Where exactly is Nemotron used?"
  Answer: Super answers questions, runs agents and decides which tool to use. Ultra plans agent teams and splits work among helpers.
- [ ] **15.8** "Why isn't Nemotron reading the screen?"
  Answer: no Nemotron model on Token Factory accepts images yet; all four return 400. We measured the vision models with `scripts/grounding-eval.mjs` and picked Kimi-K3, which scored 12/12.
- [ ] **15.9** "How do you keep costs down?"
  Answer: each job goes to its own model. The cheap Super handles most of the work, and Ultra and the vision model are used only where they pay off. Limits on steps, time and money are enforced in Rust.
- [ ] **15.10** "How is it safe to let agents act on my computer?"
  Answer: you approve the plan first, it asks before risky actions, every file change can be undone, keys stay in the OS keychain, and password fields are blanked out.
- [ ] **15.11** "What does Tavily add?"
  Answer: search returns the relevant part of each page, Extract reads pages, and Research produces cited reports. Research asks first, since it uses more credits.
- [ ] **15.12** "What's next?"
  Answer: a Nemotron vision model on Token Factory, to run fully on Nemotron, and scheduled agents on Nebius Serverless Jobs.

### Submission requirements (Devpost, deadline Oct 30, 2026 10:00am PDT)

- [ ] **15.13** The repo is public: https://github.com/AbyAbyss/helpy
- [ ] **15.14** The MIT license is in the repo.
- [ ] **15.15** The README explains how Nemotron and Token Factory are used, and how to set up a Nebius key.
- [ ] **15.16** A downloadable build exists. Re-run Actions → Release, check the macOS and Windows installers, then publish the draft release.
- [ ] **15.17** Install the released build on a clean account and run 13.1, 1.1 and 4.1.
- [ ] **15.18** The demo video is 3 minutes or less, follows section 14, and Nemotron is named out loud.
- [ ] **15.19** The video is uploaded (YouTube or Vimeo) and the link works when you're logged out.
- [ ] **15.20** The track is set to **Personal AI**.
- [ ] **15.21** The Devpost text is pasted from `docs/hackathon-submission.md`: tagline, what it does, how it was built, challenges and what's next.
- [ ] **15.22** "Built with" lists NVIDIA Nemotron, Nebius Token Factory and Tavily.
- [ ] **15.23** The feedback section is filled in, for the Most Valuable Feedback prize, from the notes in `docs/hackathon-submission.md`.
- [ ] **15.24** Also enter the Best Use of Tavily prize.
- [ ] **15.25** Before the deadline, the submission is marked **Submitted**, not draft.

## 16. Show the judges: tasks that show off Helpy

These are picked to impress in a demo. Each one shows a different strength, and together they cover the Personal AI track, Nemotron, Token Factory and Tavily. Practise each one until it works first time. The **Shows** line is what to point out while it runs.

### Everyday "help me on my screen"

- [ ] **16.1** In System Settings, say: "How do I turn on dark mode?"
  - **Shows:** Helpy points at the real control and waits for your click, rather than giving you a paragraph to read.
- [ ] **16.2** In Gmail or Outlook on the web, say: "How do I set up an out-of-office reply?"
  - **Shows:** a multi-step walkthrough in an app it hasn't seen before.
- [ ] **16.3** In a spreadsheet (Google Sheets or Numbers), say: "How do I make a chart from this data?"
  - **Shows:** it reads your actual data and guides you through charting it.
- [ ] **16.4** With a confusing settings page or error dialog open, say: "What does this mean and what should I do?"
  - **Shows:** it understands whatever is on screen right now.

### Learning and explaining, like a teacher at a whiteboard

- [ ] **16.5** Say: "Explain the Pythagorean theorem on my screen with a triangle."
  - **Shows:** it draws, writes and colour-codes step by step while speaking.
- [ ] **16.6** Say: "Explain how the heart pumps blood, with a picture."
  - **Shows:** it finds a real diagram and annotates it live.
- [ ] **16.7** Open a chart in a news article or on Wikipedia. Say: "Explain this chart to me."
  - **Shows:** it explains marks drawn on top of someone else's content.
- [ ] **16.8** Circle a paragraph in a foreign language (Alt+Shift+C) and ask for a translation.
  - **Shows:** Circle to explain works on any app.
- [ ] **16.9** Circle a maths formula or code snippet and ask: "Explain this simply."
  - **Shows:** explanations for students and developers.

### Live web facts drawn on screen (Tavily and Nemotron)

- [ ] **16.10** Say: "Show me a chart of the US dollar to Indian rupee exchange rate over the last 5 years, on my screen."
  - **Shows:** in one flow, Tavily search finds real data, the chart is drawn on screen, and then it's explained.
- [ ] **16.11** Say: "How has the price of gold changed this year? Show it as a chart."
  - **Shows:** fresh data, never made up.
- [ ] **16.12** Say: "What's the weather in London this week, and should I take an umbrella tomorrow?"
  - **Shows:** a quick lookup with a judgement call.

### Agents that do the work (Nemotron Ultra plans, Super works)

- [ ] **16.13** Say: "Research the best noise-cancelling headphones under £200 and give me a shortlist."
  - **Shows:** the plan card, the agent working in the dock, then a table and a price chart. Press **Show on my screen** at the end.
- [ ] **16.14** Say: "Research three standing desks under £400, then write me a one-page summary."
  - **Shows:** a team of agents, with the writer waiting for the researcher.
- [ ] **16.15** Say: "Plan a 3-day trip to Edinburgh on a budget: what to see, where to eat, and how to get around."
  - **Shows:** personal planning, with a structured result and sources.
- [ ] **16.16** Say: "Tidy my Downloads folder into folders by type." When it finishes, press **Undo**.
  - **Shows:** an agent acting on your real files, with full undo.
- [ ] **16.17** Say: "Remind me to submit the hackathon project on October 29th at 6pm."
  - **Shows:** a real reminder, created by voice.
- [ ] **16.18** Say: "Build me a simple expense tracker web app."
  - **Shows:** an app built and opened, with approval at each step.
- [ ] **16.19** 💰 Say: "Do deep research on whether solar panels are worth it in the UK in 2026."
  - **Shows:** Tavily Research, a cited report, and Helpy asking before anything costly.

### Several things at once

- [ ] **16.20** Say: "What's the time in New York, convert 250 pounds to dollars, and open the Nebius website."
  - **Shows:** one spoken message, three different tools.
- [ ] **16.21** Say: "Explain what a GPU does, and research the best laptop GPUs for AI under £1500."
  - **Shows:** in one request, it answers the first part directly and hands the second part to agents.

### Safety and trust (good for judges' questions)

- [ ] **16.22** Open a page with a password field, then ask: "What's on my screen?"
  - **Shows:** the password field is blanked out in what Helpy sees.
- [ ] **16.23** Start an agent that wants to open a website.
  - **Shows:** it asks before opening a link in your browser.
- [ ] **16.24** Open the agent panel on any finished agent.
  - **Shows:** every step, tool call, token and penny it used.
- [ ] **16.25** Settings → Usage.
  - **Shows:** what the whole demo cost, by model.

### Best 3-minute order for the judges

- [ ] **16.26** Practise this order: 16.1 (dark mode) → 16.5 or 16.6 (explain with drawing) → 16.10 (live chart) → 16.13 (agent research, then **Show on my screen**) → 16.24 (transparency).
  - Mention the models as you go: "Nemotron Super answers, Nemotron Ultra plans, Kimi-K3 reads the screen, all on Nebius Token Factory, with Tavily for the web."
