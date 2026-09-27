//! Helpy's own app windows (not the overlays).

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, PhysicalSize};
use ts_rs::TS;

use crate::overlay::Overlays;

pub const SETTINGS: &str = "settings";
pub const ASK: &str = "ask";
pub const PILL: &str = "pill";
/// The walkthrough step card.
pub const STEP: &str = "step";
/// The agent plan card.
pub const PLAN: &str = "plan";
/// The agent dock at the screen edge.
pub const DOCK: &str = "dock";
/// The card that slides out of the dock for one agent.
pub const DOCK_CARD: &str = "dockcard";
/// Which agent the dock card shows (None: it's closed).
pub const DOCK_CARD_EVENT: &str = "dock://card";
/// The agent panel.
pub const AGENTS: &str = "agents";
pub const AGENTS_FOCUS_EVENT: &str = "agents://focus";
/// Focusing this instead of an agent id opens the approval inbox.
pub const INBOX: &str = "inbox";
/// Space between the dock and the screen edge, logical pixels.
const DOCK_MARGIN: f64 = 10.0;
/// From the bottom of the dock to the middle of its last chip (padding plus
/// half a chip), logical pixels.
const LAST_CHIP_INSET: f64 = 44.0;
/// The dock's size with one chip, logical pixels.
const ONE_CHIP_DOCK: (f64, f64) = (84.0, 88.0);
pub const BUDDY_HANDOFF_EVENT: &str = "buddy://handoff";
/// Tells the dock a chip is on its way, so it drops in when the copy lands.
pub const DOCK_INCOMING_EVENT: &str = "dock://incoming";

/// Where the buddy's copy flies when agents start, in one overlay's own
/// logical pixels. It can be off that overlay's screen: the copy then flies
/// off toward the monitor the dock is on.
#[derive(Serialize, TS, Clone, Copy, Debug)]
#[ts(export)]
pub struct BuddyHandoff {
    pub x: f64,
    pub y: f64,
}
/// Plan card size in logical pixels (matches tauri.conf.json).
const PLAN_SIZE: (f64, f64) = (460.0, 520.0);

/// Voice pill size in logical pixels (matches tauri.conf.json).
const PILL_SIZE: (f64, f64) = (300.0, 120.0);
pub const OPEN_SECTION_EVENT: &str = "settings://open-section";

/// Ask panel size in logical pixels (matches tauri.conf.json).
const ASK_SIZE: (f64, f64) = (440.0, 560.0);
/// Gap between the cursor and the panel, logical pixels.
const GAP: f64 = 18.0;

pub fn show_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(SETTINGS) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Where the panel's top-left corner goes: beside the cursor, flipped to the
/// other side when it wouldn't fit, and always inside the monitor.
pub fn place_near(
    cursor: (f64, f64),
    monitor: (f64, f64, f64, f64),
    size: (f64, f64),
    gap: f64,
) -> (f64, f64) {
    let (mx, my, mw, mh) = monitor;
    let (w, h) = size;
    let mut x = cursor.0 + gap;
    if x + w > mx + mw {
        x = cursor.0 - gap - w;
    }
    let mut y = cursor.1 + gap;
    if y + h > my + mh {
        y = cursor.1 - gap - h;
    }
    (
        x.clamp(mx, (mx + mw - w).max(mx)),
        y.clamp(my, (my + mh - h).max(my)),
    )
}

/// Opens the ask panel next to the cursor and focuses it.
pub fn show_ask(app: &AppHandle) {
    let Some(w) = app.get_webview_window(ASK) else {
        return;
    };
    if let Ok(cursor) = app.cursor_position() {
        let monitors = app.state::<Overlays>().snapshot();
        if let Some((_, m)) = monitors
            .iter()
            .find(|(_, m)| m.contains(cursor.x, cursor.y))
        {
            let size = (ASK_SIZE.0 * m.scale, ASK_SIZE.1 * m.scale);
            let (x, y) = place_near(
                (cursor.x, cursor.y),
                (m.x as f64, m.y as f64, m.width as f64, m.height as f64),
                size,
                GAP * m.scale,
            );
            let _ = w.set_size(LogicalSize::new(ASK_SIZE.0, ASK_SIZE.1));
            let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
        }
    }
    let _ = w.show();
    let _ = w.set_always_on_top(true);
    let _ = w.set_focus();
    let _ = w.emit_to(ASK, "ask://shown", ());
}

