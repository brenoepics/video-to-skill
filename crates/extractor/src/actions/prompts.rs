//! What each model is asked, and at what frame rate and size: copied from
//! the reference pipeline (xlang-ai/VideoAgentTrek at 0468503, MIT
//! licence, `video2action/stages/keyframe_detector.py` and
//! `action_identifier.py`), because a fine-tune answers best to the
//! prompt and input it was trained on. The pixel budgets reproduce
//! `smart_resize` from its `utils/qwen_vl_utils.py`, Python's banker's
//! rounding included.

/// Stage 1, system turn.
pub const DETECTOR_SYSTEM: &str = "You are a helpful assistant. you can identify all user \
interface interactions in this video, providing their action types and timestamps and output \
in JSON format.";
/// Stage 1, user turn after the video.
pub const DETECTOR_PROMPT: &str = "Identify all user interface interactions in this video, \
providing their action types and timestamps. Please output in JSON format.";
/// Stage 1 samples its 10 s clips at 2 fps.
pub const DETECTOR_FPS: f64 = 2.0;

/// Stage 2, system turn: the `computer_use` tool both models were
/// tuned against (its "168x224" screen size is upstream's, verbatim).
pub const PARAMETERIZER_SYSTEM: &str = include_str!("parameterizer_system.txt");
/// Stage 2, user turn after the video.
pub const PARAMETERIZER_PROMPT: &str =
    "Here is a video clip of a GUI interaction, please identify the interaction";
/// Stage 2 sees at most this many frames of an action clip.
const PARAMETERIZER_MAX_FRAMES: f64 = 20.0;

/// Both stages generate at most this many tokens.
pub const MAX_NEW_TOKENS: u32 = 256;

/// Upstream's fallback size when its resize fails (width, height).
const FALLBACK_SIZE: (u32, u32) = (896, 672);
const FACTOR: f64 = 28.0;
/// One visual token: a 14 px patch, merged 2x2.
const PIXELS_PER_TOKEN: f64 = 14.0 * 14.0 * 4.0;

/// The (width, height) the detector sees a `width` x `height` clip at:
/// at most 1280 visual tokens per frame.
#[must_use]
pub fn detector_request_size(width: u32, height: u32) -> (u32, u32) {
    smart_resize(
        width,
        height,
        PIXELS_PER_TOKEN,
        PIXELS_PER_TOKEN * 1280.0,
        50_000.0,
    )
    .unwrap_or(FALLBACK_SIZE)
}

/// The (width, height) the parameterizer sees an action clip at: a
/// 20 000-token budget shared across its frames, counted — as upstream
/// does — at the source frame rate.
#[must_use]
pub fn parameterizer_request_size(
    width: u32,
    height: u32,
    secs: f64,
    source_fps: f64,
) -> (u32, u32) {
    let frames = (secs * source_fps).floor();
    if frames < 1.0 {
        return FALLBACK_SIZE;
    }
    let max_pixels = 20_000.0 / frames * 2.0 * FACTOR * FACTOR;
    smart_resize(width, height, 16.0 * FACTOR * FACTOR, max_pixels, 8192.0).unwrap_or(FALLBACK_SIZE)
}

/// The highest of upstream's candidate rates that keeps an action clip
/// of `secs` within 20 frames, never above the source's own rate.
#[must_use]
pub fn dynamic_fps(secs: f64, source_fps: f64) -> f64 {
    [24.0, 12.0, 8.0, 6.0, 4.0, 3.0, 2.0, 1.0]
        .into_iter()
        .map(|fps: f64| fps.min(source_fps))
        .find(|fps| (secs * fps).floor() <= PARAMETERIZER_MAX_FRAMES)
        .unwrap_or_else(|| source_fps.min(1.0))
}

/// Qwen2.5-VL's resize: both sides a multiple of 28, the pixel count
/// within bounds, the aspect ratio kept. `None` for a degenerate frame.
fn smart_resize(
    width: u32,
    height: u32,
    min_pixels: f64,
    max_pixels: f64,
    max_long_side: f64,
) -> Option<(u32, u32)> {
    let (mut w, mut h) = (f64::from(width), f64::from(height));
    if w < 2.0 || h < 2.0 || w.max(h) / w.min(h) > 200.0 {
        return None;
    }
    if w.max(h) > max_long_side {
        let beta = w.max(h) / max_long_side;
        (w, h) = ((w / beta).trunc(), (h / beta).trunc());
    }
    let mut h_bar = (h / FACTOR).round_ties_even() * FACTOR;
    let mut w_bar = (w / FACTOR).round_ties_even() * FACTOR;
    if h_bar * w_bar > max_pixels {
        let beta = (h * w / max_pixels).sqrt();
        h_bar = (h / beta / FACTOR).floor() * FACTOR;
        w_bar = (w / beta / FACTOR).floor() * FACTOR;
    } else if h_bar * w_bar < min_pixels {
        let beta = (min_pixels / (h * w)).sqrt();
        h_bar = (h * beta / FACTOR).ceil() * FACTOR;
        w_bar = (w * beta / FACTOR).ceil() * FACTOR;
    }
    if h_bar < FACTOR || w_bar < FACTOR {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // positive, frame-sized
    Some((w_bar as u32, h_bar as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_detector_budget_matches_upstream_smart_resize() {
        // Worked by hand through upstream's `smart_resize`.
        assert_eq!(detector_request_size(320, 240), (308, 252));
        assert_eq!(detector_request_size(1920, 1080), (1316, 728));
    }

    #[test]
    fn the_parameterizer_shares_its_budget_across_the_clip() {
        assert_eq!(
            parameterizer_request_size(1920, 1080, 1.0, 30.0),
            (1344, 756)
        );
        assert_eq!(
            parameterizer_request_size(1920, 1080, 0.01, 30.0),
            (896, 672)
        );
    }

    #[test]
    fn action_clips_are_sampled_at_most_twenty_frames() {
        assert!((dynamic_fps(1.0, 30.0) - 12.0).abs() < f64::EPSILON);
        assert!((dynamic_fps(3.2, 30.0) - 6.0).abs() < f64::EPSILON);
        assert!((dynamic_fps(0.5, 10.0) - 10.0).abs() < f64::EPSILON);
        assert!((dynamic_fps(30.0, 30.0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn the_parameterizer_prompt_is_the_computer_use_tool() {
        assert!(PARAMETERIZER_SYSTEM.starts_with("You are provided with function signatures"));
        assert!(PARAMETERIZER_SYSTEM.contains("\"left_click_drag\""));
        assert!(PARAMETERIZER_SYSTEM.ends_with("</tool_call>"));
    }
}
