//! Visual guidance: draws a step's marks on the overlay, shows the step card
//! (the only part the user can click) and waits until the user has done the
//! step, either by clicking near the target or by pressing Next.

pub mod step;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use ts_rs::TS;

use crate::capture::CaptureMeta;
use crate::overlay::{MonitorRect, Overlays};
use crate::settings::schema::{Advance, CardPosition};
use crate::settings::Settings;
use crate::windows::{self, place_near};
use step::{Mark, Step};

pub const MARKS_EVENT: &str = "guide://marks";
pub const CARD_EVENT: &str = "guide://card";
/// The step card's size in logical pixels (matches tauri.conf.json). The card
/// resizes itself to its content; this is only used to place it.
const CARD_SIZE: (f64, f64) = (360.0, 170.0);
/// Allowance around a target for a click to count, logical pixels.
const HIT_MARGIN: f64 = 12.0;
/// Time for the app to react to the click before the next screenshot.
const SETTLE: Duration = Duration::from_millis(700);

/// The marks one overlay should draw. Every overlay gets the event and keeps
/// only what is addressed to it.
#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MarksView {
    pub overlay: String,
    pub marks: Vec<Mark>,
}

#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CardView {
    pub number: u32,
    pub total: Option<u32>,
    pub instruction: String,
    /// The step is done and Helpy is looking at the result.
    pub checking: bool,
    /// A click on the target moves on; otherwise only Next does.
    pub click_advances: bool,
}

enum Command {
    Next,
    Repeat,
    Click(f64, f64),
}

#[derive(Default)]
pub struct GuideState {
    active: AtomicBool,
    card: Mutex<Option<CardView>>,
    waiter: Mutex<Option<mpsc::UnboundedSender<Command>>>,
    /// Which of Helpy's windows to bring back when the walkthrough ends.
    restore: Mutex<(bool, bool)>,
    /// None until the click listener starts, then whether it works.
    clicks: Mutex<Option<bool>>,
    /// Which native frosted-glass look the step card has, if any.
    glass: OnceLock<Option<&'static str>>,
}

/// Gives the step card a frosted, slightly see-through background where the
/// OS draws one well: vibrancy on macOS, Acrylic on Windows 11 22H2 and later.
/// Elsewhere (Linux, older Windows) the card stays opaque.
pub fn setup(app: &AppHandle) {
    let glass = app
        .get_webview_window(windows::STEP)
        .and_then(|w| apply_glass(&w));
    let _ = app.state::<GuideState>().glass.set(glass);
}

#[cfg(target_os = "macos")]
fn apply_glass(w: &tauri::WebviewWindow) -> Option<&'static str> {
    use tauri::window::{Effect, EffectState, EffectsBuilder};
    w.set_effects(
        EffectsBuilder::new()
            .effect(Effect::HudWindow)
            .state(EffectState::Active)
            .radius(14.0)
            .build(),
    )
    .ok()?;
    let _ = w.set_shadow(true);
    Some("macos")
}

#[cfg(windows)]
fn apply_glass(w: &tauri::WebviewWindow) -> Option<&'static str> {
    use tauri::window::{Color, Effect, EffectsBuilder};
    // Older builds only have an Acrylic that lags and can't round corners.
    if crate::platform::windows_build()? < 22523 {
        return None;
    }
    w.set_effects(
        EffectsBuilder::new()
            .effect(Effect::Acrylic)
            .color(Color(18, 21, 29, 150))
            .build(),
    )
    .ok()?;
    // Gives the undecorated window Windows 11's rounded corners and shadow.
    let _ = w.set_shadow(true);
    Some("windows")
}

#[cfg(not(any(target_os = "macos", windows)))]
fn apply_glass(_: &tauri::WebviewWindow) -> Option<&'static str> {
    None
}

pub fn active(app: &AppHandle) -> bool {
    app.state::<GuideState>().active.load(Ordering::SeqCst)
}

/// Starts listening for mouse clicks, once. The listener only observes; the
/// click still goes to the app underneath. It needs Accessibility permission
/// on macOS and doesn't work on Wayland; without it, steps advance with Next.
fn ensure_click_listener(app: &AppHandle) {
    let state = app.state::<GuideState>();
    let mut clicks = state.clicks.lock().unwrap();
    if clicks.is_some() {
        return;
    }
    *clicks = Some(true);
    let handle = app.clone();
    std::thread::Builder::new()
        .name("helpy-clicks".into())
        .spawn(move || {
            let app = handle.clone();
            let result = rdev::listen(move |e| {
                if !matches!(
                    e.event_type,
                    rdev::EventType::ButtonPress(rdev::Button::Left)
                ) {
                    return;
                }
                let state = app.state::<GuideState>();
                let Some(tx) = state.waiter.lock().unwrap().clone() else {
                    return;
                };
                if let Ok(p) = app.cursor_position() {
                    let _ = tx.send(Command::Click(p.x, p.y));
                }
            });
            if let Err(e) = result {
                log::warn!("click detection unavailable: {e:?}");
                let state = handle.state::<GuideState>();
                *state.clicks.lock().unwrap() = Some(false);
                // Tell a card that's already showing to ask for Next instead.
                let card = state.card.lock().unwrap().clone();
                if let Some(mut card) = card {
                    card.click_advances = false;
                    show_card_view(&handle, card);
                }
            }
        })
        .expect("spawn click listener");
}