/// Shows the voice waveform just right of the cursor, without taking focus
/// from the app the user is in.
pub fn show_pill(app: &AppHandle) {
    let Some(w) = app.get_webview_window(PILL) else {
        return;
    };
    if let Ok(cursor) = app.cursor_position() {
        let monitors = app.state::<Overlays>().snapshot();
        if let Some((_, m)) = monitors
            .iter()
            .find(|(_, m)| m.contains(cursor.x, cursor.y))
        {
            let size = (PILL_SIZE.0 * m.scale, PILL_SIZE.1 * m.scale);
            // Sit level with the cursor, where the eye already is.
            let at = (cursor.x, cursor.y - 22.0 * m.scale);
            let (x, y) = place_near(
                at,
                (m.x as f64, m.y as f64, m.width as f64, m.height as f64),
                size,
                14.0 * m.scale,
            );
            let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
        }
    }
    let _ = w.show();
    let _ = w.set_always_on_top(true);
}

/// Shows the plan card beside the cursor and focuses it, so Enter starts
/// the plan and Esc drops it.
pub fn show_plan(app: &AppHandle) {
    let Some(w) = app.get_webview_window(PLAN) else {
        return;
    };
    // Beside the ask panel when it's open, so the card doesn't cover it.
    if let Some(ask) = app
        .get_webview_window(ASK)
        .filter(|a| a.is_visible().unwrap_or(false))
    {
        if let (Ok(p), Ok(s)) = (ask.outer_position(), ask.outer_size()) {
            let monitors = app.state::<Overlays>().snapshot();
            if let Some((_, m)) = monitors
                .iter()
                .find(|(_, m)| m.contains(p.x as f64, p.y as f64))
            {
                let width = PLAN_SIZE.0 * m.scale;
                let gap = 12.0 * m.scale;
                let left = p.x as f64 - gap - width;
                let x = if left >= m.x as f64 {
                    left
                } else {
                    p.x as f64 + s.width as f64 + gap
                };
                let x = x.min(m.x as f64 + m.width as f64 - width).max(m.x as f64);
                let _ = w.set_position(PhysicalPosition::new(x.round() as i32, p.y));
                let _ = w.show();
                let _ = w.set_always_on_top(true);
                let _ = w.set_focus();
                return;
            }
        }
    }
    if let Ok(cursor) = app.cursor_position() {
        let monitors = app.state::<Overlays>().snapshot();
        if let Some((_, m)) = monitors
            .iter()
            .find(|(_, m)| m.contains(cursor.x, cursor.y))
        {
            let size = (PLAN_SIZE.0 * m.scale, PLAN_SIZE.1 * m.scale);
            let (x, y) = place_near(
                (cursor.x, cursor.y),
                (m.x as f64, m.y as f64, m.width as f64, m.height as f64),
                size,
                GAP * m.scale,
            );
            let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
        }
    }
    let _ = w.show();
    let _ = w.set_always_on_top(true);
    let _ = w.set_focus();
}

pub fn hide_plan(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PLAN) {
        let _ = w.hide();
    }
}

/// The monitor the dock lives on: the primary one.
fn dock_monitor(app: &AppHandle) -> Option<crate::overlay::MonitorRect> {
    let primary = app.primary_monitor().ok().flatten()?;
    let snapshot = app.state::<Overlays>().snapshot();
    snapshot
        .iter()
        .map(|(_, m)| *m)
        .find(|m| m.x == primary.position().x && m.y == primary.position().y)
        .or_else(|| snapshot.first().map(|(_, m)| *m))
}

/// Where the dock's top-left goes for a size (physical pixels): against the
/// chosen edge, centred vertically, so the chips never jump as it resizes.
pub fn dock_origin(
    m: (f64, f64, f64, f64),
    size: (f64, f64),
    margin: f64,
    left: bool,
) -> (f64, f64) {
    let (mx, my, mw, mh) = m;
    let x = if left {
        mx + margin
    } else {
        mx + mw - size.0 - margin
    };
    let y = (my + (mh - size.1) / 2.0).max(my);
    (x, y)
}

/// The dock page reports its size; 0 hides it (nothing to show).
#[tauri::command]
pub fn dock_layout(app: AppHandle, width: f64, height: f64) {
    let Some(w) = app.get_webview_window(DOCK) else {
        return;
    };
    let s = app.state::<crate::settings::SettingsStore>().get();
    if width <= 0.0 || height <= 0.0 || !s.agents.dock {
        let _ = w.hide();
        return;
    }
    let Some(m) = dock_monitor(&app) else { return };
    let size = (width * m.scale, height * m.scale);
    let left = s.agents.dock_side == crate::settings::schema::DockSide::Left;
    let (x, y) = dock_origin(
        (m.x as f64, m.y as f64, m.width as f64, m.height as f64),
        size,
        DOCK_MARGIN * m.scale,
        left,
    );
    let _ = w.set_size(PhysicalSize::new(
        size.0.round() as u32,
        size.1.round() as u32,
    ));
    let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    if !w.is_visible().unwrap_or(false) {
        let _ = w.show();
        let _ = w.set_always_on_top(true);
    }
}

