// Hotkey recording, display, and "probably taken by the OS" warnings.

export type OS = "windows" | "macos" | "linux";

const MODIFIER_CODES = new Set([
  "ShiftLeft", "ShiftRight", "ControlLeft", "ControlRight", "AltLeft", "AltRight", "MetaLeft", "MetaRight", "OSLeft", "OSRight",
]);

/**
 * Builds a Tauri accelerator ("Alt+Shift+H") from a keydown event.
 * Returns null while only modifiers are held.
 */
export function acceleratorFromEvent(e: KeyboardEvent, os: OS): string | null {
  if (MODIFIER_CODES.has(e.code)) return null;
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Control");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  if (e.metaKey) parts.push(os === "macos" ? "Command" : "Super");
  let key = e.code;
  if (key.startsWith("Key")) key = key.slice(3);
  else if (key.startsWith("Digit")) key = key.slice(5);
  parts.push(key);
  return parts.join("+");
}

export function hasModifier(acc: string): boolean {
  return /(^|\+)(Control|Ctrl|Alt|Option|Shift|Command|Cmd|Super|CommandOrControl|CmdOrCtrl)\+/i.test(acc);
}

export function isFunctionKey(acc: string): boolean {
  return /(^|\+)F([1-9]|1[0-2])$/.test(acc);
}

const KEY_LABELS: Record<string, string> = {
  Comma: ",", Period: ".", Slash: "/", Semicolon: ";", Quote: "'", BracketLeft: "[", BracketRight: "]",
  Backslash: "\\", Minus: "-", Equal: "=", Backquote: "`", Space: "Space", Escape: "Esc",
  ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→", Enter: "Enter", Backspace: "⌫", Tab: "Tab",
};

/** Splits an accelerator into keycap labels for the current OS. */
export function keycaps(acc: string, os: OS): string[] {
  if (!acc) return [];
  return acc.split("+").map((raw) => {
    const t = raw.toLowerCase();
    if (os === "macos") {
      if (t === "control" || t === "ctrl") return "⌃";
      if (t === "alt" || t === "option") return "⌥";
      if (t === "shift") return "⇧";
      if (["command", "cmd", "super", "commandorcontrol", "cmdorctrl"].includes(t)) return "⌘";
    } else {
      if (["control", "ctrl", "commandorcontrol", "cmdorctrl"].includes(t)) return "Ctrl";
      if (t === "alt" || t === "option") return "Alt";
      if (t === "shift") return "Shift";
      if (["command", "cmd", "super"].includes(t)) return os === "windows" ? "Win" : "Super";
    }
    return KEY_LABELS[raw] ?? raw;
  });
}

/** Canonical form so "Shift+Alt+H" and "Alt+Shift+H" compare equal. */
export function canonical(acc: string, os: OS): string {
  const parts = acc.split("+").map((p) => {
    const t = p.toLowerCase();
    if (t === "ctrl") return "control";
    if (t === "option") return "alt";
    if (t === "cmd" || t === "command") return "super";
    if (t === "commandorcontrol" || t === "cmdorctrl") return os === "macos" ? "super" : "control";
    return t;
  });
  const key = parts.pop()!;
  return [...parts.sort(), key].join("+");
}

/** Combinations the OS or nearly every app already uses. Registering them is allowed but warned. */
const RESERVED: Record<OS, Record<string, string>> = {
  windows: {
    "alt+tab": "switches windows", "alt+f4": "closes windows", "alt+space": "opens the window menu",
    "super+l": "locks Windows", "super+d": "shows the desktop", "super+e": "opens File Explorer", "super+r": "opens Run",
    "super+tab": "opens Task View", "super+v": "opens clipboard history", "shift+super+s": "takes a screenshot",
    "control+c": "copies", "control+v": "pastes", "control+x": "cuts", "control+z": "undoes", "control+s": "saves",
    "control+a": "selects all", "control+f": "finds", "control+w": "closes tabs", "control+t": "opens tabs",
    "alt+control+delete": "opens the security screen", "control+shift+escape": "opens Task Manager", "printscreen": "takes a screenshot",
  },
  macos: {
    "super+space": "opens Spotlight", "super+tab": "switches apps", "super+q": "quits apps", "super+w": "closes windows",
    "super+h": "hides apps", "super+m": "minimizes", "super+c": "copies", "super+v": "pastes", "super+x": "cuts",
    "super+z": "undoes", "super+s": "saves", "super+a": "selects all", "super+f": "finds", "super+t": "opens tabs",
    "shift+super+3": "takes a screenshot", "shift+super+4": "takes a screenshot", "shift+super+5": "opens screenshot tools",
    "control+space": "switches input source", "control+super+q": "locks the screen", "alt+super+escape": "force quits apps",
    "control+super+space": "opens the emoji picker",
  },
  linux: {
    "alt+tab": "switches windows", "alt+f4": "closes windows", "super+l": "locks the screen", "control+alt+t": "opens a terminal",
    "control+c": "copies", "control+v": "pastes", "control+x": "cuts", "control+z": "undoes", "control+s": "saves",
    "control+a": "selects all", "control+alt+delete": "logs out", "printscreen": "takes a screenshot", "super+a": "shows apps",
  },
};

export function reservedReason(acc: string, os: OS): string | null {
  if (!acc) return null;
  return RESERVED[os][canonical(acc, os)] ?? null;
}