fn click_advances(app: &AppHandle, s: &Settings) -> bool {
    s.guidance.advance == Advance::OnClick
        && *app.state::<GuideState>().clicks.lock().unwrap() != Some(false)
}

fn show_card_view(app: &AppHandle, card: CardView) {
    *app.state::<GuideState>().card.lock().unwrap() = Some(card.clone());
    let _ = app.emit(CARD_EVENT, card);
}

fn emit_marks(app: &AppHandle, overlay: &str, marks: Vec<Mark>) {
    let _ = app.emit(
        MARKS_EVENT,
        MarksView {
            overlay: overlay.into(),
            marks,
        },
    );
}

/// Hides the ask panel and voice pill so they don't cover what the step
/// points at, and takes Escape to stop the walkthrough.
fn begin(app: &AppHandle) {
    let state = app.state::<GuideState>();
    if state.active.swap(true, Ordering::SeqCst) {
        return;
    }
    let visible = |label: &str| {
        app.get_webview_window(label)
            .is_some_and(|w| w.is_visible().unwrap_or(false))
    };
    *state.restore.lock().unwrap() = (visible(windows::ASK), visible(windows::PILL));
    if let Some(w) = app.get_webview_window(windows::ASK) {
        let _ = w.hide();
    }
    windows::hide_pill(app);
    ensure_click_listener(app);
    crate::voice::update_escape(app);
}

/// Clears the marks, hides the card and brings back the panel or pill. Safe
/// to call when no walkthrough is running.
pub fn end(app: &AppHandle) {
    let state = app.state::<GuideState>();
    if !state.active.swap(false, Ordering::SeqCst) {
        return;
    }
    state.waiter.lock().unwrap().take();
    state.card.lock().unwrap().take();
    emit_marks(app, "", Vec::new());
    if let Some(w) = app.get_webview_window(windows::STEP) {
        let _ = w.hide();
    }
    let (ask, pill) = *state.restore.lock().unwrap();
    if ask {
        if let Some(w) = app.get_webview_window(windows::ASK) {
            let _ = w.show();
        }
    } else if pill {
        windows::show_pill(app);
    }
    crate::voice::update_escape(app);
}

/// The overlay window on the monitor a screenshot came from.
fn overlay_for(app: &AppHandle, meta: CaptureMeta) -> Option<(String, MonitorRect)> {
    app.state::<Overlays>()
        .snapshot()
        .into_iter()
        .find(|(_, m)| m.x == meta.monitor_x && m.y == meta.monitor_y)
}

/// Top-left corner of the card in physical pixels.
fn card_origin(
    position: CardPosition,
    m: &MonitorRect,
    bounds: Option<(f64, f64, f64, f64)>,
) -> (f64, f64) {
    let size = (CARD_SIZE.0 * m.scale, CARD_SIZE.1 * m.scale);
    let monitor = (m.x as f64, m.y as f64, m.width as f64, m.height as f64);
    let margin = 28.0 * m.scale;
    let centre_x = m.x as f64 + (m.width as f64 - size.0) / 2.0;
    match (position, bounds) {
        (CardPosition::NearTarget, Some((_, _, right, bottom))) => {
            place_near((right, bottom), monitor, size, 16.0 * m.scale)
        }
        (CardPosition::Top, _) => (centre_x, m.y as f64 + margin),
        _ => (centre_x, m.y as f64 + m.height as f64 - size.1 - margin),
    }
}

fn say(app: &AppHandle, s: &Settings, text: &str) {
    if s.voice_output.voice_guidance {
        let speaker = &app.state::<crate::voice::VoiceState>().speaker;
        speaker.stop();
        speaker.say(text);
    }
}

