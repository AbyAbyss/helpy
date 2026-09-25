//! Circle to explain: the user draws around part of the screen and Helpy
//! explains it, labels a diagram's parts, copies or translates its text, or
//! answers questions about it. Only while this is open does an overlay take
//! the mouse; closing it makes the overlay click-through again.

pub mod parts;

use std::sync::Mutex;

use image::{imageops, DynamicImage, RgbaImage};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::ai::ask::{action_for, AskAction};
use crate::ai::call::{self, Progress};
use crate::ai::error::ErrorKind;
use crate::ai::types::{ChatRequest, Message, Part, Role};
use crate::overlay::{MonitorRect, Overlays};
use crate::settings::schema::{CircleAction, SelectionShape};
use crate::settings::SettingsStore;
use parts::{Crop, LabelPart, PartsFilter};

pub const EVENT: &str = "circle://event";

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum CircleEvent {
    /// Selection mode is on, on this overlay. `problem` says why nothing can
    /// be explained right now (no model, capture paused).
    Select {
        overlay: String,
        shape: SelectionShape,
        problem: Option<String>,
    },
    Closed,
    /// An action started; `question` is set for the user's own questions.
    Begin {
        action: CircleAction,
        question: Option<String>,
    },
    /// An attempt started on this model. Text from an earlier attempt is void.
    Attempt {
        model: String,
    },
    Text {
        text: String,
    },
    Retry {
        retry: u32,
        limit: u32,
        reason: String,
        wait: u32,
    },
    Parts {
        parts: Vec<LabelPart>,
    },
    Copied {
        chars: u32,
    },
    Done {
        model: String,
    },
    Error {
        message: String,
        action: Option<AskAction>,
    },
}

struct Session {
    overlay: String,
    monitor: MonitorRect,
    /// The screen as it was when selection started, until a crop is taken.
    frame: Option<RgbaImage>,
    crop: Option<(Crop, String)>,
    messages: Vec<Message>,
}

#[derive(Default)]
pub struct CircleState {
    session: Mutex<Option<Session>>,
    running: Mutex<Option<CancellationToken>>,
}

fn emit(app: &AppHandle, e: CircleEvent) {
    let _ = app.emit(EVENT, e);
}

pub fn active(app: &AppHandle) -> bool {
    app.state::<CircleState>().session.lock().unwrap().is_some()
}

/// Lets the overlay take the mouse and keyboard, or makes it click-through.
fn capture_input(app: &AppHandle, label: &str, on: bool) {
    let Some(w) = app.get_webview_window(label) else {
        return;
    };
    let _ = w.set_ignore_cursor_events(!on);
    let _ = w.set_focusable(on);
    if on {
        let _ = w.set_focus();
    }
}

/// The hotkey and the tray item: start selecting, or close if open.
pub fn toggle(app: &AppHandle) {
    if active(app) {
        end(app);
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move { begin(&app).await });
}

async fn begin(app: &AppHandle) {
    let Ok(cursor) = app.cursor_position() else {
        return;
    };
    let Some((overlay, monitor)) = app
        .state::<Overlays>()
        .snapshot()
        .into_iter()
        .find(|(_, m)| m.contains(cursor.x, cursor.y))
    else {
        return;
    };
    {
        let state = app.state::<CircleState>();
        let mut session = state.session.lock().unwrap();
        if session.is_some() {
            return;
        }
        *session = Some(Session {
            overlay: overlay.clone(),
            monitor,
            frame: None,
            crop: None,
            messages: Vec::new(),
        });
    }
    let settings = app.state::<SettingsStore>().get();
    let mut problem = parts::models(&settings.ai).err();
    // Freeze what's on screen now, before anything of Helpy's is drawn.
    match crate::capture::grab_cursor_monitor(app).await {
        Ok(frame) if frame.monitor == monitor => {
            if let Some(s) = app.state::<CircleState>().session.lock().unwrap().as_mut() {
                s.frame = Some(frame.image);
            }
        }
        Ok(_) => problem = Some("The screen changed while Helpy was looking. Try again".into()),
        Err(e) => problem = problem.or(Some(e.message)),
    }
    // Closed while capturing.
    if !active(app) {
        return;
    }
    capture_input(app, &overlay, true);
    crate::voice::update_escape(app);
    emit(
        app,
        CircleEvent::Select {
            overlay,
            shape: settings.circle.default_shape,
            problem,
        },
    );
}