/// The dock card's state: which agent, where it points, and whether the
/// mouse is over the chips or the card (it closes shortly after it's over
/// neither, unless the user is typing in it).
#[derive(Default)]
pub struct DockCard {
    inner: std::sync::Mutex<CardState>,
}

#[derive(Default)]
struct CardState {
    open: Option<String>,
    /// The hovered chip's centre, in logical pixels from the dock's top.
    anchor: f64,
    /// The card's size in logical pixels, as the page last reported it.
    size: (f64, f64),
    over_dock: bool,
    over_card: bool,
    pinned: bool,
    /// Bumped on every change, so a pending close can tell it's stale.
    generation: u64,
}

/// Gap between the dock and its card, logical pixels.
const CARD_GAP: f64 = 10.0;
/// How long the card stays after the mouse leaves it and the chips.
const CARD_LINGER: std::time::Duration = std::time::Duration::from_millis(260);

fn emit_card(app: &AppHandle, open: &Option<String>) {
    let _ = app.emit(DOCK_CARD_EVENT, open);
}

/// Puts the card beside the dock, level with its chip, inside the monitor.
fn place_card(app: &AppHandle, st: &CardState) {
    let (Some(card), Some(dock), Some(m)) = (
        app.get_webview_window(DOCK_CARD),
        app.get_webview_window(DOCK),
        dock_monitor(app),
    ) else {
        return;
    };
    let (Ok(dp), Ok(ds)) = (dock.outer_position(), dock.outer_size()) else {
        return;
    };
    if st.open.is_none() || st.size.0 < 1.0 || st.size.1 < 1.0 {
        let _ = card.hide();
        return;
    }
    let (w, h) = (st.size.0 * m.scale, st.size.1 * m.scale);
    let left = app
        .state::<crate::settings::SettingsStore>()
        .get()
        .agents
        .dock_side
        == crate::settings::schema::DockSide::Left;
    let gap = CARD_GAP * m.scale;
    let x = if left {
        dp.x as f64 + ds.width as f64 + gap
    } else {
        dp.x as f64 - w - gap
    };
    let y = dp.y as f64 + st.anchor * m.scale - h / 2.0;
    let edge = 8.0 * m.scale;
    let y = y.clamp(
        m.y as f64 + edge,
        (m.y as f64 + m.height as f64 - h - edge).max(m.y as f64),
    );
    let _ = card.set_size(PhysicalSize::new(w.round() as u32, h.round() as u32));
    let _ = card.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    if !card.is_visible().unwrap_or(false) {
        let _ = card.show();
        let _ = card.set_always_on_top(true);
    }
}

/// Closes the card after a moment unless the mouse comes back or the user
/// is typing in it.
fn close_soon(app: &AppHandle, st: &mut CardState) {
    st.generation += 1;
    if st.over_dock || st.over_card || st.pinned || st.open.is_none() {
        return;
    }
    let generation = st.generation;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(CARD_LINGER).await;
        let state = app.state::<DockCard>();
        let mut st = state.inner.lock().unwrap();
        if st.generation == generation {
            close(&app, &mut st);
        }
    });
}

fn close(app: &AppHandle, st: &mut CardState) {
    st.open = None;
    st.pinned = false;
    st.generation += 1;
    emit_card(app, &st.open);
    if let Some(w) = app.get_webview_window(DOCK_CARD) {
        let _ = w.set_focusable(false);
        let _ = w.hide();
    }
}

/// A chip was hovered: show that agent's card level with it.
#[tauri::command]
pub fn dock_card_show(app: AppHandle, id: String, anchor: f64) {
    let state = app.state::<DockCard>();
    let mut st = state.inner.lock().unwrap();
    st.over_dock = true;
    st.anchor = anchor;
    st.generation += 1;
    if st.open.as_deref() != Some(id.as_str()) {
        if st.pinned {
            return;
        }
        st.open = Some(id);
        emit_card(&app, &st.open);
    }
    place_card(&app, &st);
}