/// Shows one step and waits until the user has done it. Returns false when
/// the question was cancelled (Stop, Escape) first.
pub async fn show(
    app: &AppHandle,
    s: &Settings,
    number: u32,
    step: &Step,
    meta: CaptureMeta,
    cancel: &CancellationToken,
) -> bool {
    begin(app);
    let state = app.state::<GuideState>();
    let (tx, mut rx) = mpsc::unbounded_channel();
    *state.waiter.lock().unwrap() = Some(tx);

    let marks = step::marks(step, meta);
    let targets = step::targets(step, meta);
    let overlay = overlay_for(app, meta);
    let label = overlay.as_ref().map(|(l, _)| l.clone()).unwrap_or_default();
    let draw = || emit_marks(app, &label, marks.clone());
    draw();

    show_card_view(
        app,
        CardView {
            number,
            total: step.total.map(|t| t.max(number)),
            instruction: step.instruction.clone(),
            checking: false,
            click_advances: click_advances(app, s),
        },
    );
    if let Some(w) = app.get_webview_window(windows::STEP) {
        // Without a matching overlay (the monitors just changed), the card
        // still shows where it was, so Next and Stop stay reachable.
        if let Some((_, m)) = &overlay {
            let around = step::bounds(&targets, HIT_MARGIN * 3.0 * m.scale);
            let (x, y) = card_origin(s.guidance.card_position, m, around);
            let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
        }
        let _ = w.show();
        let _ = w.set_always_on_top(true);
    }
    say(app, s, &step::speech(step));

    let margin = HIT_MARGIN * meta.scale_factor;
    let fade = s.guidance.annotation_seconds;
    let fade_after = tokio::time::sleep(Duration::from_secs(fade.max(1) as u64));
    tokio::pin!(fade_after);
    let mut faded = fade == 0;

    let done = loop {
        tokio::select! {
            _ = cancel.cancelled() => break false,
            _ = &mut fade_after, if !faded => {
                faded = true;
                emit_marks(app, &label, Vec::new());
            }
            cmd = rx.recv() => match cmd {
                None | Some(Command::Next) => break true,
                Some(Command::Repeat) => {
                    draw();
                    say(app, s, &step::speech(step));
                }
                Some(Command::Click(x, y)) => {
                    if click_advances(app, s)
                        && !on_card(app, x, y)
                        && step::hit(&targets, x, y, margin)
                    {
                        break true;
                    }
                }
            }
        }
    };
    state.waiter.lock().unwrap().take();
    emit_marks(app, &label, Vec::new());
    if done {
        // Clone first: show_card_view takes the same lock.
        let card = state.card.lock().unwrap().clone();
        if let Some(mut card) = card {
            card.checking = true;
            show_card_view(app, card);
        }
        tokio::select! {
            _ = cancel.cancelled() => return false,
            _ = tokio::time::sleep(SETTLE) => {}
        }
    }
    done
}

fn on_card(app: &AppHandle, x: f64, y: f64) -> bool {
    let Some(w) = app.get_webview_window(windows::STEP) else {
        return false;
    };
    let (Ok(p), Ok(size)) = (w.outer_position(), w.outer_size()) else {
        return false;
    };
    x >= p.x as f64
        && y >= p.y as f64
        && x < p.x as f64 + size.width as f64
        && y < p.y as f64 + size.height as f64
}

/// The step card's buttons: "next", "repeat" or "stop".
#[tauri::command]
pub fn guide_action(app: AppHandle, action: String) {
    let state = app.state::<GuideState>();
    let send = |c: Command| {
        if let Some(tx) = state.waiter.lock().unwrap().as_ref() {
            let _ = tx.send(c);
        }
    };
    match action.as_str() {
        "next" => send(Command::Next),
        "repeat" => send(Command::Repeat),
        "stop" => crate::ai::ask::cancel(&app),
        _ => {}
    }
}

/// The step card's native glass ("macos", "windows"), or None when it's opaque.
#[tauri::command]
pub fn guide_card_glass(state: tauri::State<GuideState>) -> Option<&'static str> {
    state.glass.get().copied().flatten()
}

/// The card currently shown, for a card window that loads late.
#[tauri::command]
pub fn guide_card(state: tauri::State<GuideState>) -> Option<CardView> {
    state.card.lock().unwrap().clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: MonitorRect = MonitorRect {
        x: 0,
        y: 0,
        width: 3840,
        height: 2160,
        scale: 2.0,
    };

    #[test]
    fn card_sits_beside_the_target_or_at_the_chosen_edge() {
        let near = card_origin(
            CardPosition::NearTarget,
            &M,
            Some((100.0, 100.0, 400.0, 300.0)),
        );
        assert_eq!(near, (432.0, 332.0));
        // Top and bottom are centred, inside the monitor.
        assert_eq!(card_origin(CardPosition::Top, &M, None), (1560.0, 56.0));
        assert_eq!(
            card_origin(CardPosition::Bottom, &M, None),
            (1560.0, 2160.0 - 340.0 - 56.0)
        );
        // No target to stand next to: bottom centre.
        assert_eq!(
            card_origin(CardPosition::NearTarget, &M, None),
            card_origin(CardPosition::Bottom, &M, None)
        );
    }
}
