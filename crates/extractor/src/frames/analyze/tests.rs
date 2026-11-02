//! Screen-state segmentation on drawn thumbnails: each expectation
//! follows from what was drawn, never from the implementation.

use super::super::sample::{THUMB_H, THUMB_W};
use super::*;

const RULE: StateRule = StateRule {
    change_fraction: CHANGE_FRACTION,
    steady_samples: 1,
};

/// A dark screen with `lines` text-like rows (3 rows tall, up to 120
/// wide) stacked from the top, scrolled up by `scroll` rows.
fn screen(lines: usize, scroll: usize) -> Vec<u8> {
    let mut luma = vec![20u8; THUMB_W * THUMB_H];
    for line in 0..lines {
        for row in line * 5..line * 5 + 3 {
            let Some(screen_row) = row.checked_sub(scroll) else {
                continue;
            };
            if screen_row >= THUMB_H {
                continue;
            }
            let len = 60 + (line * 23) % 60;
            luma[screen_row * THUMB_W..screen_row * THUMB_W + len].fill(200);
        }
    }
    luma
}

fn frames(lumas: &[Vec<u8>]) -> Vec<SampledFrame> {
    lumas
        .iter()
        .enumerate()
        .map(|(i, luma)| SampledFrame::from_luma(f64::from(u32::try_from(i).unwrap()), luma))
        .collect()
}

fn states(lumas: &[Vec<u8>]) -> Vec<(usize, usize)> {
    segment_states(&frames(lumas), &RULE)
        .iter()
        .map(|s| (s.start, s.evidence))
        .collect()
}

#[test]
fn a_still_screen_is_one_state_with_its_first_frame_as_evidence() {
    assert_eq!(states(&vec![screen(4, 0); 6]), vec![(0, 0)]);
    assert_eq!(segment_states(&[], &RULE), Vec::<State>::new());
}

#[test]
fn each_new_line_starts_a_state_whose_evidence_is_the_first_steady_frame() {
    let lumas = [
        screen(4, 0),
        screen(4, 0),
        screen(5, 0), // a line printed
        screen(5, 0),
        screen(5, 0),
        screen(6, 0), // another
        screen(6, 0),
    ];
    assert_eq!(states(&lumas), vec![(0, 0), (2, 2), (5, 5)]);
}

#[test]
fn a_transition_yields_one_state_whose_evidence_is_where_it_settles() {
    let lumas = [
        screen(4, 0),
        screen(4, 0),
        screen(5, 0), // output printing line by line…
        screen(6, 0),
        screen(7, 0),
        screen(7, 0), // …settled here
        screen(7, 0),
    ];
    assert_eq!(states(&lumas), vec![(0, 0), (2, 4)]);
}

#[test]
fn an_opening_animation_puts_the_first_evidence_frame_where_it_settles() {
    let lumas = [screen(1, 0), screen(3, 0), screen(5, 0), screen(5, 0)];
    assert_eq!(states(&lumas), vec![(0, 2)]);
}

#[test]
fn a_slow_scroll_is_folded_into_its_state() {
    let lumas: Vec<Vec<u8>> = [0, 0, 0, 5, 10, 15, 20, 20, 20]
        .iter()
        .map(|&scroll| screen(18, scroll))
        .collect();
    assert_eq!(states(&lumas), vec![(0, 0)]);
}

#[test]
fn a_line_printed_after_a_scroll_is_still_a_new_state() {
    let lumas = [
        screen(18, 0),
        screen(18, 0),
        screen(18, 6),
        screen(18, 12),
        screen(18, 12),
        screen(19, 12), // the scrolled screen then gains a line
        screen(19, 12),
    ];
    assert_eq!(states(&lumas), vec![(0, 0), (5, 5)]);
}

#[test]
fn a_hard_cut_to_another_screen_is_a_state() {
    let light = vec![230u8; THUMB_W * THUMB_H];
    let lumas = [screen(6, 0), screen(6, 0), light.clone(), light];
    assert_eq!(states(&lumas), vec![(0, 0), (2, 2)]);
}

