//! What Helpy remembers about the user across conversations: short notes
//! the model saves with the `remember` tool, or that a review of what the
//! user said picks out once those messages leave the conversation. Kept in
//! SQLite, shown and editable in Settings → Usage & budgets.

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use super::context::SUMMARY_TOKENS;
use super::types::{ChatRequest, Message, Part, Role, StopReason, ToolDef};
use crate::agents::runner::truncate;
use crate::agents::AgentsState;
use crate::settings::Settings;

pub const REMEMBER: &str = "remember";
pub const CHANGED_EVENT: &str = "memory://changed";
/// Notes kept at most; the model is told when the memory is full.
pub const MAX_NOTES: usize = 60;
/// Longest note, in characters.
pub const MAX_NOTE_CHARS: usize = 200;
/// The most of the user's words one review reads, in characters.
const REVIEW_CHARS: usize = 12_000;
/// One review at a time, so each sees the notes the last one left.
static REVIEWING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

/// Reviews what the user said in messages that are leaving the conversation
/// (summarized away, dropped, or cleared) for lasting facts, in the
/// background. The model sees the notes it already has, so a fact is saved
/// once and a changed one replaces the note it updates.
pub fn mine(app: &AppHandle, settings: &Settings, messages: &[Message]) {
    if !settings.ai.remember {
        return;
    }
    let said = user_said(messages);
    if said.is_empty() {
        return;
    }
    let Ok(plan) = super::ask::plan(&settings.ai, &settings.answer_style) else {
        return;
    };
    let models = super::ask::candidates(&settings.ai, &plan, false, false, false);
    let app = app.clone();
    let settings = settings.clone();
    tauri::async_runtime::spawn(async move {
        let _one = REVIEWING.lock().await;
        let req = review_request(&list(&app), &said);
        let cancel = CancellationToken::new();
        match super::call::stream(&app, &settings, "memory", &models, req, &cancel, &|_| {}).await {
            Ok(c) => {
                if c.stop == StopReason::MaxTokens {
                    log::warn!("memory: the review ran out of room before finishing");
                }
                let (added, merged) = apply(&app, &parse_review(&c.text()));
                log::info!("memory: reviewed the conversation: {added} new notes, {merged} updated");
            }
            Err(e) => log::warn!("memory: couldn't review the conversation: {}", e.message),
        }
    });
}

/// The user's own words, one line each. Helpy's answers, tool results,
/// images and the label in front of a screenshot are left out, so nothing
/// read from the screen can become a note.
fn user_said(messages: &[Message]) -> String {
    let mut out = String::new();
    for m in messages.iter().filter(|m| m.role == Role::User) {
        for (i, p) in m.parts.iter().enumerate() {
            let labels_image = matches!(m.parts.get(i + 1), Some(Part::Image { .. }));
            if let (Part::Text(t), false) = (p, labels_image) {
                let t = t.trim();
                if !t.is_empty() {
                    out += t;
                    out.push('\n');
                }
            }
        }
    }
    truncate(&out, REVIEW_CHARS)
}

fn review_request(notes: &[Note], said: &str) -> ChatRequest {
    let mut system = String::from(
        "You keep the notes Helpy, a desktop assistant, has about its user. Below is what the user said in a \
         conversation that just ended. Pick out lasting facts about the user that will matter in later \
         conversations: their name, what they do, the apps and tools they use, how they like answers, recurring \
         tasks. Leave out one-off details, the questions themselves, and anything that isn't about the user.\n\n",
    );
    if notes.is_empty() {
        system += "Helpy has no notes yet.\n\n";
    } else {
        system += "Helpy's notes so far:\n";
        for n in notes {
            system += &format!("[{}] {}\n", n.id, n.text);
        }
        system.push('\n');
    }
    system += &format!(
        "Reply with JSON only: {{\"notes\": [{{\"text\": \"...\", \"replaces\": null}}]}}. One sentence per note, \
         under {MAX_NOTE_CHARS} characters. Never repeat or rephrase a fact already in the notes. When a new fact \
         updates or contradicts a note, set \"replaces\" to that note's number and write one merged sentence. \
         Reply {{\"notes\": []}} when there is nothing new."
    );
    ChatRequest {
        model: String::new(),
        system,
        messages: vec![Message::user_text(said)],
        tools: Vec::new(),
        max_tokens: SUMMARY_TOKENS,
        temperature: 0.0,
    }
}

#[derive(Debug, PartialEq)]
struct Review {
    text: String,
    /// The note this one supersedes, if any.
    replaces: Option<i64>,
}

