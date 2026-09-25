import { describe, expect, it } from "vitest";
import { captionText, initial, onAsk, onPartial, onVoice } from "./pillState";

describe("voice pill state", () => {
  it("shows the live transcript while listening, then the question", () => {
    let s = onPartial(initial, "where is my");
    expect(s.caption).toBe("where is my");
    s = onVoice(s, { phase: "transcribing" });
    expect(s.mode).toBe("working");
    s = onVoice(s, { phase: "thinking", transcript: "Where is my spam folder?" });
    expect(s.caption).toBe("“Where is my spam folder?”");
  });

  it("streams the answer for voice questions only", () => {
    const working = onVoice(initial, { phase: "thinking", transcript: "q" });
    expect(onAsk(working, { type: "text", text: "Open" }, false)).toBe(working);
    let s = onAsk(working, { type: "text", text: "Open " }, true);
    s = onAsk(s, { type: "text", text: "Junk Email." }, true);
    expect(s).toMatchObject({ mode: "answering", caption: "Open Junk Email." });
    expect(onAsk(s, { type: "done", model: "m", tokens: 1, offerAgents: false }, true).mode).toBe("done");
    expect(onAsk(s, { type: "done", model: "m", tokens: 1, offerAgents: true }, true).hint).toContain("do it");
  });

  it("errors and empty recordings become a message", () => {
    expect(onVoice(initial, { phase: "error", message: "No microphone found" })).toEqual({
      mode: "message",
      caption: "No microphone found",
      tone: "error",
    });
    expect(onVoice(initial, { phase: "idle", message: "I didn't hear anything" }).mode).toBe("message");
  });

  it("captions are plain and short", () => {
    expect(captionText("1. Click **Junk Email** in [Outlook](https://x)")).toBe("1. Click Junk Email in Outlook");
    expect(captionText("a".repeat(300)).length).toBe(160);
  });
});