#[test]
fn small_edits_that_add_up_end_the_state_once_they_have_drifted() {
    // One short "word" per sample along a line: no single step is a
    // change, but the screen drifts away from its evidence frame.
    let mut lumas = vec![screen(2, 0)];
    for word in 0..6usize {
        let mut next = lumas[word].clone();
        let col = word * 24;
        for r in 20..23 {
            next[r * THUMB_W + col..r * THUMB_W + col + 16].fill(200);
        }
        lumas.push(next);
    }
    let found = states(&lumas);
    assert_eq!(
        found.len(),
        2,
        "a line's worth of words is one new state: {found:?}"
    );
    assert!(found[1].0 >= 3, "one word is not a change: {found:?}");
}

/// `screen(lines, 0)` with a noisy "facecam" in the bottom-right
/// corner whose picture is drawn from `seed` (equal seeds, same picture).
fn screen_with_facecam(lines: usize, seed: u32) -> Vec<u8> {
    let mut luma = screen(lines, 0);
    for r in 60..THUMB_H {
        for c in 110..THUMB_W {
            let pixel = u32::try_from(r * THUMB_W + c).unwrap();
            let noise = pixel.wrapping_mul(2_654_435_761u32.wrapping_mul(seed + 1));
            luma[r * THUMB_W + c] = (noise >> 24) as u8;
        }
    }
    luma
}

#[test]
fn a_region_that_keeps_moving_is_ignored_when_looking_for_states() {
    // The presenter moves every second; the terminal prints one line.
    let lumas: Vec<Vec<u8>> = (0..12u32)
        .map(|i| screen_with_facecam(if i >= 6 { 5 } else { 4 }, i))
        .collect();
    let found = states(&lumas);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0].0, 0);
    assert!(
        found[0].1 <= LIVE_MIN,
        "the camera is live once seen moving: {found:?}"
    );
    assert_eq!(found[1], (6, 6));
}

#[test]
fn a_presenter_who_pauses_and_moves_again_does_not_start_a_state() {
    let paused = 8..16u32;
    let lumas: Vec<Vec<u8>> = (0..24u32)
        .map(|i| screen_with_facecam(4, if paused.contains(&i) { 8 } else { i }))
        .collect();
    let found = states(&lumas);
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn motion_density_is_zero_for_a_still_screen_and_positive_for_a_scroll() {
    assert!(motion_density(&frames(&[screen(4, 0), screen(4, 0)])).abs() < f64::EPSILON);
    assert!(motion_density(&frames(&[screen(18, 0), screen(18, 6)])) > 0.01);
    assert!(motion_density(&frames(&[screen(4, 0)])).abs() < f64::EPSILON);
}

/// The region of state `index`, as `(x, y, width, height)`.
fn region_of(lumas: &[Vec<u8>], index: usize) -> Option<(usize, usize, usize, usize)> {
    segment_states(&frames(lumas), &RULE)[index]
        .region
        .map(|r| (r.x, r.y, r.width, r.height))
}

#[test]
fn a_new_line_s_region_is_where_the_line_is() {
    let lumas = [screen(4, 0), screen(4, 0), screen(5, 0), screen(5, 0)];
    assert_eq!(region_of(&lumas, 0), None, "nothing came before the first");
    // Line 4 occupies rows 20..23 and columns 0..92 (60 + 92 % 60).
    let (x, y, width, height) = region_of(&lumas, 1).unwrap();
    assert!(x == 0 && x + width >= 92, "{x}..{}", x + width);
    assert!(y <= 20 && y + height >= 23, "{y}..{}", y + height);
    assert!(
        height < THUMB_H / 2 && width < THUMB_W,
        "a line is not the screen"
    );
}

#[test]
fn a_cut_to_another_screen_has_no_region() {
    let light = vec![230u8; THUMB_W * THUMB_H];
    let lumas = [screen(6, 0), screen(6, 0), light.clone(), light];
    assert_eq!(region_of(&lumas, 1), None);
}

#[test]
fn a_state_after_a_scroll_is_measured_against_the_scrolled_screen() {
    let lumas = [
        screen(18, 0),
        screen(18, 0),
        screen(18, 6),
        screen(18, 12),
        screen(18, 12),
        screen(19, 12), // line 18 appears at rows 90-12=78..81
        screen(19, 12),
    ];
    let (_, y, _, height) = region_of(&lumas, 1).unwrap();
    assert!(y <= 78 && y + height >= 81, "{y}..{}", y + height);
    assert!(
        height < THUMB_H / 2,
        "only the new line changed: {height} rows"
    );
}