/// The notes in the model's reply; none when it isn't the JSON asked for.
fn parse_review(reply: &str) -> Vec<Review> {
    let json = reply
        .find('{')
        .zip(reply.rfind('}'))
        .filter(|(s, e)| s < e)
        .map(|(s, e)| &reply[s..=e]);
    let v = match json.map(serde_json::from_str::<serde_json::Value>) {
        Some(Ok(v)) => v,
        Some(Err(e)) => {
            log::warn!("memory: the review's reply wasn't valid JSON: {e}");
            return Vec::new();
        }
        None => {
            log::warn!("memory: the review's reply had no JSON: {}", truncate(reply, 200));
            return Vec::new();
        }
    };
    v["notes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| {
            let text = n["text"].as_str()?.trim().to_string();
            let replaces = &n["replaces"];
            (!text.is_empty()).then(|| Review {
                text,
                replaces: replaces
                    .as_i64()
                    .or_else(|| replaces.as_str().and_then(|s| s.trim().parse().ok())),
            })
        })
        .collect()
}

/// Saves the reviewed notes: one that supersedes a note replaces it in
/// place, the rest are added under the usual rules. (added, updated)
fn apply(app: &AppHandle, reviews: &[Review]) -> (usize, usize) {
    let agents = app.state::<AgentsState>();
    let store = agents.store();
    let existing = store.notes();
    let (mut added, mut updated) = (0, 0);
    for r in reviews {
        if r.text.chars().count() > MAX_NOTE_CHARS {
            continue;
        }
        match r.replaces.filter(|id| existing.iter().any(|n| n.id == *id)) {
            Some(id) => {
                store.update_note(id, &r.text);
                updated += 1;
            }
            None => match remember(app, &r.text) {
                Ok(_) => added += 1,
                Err(why) => log::info!("memory: didn't save \"{}\": {why}", r.text),
            },
        }
    }
    if updated > 0 {
        let _ = app.emit(CHANGED_EVENT, ());
    }
    (added, updated)
}

#[tauri::command]
pub fn ai_notes(app: AppHandle) -> Vec<Note> {
    list(&app)
}

/// A note typed by hand in Settings; same rules as one the model saves.
#[tauri::command]
pub fn ai_note_add(app: AppHandle, text: String) -> Result<Note, String> {
    remember(&app, &text)
}

#[tauri::command]
pub fn ai_note_update(app: AppHandle, id: i64, text: String) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Nothing to remember.".into());
    }
    if text.chars().count() > MAX_NOTE_CHARS {
        return Err(format!("Keep a note under {MAX_NOTE_CHARS} characters."));
    }
    app.state::<AgentsState>().store().update_note(id, text);
    let _ = app.emit(CHANGED_EVENT, ());
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_review_reads_only_what_the_user_typed() {
        let messages = vec![
            Message::user_text("  I'm Sam, I use Outlook for work  "),
            Message {
                role: Role::Assistant,
                parts: vec![Part::Text("Noted, Sam.".into())],
            },
            Message {
                role: Role::User,
                parts: vec![
                    Part::ToolResult {
                        id: "1".into(),
                        name: "view_screen".into(),
                        parts: vec![Part::Text("password: hunter2".into())],
                        is_error: false,
                    },
                    Part::Image {
                        media_type: "image/jpeg".into(),
                        data: "...".into(),
                    },
                ],
            },
            Message {
                role: Role::User,
                parts: vec![
                    Part::Text("My screen (Built-in, 1440x900 pixels):".into()),
                    Part::Image {
                        media_type: "image/jpeg".into(),
                        data: "...".into(),
                    },
                    Part::Text("thanks".into()),
                ],
            },
        ];
        assert_eq!(user_said(&messages), "I'm Sam, I use Outlook for work\nthanks\n");
        assert_eq!(user_said(&[]), "");
    }

    #[test]
    fn the_reply_is_read_leniently_and_garbage_saves_nothing() {
        let reply = "Here you go:\n```json\n{\"notes\": [\n  {\"text\": \" Prefers short answers \", \"replaces\": null},\n  {\"text\": \"Uses Gmail for work email now\", \"replaces\": \"3\"},\n  {\"text\": \"\"}\n]}\n```";
        assert_eq!(
            parse_review(reply),
            vec![
                Review { text: "Prefers short answers".into(), replaces: None },
                Review { text: "Uses Gmail for work email now".into(), replaces: Some(3) },
            ]
        );
        assert!(parse_review("{\"notes\": []}").is_empty());
        assert!(parse_review("Nothing new about the user.").is_empty());
        assert!(parse_review("} not json {").is_empty());
    }
}
