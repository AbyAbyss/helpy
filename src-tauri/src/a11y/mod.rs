//! What the OS accessibility layer says about the screen: the control under
//! a point (so guidance lands on real controls), password fields (so
//! screenshots can blank them), and the address of the browser page in
//! front (for "pull the data from this page").
//!
//! Linux uses AT-SPI, Windows UI Automation and macOS the AX API (which needs
//! the Accessibility permission). Every query has a short time limit, so a
//! slow or hung app never holds anything up; no answer just means Helpy
//! goes on without it.

use std::time::Duration;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[cfg(target_os = "linux")]
use linux as os;
#[cfg(target_os = "macos")]
use macos as os;
#[cfg(windows)]
use windows as os;

const LIMIT: Duration = Duration::from_millis(900);

/// A rectangle in global physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    fn area(&self) -> f64 {
        self.w.max(0.0) * self.h.max(0.0)
    }

    fn intersection(&self, o: &Rect) -> f64 {
        let w = (self.x + self.w).min(o.x + o.w) - self.x.max(o.x);
        let h = (self.y + self.h).min(o.y + o.h) - self.y.max(o.y);
        w.max(0.0) * h.max(0.0)
    }
}

/// A control on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub rect: Rect,
    pub role: String,
    pub name: String,
}

/// Whether Helpy may read other apps' controls and click for the user:
/// Some on macOS, where that needs the Accessibility permission.
pub fn trusted() -> Option<bool> {
    #[cfg(target_os = "macos")]
    return Some(macos::trusted());
    #[cfg(not(target_os = "macos"))]
    None
}

/// Shows macOS's own prompt to allow Accessibility; nothing elsewhere.
pub fn request_trust() {
    #[cfg(target_os = "macos")]
    macos::request_access();
}

/// The deepest control at a point in global physical pixels. `scale` is the
/// monitor's scale factor (macOS works in points).
pub async fn element_at(x: f64, y: f64, scale: f64) -> Option<Element> {
    tokio::time::timeout(LIMIT, os::element_at(x, y, scale))
        .await
        .ok()
        .flatten()
        .filter(|e| e.rect.w >= 1.0 && e.rect.h >= 1.0)
}

/// Password fields showing in the window in front.
pub async fn password_fields(scale: f64) -> Vec<Rect> {
    tokio::time::timeout(LIMIT, os::password_fields(scale))
        .await
        .unwrap_or_default()
}

/// The address of the page in the browser in front, if a browser is in
/// front and says.
pub async fn browser_url() -> Option<String> {
    let raw = tokio::time::timeout(LIMIT, os::browser_url())
        .await
        .ok()
        .flatten()?;
    normalize_url(&raw)
}

/// Whether a request is about the page the user has open.
pub fn refers_to_page(request: &str) -> bool {
    let r = request.to_lowercase();
    [
        "this page",
        "this site",
        "this website",
        "this tab",
        "this list",
        "this table",
        "current page",
        "the page i",
        "this link",
        "this url",
        "this shop",
        "this store",
    ]
    .iter()
    .any(|w| r.contains(w))
}

/// Address bars often leave out "https://"; search text and browser pages
/// aren't addresses.
pub fn normalize_url(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() || t.contains(char::is_whitespace) {
        return None;
    }
    let full = if t.starts_with("http://") || t.starts_with("https://") {
        t.to_string()
    } else if t.contains("://") || t.starts_with("about:") || t.starts_with("chrome:") {
        return None;
    } else if let Some(host) = t
        .split('/')
        .next()
        .filter(|h| h.contains('.') || h.starts_with("localhost"))
    {
        // Browsers hide the scheme. Sites are https these days; a bare IP
        // address or localhost is most likely a plain http server.
        let name = host.rsplit_once(':').map_or(host, |(n, _)| n);
        let local = name == "localhost" || name.parse::<std::net::IpAddr>().is_ok();
        format!("{}://{t}", if local { "http" } else { "https" })
    } else {
        return None;
    };
    reqwest::Url::parse(&full).ok().map(|u| u.to_string())
}

