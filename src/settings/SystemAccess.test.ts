import { describe, expect, it } from "vitest";
import { missing } from "./SystemAccess";

describe("missing", () => {
  it("names only permissions turned off, not ones never asked or absent", () => {
    expect(missing({ microphone: "notAsked", screen: "denied", accessibility: "allowed" })).toEqual(["Screen Recording"]);
    expect(missing({ microphone: "denied", screen: null, accessibility: "denied" })).toEqual(["Microphone", "Accessibility"]);
  });
});
