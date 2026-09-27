//! What a guidance step is, as the model sends it and as the overlay draws
//! it. Everything here is pure so it can be tested without a screen.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use ts_rs::TS;

use crate::ai::types::ToolDef;
use crate::capture::CaptureMeta;

pub const SHOW_STEP: &str = "show_step";
const MAX_ACTIONS: usize = 6;
const MAX_INSTRUCTION: usize = 300;
const MAX_LABEL: usize = 60;
/// Corners in one line or shape.
const MAX_POINTS: usize = 24;
/// How far outside the screenshot a coordinate may be before it's rejected
/// instead of pulled back to the edge, as a share of the image size.
const EDGE_SLACK: f64 = 0.03;

/// One drawing or speaking action, in screenshot pixels.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Action {
    Highlight {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        #[serde(default)]
        label: Option<String>,
    },
    Point {
        x: f64,
        y: f64,
        #[serde(default)]
        label: Option<String>,
    },
    Arrow {
        from_x: f64,
        from_y: f64,
        to_x: f64,
        to_y: f64,
        #[serde(default)]
        label: Option<String>,
    },
    /// A line through points: open, or closed into a shape, straight or
    /// smoothed into a curve.
    Line {
        points: Vec<[f64; 2]>,
        #[serde(default)]
        closed: bool,
        #[serde(default)]
        curved: bool,
        #[serde(default)]
        label: Option<String>,
    },
    Speak {
        text: String,
    },
}

/// A step as the model sends it.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StepInput {
    pub instruction: String,
    #[serde(default)]
    pub step: Option<u32>,
    #[serde(default)]
    pub total: Option<u32>,
    #[serde(default)]
    pub actions: Vec<Action>,
}

/// A checked step: every coordinate is inside the screenshot.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub instruction: String,
    pub total: Option<u32>,
    pub actions: Vec<Action>,
}

/// A shape on one overlay, in that overlay's CSS pixels.
#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum Mark {
    Highlight {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        label: Option<String>,
        /// The model's own numbers, for the debug view.
        raw: String,
    },
    Point {
        x: f64,
        y: f64,
        label: Option<String>,
        raw: String,
    },
    Arrow {
        from_x: f64,
        from_y: f64,
        to_x: f64,
        to_y: f64,
        label: Option<String>,
        raw: String,
    },
    Line {
        points: Vec<[f64; 2]>,
        closed: bool,
        curved: bool,
        label: Option<String>,
        raw: String,
    },
}

/// A place the user is expected to click, in global physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Rect { x: f64, y: f64, w: f64, h: f64 },
    Spot { x: f64, y: f64 },
}

pub fn tool() -> ToolDef {
    ToolDef {
        name: SHOW_STEP.into(),
        description: "Show the user one step on their screen: highlight, point at or draw an arrow to the \
            exact place to click or look, or draw lines and shapes over what's on screen to explain it, with a \
            short instruction. Coordinates are pixels in the most recent \
            screenshot. The tool returns once the user has done the step, with a new screenshot, so you can \
            check the result and show the next step. Use it when the user asks where something is or how to \
            do something themselves in the app in front of them, not when they ask you to do it for them."
            .into(),
        schema: schema(),
    }
}