/// The mouse entered or left the chips ("dock") or the card ("card").
#[tauri::command]
pub fn dock_card_hover(app: AppHandle, from: String, inside: bool) {
    let state = app.state::<DockCard>();
    let mut st = state.inner.lock().unwrap();
    if from == "dock" {
        st.over_dock = inside;
    } else {
        st.over_card = inside;
    }
    close_soon(&app, &mut st);
}

/// The card page reports its size; it's placed (and shown) from that.
#[tauri::command]
pub fn dock_card_layout(app: AppHandle, width: f64, height: f64) {
    let state = app.state::<DockCard>();
    let mut st = state.inner.lock().unwrap();
    st.size = (width, height);
    place_card(&app, &st);
}

/// Lets the card take typing while its input is in use, and keeps it open.
#[tauri::command]
pub fn dock_card_pin(app: AppHandle, on: bool) {
    let state = app.state::<DockCard>();
    let mut st = state.inner.lock().unwrap();
    st.pinned = on;
    if let Some(w) = app.get_webview_window(DOCK_CARD) {
        let _ = w.set_focusable(on);
        if on {
            let _ = w.set_focus();
        }
    }
    close_soon(&app, &mut st);
}

#[tauri::command]
pub fn dock_card_close(app: AppHandle) {
    let state = app.state::<DockCard>();
    let mut st = state.inner.lock().unwrap();
    close(&app, &mut st);
}

/// The agent the card shows, for a card page that loads late.
#[tauri::command]
pub fn dock_card_current(state: tauri::State<DockCard>) -> Option<String> {
    state.inner.lock().unwrap().open.clone()
}

/// The hand-off (R6): the voice pill glides to the dock and becomes the new
/// agent's chip. The pill page morphs itself on the event.
pub fn fly_pill_to_dock(app: &AppHandle) {
    let (Some(pill), Some(dock)) = (app.get_webview_window(PILL), app.get_webview_window(DOCK))
    else {
        return;
    };
    if !pill.is_visible().unwrap_or(false) {
        return;
    }
    let (Ok(from), Ok(size)) = (pill.outer_position(), pill.outer_size()) else {
        hide_pill(app);
        return;
    };
    // Aim for the bottom of the chip column, where a new chip appears.
    let target = match (dock.outer_position(), dock.outer_size(), dock.is_visible()) {
        (Ok(p), Ok(s), Ok(true)) => (
            p.x as f64 + s.width as f64 - size.width as f64 / 2.0,
            p.y as f64 + s.height as f64 - size.height as f64 / 2.0,
        ),
        _ => match dock_monitor(app) {
            Some(m) => (
                m.x as f64 + m.width as f64 - size.width as f64,
                m.y as f64 + m.height as f64 / 2.0,
            ),
            None => {
                hide_pill(app);
                return;
            }
        },
    };
    let _ = app.emit_to(PILL, "pill://handoff", ());
    let app = app.clone();
    std::thread::spawn(move || {
        const FRAMES: u32 = 28;
        let start = (from.x as f64, from.y as f64);
        let end = (
            target.0 - size.width as f64 / 2.0,
            target.1 - size.height as f64 / 2.0,
        );
        for i in 1..=FRAMES {
            let t = i as f64 / FRAMES as f64;
            let e = 1.0 - (1.0 - t).powi(3);
            let x = start.0 + (end.0 - start.0) * e;
            // A slight arc upward on the way.
            let y = start.1 + (end.1 - start.1) * e - (t * (1.0 - t)) * 120.0;
            let _ = pill.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
        hide_pill(&app);
    });
}

/// Whether new agents are handed off by the buddy (it's showing and there's
/// a dock to fly to) rather than by the voice pill.
fn buddy_hands_off(app: &AppHandle) -> bool {
    app.state::<crate::settings::SettingsStore>()
        .get()
        .agents
        .dock
        && crate::cursor::buddy_shown(app)
}

/// The hand-off from the buddy: a copy peels off beside the cursor and flies
/// to the dock, where the new agent's chip drops in. Each overlay page runs
/// the flight itself; only the one showing the buddy draws it. Call it just
/// before the agents are created.
pub fn buddy_to_dock(app: &AppHandle) {
    if !buddy_hands_off(app) {
        return;
    }
    let _ = app.emit_to(DOCK, DOCK_INCOMING_EVENT, ());
    let app = app.clone();
    std::thread::spawn(move || {
        // Give the dock a moment to make room for the new chip.
        std::thread::sleep(std::time::Duration::from_millis(150));
        let Some((tx, ty)) = last_chip_centre(&app) else {
            return;
        };
        for (label, m) in app.state::<Overlays>().snapshot() {
            let (x, y) = m.to_local_logical(tx, ty);
            let _ = app.emit_to(label.as_str(), BUDDY_HANDOFF_EVENT, BuddyHandoff { x, y });
        }
    });
}

/// After agents start from a spoken request: the buddy already carries the
/// hand-off when it's showing, so the pill just goes; otherwise the pill
/// glides to the dock itself.
pub fn pill_after_start(app: &AppHandle) {
    if buddy_hands_off(app) {
        hide_pill(app);
    } else {
        fly_pill_to_dock(app);
    }
}

/// The middle of the dock's newest chip in global physical pixels: the
/// bottom of the column, or where a first chip will appear when the dock is
/// still hidden.
fn last_chip_centre(app: &AppHandle) -> Option<(f64, f64)> {
    let m = dock_monitor(app)?;
    if let Some(dock) = app.get_webview_window(DOCK) {
        if let (Ok(p), Ok(s), Ok(true)) =
            (dock.outer_position(), dock.outer_size(), dock.is_visible())
        {
            return Some((
                p.x as f64 + s.width as f64 / 2.0,
                p.y as f64 + s.height as f64 - LAST_CHIP_INSET * m.scale,
            ));
        }
    }
    let left = app
        .state::<crate::settings::SettingsStore>()
        .get()
        .agents
        .dock_side
        == crate::settings::schema::DockSide::Left;
    let size = (ONE_CHIP_DOCK.0 * m.scale, ONE_CHIP_DOCK.1 * m.scale);
    let (x, y) = dock_origin(
        (m.x as f64, m.y as f64, m.width as f64, m.height as f64),
        size,
        DOCK_MARGIN * m.scale,
        left,
    );
    Some((x + size.0 / 2.0, y + size.1 / 2.0))
}

pub fn hide_pill(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(PILL) {
        let _ = w.hide();
    }
}

#[tauri::command]
pub fn ask_hide(app: AppHandle) {
    if let Some(w) = app.get_webview_window(ASK) {
        let _ = w.hide();
    }
}

/// Opens the agent panel, at one agent if given.
pub fn show_agents(app: &AppHandle, id: Option<String>) {
    if let Some(w) = app.get_webview_window(AGENTS) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        if let Some(id) = id {
            let _ = w.emit_to(AGENTS, AGENTS_FOCUS_EVENT, id);
        }
    }
}

