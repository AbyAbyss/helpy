import { describe, expect, it } from "vitest";
import type { AskEvent } from "../bindings/AskEvent";
import { apply, waiting, type Item } from "./transcript";

const run = (events: AskEvent[], start: Item[] = [{ kind: "user", text: "q", voice: false }]) => events.reduce(apply, start);

describe("ask transcript", () => {
  it("adds typed and spoken questions as user messages", () => {
    const items = run([{ type: "question", text: "Where is spam?", voice: true, fresh: false }], []);
    expect(items).toEqual([{ kind: "user", text: "Where is spam?", voice: true }]);
  });

  it("a fresh question clears the earlier conversation", () => {
    const items = run([{ type: "text", text: "Old answer" }, { type: "question", text: "New task", voice: true, fresh: true }]);
    expect(items).toEqual([{ kind: "user", text: "New task", voice: true }]);
  });

  it("lists walkthrough steps", () => {
    const items = run([{ type: "step", number: 2, total: 4, instruction: "Click Share." }]);
    expect(items[1]).toEqual({ kind: "step", number: 2, total: 4, text: "Click Share." });
  });

  it("streams text into one answer", () => {
    const items = run([{ type: "text", text: "Open " }, { type: "text", text: "Junk Email." }, { type: "checkpoint" }]);
    expect(items[1]).toEqual({ kind: "assistant", text: "Open Junk Email.", final: true });
  });

  it("a retry throws away the failed attempt's partial text", () => {
    const items = run([
      { type: "text", text: "Half an ans" },
      { type: "retry", retry: 1, limit: 3, reason: "the provider timed out", wait: 2000, model: "llama · Ollama" },
    ]);
    expect(items.map((i) => i.kind)).toEqual(["user", "retry"]);
    expect(items[1]).toMatchObject({ text: "Retry 1 of 3: the provider timed out. Trying llama · Ollama in 2 s…" });
    const after = run([{ type: "text", text: "Full answer" }], items);
    expect(after.map((i) => i.kind)).toEqual(["user", "assistant"]);
  });

  it("keeps text from a finished step before a screenshot", () => {
    const items = run([
      { type: "text", text: "Let me look." },
      { type: "checkpoint" },
      { type: "screen", thumbnail: "data:", monitor: "Display 1" },
      { type: "text", text: "Click Junk Email." },
      { type: "error", message: "Stopped.", action: null },
    ]);
    expect(items.map((i) => i.kind)).toEqual(["user", "assistant", "screen", "error"]);
    expect(items[1]).toMatchObject({ text: "Let me look.", final: true });
  });

  it("shows the thinking dots only while nothing is streaming", () => {
    expect(waiting([{ kind: "user", text: "q", voice: false }], true)).toBe(true);
    expect(waiting([{ kind: "user", text: "q", voice: false }], false)).toBe(false);
    expect(waiting(run([{ type: "text", text: "a" }]), true)).toBe(false);
    expect(waiting(run([{ type: "screenPermission", id: 1 }]), true)).toBe(false);
  });
});