fn schema() -> Value {
    let label = json!({ "type": "string", "description": "A few words shown next to the mark." });
    json!({
        "type": "object",
        "properties": {
            "instruction": { "type": "string", "description": "One short sentence telling the user what to do." },
            "step": { "type": "integer", "description": "This step's number, starting at 1." },
            "total": { "type": "integer", "description": "Your best estimate of the number of steps." },
            "actions": {
                "type": "array",
                "maxItems": MAX_ACTIONS,
                "items": {
                    "type": "object",
                    "properties": {
                        "type": { "type": "string", "enum": ["highlight", "point", "arrow", "line", "speak"] },
                        "x": { "type": "number" }, "y": { "type": "number" },
                        "width": { "type": "number" }, "height": { "type": "number" },
                        "fromX": { "type": "number" }, "fromY": { "type": "number" },
                        "toX": { "type": "number" }, "toY": { "type": "number" },
                        "points": {
                            "type": "array",
                            "maxItems": MAX_POINTS,
                            "items": { "type": "array", "items": { "type": "number" } },
                            "description": "For line: [[x, y], ...], at least 2 points."
                        },
                        "closed": { "type": "boolean", "description": "For line: join the last point to the first, making a shape." },
                        "curved": { "type": "boolean", "description": "For line: a smooth curve through the points." },
                        "label": label,
                        "text": { "type": "string", "description": "For speak: what to say aloud." }
                    },
                    "required": ["type"]
                },
                "description": "highlight: a box (x, y = top-left corner, width, height) around a control. \
                    point: a pointer at x, y. arrow: from fromX, fromY to toX, toY. line: a line through points, \
                    closed for a triangle, square or other shape, curved for a curve; use lines to trace, \
                    underline or build on what's on screen (e.g. the sides of a triangle, a square on one side). \
                    Lines draw one after another in order. speak: say text aloud."
            }
        },
        "required": ["instruction", "actions"]
    })
}

/// How models without tool calling give a step: a reply that is only this
/// JSON object.
pub fn json_instructions() -> String {
    "When the user asks where something is or how to do something themselves in the app on screen (not \
     when they ask you to do it for them), and you have seen the screen, reply with only a JSON object and no other text: \
     {\"step\": {\"instruction\": \"...\", \"step\": 1, \"total\": 3, \"actions\": [...]}}. \
     Actions: {\"type\": \"highlight\", \"x\", \"y\", \"width\", \"height\", \"label\"} (x, y = top-left corner), \
     {\"type\": \"point\", \"x\", \"y\", \"label\"}, {\"type\": \"arrow\", \"fromX\", \"fromY\", \"toX\", \"toY\", \"label\"}, \
     {\"type\": \"line\", \"points\": [[x, y], ...], \"closed\", \"curved\", \"label\"} (closed makes a shape; use lines to trace or build on what's on screen), \
     {\"type\": \"speak\", \"text\"}. Coordinates are pixels in the latest screenshot. After the user does the \
     step you get a new screenshot; reply with the next step the same way, or with normal text when done."
        .into()
}

/// A step given as a JSON reply, if that's what the reply is. Tolerates a
/// Markdown code fence around it.
pub fn parse_reply(text: &str) -> Option<Value> {
    let t = text.trim();
    let t = t
        .strip_prefix("```json")
        .or_else(|| t.strip_prefix("```"))
        .map(|s| s.trim_end().trim_end_matches("```"))
        .unwrap_or(t)
        .trim();
    let v: Value = serde_json::from_str(t).ok()?;
    v.get("step").cloned()
}

/// Whether streamed text so far might still turn out to be a JSON step, so
/// it should be held back instead of shown.
pub fn may_be_json(held: &str) -> bool {
    let t = held.trim_start();
    t.is_empty() || t.starts_with('{') || t.starts_with("```") || "```".starts_with(t)
}

fn clean(s: &str, max: usize) -> String {
    let s = s.trim();
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", s[..i].trim_end()),
        None => s.to_string(),
    }
}

