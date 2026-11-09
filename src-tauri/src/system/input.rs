//! Clicking, typing and pressing keys for the user, from the chat: the model
//! points at a spot in its latest screenshot, or names text and keys.

use std::time::Duration;

use enigo::{Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse};
use serde_json::{json, Value};

use crate::agents::allow::AlwaysAllow;
use crate::agents::model::ActionKind;
use crate::agents::runner::Gate;
use crate::ai::types::ToolDef;
use crate::settings::schema::{Approvals, Rule};

pub const CLICK: &str = "click_screen";
pub const TYPE: &str = "type_text";
pub const PRESS: &str = "press_keys";
const TOOLS: [&str; 3] = [CLICK, TYPE, PRESS];
/// Characters typed in one call.
const MAX_TYPE: usize = 2000;

pub fn is_tool(name: &str) -> bool {
    TOOLS.contains(&name)
}

pub fn defs() -> Vec<ToolDef> {
    let def = |name: &str, description: &str, props: Value, required: &[&str]| ToolDef {
        name: name.into(),
        description: description.into(),
        schema: json!({ "type": "object", "properties": props, "required": required }),
    };
    vec![
        def(
            CLICK,
            "Click on the user's screen for them, at a point in the most recent screenshot (look at the screen \
             first). Aim at the middle of the button, link, field or item. Returns a new screenshot so you can \
             check what happened.",
            json!({
                "x": { "type": "number", "description": "Pixels from the left of the latest screenshot." },
                "y": { "type": "number", "description": "Pixels from the top of the latest screenshot." },
                "target": { "type": "string", "description": "What's there, as the user would name it, e.g. \"Start chart button\"." },
                "button": { "type": "string", "enum": ["left", "right"], "description": "Defaults to left." },
                "double": { "type": "boolean", "description": "Double-click, e.g. to open a file." }
            }),
            &["x", "y", "target"],
        ),
        def(
            TYPE,
            "Type text into whatever has the keyboard focus in the user's app, as if they typed it. Click the \
             field first when it isn't focused.",
            json!({ "text": { "type": "string", "description": "The exact text to type." } }),
            &["text"],
        ),
        def(
            PRESS,
            "Press a key or shortcut in the user's app, e.g. \"enter\", \"tab\", \"cmd+s\", \"ctrl+shift+t\", \
             \"down\". Use the shortcut keys of the user's system.",
            json!({ "keys": { "type": "string", "description": "Keys joined with +." } }),
            &["keys"],
        ),
    ]
}

fn s<'a>(args: &'a Value, key: &str) -> &'a str {
    args[key].as_str().unwrap_or("").trim()
}

fn describe(tool: &str, args: &Value) -> String {
    match tool {
        CLICK => {
            let how = match (args["double"].as_bool().unwrap_or(false), s(args, "button")) {
                (true, _) => "Double-click",
                (_, "right") => "Right-click",
                _ => "Click",
            };
            format!("{how} “{}” on your screen", s(args, "target"))
        }
        TYPE => {
            let text = s(args, "text");
            let short: String = text.chars().take(80).collect();
            let more = if text.chars().count() > 80 { "…" } else { "" };
            format!("Type “{short}{more}”")
        }
        _ => format!("Press {}", s(args, "keys")),
    }
}

pub fn gate(a: &Approvals, tool: &str, args: &Value) -> Gate {
    match a.screen_control {
        Rule::Allow => Gate::Allow,
        Rule::Never => Gate::Never("the user turned clicking and typing off in Settings → Permissions.".into()),
        Rule::Ask => Gate::Ask {
            kind: ActionKind::System,
            source: "Your screen".into(),
            summary: describe(tool, args),
            detail: String::new(),
            editable: Vec::new(),
            always: AlwaysAllow::approval("screenControl"),
        },
    }
}

/// A shortcut such as "cmd+shift+t": the keys held, then the key pressed.
pub fn parse_keys(text: &str) -> Result<Vec<Key>, String> {
    let keys = text
        .split('+')
        .map(|k| k.trim().to_lowercase())
        .filter(|k| !k.is_empty())
        .map(|k| key(&k).ok_or_else(|| format!("There's no key called \"{k}\".")))
        .collect::<Result<Vec<_>, _>>()?;
    if keys.is_empty() || keys.len() > 5 {
        return Err("Give one key, or a shortcut of up to 5 keys joined with +.".into());
    }
    Ok(keys)
}

