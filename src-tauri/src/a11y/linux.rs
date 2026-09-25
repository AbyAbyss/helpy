//! AT-SPI. On X11 its screen coordinates are the same physical pixels as
//! the rest of Helpy; native Wayland apps don't report screen positions.

use atspi::connection::set_session_accessibility;
use atspi::proxy::accessible::{AccessibleProxy, ObjectRefExt};
use atspi::proxy::proxy_ext::ProxyExt;
use atspi::{
    AccessibilityConnection, CoordType, MatchType, ObjectMatchRule, Role, SortOrder, State,
};
use tokio::sync::OnceCell;

use super::{Element, Rect};

use atspi::zbus::Connection;

static CONN: OnceCell<Option<AccessibilityConnection>> = OnceCell::const_new();

async fn conn() -> Option<&'static AccessibilityConnection> {
    CONN.get_or_init(|| async {
        // GTK, Chromium and Firefox only publish their controls once
        // something asks for them, as a screen reader does.
        let _ = set_session_accessibility(true).await;
        AccessibilityConnection::new().await.ok()
    })
    .await
    .as_ref()
}

/// Every top-level window of every app, with the app's name.
async fn frames(ac: &'static AccessibilityConnection) -> Vec<(String, AccessibleProxy<'static>)> {
    let c = ac.connection();
    let Ok(root) = ac.root_accessible_on_registry().await else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for app in root.get_children().await.unwrap_or_default() {
        let Ok(app) = app.into_accessible_proxy(c).await else {
            continue;
        };
        let name = app.name().await.unwrap_or_default();
        for w in app.get_children().await.unwrap_or_default() {
            if let Ok(w) = w.into_accessible_proxy(c).await {
                out.push((name.clone(), w));
            }
        }
    }
    out
}

async fn has_state(a: &AccessibleProxy<'_>, s: State) -> bool {
    a.get_state().await.is_ok_and(|set| set.contains(s))
}

async fn extents(a: &AccessibleProxy<'_>) -> Option<Rect> {
    let comp = a.proxies().await.ok()?.component().await.ok()?;
    let (x, y, w, h) = comp.get_extents(CoordType::Screen).await.ok()?;
    Some(Rect {
        x: x as f64,
        y: y as f64,
        w: w as f64,
        h: h as f64,
    })
}

/// Showing elements with one of these roles under `within`: asked of the
/// app in one go where it supports that (Chromium, Firefox), otherwise by
/// walking the tree (GTK), up to a limit.
async fn find(
    c: &'static Connection,
    within: &AccessibleProxy<'static>,
    roles: &[Role],
    count: i32,
) -> Vec<AccessibleProxy<'static>> {
    let mut out = Vec::new();
    if let Ok(col) = async { within.proxies().await?.collection().await }.await {
        let rule = ObjectMatchRule::builder()
            .roles(roles, MatchType::Any)
            .states([State::Showing], MatchType::All)
            .build();
        for r in col
            .get_matches(rule, SortOrder::Canonical, count, true)
            .await
            .unwrap_or_default()
        {
            if let Ok(a) = r.into_accessible_proxy(c).await {
                out.push(a);
            }
        }
        if !out.is_empty() {
            return out;
        }
    }
    let mut queue = std::collections::VecDeque::from([within.clone()]);
    let mut seen = 0;
    while let Some(a) = queue.pop_front() {
        seen += 1;
        if seen > 1500 || out.len() >= count as usize {
            break;
        }
        if a.get_role().await.is_ok_and(|r| roles.contains(&r))
            && has_state(&a, State::Showing).await
        {
            out.push(a.clone());
        }
        for child in a.get_children().await.unwrap_or_default() {
            if let Ok(child) = child.into_accessible_proxy(c).await {
                queue.push_back(child);
            }
        }
    }
    out
}

pub async fn element_at(x: f64, y: f64, _scale: f64) -> Option<Element> {
    let ac = conn().await?;
    let c = ac.connection();
    let (px, py) = (x.round() as i32, y.round() as i32);
    let mut hit = None;
    for (_, w) in frames(ac).await {
        let Ok(p) = w.proxies().await else { continue };
        let Ok(comp) = p.component().await else {
            continue;
        };
        if !has_state(&w, State::Showing).await
            || !comp
                .contains(px, py, CoordType::Screen)
                .await
                .unwrap_or(false)
        {
            continue;
        }
        let active = has_state(&w, State::Active).await;
        if hit.is_none() || active {
            hit = Some(w);
        }
        if active {
            break;
        }
    }
    let mut cur = hit?;
    for _ in 0..40 {
        let Some(comp) = async { cur.proxies().await.ok()?.component().await.ok() }.await else {
            break;
        };
        let Ok(child) = comp
            .get_accessible_at_point(px, py, CoordType::Screen)
            .await
        else {
            break;
        };
        if child.is_null() || child.path().as_str() == cur.inner().path().as_str() {
            break;
        }
        let Ok(next) = child.into_accessible_proxy(c).await else {
            break;
        };
        cur = next;
    }
    Some(Element {
        rect: extents(&cur).await?,
        role: cur.get_role_name().await.unwrap_or_default(),
        name: cur.name().await.unwrap_or_default(),
    })
}

/// Password fields in every window showing, not just the one in front:
/// any of them can be in a screenshot.
pub async fn password_fields(_scale: f64) -> Vec<Rect> {
    let Some(ac) = conn().await else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (_, w) in frames(ac).await {
        if !has_state(&w, State::Showing).await {
            continue;
        }
        for f in find(ac.connection(), &w, &[Role::PasswordText], 20).await {
            if let Some(r) = extents(&f).await {
                out.push(r);
            }
        }
    }
    out
}

const BROWSERS: [&str; 7] = [
    "chrom", "firefox", "brave", "edge", "vivaldi", "opera", "epiphany",
];

pub async fn browser_url() -> Option<String> {
    let ac = conn().await?;
    // The browser in front; when Helpy's own panel is in front (the user is
    // typing the request), the browser window showing behind it.
    let mut found = None;
    for (app, w) in frames(ac).await {
        let app = app.to_lowercase();
        if !BROWSERS.iter().any(|b| app.contains(b)) || !has_state(&w, State::Showing).await {
            continue;
        }
        let active = has_state(&w, State::Active).await;
        if found.is_none() || active {
            found = Some(w);
        }
        if active {
            break;
        }
    }
    let w = found?;
    for e in find(ac.connection(), &w, &[Role::Entry, Role::Text], 30).await {
        let name = e.name().await.unwrap_or_default().to_lowercase();
        if !(name.contains("address") || name.contains("location") || name.contains("url")) {
            continue;
        }
        let text = async {
            e.proxies()
                .await
                .ok()?
                .text()
                .await
                .ok()?
                .get_text(0, -1)
                .await
                .ok()
        }
        .await;
        if let Some(t) = text.filter(|t| !t.trim().is_empty()) {
            return Some(t);
        }
    }
    None
}