/// Closes Circle to explain and gives the mouse back to other apps.
pub fn end(app: &AppHandle) {
    let state = app.state::<CircleState>();
    if let Some(c) = state.running.lock().unwrap().take() {
        c.cancel();
    }
    let Some(session) = state.session.lock().unwrap().take() else {
        return;
    };
    capture_input(app, &session.overlay, false);
    emit(app, CircleEvent::Closed);
    crate::voice::update_escape(app);
}

/// The user finished drawing. `points` are the outline in the overlay's CSS
/// pixels; the box around them is what gets explained.
// Async so the crop and JPEG encoding run off the main thread.
#[tauri::command]
pub async fn circle_select(app: AppHandle, points: Vec<(f64, f64)>) -> Result<(), String> {
    let settings = app.state::<SettingsStore>().get();
    {
        let state = app.state::<CircleState>();
        let mut guard = state.session.lock().unwrap();
        let s = guard.as_mut().ok_or("Circle to explain isn't open")?;
        let crop = Crop::around(&points, &s.monitor).ok_or("Draw around something a bit bigger")?;
        let frame = s.frame.take().ok_or("Helpy couldn't see the screen")?;
        let cut = imageops::crop_imm(&frame, crop.x, crop.y, crop.width, crop.height).to_image();
        let img = DynamicImage::ImageRgba8(cut);
        let sized = if (img.width(), img.height()) == (crop.image_width, crop.image_height) {
            img
        } else {
            img.resize_exact(
                crop.image_width,
                crop.image_height,
                imageops::FilterType::Triangle,
            )
        };
        use base64::Engine;
        let jpeg =
            base64::engine::general_purpose::STANDARD.encode(crate::capture::jpeg(&sized, 85));
        s.crop = Some((crop, jpeg));
        s.messages.clear();
    }
    if settings.circle.default_action != CircleAction::Menu {
        start_action(&app, settings.circle.default_action, None);
    }
    Ok(())
}

/// Runs an action on the selection: one of the buttons, or `question`
/// typed by the user (with the Menu action).
#[tauri::command]
pub fn circle_action(
    app: AppHandle,
    action: CircleAction,
    question: Option<String>,
) -> Result<(), String> {
    if app.state::<CircleState>().running.lock().unwrap().is_some() {
        return Err("Helpy is still working on the last one".into());
    }
    start_action(&app, action, question.filter(|q| !q.trim().is_empty()));
    Ok(())
}

#[tauri::command]
pub fn circle_close(app: AppHandle) {
    end(&app);
}

fn start_action(app: &AppHandle, action: CircleAction, question: Option<String>) {
    let cancel = CancellationToken::new();
    *app.state::<CircleState>().running.lock().unwrap() = Some(cancel.clone());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        run(&app, action, question, &cancel).await;
        // A cancelled run was closed; the slot may already hold a newer one.
        if !cancel.is_cancelled() {
            *app.state::<CircleState>().running.lock().unwrap() = None;
        }
    });
}