fn key(name: &str) -> Option<Key> {
    Some(match name {
        "cmd" | "command" | "meta" | "super" | "win" | "windows" => Key::Meta,
        "ctrl" | "control" => Key::Control,
        "alt" | "option" | "opt" => Key::Alt,
        "shift" => Key::Shift,
        "enter" | "return" => Key::Return,
        "tab" => Key::Tab,
        "esc" | "escape" => Key::Escape,
        "space" | "spacebar" => Key::Space,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "up" | "arrowup" => Key::UpArrow,
        "down" | "arrowdown" => Key::DownArrow,
        "left" | "arrowleft" => Key::LeftArrow,
        "right" | "arrowright" => Key::RightArrow,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "f1" => Key::F1,
        "f2" => Key::F2,
        "f3" => Key::F3,
        "f4" => Key::F4,
        "f5" => Key::F5,
        "f6" => Key::F6,
        "f7" => Key::F7,
        "f8" => Key::F8,
        "f9" => Key::F9,
        "f10" => Key::F10,
        "f11" => Key::F11,
        "f12" => Key::F12,
        "plus" => Key::Unicode('+'),
        _ => {
            let mut chars = name.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            Key::Unicode(c)
        }
    })
}

fn enigo() -> Result<Enigo, String> {
    #[cfg(target_os = "macos")]
    if crate::a11y::trusted() == Some(false) {
        crate::a11y::request_trust();
        return Err(
            "Helpy isn't allowed to control the mouse and keyboard yet. Allow Helpy in System Settings → Privacy & \
             Security → Accessibility, then try again."
                .into(),
        );
    }
    Enigo::new(&enigo::Settings::default()).map_err(|e| format!("Helpy couldn't control the mouse and keyboard ({e})."))
}

fn pause() {
    std::thread::sleep(Duration::from_millis(60));
}

/// Clicks at a point in global physical pixels, then puts the pointer back.
pub async fn click(x: f64, y: f64, scale: f64, right: bool, double: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut e = enigo()?;
        let fail = |e: enigo::InputError| format!("Helpy couldn't click there ({e}).");
        // macOS moves the pointer in points.
        let k = if cfg!(target_os = "macos") { scale } else { 1.0 };
        let back = e.location().ok();
        e.move_mouse((x / k).round() as i32, (y / k).round() as i32, Coordinate::Abs).map_err(fail)?;
        pause();
        let button = if right { Button::Right } else { Button::Left };
        for _ in 0..if double { 2 } else { 1 } {
            e.button(button, Direction::Click).map_err(fail)?;
            std::thread::sleep(Duration::from_millis(40));
        }
        pause();
        if let Some((bx, by)) = back {
            let _ = e.move_mouse(bx, by, Coordinate::Abs);
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

pub async fn type_text(text: String) -> Result<String, String> {
    if text.is_empty() {
        return Err("There's no text to type.".into());
    }
    if text.chars().count() > MAX_TYPE {
        return Err(format!("That's more than {MAX_TYPE} characters; type it in parts."));
    }
    tauri::async_runtime::spawn_blocking(move || {
        enigo()?.text(&text).map_err(|e| format!("Helpy couldn't type ({e})."))?;
        Ok(format!("Typed {} characters.", text.chars().count()))
    })
    .await
    .map_err(|e| e.to_string())?
}

pub async fn press(keys: String) -> Result<String, String> {
    let parsed = parse_keys(&keys)?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut e = enigo()?;
        let fail = |e: enigo::InputError| format!("Helpy couldn't press {keys} ({e}).");
        let (last, held) = parsed.split_last().expect("parse_keys gives at least one");
        for k in held {
            e.key(*k, Direction::Press).map_err(fail)?;
        }
        let pressed = e.key(*last, Direction::Click).map_err(fail);
        // Let go of the held keys even when the press failed.
        for k in held.iter().rev() {
            let _ = e.key(*k, Direction::Release);
        }
        pressed?;
        Ok(format!("Pressed {keys}."))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_parse_in_any_spelling() {
        assert_eq!(parse_keys("Cmd+Shift+T").unwrap(), [Key::Meta, Key::Shift, Key::Unicode('t')]);
        assert_eq!(parse_keys("enter").unwrap(), [Key::Return]);
        assert_eq!(parse_keys("ctrl + plus").unwrap(), [Key::Control, Key::Unicode('+')]);
        assert!(parse_keys("cmd+hyper").is_err());
        assert!(parse_keys("").is_err());
    }

    #[test]
    fn requests_say_what_will_happen() {
        let a = Approvals::default();
        let Gate::Ask { summary, always, .. } = gate(&a, CLICK, &json!({ "x": 1, "y": 2, "target": "Start chart" })) else {
            panic!("clicking asks by default");
        };
        assert_eq!(summary, "Click “Start chart” on your screen");
        assert!(always.is_some());
        let off = Approvals { screen_control: Rule::Never, ..a };
        assert!(matches!(gate(&off, PRESS, &json!({ "keys": "cmd+q" })), Gate::Never(_)));
        assert_eq!(describe(CLICK, &json!({ "target": "a.txt", "double": true })), "Double-click “a.txt” on your screen");
    }
}
