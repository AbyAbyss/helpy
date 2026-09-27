//! Accessibility snapping: a model's coordinates are a guess from a
//! screenshot, so highlights and pointers move onto the real control under
//! them when the system can say where it is. Without an answer they stay
//! where the model put them.

use super::step::{Action, Step};
use crate::a11y::{self, Rect};
use crate::capture::CaptureMeta;

pub async fn snap(mut step: Step, meta: CaptureMeta) -> Step {
    let screen = (meta.monitor_width as f64, meta.monitor_height as f64);
    let (iw, ih) = (meta.image_width as f64, meta.image_height as f64);
    let back = |x: f64, y: f64| {
        let (ix, iy) = meta.from_global_physical(x, y);
        (ix.clamp(0.0, iw), iy.clamp(0.0, ih))
    };
    for a in &mut step.actions {
        match a {
            Action::Highlight {
                x,
                y,
                width,
                height,
                ..
            } => {
                let (gx, gy) = meta.to_global_physical(*x, *y);
                let (gx2, gy2) = meta.to_global_physical(*x + *width, *y + *height);
                let model = Rect {
                    x: gx,
                    y: gy,
                    w: gx2 - gx,
                    h: gy2 - gy,
                };
                let (cx, cy) = model.center();
                let Some(el) = a11y::element_at(cx, cy, meta.scale_factor).await else {
                    continue;
                };
                if let Some(r) = a11y::snap_box(model, &el, screen) {
                    let (x0, y0) = back(r.x, r.y);
                    let (x1, y1) = back(r.x + r.w, r.y + r.h);
                    (*x, *y, *width, *height) = (x0, y0, x1 - x0, y1 - y0);
                }
            }
            Action::Point { x, y, .. }
            | Action::Arrow {
                to_x: x, to_y: y, ..
            } => {
                let (gx, gy) = meta.to_global_physical(*x, *y);
                let Some(el) = a11y::element_at(gx, gy, meta.scale_factor).await else {
                    continue;
                };
                if let Some((sx, sy)) = a11y::snap_point(&el, screen) {
                    (*x, *y) = back(sx, sy);
                }
            }
            Action::Line { .. }
            | Action::Text { .. }
            | Action::Image { .. }
            | Action::Speak { .. } => {}
        }
    }
    step
}