/// Checks a step against the screenshot it refers to. Coordinates slightly
/// off the edge are pulled in; anything further out is an error the model
/// can correct.
pub fn validate(input: &Value, width: u32, height: u32) -> Result<Step, String> {
    let s: StepInput = serde_json::from_value(input.clone())
        .map_err(|e| format!("The step isn't in the expected shape: {e}"))?;
    let instruction = clean(&s.instruction, MAX_INSTRUCTION);
    if instruction.is_empty() {
        return Err("Give the step a short instruction.".into());
    }
    if s.actions.len() > MAX_ACTIONS {
        return Err(format!("Use at most {MAX_ACTIONS} actions per step."));
    }
    let (w, h) = (width as f64, height as f64);
    let fit = |v: f64, max: f64, name: &str| -> Result<f64, String> {
        let slack = max * EDGE_SLACK;
        if !v.is_finite() || v < -slack || v > max + slack {
            return Err(format!(
                "{name} = {v} is outside the screenshot, which is {width}x{height} pixels."
            ));
        }
        Ok(v.clamp(0.0, max))
    };
    let label = |l: Option<String>| {
        l.map(|l| clean(&l, MAX_LABEL))
            .filter(|l: &String| !l.is_empty())
    };
    let mut actions = Vec::new();
    for a in s.actions {
        actions.push(match a {
            Action::Highlight {
                x,
                y,
                width: bw,
                height: bh,
                label: l,
            } => {
                let (x0, y0) = (fit(x, w, "x")?, fit(y, h, "y")?);
                let (x1, y1) = (fit(x + bw, w, "x + width")?, fit(y + bh, h, "y + height")?);
                if x1 - x0 < 2.0 || y1 - y0 < 2.0 {
                    return Err("A highlight needs a positive width and height.".into());
                }
                Action::Highlight {
                    x: x0,
                    y: y0,
                    width: x1 - x0,
                    height: y1 - y0,
                    label: label(l),
                }
            }
            Action::Point { x, y, label: l } => Action::Point {
                x: fit(x, w, "x")?,
                y: fit(y, h, "y")?,
                label: label(l),
            },
            Action::Arrow {
                from_x,
                from_y,
                to_x,
                to_y,
                label: l,
            } => Action::Arrow {
                from_x: fit(from_x, w, "fromX")?,
                from_y: fit(from_y, h, "fromY")?,
                to_x: fit(to_x, w, "toX")?,
                to_y: fit(to_y, h, "toY")?,
                label: label(l),
            },
            Action::Line {
                points,
                closed,
                curved,
                label: l,
            } => {
                let least = if closed { 3 } else { 2 };
                if points.len() < least || points.len() > MAX_POINTS {
                    return Err(format!(
                        "A {} needs {least} to {MAX_POINTS} points.",
                        if closed { "closed line" } else { "line" }
                    ));
                }
                let points = points
                    .into_iter()
                    .map(|[x, y]| Ok([fit(x, w, "x")?, fit(y, h, "y")?]))
                    .collect::<Result<Vec<_>, String>>()?;
                Action::Line {
                    points,
                    closed,
                    curved,
                    label: label(l),
                }
            }
            Action::Speak { text } => Action::Speak {
                text: clean(&text, MAX_INSTRUCTION),
            },
        });
    }
    Ok(Step {
        instruction,
        total: s.total.filter(|t| (1..=100).contains(t)),
        actions,
    })
}

fn raw(points: &[(f64, f64)]) -> String {
    points
        .iter()
        .map(|(x, y)| format!("{x:.0},{y:.0}"))
        .collect::<Vec<_>>()
        .join(" → ")
}

/// The step's shapes in the overlay's CSS pixels.
pub fn marks(step: &Step, meta: CaptureMeta) -> Vec<Mark> {
    // Screenshot pixels per overlay CSS pixel, for sizes.
    let kx = meta.monitor_width as f64 / meta.image_width as f64 / meta.scale_factor;
    let ky = meta.monitor_height as f64 / meta.image_height as f64 / meta.scale_factor;
    step.actions
        .iter()
        .filter_map(|a| match a.clone() {
            Action::Highlight {
                x,
                y,
                width,
                height,
                label,
            } => {
                let (ox, oy) = meta.to_overlay(x, y);
                Some(Mark::Highlight {
                    x: ox,
                    y: oy,
                    width: width * kx,
                    height: height * ky,
                    label,
                    raw: format!("{} {width:.0}x{height:.0}", raw(&[(x, y)])),
                })
            }
            Action::Point { x, y, label } => {
                let (ox, oy) = meta.to_overlay(x, y);
                Some(Mark::Point {
                    x: ox,
                    y: oy,
                    label,
                    raw: raw(&[(x, y)]),
                })
            }
            Action::Arrow {
                from_x,
                from_y,
                to_x,
                to_y,
                label,
            } => {
                let (fx, fy) = meta.to_overlay(from_x, from_y);
                let (tx, ty) = meta.to_overlay(to_x, to_y);
                Some(Mark::Arrow {
                    from_x: fx,
                    from_y: fy,
                    to_x: tx,
                    to_y: ty,
                    label,
                    raw: raw(&[(from_x, from_y), (to_x, to_y)]),
                })
            }
            Action::Line {
                points,
                closed,
                curved,
                label,
            } => {
                let pairs: Vec<(f64, f64)> = points.iter().map(|&[x, y]| (x, y)).collect();
                Some(Mark::Line {
                    points: pairs
                        .iter()
                        .map(|&(x, y)| {
                            let (ox, oy) = meta.to_overlay(x, y);
                            [ox, oy]
                        })
                        .collect(),
                    closed,
                    curved,
                    label,
                    raw: raw(&pairs),
                })
            }
            Action::Speak { .. } => None,
        })
        .collect()
}

