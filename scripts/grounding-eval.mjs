// How well can a model point at things on screen? Renders real web pages in
// headless Chrome, where every target's exact box is known, asks each model
// where to click, and scores the answers.
//
//   node scripts/grounding-eval.mjs --list nebius
//   node scripts/grounding-eval.mjs nebius:nvidia/<vision-model> anthropic:claude-sonnet-5
//
// GROUNDING_EXTRA adds fields to every request, e.g.
//   GROUNDING_EXTRA='{"chat_template_kwargs":{"thinking":false}}'
//
// Keys come from NEBIUS_API_KEY / ANTHROPIC_API_KEY / OPENAI_API_KEY, or from
// the dev build's saved keys (add the provider in Helpy's settings first).
// Writes grounding-out/report.html with every answer drawn on its screenshot.

import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";

const CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const PORT = 9333;
const [W, H] = [1280, 800];
const OUT = "grounding-out";

const PROVIDERS = {
  nebius: { url: "https://api.tokenfactory.nebius.com/v1", env: "NEBIUS_API_KEY" },
  anthropic: { url: "https://api.anthropic.com/v1", env: "ANTHROPIC_API_KEY" },
  openai: { url: "https://api.openai.com/v1", env: "OPENAI_API_KEY" },
};

// A target is found by CSS selector, or by its exact visible text.
const CASES = [
  { url: "https://en.wikipedia.org/wiki/Nvidia", task: "search Wikipedia", selector: "#searchInput, input[name=search]" },
  { url: "https://en.wikipedia.org/wiki/Nvidia", task: "see the edit history of this article", selector: "#ca-history a" },
  { url: "https://en.wikipedia.org/wiki/Nvidia", task: "open the article's talk page", selector: "#ca-talk a" },
  { url: "https://news.ycombinator.com", task: "submit a new story", text: "submit" },
  { url: "https://news.ycombinator.com", task: "log in", text: "login" },
  { url: "https://news.ycombinator.com", task: "see the Ask HN posts", text: "ask" },
  { url: "https://github.com/AbyAbyss/helpy", task: "see this repository's pull requests", selector: "a[href='/AbyAbyss/helpy/pulls']" },
  { url: "https://github.com/AbyAbyss/helpy", task: "open the LICENSE file", text: "LICENSE" },
  { url: "https://www.python.org", task: "open the Downloads menu", selector: "#downloads > a" },
  { url: "https://www.python.org", task: "search python.org", selector: "#id-search-field" },
  { url: "https://developer.mozilla.org/en-US/", task: "open the MDN blog", selector: "a[href='/en-US/blog/']" },
  { url: "https://www.rust-lang.org", task: "go to the Install page", text: "Install" },
];

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function key(provider) {
  const env = process.env[PROVIDERS[provider].env];
  if (env) return env;
  try {
    const saved = JSON.parse(readFileSync(join(homedir(), "Library/Application Support/com.helpy.app/dev-secrets.json"), "utf8"));
    if (saved[`provider:${provider}`]) return saved[`provider:${provider}`];
  } catch {}
  throw new Error(`No key for ${provider}: set ${PROVIDERS[provider].env} or add it in Helpy's settings`);
}

