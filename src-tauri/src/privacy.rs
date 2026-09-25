//! Privacy: apps Helpy never captures, blanked-out password fields, and
//! offline mode (only models and speech on this computer or its network).

use image::{Rgba, RgbaImage};

use crate::ai::error::{ErrorKind, ProviderError};
use crate::settings::schema::{ModelRef, Privacy, ProviderConfig, ProviderKind};
use crate::settings::Settings;

/// A window on the captured screen, in the image's pixels.
#[derive(Clone, Debug)]
pub struct Win {
    pub app: String,
    pub title: String,
    pub rect: Rect,
    pub focused: bool,
}

/// x, y, width, height in image pixels.
pub type Rect = (f64, f64, f64, f64);

/// The rule that blocks this window, if any. Rules match the app's name or
/// the window's title, ignoring case ("1Password", "bank").
pub fn blocked_by<'a>(w: &Win, rules: &'a [String]) -> Option<&'a str> {
    let (app, title) = (w.app.to_lowercase(), w.title.to_lowercase());
    rules.iter().map(|r| r.trim()).find(|r| {
        let r = r.to_lowercase();
        !r.is_empty() && (app.contains(&r) || title.contains(&r))
    })
}

/// What to do with a capture: refuse it when a blocked app is in front,
/// otherwise the areas to blank out.
#[derive(Debug, PartialEq)]
pub enum Verdict {
    Refuse(String),
    Blank(Vec<Rect>),
}

pub fn check(windows: &[Win], rules: &[String]) -> Verdict {
    if let Some(w) = windows.iter().find(|w| w.focused) {
        if let Some(r) = blocked_by(w, rules) {
            return Verdict::Refuse(format!(
                "Helpy doesn't look at the screen while {} is in front (Settings → Privacy, \"{r}\")",
                if w.app.is_empty() { &w.title } else { &w.app }
            ));
        }
    }
    Verdict::Blank(
        windows
            .iter()
            .filter(|w| blocked_by(w, rules).is_some())
            .map(|w| w.rect)
            .collect(),
    )
}

/// Paints these areas over, clipped to the image.
pub fn blank(image: &mut RgbaImage, rects: &[Rect]) {
    let (iw, ih) = (image.width() as f64, image.height() as f64);
    for &(x, y, w, h) in rects {
        let (x0, y0) = (x.max(0.0).floor() as u32, y.max(0.0).floor() as u32);
        let (x1, y1) = ((x + w).min(iw).ceil() as u32, (y + h).min(ih).ceil() as u32);
        for py in y0..y1 {
            for px in x0..x1 {
                image.put_pixel(px, py, Rgba([40, 40, 44, 255]));
            }
        }
    }
}

/// Applies the blocklist to a capture of `monitor`, whose top left and
/// size are in the same units xcap gives its windows.
pub fn guard(
    image: &mut RgbaImage,
    monitor: (f64, f64, f64, f64),
    p: &Privacy,
) -> Result<(), String> {
    if p.blocked_apps.iter().all(|r| r.trim().is_empty()) {
        return Ok(());
    }
    let windows = windows_on(monitor, image.width() as f64);
    match check(&windows, &p.blocked_apps) {
        Verdict::Refuse(why) => Err(why),
        Verdict::Blank(rects) => {
            blank(image, &rects);
            Ok(())
        }
    }
}

/// Visible windows of other apps that touch the monitor, in image pixels.
fn windows_on((mx, my, mw, mh): (f64, f64, f64, f64), image_width: f64) -> Vec<Win> {
    let k = if mw > 0.0 { image_width / mw } else { 1.0 };
    let own = std::process::id();
    let Ok(all) = xcap::Window::all() else {
        return Vec::new();
    };
    all.into_iter()
        .filter(|w| w.pid().ok() != Some(own) && !w.is_minimized().unwrap_or(false))
        .filter_map(|w| {
            let (x, y) = (w.x().ok()? as f64, w.y().ok()? as f64);
            let (ww, wh) = (w.width().ok()? as f64, w.height().ok()? as f64);
            let touches = x < mx + mw && x + ww > mx && y < my + mh && y + wh > my;
            touches.then(|| Win {
                app: w.app_name().unwrap_or_default(),
                title: w.title().unwrap_or_default(),
                // Some systems give the window without its frame, so the
                // title bar (which can show a page or account name) is
                // covered too.
                rect: (
                    (x - mx - 4.0) * k,
                    (y - my - 36.0) * k,
                    (ww + 8.0) * k,
                    (wh + 40.0) * k,
                ),
                focused: w.is_focused().unwrap_or(false),
            })
        })
        .collect()
}

/// Whether this desktop tells Helpy which windows are open. Native Wayland
/// and bare X servers don't, and then the blocklist can't work.
#[tauri::command]
pub async fn privacy_can_see_windows() -> bool {
    tauri::async_runtime::spawn_blocking(|| xcap::Window::all().is_ok())
        .await
        .unwrap_or(false)
}

