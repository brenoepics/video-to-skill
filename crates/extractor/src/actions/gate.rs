//! The cursor gate: only screen states with a pointer on screen for most
//! of their samples (`GUI_PRESENCE`, ScreenFilter's 80 %) are screen
//! recording worth a 7B model's time. Adjacent gated states merge into
//! one stretch, cut into clips no longer than the detector was run on
//! upstream (Video2Action splits videos into 10 s segments).

use super::Window;
use crate::frames::cursor::evidence::GUI_PRESENCE;
use crate::frames::Shot;

/// Longest clip handed to the detector.
pub const MAX_WINDOW_SECS: f64 = 10.0;

/// The detector clips for `shots`, in time order.
#[must_use]
pub fn screen_recording_windows(shots: &[Shot]) -> Vec<Window> {
    let mut stretches: Vec<Window> = Vec::new();
    for shot in shots {
        if shot.cursor_presence.is_none_or(|p| p < GUI_PRESENCE) || shot.end_secs <= shot.start_secs
        {
            continue;
        }
        match stretches.last_mut() {
            Some(last) if (shot.start_secs - last.end_secs).abs() < 1e-6 => {
                last.end_secs = shot.end_secs;
            }
            _ => stretches.push(Window {
                start_secs: shot.start_secs,
                end_secs: shot.end_secs,
            }),
        }
    }
    stretches.into_iter().flat_map(split).collect()
}

/// Cut a stretch into the fewest equal clips of at most
/// [`MAX_WINDOW_SECS`], so no clip is a sliver too short to hold an
/// action and its result.
fn split(stretch: Window) -> Vec<Window> {
    let span = stretch.end_secs - stretch.start_secs;
    let count = (span / MAX_WINDOW_SECS - 1e-9).ceil().max(1.0);
    let length = span / count;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small, positive
    let count = count as usize;
    (0..count)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)] // small
            let at = |k: usize| stretch.start_secs + length * k as f64;
            Window {
                start_secs: at(i),
                end_secs: if i + 1 == count {
                    stretch.end_secs
                } else {
                    at(i + 1)
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frames::Keyframe;

    fn shot(start_secs: f64, end_secs: f64, presence: Option<f64>) -> Shot {
        Shot {
            index: 0,
            start_secs,
            end_secs,
            motion_density: 0.0,
            keyframe: Keyframe {
                timestamp_secs: start_secs,
                path: String::new(),
                native_path: None,
                hash: String::new(),
            },
            interior_keyframes: Vec::new(),
            changed_region: None,
            cursor_presence: presence,
            cursor: None,
        }
    }

    fn spans(windows: &[Window]) -> Vec<(f64, f64)> {
        windows.iter().map(|w| (w.start_secs, w.end_secs)).collect()
    }

    #[test]
    fn adjacent_screen_recording_states_merge_before_being_cut() {
        let shots = [
            shot(0.0, 6.0, Some(0.9)),
            shot(6.0, 12.0, Some(1.0)),
            shot(12.0, 15.0, Some(0.5)),
            shot(15.0, 18.0, None),
            shot(18.0, 21.0, Some(0.8)),
        ];

        assert_eq!(
            spans(&screen_recording_windows(&shots)),
            [(0.0, 6.0), (6.0, 12.0), (18.0, 21.0)]
        );
    }

    #[test]
    fn clips_never_exceed_ten_seconds_and_leave_no_sliver() {
        let shots = [shot(0.0, 10.5, Some(0.9)), shot(20.0, 50.0, Some(0.9))];

        assert_eq!(
            spans(&screen_recording_windows(&shots)),
            [
                (0.0, 5.25),
                (5.25, 10.5),
                (20.0, 30.0),
                (30.0, 40.0),
                (40.0, 50.0)
            ]
        );
    }
}
