import { describe, expect, it } from "vitest";
import defaults from "../bindings/defaults.json";
import { FIELDS, SECTIONS, searchFields } from "./registry";

describe("settings registry", () => {
  it("has a row for every setting of each settings group it shows", () => {
    const shown = new Set(FIELDS.map((f) => f.path.split(".")[0] as keyof typeof defaults));
    for (const group of shown) {
      const expected = Object.keys(defaults[group]).map((k) => `${group}.${k}`).sort();
      const actual = FIELDS.filter((f) => f.path.startsWith(`${group}.`)).map((f) => f.path).sort();
      expect(actual).toEqual(expected);
    }
  });

  it("puts every row in a section that exists", () => {
    for (const f of FIELDS) expect(SECTIONS.map((s) => s.id)).toContain(f.section);
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
    expect(searchFields("ollama").map((f) => f.path)).toEqual(["ai.providers"]);
    expect(searchFields("screenshot").map((f) => f.path)).toContain("answerStyle.screenAccess");
    expect(searchFields("brave").map((f) => f.path)).toEqual(["agents.searchEngine"]);
    expect(searchFields("desktop access").map((f) => f.path)).toEqual(["agents.approvedFolders"]);
  });
});
