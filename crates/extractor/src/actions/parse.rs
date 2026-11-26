//! Reading the models' answers the way the reference pipeline does:
//! stage 1 answers with a JSON array of `{action_type, start_time,
//! end_time}` somewhere in its text, stage 2 with `<tool_call>` blocks of
//! `computer_use` arguments. Anything else is no answer, never a guess.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value;

use super::{Action, Button, Point};

static DETECTIONS: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?s)\[\s*\{.*\}\s*\]").ok());
static TOOL_CALLS: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?s)<tool_call>\s*(\{.*?\})\s*</tool_call>").ok());

/// One action stage 1 found, in seconds from the start of its clip.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Detection {
    pub label: String,
    pub start_secs: f64,
    pub end_secs: f64,
}

/// The detections in a stage-1 answer: upstream's array of objects
/// anywhere in the text, or an answer that is just an array (`[]`: no
/// action). `None` when it holds no readable array.
pub(super) fn detections(text: &str) -> Option<Vec<Detection>> {
    let found = DETECTIONS
        .as_ref()?
        .find(text)
        .map_or(text.trim(), |found| found.as_str());
    let items: Vec<Value> = serde_json::from_str(found).ok()?;
    Some(
        items
            .iter()
            .filter_map(|item| {
                Some(Detection {
                    label: item.get("action_type")?.as_str()?.to_string(),
                    start_secs: number(item.get("start_time")?)?,
                    end_secs: number(item.get("end_time")?)?,
                })
            })
            .collect(),
    )
}

/// The typed actions in a stage-2 answer. Coordinates are in the
/// `size` (width, height) the clip was shown at, and come back as
/// fractions of the frame.
pub(super) fn calls(text: &str, size: (u32, u32)) -> Vec<Action> {
    let Some(pattern) = TOOL_CALLS.as_ref() else {
        return Vec::new();
    };
    pattern
        .captures_iter(text)
        .filter_map(|capture| serde_json::from_str::<Value>(capture.get(1)?.as_str()).ok())
        .filter_map(|call| call.get("arguments").and_then(|args| action(args, size)))
        .collect()
}

fn action(args: &Value, size: (u32, u32)) -> Option<Action> {
    let at = || args.get("coordinate").and_then(|c| point(c, size));
    let click = |button, count| Action::Click {
        button,
        count,
        at: at(),
    };
    Some(match args.get("action")?.as_str()? {
        "left_click" => click(Button::Left, 1),
        "double_click" => click(Button::Left, 2),
        "right_click" => click(Button::Right, 1),
        "middle_click" => click(Button::Middle, 1),
        "left_click_drag" => Action::Drag { to: at() },
        "scroll" => Action::Scroll {
            pixels: args.get("pixels").and_then(number),
            at: at(),
        },
        "type" => Action::Type {
            text: args.get("text").and_then(Value::as_str).map(str::to_string),
        },
        "key" => Action::Keypress {
            keys: args
                .get("keys")
                .and_then(Value::as_array)
                .map(|keys| {
                    keys.iter()
                        .filter_map(|key| key.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default(),
        },
        // mouse_move, wait and terminate change nothing on screen.
        _ => return None,
    })
}

/// The bare type a stage-1 label names, for when stage 2 adds nothing.
pub(super) fn from_label(label: &str) -> Option<Action> {
    let label = label.to_ascii_lowercase();
    let click = |button, count| Action::Click {
        button,
        count,
        at: None,
    };
    Some(match label.as_str() {
        "double_click" | "double click" => click(Button::Left, 2),
        "right_click" | "right click" => click(Button::Right, 1),
        "middle_click" | "middle click" => click(Button::Middle, 1),
        "click" | "left_click" | "left click" => click(Button::Left, 1),
        "drag" | "left_click_drag" => Action::Drag { to: None },
        "scroll" => Action::Scroll {
            pixels: None,
            at: None,
        },
        "type" | "typing" => Action::Type { text: None },
        "press" | "key" | "keypress" | "hotkey" => Action::Keypress { keys: Vec::new() },
        _ => return None,
    })
}

/// Detection F1 per type on VideoAgentTrek's held-out set (Table 1).
pub(super) fn published_f1(action: &Action) -> f64 {
    match action {
        Action::Click { .. } => 0.82,
        Action::Drag { .. } => 0.62,
        Action::Scroll { .. } => 0.86,
        Action::Type { .. } => 0.75,
        Action::Keypress { .. } => 0.14,
    }
}

fn point(coordinate: &Value, (width, height): (u32, u32)) -> Option<Point> {
    let [x, y] = coordinate.as_array()?.as_slice() else {
        return None;
    };
    let fraction = |v: f64, side: u32| {
        let f = v / f64::from(side);
        (side > 0 && (0.0..=1.0).contains(&f)).then(|| (f * 10_000.0).round() / 10_000.0)
    };
    Some(Point {
        x: fraction(number(x)?, width)?,
        y: fraction(number(y)?, height)?,
    })
}

/// A JSON number, or a string holding one (upstream reads both).
fn number(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
        .filter(|v: &f64| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_one_reads_the_array_out_of_surrounding_prose() {
        let found = detections(
            "Here you go:\n[{\"action_type\": \"scroll\", \"start_time\": 1, \"end_time\": 3.5}]\nDone.",
        )
        .unwrap();

        assert_eq!(
            found,
            [Detection {
                label: "scroll".into(),
                start_secs: 1.0,
                end_secs: 3.5
            }]
        );
        assert_eq!(detections("no actions here"), None);
        assert_eq!(detections(" [] "), Some(Vec::new()));
        assert_eq!(
            detections(r#"[{"action_type": "type"}] then [{"action_type": "key"}]"#),
            None,
            "two arrays are not one answer"
        );
    }

    #[test]
    fn stage_two_types_every_call_and_drops_the_ones_that_change_nothing() {
        let text = "<tool_call>\n{\"name\": \"computer_use\", \"arguments\": {\"action\": \"type\", \"text\": \"hello\"}}\n</tool_call>\n\
             <tool_call>{\"name\": \"computer_use\", \"arguments\": {\"action\": \"key\", \"keys\": [\"ctrl\", \"s\"]}}</tool_call>\n\
             <tool_call>{\"name\": \"computer_use\", \"arguments\": {\"action\": \"mouse_move\", \"coordinate\": [1, 1]}}</tool_call>\n\
             <tool_call>{\"name\": \"computer_use\", \"arguments\": {\"action\": \"scroll\", \"pixels\": -300, \"coordinate\": [50, 900]}}</tool_call>";

        assert_eq!(
            calls(text, (100, 200)),
            [
                Action::Type {
                    text: Some("hello".into())
                },
                Action::Keypress {
                    keys: vec!["ctrl".into(), "s".into()]
                },
                Action::Scroll {
                    pixels: Some(-300.0),
                    at: None,
                },
            ],
            "a point outside the frame is dropped, not clamped"
        );
    }
}