async function api(provider, path, body) {
  const res = await fetch(PROVIDERS[provider].url + path, {
    method: body ? "POST" : "GET",
    headers: { authorization: `Bearer ${key(provider)}`, "content-type": "application/json" },
    body: body && JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`${provider} ${res.status}: ${(await res.text()).slice(0, 300)}`);
  return res.json();
}

// ---- Chrome over the DevTools protocol ----

async function browser() {
  const proc = spawn(CHROME, [
    "--headless=new", `--remote-debugging-port=${PORT}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), "ground-"))}`,
    `--window-size=${W},${H}`, "--hide-scrollbars", "--no-first-run", "about:blank",
  ], { stdio: "ignore" });
  for (let i = 0; i < 50; i++) {
    try {
      const tabs = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();
      const page = tabs.find((t) => t.type === "page");
      if (page) return { proc, page: await connect(page.webSocketDebuggerUrl) };
    } catch {}
    await sleep(200);
  }
  proc.kill();
  throw new Error("Chrome didn't start");
}

function connect(url) {
  return new Promise((resolve) => {
    const ws = new WebSocket(url);
    let id = 0;
    const waiting = new Map();
    const events = [];
    ws.onmessage = (m) => {
      const msg = JSON.parse(m.data);
      if (msg.id && waiting.has(msg.id)) {
        const { ok, fail } = waiting.get(msg.id);
        waiting.delete(msg.id);
        msg.error ? fail(new Error(msg.error.message)) : ok(msg.result);
      } else if (msg.method) {
        events.forEach((e) => e(msg));
      }
    };
    const send = (method, params = {}) => new Promise((ok, fail) => {
      waiting.set(++id, { ok, fail });
      ws.send(JSON.stringify({ id, method, params }));
    });
    const once = (method, ms) => new Promise((ok) => {
      const e = (msg) => msg.method === method && (events.splice(events.indexOf(e), 1), ok());
      events.push(e);
      setTimeout(ok, ms);
    });
    ws.onopen = () => resolve({ send, once, close: () => ws.close() });
  });
}

const FIND = (c) => `(() => {
  const visible = (el) => { const r = el.getBoundingClientRect(); return r.width > 0 && r.height > 0 && r.bottom > 0 && r.top < ${H} ? r : null; };
  let el = null;
  ${c.selector
    ? `el = [...document.querySelectorAll(${JSON.stringify(c.selector)})].find(visible);`
    : `el = [...document.querySelectorAll("a, button, [role=button], [role=tab]")].find((e) => e.innerText?.trim() === ${JSON.stringify(c.text)} && visible(e));`}
  const r = el && visible(el);
  return r ? JSON.stringify({ x: r.left, y: r.top, width: r.width, height: r.height }) : null;
})()`;

async function capture() {
  const { proc, page } = await browser();
  await page.send("Page.enable");
  await page.send("Emulation.setDeviceMetricsOverride", { width: W, height: H, deviceScaleFactor: 1, mobile: false });
  const cases = [];
  let current = null;
  try {
    for (const c of CASES) {
      if (c.url !== current) {
        const loaded = page.once("Page.loadEventFired", 20000);
        await page.send("Page.navigate", { url: c.url });
        await loaded;
        await sleep(1500);
        current = c.url;
      }
      const { result } = await page.send("Runtime.evaluate", { expression: FIND(c), returnByValue: true });
      if (!result.value) {
        console.warn(`skipped (target not found): ${c.task} on ${c.url}`);
        continue;
      }
      const shot = await page.send("Page.captureScreenshot", { format: "png" });
      cases.push({ ...c, box: JSON.parse(result.value), image: shot.data });
    }
  } finally {
    page.close();
    proc.kill();
  }
  return cases;
}

// ---- Asking the models ----

const prompt = (task) =>
  `This is a ${W}x${H} pixel screenshot. Where should the user click to ${task}? ` +
  `Reply with only JSON: {"x": <number>, "y": <number>}, in pixels of this image, with 0,0 at the top left.`;

async function ask(spec, c) {
  const [provider, ...rest] = spec.split(":");
  const model = rest.join(":");
  const started = Date.now();
  const res = await api(provider, "/chat/completions", {
    ...JSON.parse(process.env.GROUNDING_EXTRA ?? "{}"),
    model,
    max_tokens: 2000,
    messages: [{
      role: "user",
      content: [
        { type: "image_url", image_url: { url: `data:image/png;base64,${c.image}` } },
        { type: "text", text: prompt(c.task) },
      ],
    }],
  });
  const text = res.choices?.[0]?.message?.content ?? "";
  // Reasoning models may think out loud first; the last {x, y} is the answer.
  const found = [...text.matchAll(/\{[^{}]*"x"\s*:\s*(-?[\d.]+)[^{}]*"y"\s*:\s*(-?[\d.]+)[^{}]*\}/g)].pop();
  return { point: found && { x: +found[1], y: +found[2] }, text, ms: Date.now() - started };
}

function score(c, point) {
  if (!point) return { hit: false, dist: null };
  const { x, y, width, height } = c.box;
  const m = 4; // a few pixels of slack, as snapping would forgive
  const hit = point.x >= x - m && point.x <= x + width + m && point.y >= y - m && point.y <= y + height + m;
  const dist = Math.hypot(point.x - (x + width / 2), point.y - (y + height / 2));
  return { hit, dist };
}

function report(cases, results, models) {
  const colors = ["#e5484d", "#0090ff", "#30a46c", "#f76b15", "#8e4ec6"];
  const rows = cases.map((c, i) => {
    const marks = models.map((m, j) => {
      const p = results[m][i]?.point;
      return p ? `<circle cx="${p.x}" cy="${p.y}" r="7" fill="none" stroke="${colors[j % 5]}" stroke-width="3"/>` : "";
    }).join("");
    const b = c.box;
    const answers = models.map((m, j) => {
      const r = results[m][i];
      return `<li style="color:${colors[j % 5]}">${m}: ${r.error ? "error: " + r.error : r.hit ? "hit" : "miss"}${r.dist != null ? `, ${Math.round(r.dist)} px from centre` : ""}</li>`;
    }).join("");
    return `<section><h3>${c.task} <small>${c.url}</small></h3><ul>${answers}</ul>
      <svg viewBox="0 0 ${W} ${H}" width="100%"><image href="data:image/png;base64,${c.image}" width="${W}" height="${H}"/>
      <rect x="${b.x}" y="${b.y}" width="${b.width}" height="${b.height}" fill="none" stroke="#ffd60a" stroke-width="2" stroke-dasharray="5 3"/>${marks}</svg></section>`;
  }).join("");
  writeFileSync(join(OUT, "report.html"), `<!doctype html><meta charset="utf-8"><title>Grounding eval</title>
    <body style="font:14px system-ui;max-width:1100px;margin:auto;padding:16px">
    <p>Dashed yellow: the real target. Circles: where each model pointed.</p>${rows}`);
}

async function main() {
  const args = process.argv.slice(2);
  if (args[0] === "--list") {
    const { data } = await api(args[1] ?? "nebius", "/models");
    data.map((m) => m.id).sort().forEach((id) => console.log(id));
    return;
  }
  if (!args.length) throw new Error("Name models as provider:model, e.g. nebius:nvidia/<model>");
  mkdirSync(OUT, { recursive: true });
  const cases = await capture();
  console.log(`${cases.length} targets captured\n`);
  const results = {};
  for (const m of args) {
    results[m] = [];
    for (const c of cases) {
      try {
        const a = await ask(m, c);
        results[m].push({ ...a, ...score(c, a.point) });
      } catch (e) {
        results[m].push({ error: e.message, hit: false, dist: null });
      }
    }
    const r = results[m];
    const dists = r.map((x) => x.dist).filter((d) => d != null).sort((a, b) => a - b);
    const ms = r.filter((x) => x.ms).map((x) => x.ms);
    console.log(`${m}\n  hits ${r.filter((x) => x.hit).length}/${r.length}` +
      `, median ${dists.length ? Math.round(dists[dists.length >> 1]) : "-"} px from centre` +
      `, ${r.filter((x) => !x.point && !x.error).length} unreadable, ${r.filter((x) => x.error).length} errors` +
      `, ${ms.length ? (ms.reduce((a, b) => a + b, 0) / ms.length / 1000).toFixed(1) : "-"} s average`);
    r.filter((x) => x.error).slice(0, 2).forEach((x) => console.log(`  error: ${x.error}`));
  }
  report(cases, results, args);
  writeFileSync(join(OUT, "results.json"), JSON.stringify(
    Object.fromEntries(args.map((m) => [m, results[m].map(({ text, ...r }, i) => ({ task: cases[i].task, ...r, reply: text?.slice(-300) }))])), null, 2));
  console.log(`\nReport: ${join(OUT, "report.html")}`);
}

main().catch((e) => {
  console.error(e.message);
  process.exit(1);
});