/// What to say aloud for a step: its speak actions, or the instruction.
pub fn speech(step: &Step) -> String {
    let said: Vec<&str> = step
        .actions
        .iter()
        .filter_map(|a| match a {
            Action::Speak { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    if said.is_empty() {
        step.instruction.clone()
    } else {
        said.join(" ")
    }
}

/// Where the user should click, in global physical pixels.
pub fn targets(step: &Step, meta: CaptureMeta) -> Vec<Target> {
    let kx = meta.monitor_width as f64 / meta.image_width as f64;
    let ky = meta.monitor_height as f64 / meta.image_height as f64;
    step.actions
        .iter()
        .filter_map(|a| match *a {
            Action::Highlight {
                x,
                y,
                width,
                height,
                ..
            } => {
                let (gx, gy) = meta.to_global_physical(x, y);
                Some(Target::Rect {
                    x: gx,
                    y: gy,
                    w: width * kx,
                    h: height * ky,
                })
            }
            Action::Point { x, y, .. } => {
                let (gx, gy) = meta.to_global_physical(x, y);
                Some(Target::Spot { x: gx, y: gy })
            }
            Action::Arrow { to_x, to_y, .. } => {
                let (gx, gy) = meta.to_global_physical(to_x, to_y);
                Some(Target::Spot { x: gx, y: gy })
            }
            // Lines explain; they aren't places to click.
            Action::Line { .. } | Action::Speak { .. } => None,
        })
        .collect()
}

/// Where Helpy clicks for "Do it": the middle of the first target.
pub fn click_point(targets: &[Target]) -> Option<(f64, f64)> {
    targets.first().map(|t| match *t {
        Target::Rect { x, y, w, h } => (x + w / 2.0, y + h / 2.0),
        Target::Spot { x, y } => (x, y),
    })
}

/// Whether a click at (x, y) is on or near one of the targets. `margin` is in
/// physical pixels: a small allowance around boxes, and the radius around
/// points and arrow tips.
pub fn hit(targets: &[Target], x: f64, y: f64, margin: f64) -> bool {
    targets.iter().any(|t| match *t {
        Target::Rect { x: rx, y: ry, w, h } => {
            x >= rx - margin && x <= rx + w + margin && y >= ry - margin && y <= ry + h + margin
        }
        Target::Spot { x: sx, y: sy } => (x - sx).hypot(y - sy) <= margin * 2.5,
    })
}

/// The area the step's marks cover, in global physical pixels, so the step
/// card can sit beside it instead of on top of it. Points count as a circle
/// of `radius`, since what they point at has some size.
pub fn bounds(targets: &[Target], radius: f64) -> Option<(f64, f64, f64, f64)> {
    let mut it = targets.iter().map(|t| match *t {
        Target::Rect { x, y, w, h } => (x, y, x + w, y + h),
        Target::Spot { x, y } => (x - radius, y - radius, x + radius, y + radius),
    });
    let first = it.next()?;
    Some(it.fold(first, |a, b| {
        (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpy_clicks_the_middle_of_the_first_target() {
        let t = [
            Target::Rect {
                x: 100.0,
                y: 50.0,
                w: 40.0,
                h: 20.0,
            },
            Target::Spot { x: 1.0, y: 1.0 },
        ];
        assert_eq!(click_point(&t), Some((120.0, 60.0)));
        assert_eq!(
            click_point(&[Target::Spot { x: 5.0, y: 6.0 }]),
            Some((5.0, 6.0))
        );
        assert_eq!(click_point(&[]), None);
    }

    /// A 4K monitor at 200% to the right of a 1080p one, sent at 1568x882.
    fn meta() -> CaptureMeta {
        CaptureMeta {
            monitor_x: 1920,
            monitor_y: 0,
            monitor_width: 3840,
            monitor_height: 2160,
            scale_factor: 2.0,
            image_width: 1568,
            image_height: 882,
        }
    }

    #[test]
    fn parses_every_action_kind() {
        let s = validate(
            &json!({
                "instruction": "  Click Share  ",
                "step": 1,
                "total": 3,
                "actions": [
                    { "type": "highlight", "x": 10, "y": 20, "width": 100, "height": 40, "label": "Share" },
                    { "type": "point", "x": 60, "y": 40 },
                    { "type": "arrow", "fromX": 300, "fromY": 300, "toX": 60, "toY": 40, "label": "" },
                    { "type": "speak", "text": "Click the Share button." }
                ]
            }),
            1568,
            882,
        )
        .unwrap();
        assert_eq!(s.instruction, "Click Share");
        assert_eq!(s.total, Some(3));
        assert_eq!(s.actions.len(), 4);
        // An empty label counts as no label.
        assert!(matches!(&s.actions[2], Action::Arrow { label: None, .. }));
        assert_eq!(speech(&s), "Click the Share button.");
    }

    #[test]
    fn rejects_bad_steps_with_a_message_the_model_can_act_on() {
        let bad = |v: Value| validate(&v, 1568, 882).unwrap_err();
        assert!(bad(
            json!({ "instruction": "x", "actions": [{ "type": "point", "x": 2400, "y": 10 }] })
        )
        .contains("1568x882"));
        assert!(bad(json!({ "instruction": " ", "actions": [] })).contains("instruction"));
        assert!(
            bad(json!({ "instruction": "x", "actions": [{ "type": "circle" }] })).contains("shape")
        );
        assert!(bad(json!({ "instruction": "x", "actions": [
            { "type": "highlight", "x": 5, "y": 5, "width": 0, "height": 10 }
        ] }))
        .contains("width"));
        let many: Vec<Value> = (0..7)
            .map(|_| json!({ "type": "point", "x": 1, "y": 1 }))
            .collect();
        assert!(bad(json!({ "instruction": "x", "actions": many })).contains("at most"));
    }

    #[test]
    fn pulls_coordinates_just_off_the_edge_back_in() {
        let s = validate(
            &json!({ "instruction": "x", "actions": [
                { "type": "highlight", "x": -8, "y": 860, "width": 60, "height": 40 }
            ] }),
            1568,
            882,
        )
        .unwrap();
        assert_eq!(
            s.actions[0],
            Action::Highlight {
                x: 0.0,
                y: 860.0,
                width: 52.0,
                height: 22.0,
                label: None
            }
        );
    }

    #[test]
    fn maps_marks_to_overlay_pixels_on_a_scaled_monitor() {
        let s = validate(
            &json!({ "instruction": "x", "actions": [
                { "type": "highlight", "x": 784, "y": 441, "width": 196, "height": 98 },
                { "type": "arrow", "fromX": 0, "fromY": 0, "toX": 1568, "toY": 882 }
            ] }),
            1568,
            882,
        )
        .unwrap();
        let m = marks(&s, meta());
        // The screenshot's centre is the centre of the 1920x1080 CSS overlay,
        // and sizes scale by 3840 / 1568 / 2.
        let Mark::Highlight {
            x,
            y,
            width,
            height,
            ..
        } = &m[0]
        else {
            panic!()
        };
        assert!((x - 960.0).abs() < 0.01 && (y - 540.0).abs() < 0.01);
        assert!((width - 240.0).abs() < 0.01 && (height - 120.0).abs() < 0.2);
        let Mark::Arrow {
            to_x, to_y, raw, ..
        } = &m[1]
        else {
            panic!()
        };
        assert!((to_x - 1920.0).abs() < 0.01 && (to_y - 1080.0).abs() < 0.01);
        assert_eq!(raw, "0,0 → 1568,882");
    }

    #[test]
    fn clicks_near_a_target_count_and_others_do_not() {
        let s = validate(
            &json!({ "instruction": "x", "actions": [
                { "type": "highlight", "x": 784, "y": 441, "width": 196, "height": 98 },
                { "type": "point", "x": 100, "y": 100 }
            ] }),
            1568,
            882,
        )
        .unwrap();
        let t = targets(&s, meta());
        // The box is 480x240 physical pixels at (3840, 1080).
        let Target::Rect { x, y, w, h } = t[0] else {
            panic!()
        };
        assert!((x - 3840.0).abs() < 0.01 && (y - 1080.0).abs() < 0.01);
        assert!((w - 480.0).abs() < 0.01 && (h - 240.0).abs() < 0.3);
        assert!(hit(&t, 4000.0, 1200.0, 32.0));
        assert!(hit(&t, 3820.0, 1070.0, 32.0));
        assert!(!hit(&t, 3700.0, 1200.0, 32.0));
        // The point sits at (1920 + 244.9, 244.9).
        assert!(hit(&t, 2200.0, 250.0, 32.0));
        assert!(!hit(&t, 2300.0, 250.0, 32.0));
        // The box's corner and the point, grown by the radius.
        let b = bounds(&t, 10.0).unwrap();
        assert_eq!(
            [b.0, b.1, b.2, b.3].map(f64::round),
            [2155.0, 235.0, 4320.0, 1320.0]
        );
    }

    #[test]
    fn reads_a_json_step_reply_and_holds_back_only_json_looking_text() {
        let v = parse_reply("```json\n{\"step\": {\"instruction\": \"Go\", \"actions\": []}}\n```")
            .unwrap();
        assert_eq!(v["instruction"], "Go");
        assert!(parse_reply("{\"answer\": 1}").is_none());
        assert!(parse_reply("Open the File menu.").is_none());
        assert!(may_be_json("  {\"st"));
        assert!(may_be_json("``"));
        assert!(!may_be_json("Open"));
    }

    #[test]
    fn lines_map_to_overlay_pixels_and_are_not_click_targets() {
        let s = validate(
            &json!({ "instruction": "x", "actions": [
                { "type": "line", "points": [[0, 0], [784, 441], [1568, 882]], "closed": true, "label": "Leg A" },
                { "type": "line", "points": [[10, 10], [20, 20]], "curved": true }
            ] }),
            1568,
            882,
        )
        .unwrap();
        assert!(targets(&s, meta()).is_empty());
        let m = marks(&s, meta());
        let Mark::Line {
            points,
            closed,
            curved,
            label,
            raw,
        } = &m[0]
        else {
            panic!()
        };
        assert!(*closed && !*curved);
        assert_eq!(label.as_deref(), Some("Leg A"));
        assert!((points[1][0] - 960.0).abs() < 0.01 && (points[1][1] - 540.0).abs() < 0.01);
        assert!((points[2][0] - 1920.0).abs() < 0.01);
        assert_eq!(raw, "0,0 → 784,441 → 1568,882");
        assert!(matches!(
            &m[1],
            Mark::Line {
                curved: true,
                closed: false,
                ..
            }
        ));
    }

    #[test]
    fn rejects_lines_with_too_few_points_or_off_screen_corners() {
        let bad = |v: Value| validate(&v, 1568, 882).unwrap_err();
        assert!(bad(json!({ "instruction": "x", "actions": [
            { "type": "line", "points": [[1, 1]] }
        ] }))
        .contains("2 to"));
        assert!(bad(json!({ "instruction": "x", "actions": [
            { "type": "line", "points": [[1, 1], [5, 5]], "closed": true }
        ] }))
        .contains("3 to"));
        assert!(bad(json!({ "instruction": "x", "actions": [
            { "type": "line", "points": [[1, 1], [5, 3000]] }
        ] }))
        .contains("1568x882"));
    }

    #[test]
    fn long_text_is_shortened_on_a_character_boundary() {
        let long = "é".repeat(80);
        assert_eq!(clean(&long, MAX_LABEL).chars().count(), MAX_LABEL + 1);
    }
}
