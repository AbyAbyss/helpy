import { describe, expect, it } from "vitest";
import defaults from "../bindings/defaults.json";
import { FIELDS, SECTIONS, searchFields } from "./registry";

describe("settings registry", () => {
  it("has a row for every setting in the sections it shows", () => {
    for (const { id } of SECTIONS) {
      const expected = Object.keys(defaults[id]).map((k) => `${id}.${k}`).sort();
      const actual = FIELDS.filter((f) => f.section === id).map((f) => f.path).sort();
      expect(actual).toEqual(expected);
    }
  });

  it("lists each setting once", () => {
    const paths = FIELDS.map((f) => f.path);
    expect(new Set(paths).size).toBe(paths.length);
  });

  it("finds settings by label, keyword and section", () => {
    expect(searchFields("dark mode").map((f) => f.path)).toEqual(["general.theme"]);
    expect(searchFields("push to talk").map((f) => f.path)).toContain("hotkeys.voiceMode");
    expect(searchFields("buddy size").map((f) => f.path)).toEqual(["buddy.size"]);
    expect(searchFields("  ")).toEqual([]);
  });
});