/// A provider on this computer or its local network.
pub fn is_local(p: &ProviderConfig) -> bool {
    if matches!(
        p.kind,
        ProviderKind::Anthropic | ProviderKind::OpenAi | ProviderKind::Gemini
    ) && !p.base_url.trim().is_empty()
        && !local_url(&p.base_url)
    {
        return false;
    }
    let default_local = matches!(
        p.kind,
        ProviderKind::Ollama | ProviderKind::LmStudio | ProviderKind::LlamaCpp
    );
    if p.base_url.trim().is_empty() {
        return default_local;
    }
    local_url(&p.base_url)
}

fn local_url(url: &str) -> bool {
    let Ok(u) = reqwest::Url::parse(url.trim()) else {
        return false;
    };
    match u.host() {
        Some(url::Host::Domain(d)) => d == "localhost" || d.ends_with(".local"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback() || (ip.segments()[0] & 0xfe00) == 0xfc00,
        None => false,
    }
}

/// In offline mode, only the models on local providers; an error when none
/// of `models` is one.
pub fn allowed_models(s: &Settings, models: &[ModelRef]) -> Result<Vec<ModelRef>, ProviderError> {
    if !s.privacy.offline {
        return Ok(models.to_vec());
    }
    let local: Vec<_> = models
        .iter()
        .filter(|r| s.ai.model(r).is_some_and(|(p, _)| is_local(p)))
        .cloned()
        .collect();
    if local.is_empty() {
        return Err(ProviderError::new(
            ErrorKind::Setup,
            "Offline mode is on and no model on this computer is set for this. Choose a local model (Ollama, LM Studio, llama.cpp) in Settings → AI providers, or turn offline mode off in Settings → Privacy",
        ));
    }
    Ok(local)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(app: &str, title: &str, focused: bool) -> Win {
        Win {
            app: app.into(),
            title: title.into(),
            rect: (10.0, 10.0, 20.0, 20.0),
            focused,
        }
    }

    #[test]
    fn refuses_when_a_blocked_app_is_in_front_and_blanks_it_behind() {
        let rules = vec!["1Password".to_string(), "bank".to_string()];
        let v = check(&[win("1Password 8", "Vault", true)], &rules);
        assert!(matches!(v, Verdict::Refuse(m) if m.contains("1Password 8")));
        let v = check(
            &[
                win("Firefox", "Chase Bank - Accounts", false),
                win("Code", "main.rs", true),
            ],
            &rules,
        );
        assert_eq!(v, Verdict::Blank(vec![(10.0, 10.0, 20.0, 20.0)]));
        assert_eq!(
            check(&[win("Code", "main.rs", true)], &rules),
            Verdict::Blank(vec![])
        );
        assert_eq!(
            check(&[win("Code", "x", true)], &["  ".into()]),
            Verdict::Blank(vec![])
        );
    }

    #[test]
    fn blanking_stays_inside_the_image() {
        let mut img = RgbaImage::from_pixel(10, 10, Rgba([255, 255, 255, 255]));
        blank(&mut img, &[(-5.0, 8.0, 100.0, 100.0)]);
        assert_eq!(img.get_pixel(0, 9), &Rgba([40, 40, 44, 255]));
        assert_eq!(img.get_pixel(0, 7), &Rgba([255, 255, 255, 255]));
    }

    #[test]
    fn offline_mode_keeps_only_local_models() {
        use crate::settings::schema::ModelConfig;
        let provider = |id: &str, kind| ProviderConfig {
            id: id.into(),
            kind,
            models: vec![ModelConfig {
                id: "m".into(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut s = Settings::default();
        s.ai.providers = vec![
            provider("cloud", ProviderKind::Anthropic),
            provider("home", ProviderKind::Ollama),
        ];
        let r = |p: &str| ModelRef {
            provider_id: p.into(),
            model: "m".into(),
        };
        let both = [r("cloud"), r("home")];
        assert_eq!(allowed_models(&s, &both).unwrap().len(), 2);
        s.privacy.offline = true;
        assert_eq!(allowed_models(&s, &both).unwrap(), [r("home")]);
        assert!(allowed_models(&s, &[r("cloud")])
            .unwrap_err()
            .message
            .contains("Offline mode"));
    }

    #[test]
    fn local_providers_are_the_ones_on_this_computer_or_network() {
        let p = |kind, url: &str| ProviderConfig {
            kind,
            base_url: url.into(),
            ..Default::default()
        };
        assert!(is_local(&p(ProviderKind::Ollama, "")));
        assert!(is_local(&p(
            ProviderKind::Ollama,
            "http://192.168.1.20:11434"
        )));
        assert!(!is_local(&p(
            ProviderKind::Ollama,
            "https://ollama.example.com"
        )));
        assert!(is_local(&p(
            ProviderKind::OpenAiCompatible,
            "http://localhost:8080/v1"
        )));
        assert!(!is_local(&p(
            ProviderKind::OpenAiCompatible,
            "https://api.groq.com/openai/v1"
        )));
        assert!(!is_local(&p(ProviderKind::Anthropic, "")));
        assert!(!is_local(&p(
            ProviderKind::OpenAi,
            "https://api.openai.com/v1"
        )));
    }
}