async fn run(
    app: &AppHandle,
    action: CircleAction,
    question: Option<String>,
    cancel: &CancellationToken,
) {
    let settings = app.state::<SettingsStore>().get();
    let fail = |message: String, action: Option<AskAction>| {
        emit(app, CircleEvent::Error { message, action })
    };
    let models = match parts::models(&settings.ai) {
        Ok(m) => m,
        Err(m) => return fail(m, Some(AskAction::OpenProviders)),
    };
    // Build the conversation: the image goes in the first message only.
    let (crop, messages) = {
        let state = app.state::<CircleState>();
        let mut guard = state.session.lock().unwrap();
        let Some(s) = guard.as_mut() else { return };
        let Some((crop, jpeg)) = s.crop.clone() else {
            return;
        };
        let text = match &question {
            Some(q) => q.clone(),
            None => match parts::prompt(action, &settings, crop) {
                Some(p) => p,
                None => return,
            },
        };
        if s.messages.is_empty() {
            s.messages.push(Message {
                role: Role::User,
                parts: vec![
                    Part::Image {
                        media_type: "image/jpeg".into(),
                        data: jpeg,
                    },
                    Part::Text(text),
                ],
            });
        } else {
            s.messages.push(Message::user_text(text));
        }
        (crop, s.messages.clone())
    };

    emit(
        app,
        CircleEvent::Begin {
            action,
            question: question.clone(),
        },
    );
    let explain = action == CircleAction::Explain && question.is_none();
    // Copied text is shown whole at the end, so "NO_TEXT" never flashes up.
    let stream_text = !(action == CircleAction::CopyText && question.is_none());
    let filter = Mutex::new(PartsFilter::default());
    let on = |p: Progress| match p {
        Progress::Started(model) => {
            *filter.lock().unwrap() = PartsFilter::default();
            emit(
                app,
                CircleEvent::Attempt {
                    model: model.to_string(),
                },
            );
        }
        Progress::Text(piece) if stream_text => {
            let shown = if explain {
                filter.lock().unwrap().push(piece)
            } else {
                Some(piece.to_string())
            };
            if let Some(text) = shown {
                emit(app, CircleEvent::Text { text });
            }
        }
        Progress::Text(_) | Progress::Spent { .. } => {}
        Progress::Retry {
            retry,
            limit,
            reason,
            wait,
            ..
        } => emit(
            app,
            CircleEvent::Retry {
                retry,
                limit,
                reason,
                wait: wait.as_millis() as u32,
            },
        ),
    };
    let base = ChatRequest {
        model: String::new(),
        system: parts::system_prompt(&settings),
        messages,
        tools: Vec::new(),
        max_tokens: settings.ai.max_response_tokens,
        temperature: settings.ai.temperature,
    };
    let result = call::stream(app, &settings, "circle", &models, base, cancel, &on).await;
    let completion = match result {
        Ok(c) => c,
        Err(e) if e.kind == ErrorKind::Cancelled => return,
        Err(e) => {
            // Forget the question that failed, so a retry starts clean.
            if let Some(s) = app.state::<CircleState>().session.lock().unwrap().as_mut() {
                s.messages.pop();
            }
            return fail(e.message.clone(), action_for(&e.kind));
        }
    };
    if let Some(s) = app.state::<CircleState>().session.lock().unwrap().as_mut() {
        s.messages.push(Message {
            role: Role::Assistant,
            parts: completion.parts.clone(),
        });
    }

    if explain {
        let (rest, json) = filter.lock().unwrap().finish();
        if let Some(text) = rest {
            emit(app, CircleEvent::Text { text });
        }
        if let Some(json) = json {
            emit(
                app,
                CircleEvent::Parts {
                    parts: parts::parse_parts(&json, crop),
                },
            );
        }
    } else if !stream_text {
        let text = completion.text().trim().to_string();
        if text == parts::NO_TEXT || text.is_empty() {
            return fail("There's no text in the selection.".into(), None);
        }
        emit(app, CircleEvent::Text { text: text.clone() });
        let chars = text.chars().count() as u32;
        match app.clipboard().write_text(text) {
            Ok(()) => emit(app, CircleEvent::Copied { chars }),
            Err(e) => return fail(format!("Couldn't copy the text: {e}"), None),
        }
    }
    emit(
        app,
        CircleEvent::Done {
            model: completion.model,
        },
    );
}

/// Hands the selection to background agents with the user's task. What
/// Helpy already said about it goes along as context.
#[tauri::command]
pub async fn circle_to_agent(app: AppHandle, task: String) -> Result<(), String> {
    let task = task.trim().to_string();
    if task.is_empty() {
        return Err("Say what the agent should do with this".into());
    }
    let (jpeg, notes) = {
        let state = app.state::<CircleState>();
        let guard = state.session.lock().unwrap();
        let s = guard.as_ref().ok_or("Circle to explain isn't open")?;
        let (_, jpeg) = s.crop.clone().ok_or("Select something first")?;
        let notes: Vec<String> = s
            .messages
            .iter()
            .filter(|m| m.role == Role::Assistant)
            .map(|m| {
                m.parts
                    .iter()
                    .filter_map(|p| match p {
                        Part::Text(t) => Some(t.clone()),
                        _ => None,
                    })
                    .collect::<String>()
            })
            .collect();
        (jpeg, notes)
    };
    let mut request = format!(
        "{task}\n\nThis is about a part of the screen the user circled (attached as a picture)."
    );
    if !notes.is_empty() {
        request += &format!(" Helpy already said this about it:\n{}", notes.join("\n"));
    }
    end(&app);
    crate::agents::planner::plan(&app, &request, Some(jpeg))
        .await
        .map(|_| ())
}

/// Copies text the card shows (a translation, a summary).
#[tauri::command]
pub fn circle_copy(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}
