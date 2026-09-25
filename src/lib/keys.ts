// Converting between keyboard events, accelerator strings (the format the
// Rust side parses) and what we show on keycaps.

export const isMac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);

const MODIFIER_CODES = new Set([
  "ShiftLeft", "ShiftRight", "ControlLeft", "ControlRight", "AltLeft", "AltRight", "MetaLeft", "MetaRight",
]);

type KeyLike = Pick<KeyboardEvent, "code" | "ctrlKey" | "altKey" | "shiftKey" | "metaKey">;

export function modifiersOf(e: KeyLike): string[] {
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Control");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  return mods;
}

/** The accelerator for a key press, or null while only modifiers are held. */
export function acceleratorFromEvent(e: KeyLike): string | null {
  if (MODIFIER_CODES.has(e.code)) return null;
  const key = e.code.replace(/^Key/, "").replace(/^Digit/, "");
  return [...modifiersOf(e), key].join("+");
}

const MAC_GLYPHS: Record<string, string> = { Control: "⌃", Alt: "⌥", Shift: "⇧", Super: "⌘" };
const PC_NAMES: Record<string, string> = { Control: "Ctrl", Alt: "Alt", Shift: "Shift", Super: "Win" };
const KEY_NAMES: Record<string, string> = {
  Space: "Space", Comma: ",", Period: ".", Slash: "/", Semicolon: ";", Quote: "'", BracketLeft: "[",
  BracketRight: "]", Backslash: "\\", Minus: "-", Equal: "=", Backquote: "`", ArrowUp: "↑", ArrowDown: "↓",
  ArrowLeft: "←", ArrowRight: "→", Escape: "Esc", Enter: "Enter", Backspace: "⌫",
};

/** Normalizes the aliases the Rust parser accepts. */
function canonicalModifier(m: string): string | null {
  switch (m.toLowerCase()) {
    case "control": case "ctrl": return "Control";
    case "alt": case "option": return "Alt";
    case "shift": return "Shift";
    case "super": case "command": case "cmd": case "meta": return "Super";
    case "commandorcontrol": case "cmdorctrl": case "commandorctrl": case "cmdorcontrol":
      return isMac ? "Super" : "Control";
    default: return null;
  }
}

/** Keycap labels for an accelerator, e.g. "Alt+Shift+Space" → ["Alt", "Shift", "Space"]. */
export function keycaps(accel: string, mac = isMac): string[] {
  if (!accel) return [];
  const parts = accel.split("+").filter(Boolean);
  const key = parts.pop() ?? "";
  const mods = parts.map((m) => canonicalModifier(m) ?? m);
  const order = mac ? ["Control", "Alt", "Shift", "Super"] : ["Control", "Super", "Alt", "Shift"];
  mods.sort((a, b) => order.indexOf(a) - order.indexOf(b));
  const names = mac ? MAC_GLYPHS : PC_NAMES;
  const k = key.replace(/^Key(?=.$)/i, "").replace(/^Digit(?=.$)/i, "");
  const known = Object.keys(KEY_NAMES).find((n) => n.toLowerCase() === k.toLowerCase());
  return [...mods.map((m) => names[m] ?? m), known ? KEY_NAMES[known] : k.length === 1 ? k.toUpperCase() : k];
}
