import { describe, expect, it } from "vitest";
import { acceleratorFromEvent, keycaps } from "./keys";

const ev = (code: string, mods: Partial<Record<"ctrlKey" | "altKey" | "shiftKey" | "metaKey", boolean>> = {}) => ({
  code, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...mods,
});

describe("acceleratorFromEvent", () => {
  it("builds modifier+key strings the Rust parser accepts", () => {
    expect(acceleratorFromEvent(ev("KeyC", { altKey: true, shiftKey: true }))).toBe("Alt+Shift+C");
    expect(acceleratorFromEvent(ev("Digit4", { ctrlKey: true }))).toBe("Control+4");
    expect(acceleratorFromEvent(ev("Space", { metaKey: true }))).toBe("Super+Space");
  });
  it("waits while only modifiers are held", () => {
    expect(acceleratorFromEvent(ev("ShiftLeft", { shiftKey: true }))).toBeNull();
  });
});

describe("keycaps", () => {
  it("orders and names modifiers per platform", () => {
    expect(keycaps("Shift+Alt+Space", false)).toEqual(["Alt", "Shift", "Space"]);
    expect(keycaps("Shift+Alt+Space", true)).toEqual(["⌥", "⇧", "Space"]);
    expect(keycaps("CommandOrControl+comma", false)).toEqual(["Ctrl", ","]);
  });
  it("shows nothing for an unbound action", () => {
    expect(keycaps("")).toEqual([]);
  });
});