/// Whether an element is the size of a control rather than a whole pane
/// or window, on a screen of this size.
fn control_sized(r: &Rect, screen: (f64, f64)) -> bool {
    r.w >= 4.0 && r.h >= 4.0 && r.w <= screen.0 * 0.5 && r.h <= screen.1 * 0.3
}

/// The box to highlight: the control's own bounds when the model's box
/// is roughly around it.
pub fn snap_box(model: Rect, el: &Element, screen: (f64, f64)) -> Option<Rect> {
    if !control_sized(&el.rect, screen) {
        return None;
    }
    let inter = model.intersection(&el.rect);
    let union = model.area() + el.rect.area() - inter;
    let iou = if union > 0.0 { inter / union } else { 0.0 };
    // A loose box around the control, or a box inside a bigger button.
    let covers = inter >= el.rect.area() * 0.9 && el.rect.area() >= model.area() * 0.3;
    let inside = inter >= model.area() * 0.9 && model.area() >= el.rect.area() * 0.3;
    (iou >= 0.3 || covers || inside).then_some(el.rect)
}

/// Where to point: the middle of the control under the model's point.
pub fn snap_point(el: &Element, screen: (f64, f64)) -> Option<(f64, f64)> {
    control_sized(&el.rect, screen).then(|| el.rect.center())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(x: f64, y: f64, w: f64, h: f64) -> Element {
        Element {
            rect: Rect { x, y, w, h },
            role: "push button".into(),
            name: "Save".into(),
        }
    }

    const SCREEN: (f64, f64) = (1920.0, 1080.0);

    #[test]
    fn boxes_snap_to_the_control_they_roughly_cover() {
        let save = el(100.0, 100.0, 80.0, 30.0);
        // A loose box around the button.
        let loose = Rect {
            x: 90.0,
            y: 92.0,
            w: 100.0,
            h: 46.0,
        };
        assert_eq!(snap_box(loose, &save, SCREEN), Some(save.rect));
        // A box inside it, off to one side.
        let inner = Rect {
            x: 110.0,
            y: 105.0,
            w: 40.0,
            h: 20.0,
        };
        assert_eq!(snap_box(inner, &save, SCREEN), Some(save.rect));
        // A box somewhere else that only touches it.
        let far = Rect {
            x: 170.0,
            y: 120.0,
            w: 200.0,
            h: 200.0,
        };
        assert_eq!(snap_box(far, &save, SCREEN), None);
        // Whole panes are never snapped to.
        let pane = el(0.0, 0.0, 1200.0, 900.0);
        assert_eq!(
            snap_box(
                Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 1200.0,
                    h: 900.0
                },
                &pane,
                SCREEN
            ),
            None
        );
    }

    #[test]
    fn points_move_to_the_middle_of_small_controls() {
        assert_eq!(
            snap_point(&el(100.0, 100.0, 80.0, 30.0), SCREEN),
            Some((140.0, 115.0))
        );
        assert_eq!(snap_point(&el(0.0, 0.0, 1900.0, 600.0), SCREEN), None);
    }

    #[test]
    fn only_requests_about_the_open_page_get_its_address() {
        assert!(refers_to_page("Pull the prices from this page into a CSV"));
        assert!(refers_to_page("save this table as a spreadsheet"));
        assert!(!refers_to_page("research standing desks under 400"));
    }

    #[test]
    fn address_bar_text_becomes_a_url() {
        assert_eq!(
            normalize_url("example.com/desks?page=2").as_deref(),
            Some("https://example.com/desks?page=2")
        );
        assert_eq!(
            normalize_url(" https://shop.example/x ").as_deref(),
            Some("https://shop.example/x")
        );
        assert_eq!(
            normalize_url("localhost:8765/").as_deref(),
            Some("http://localhost:8765/")
        );
        assert_eq!(
            normalize_url("127.0.0.1:8765").as_deref(),
            Some("http://127.0.0.1:8765/")
        );
        assert_eq!(normalize_url("standing desks under 400"), None);
        assert_eq!(normalize_url("chrome://settings"), None);
        assert_eq!(normalize_url("about:blank"), None);
        assert_eq!(normalize_url("file:///etc/passwd"), None);
        assert_eq!(normalize_url(""), None);
    }
}