#[tauri::command]
pub fn agents_open_panel(app: AppHandle, id: Option<String>) {
    show_agents(&app, id);
}

/// Opens a web page in the browser, e.g. a setup guide or a link in an
/// answer. Only http and https links.
#[tauri::command]
pub fn open_link(app: AppHandle, url: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("Only web links can be opened".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

/// Opens settings at a section, e.g. from an error in the ask panel.
#[tauri::command]
pub fn open_settings_section(app: AppHandle, section: String) {
    show_settings(&app);
    let _ = app.emit_to(SETTINGS, OPEN_SECTION_EVENT, section);
}

#[cfg(test)]
mod tests {
    use super::*;

    const MON: (f64, f64, f64, f64) = (0.0, 0.0, 1920.0, 1080.0);

    #[test]
    fn opens_below_right_of_the_cursor() {
        assert_eq!(
            place_near((100.0, 100.0), MON, (440.0, 560.0), 18.0),
            (118.0, 118.0)
        );
    }

    #[test]
    fn flips_left_and_up_near_the_bottom_right_corner() {
        assert_eq!(
            place_near((1900.0, 1000.0), MON, (440.0, 560.0), 18.0),
            (1442.0, 422.0)
        );
    }

    #[test]
    fn the_dock_hugs_the_chosen_edge_and_stays_centred() {
        let m = (1920.0, 0.0, 2560.0, 1440.0);
        assert_eq!(
            dock_origin(m, (64.0, 200.0), 10.0, false),
            (1920.0 + 2560.0 - 74.0, 620.0)
        );
        assert_eq!(dock_origin(m, (64.0, 400.0), 10.0, true), (1930.0, 520.0));
        // Taller than the screen: pinned to the top instead of off-screen.
        assert_eq!(dock_origin(m, (64.0, 2000.0), 10.0, false).1, 0.0);
    }

    #[test]
    fn stays_on_a_secondary_monitor_with_negative_coordinates() {
        let m = (-2560.0, 0.0, 2560.0, 1440.0);
        let (x, y) = place_near((-10.0, 5.0), m, (880.0, 1120.0), 36.0);
        assert!(x >= -2560.0 && x + 880.0 <= 0.0);
        assert!(y >= 0.0 && y + 1120.0 <= 1440.0);
    }
}
