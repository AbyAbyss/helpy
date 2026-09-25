//! What Helpy remembers about the user across conversations: short notes
//! the model saves with the `remember` tool, kept in SQLite, shown and
//! editable in Settings → Usage & budgets.

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use super::types::ToolDef;
use crate::agents::AgentsState;

pub const REMEMBER: &str = "remember";
pub const CHANGED_EVENT: &str = "memory://changed";
/// Notes kept at most; the model is told when the memory is full.
pub const MAX_NOTES: usize = 60;
/// Longest note, in characters.
pub const MAX_NOTE_CHARS: usize = 200;

#[derive(Serialize, serde::Deserialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Note {
    #[ts(type = "number")]
    pub id: i64,
    /// Unix milliseconds.
    #[ts(type = "number")]
    pub created: i64,
    pub text: String,
}

pub fn tool() -> ToolDef {
    ToolDef {
        name: REMEMBER.into(),
        description: format!(
            "Save one short fact about the user or their setup that will matter in later conversations \
             (apps they use, preferences, names, recurring tasks). Not one-off details, and nothing read \
             from a screenshot. At most {MAX_NOTE_CHARS} characters."
        ),
        schema: json!({
            "type": "object",
            "properties": {
                "fact": { "type": "string", "description": "One sentence, e.g. \"Uses Outlook desktop for work email\"." }
            },
            "required": ["fact"],
            "additionalProperties": false
        }),
    }
}

/// The prompt lines asking the model to save facts, when the tool is offered.
pub const INSTRUCTIONS: &str = "When the user tells you something about themselves or how they work that will \
    matter next time (the apps they use, preferences, names, recurring tasks), call remember with one short \
    sentence. Don't save one-off details or anything you read on the screen.";

/// The saved notes as a system prompt section, if there are any.
pub fn prompt_section(app: &AppHandle) -> Option<String> {
    let notes = list(app);
    if notes.is_empty() {
        return None;
    }
    let lines: Vec<String> = notes.iter().map(|n| format!("- {}", n.text)).collect();
    Some(format!(
        "Things you remember about the user from earlier conversations:\n{}",
        lines.join("\n")
    ))
}

pub fn list(app: &AppHandle) -> Vec<Note> {
    app.state::<AgentsState>().store().notes()
}

/// Saves a note, or explains why not (for the model to pass on).
pub fn remember(app: &AppHandle, fact: &str) -> Result<Note, String> {
    let fact = fact.trim();
    if fact.is_empty() {
        return Err("Nothing to remember.".into());
    }
    if fact.chars().count() > MAX_NOTE_CHARS {
        return Err(format!("Keep a note under {MAX_NOTE_CHARS} characters."));
    }
    let agents = app.state::<AgentsState>();
    let store = agents.store();
    let existing = store.notes();
    if existing.iter().any(|n| n.text.eq_ignore_ascii_case(fact)) {
        return Err("That's already remembered.".into());
    }
    if existing.len() >= MAX_NOTES {
        return Err(format!(
            "Memory is full ({MAX_NOTES} notes). The user can remove notes in Settings → Usage & budgets."
        ));
    }
    let note = store.add_note(fact, chrono::Utc::now().timestamp_millis());
    let _ = app.emit(CHANGED_EVENT, ());
    Ok(note)
}

#[tauri::command]
pub fn ai_notes(app: AppHandle) -> Vec<Note> {
    list(&app)
}

#[tauri::command]
pub fn ai_note_delete(app: AppHandle, id: i64) {
    app.state::<AgentsState>().store().delete_note(id);
    let _ = app.emit(CHANGED_EVENT, ());
}

#[tauri::command]
pub fn ai_notes_clear(app: AppHandle) {
    app.state::<AgentsState>().store().clear_notes();
    let _ = app.emit(CHANGED_EVENT, ());
}
