//! Chat history: every conversation with Helpy, typed or spoken, kept word
//! for word (the model's own copy is trimmed and summarized as it grows),
//! for the Chats window to list, read, continue and delete.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use crate::agents::AgentsState;

/// Tells the Chats window to reload.
pub const CHANGED_EVENT: &str = "chats://changed";
/// Characters of a question used as its chat's title.
const TITLE_CHARS: usize = 80;

/// One question and Helpy's answer.
#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct ChatTurn {
    /// Unix milliseconds.
    #[ts(type = "number")]
    pub at: i64,
    pub question: String,
    /// Asked out loud.
    pub voice: bool,
    /// Pictures that came with it.
    pub images: u32,
    /// PDFs that came with it, by name.
    pub documents: Vec<String>,
    /// Everything Helpy wrote, guidance steps included.
    pub answer: String,
    /// Why it wasn't answered, when it wasn't.
    pub error: Option<String>,
    pub model: String,
}

#[derive(Serialize, Deserialize, TS, Clone, Debug, PartialEq, Default)]
#[serde(default, rename_all = "camelCase")]
#[ts(export)]
pub struct Chat {
    pub id: String,
    pub title: String,
    #[ts(type = "number")]
    pub started: i64,
    #[ts(type = "number")]
    pub updated: i64,
    pub turns: Vec<ChatTurn>,
}

/// A chat as the list shows it.
#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChatSummary {
    pub id: String,
    pub title: String,
    #[ts(type = "number")]
    pub started: i64,
    #[ts(type = "number")]
    pub updated: i64,
    pub questions: u32,
    /// Some questions were spoken.
    pub voice: bool,
    /// All of them were.
    pub all_voice: bool,
    /// The start of the last answer.
    pub preview: String,
    /// Every question, for searching.
    pub asked: String,
}

impl Chat {
    fn summary(&self) -> ChatSummary {
        let last = self.turns.last();
        ChatSummary {
            id: self.id.clone(),
            title: self.title.clone(),
            started: self.started,
            updated: self.updated,
            questions: self.turns.len() as u32,
            voice: self.turns.iter().any(|t| t.voice),
            all_voice: !self.turns.is_empty() && self.turns.iter().all(|t| t.voice),
            preview: last
                .map(|t| t.error.clone().unwrap_or_else(|| first_line(&t.answer)))
                .unwrap_or_default(),
            asked: clip(&self.turns.iter().map(|t| t.question.as_str()).collect::<Vec<_>>().join(" · "), 4000),
        }
    }
}

fn first_line(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let plain = line.replace(['*', '`'], "");
    clip(plain.trim_start_matches(['#', '>', '-', ' ']), 140)
}

fn clip(text: &str, chars: usize) -> String {
    let t = text.trim();
    if t.chars().count() <= chars {
        return t.to_string();
    }
    let cut: String = t.chars().take(chars).collect();
    format!("{}…", cut.trim_end())
}

/// A question without the PDF text attached to it, and the PDFs' names.
pub fn split_documents(question: &str) -> (String, Vec<String>) {
    let mut names = Vec::new();
    let mut text = String::new();
    let mut rest = question;
    while let Some(start) = rest.find("\n\n<document name=\"") {
        text.push_str(&rest[..start]);
        let after = &rest[start + "\n\n<document name=\"".len()..];
        let Some(end) = after.find("</document>") else {
            rest = "";
            break;
        };
        if let Some(q) = after.find('"') {
            names.push(after[..q].to_string());
        }
        rest = &after[end + "</document>".len()..];
    }
    text.push_str(rest);
    (text, names)
}

/// What the Ask panel's text events add up to for one question: the parts
/// of finished model calls, and the call still writing.
#[derive(Default)]
pub struct Recording {
    done: String,
    partial: String,
}

impl Recording {
    pub fn text(&mut self, piece: &str) {
        self.partial.push_str(piece);
    }

    /// A model call finished; its text stays.
    pub fn checkpoint(&mut self) {
        let part = std::mem::take(&mut self.partial);
        self.push(&part);
    }

    /// A failed attempt's text is dropped.
    pub fn retry(&mut self) {
        self.partial.clear();
    }

    pub fn step(&mut self, number: u32, instruction: &str) {
        self.checkpoint();
        self.push(&format!("**Step {number}.** {instruction}"));
    }

    fn push(&mut self, part: &str) {
        let part = part.trim();
        if part.is_empty() {
            return;
        }
        if !self.done.is_empty() {
            self.done.push_str("\n\n");
        }
        self.done.push_str(part);
    }

    pub fn finish(mut self) -> String {
        self.checkpoint();
        self.done
    }
}

