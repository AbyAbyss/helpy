import { describe, expect, it } from "vitest";
import type { ChatSummary } from "../bindings/ChatSummary";
import { dayGroup, matches } from "./group";

const chat = (over: Partial<ChatSummary>): ChatSummary => ({
  id: "c",
  title: "Weather in Leeds",
  started: 0,
  updated: 0,
  questions: 1,
  voice: false,
  allVoice: false,
  preview: "It's raining.",
  asked: "Weather in Leeds · and tomorrow?",
  ...over,
});

describe("chat list", () => {
  it("filters by how it was asked, and searches every question", () => {
    expect(matches(chat({}), "typed", "")).toBe(true);
    expect(matches(chat({}), "voice", "")).toBe(false);
    expect(matches(chat({ voice: true, allVoice: true }), "typed", "")).toBe(false);
    expect(matches(chat({ voice: true }), "typed", "")).toBe(true);
    expect(matches(chat({}), "all", "TOMORROW")).toBe(true);
    expect(matches(chat({}), "all", "paris")).toBe(false);
  });

  it("groups by day", () => {
    const now = new Date(2026, 10, 3, 9);
    expect(dayGroup(new Date(2026, 10, 3, 1).getTime(), now)).toBe("Today");
    expect(dayGroup(new Date(2026, 10, 2, 23).getTime(), now)).toBe("Yesterday");
    expect(dayGroup(new Date(2026, 9, 30).getTime(), now)).toBe("Previous 7 days");
    expect(dayGroup(new Date(2026, 5, 1).getTime(), now)).toMatch(/June/);
  });
});