/// Adds a turn to chat `id` (a new chat when None), returning its id.
pub fn record(app: &AppHandle, id: Option<String>, turn: ChatTurn) -> String {
    let store = app.state::<AgentsState>();
    let store = store.store();
    let now = turn.at;
    let mut chat = id
        .as_deref()
        .and_then(|id| store.chat::<Chat>(id))
        .unwrap_or_else(|| Chat {
            id: id.clone().unwrap_or_else(|| format!("c{now}")),
            title: clip(&turn.question.replace('\n', " "), TITLE_CHARS),
            started: now,
            ..Default::default()
        });
    if chat.title.is_empty() {
        chat.title = clip(&turn.question.replace('\n', " "), TITLE_CHARS);
    }
    chat.updated = now;
    chat.turns.push(turn);
    store.save_chat(&chat.id, chat.updated, &chat);
    let _ = app.emit(CHANGED_EVENT, ());
    chat.id
}

#[tauri::command]
pub fn chats_list(app: AppHandle) -> Vec<ChatSummary> {
    app.state::<AgentsState>().store().chats::<Chat>().iter().map(Chat::summary).collect()
}

#[tauri::command]
pub fn chats_get(app: AppHandle, id: String) -> Option<Chat> {
    app.state::<AgentsState>().store().chat(&id)
}

#[tauri::command]
pub fn chats_delete(app: AppHandle, id: String) {
    app.state::<AgentsState>().store().delete_chat(&id);
    super::ask::forget_chat(&app, &id);
    let _ = app.emit(CHANGED_EVENT, ());
}

/// Deletes every chat, or those untouched for `days` days.
#[tauri::command]
pub fn chats_clear(app: AppHandle, days: Option<u32>) {
    let before = days.map(|d| chrono::Utc::now().timestamp_millis() - d as i64 * 86_400_000);
    app.state::<AgentsState>().store().delete_chats(before);
    if before.is_none() {
        super::ask::forget_chat(&app, "");
    }
    let _ = app.emit(CHANGED_EVENT, ());
}

/// Picks a chat up again in the Ask panel, where it left off.
#[tauri::command]
pub fn chats_continue(app: AppHandle, id: String) -> Result<(), String> {
    let chat: Chat = app
        .state::<AgentsState>()
        .store()
        .chat(&id)
        .ok_or("That chat was deleted")?;
    super::ask::continue_chat(&app, chat)
}

#[tauri::command]
pub fn chats_open(app: AppHandle, id: Option<String>) {
    crate::windows::show_chats(&app, id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_keep_finished_text_and_steps_and_drop_retries() {
        let mut r = Recording::default();
        r.text("Let me look.");
        r.checkpoint();
        r.text("half an ans");
        r.retry();
        r.step(1, "Click File");
        r.text("Done!");
        assert_eq!(r.finish(), "Let me look.\n\n**Step 1.** Click File\n\nDone!");
    }

    #[test]
    fn documents_are_named_not_kept() {
        let (q, docs) = split_documents("Sum this up\n\n<document name=\"a.pdf\">\nlots\n</document>\n\n<document name=\"b.pdf\">\nmore\n</document>");
        assert_eq!(q, "Sum this up");
        assert_eq!(docs, ["a.pdf", "b.pdf"]);
        assert_eq!(split_documents("plain").0, "plain");
    }

    #[test]
    fn summaries_show_the_last_answer_and_how_it_was_asked() {
        let chat = Chat {
            id: "c1".into(),
            title: "t".into(),
            turns: vec![
                ChatTurn { voice: true, answer: "# Hello\nthere".into(), ..Default::default() },
                ChatTurn { voice: false, answer: "**Paris** is the capital.".into(), ..Default::default() },
            ],
            ..Default::default()
        };
        let s = chat.summary();
        assert!(s.voice && !s.all_voice);
        assert_eq!(s.questions, 2);
        assert_eq!(s.preview, "Paris is the capital.");
    }

    #[test]
    fn chats_round_trip_newest_first() {
        let store = crate::agents::store::Store::in_memory();
        store.save_chat("a", 1, &Chat { id: "a".into(), updated: 1, ..Default::default() });
        store.save_chat("b", 2, &Chat { id: "b".into(), updated: 2, ..Default::default() });
        let ids: Vec<String> = store.chats::<Chat>().into_iter().map(|c| c.id).collect();
        assert_eq!(ids, ["b", "a"]);
        store.delete_chats(Some(2));
        assert_eq!(store.chats::<Chat>().len(), 1);
        store.delete_chat("b");
        assert!(store.chat::<Chat>("b").is_none());
    }
}
