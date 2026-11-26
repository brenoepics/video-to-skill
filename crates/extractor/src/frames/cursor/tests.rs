//! The tracker on drawn planes: a staircase arrow (not the template's
//! polygon) that jumps, sits still, vanishes, and shares the screen
//! with text that changes. Expectations follow from what was drawn.

use super::tracker::{Fix, Kind, Tracker};
use super::*;

const BG: u8 = 235;
const INK: u8 = 20;

/// A `MID_W`x`MID_H` plane of `bg`.
fn blank(bg: u8) -> Vec<u8> {
    vec![bg; MID_W * MID_H]
}

/// Draw a staircase arrow `h` rows tall with its tip at `(x, y)`: a
/// right triangle widening one pixel per two rows, then a two-pixel
/// tail, in `ink` with a one-pixel `edge` around it.
fn draw_arrow(plane: &mut [u8], x: usize, y: usize, h: usize, ink: u8, edge: u8) {
    let mut body = Vec::new();
    for r in 0..h {
        let cols = if r < h * 7 / 10 {
            0..=r / 2
        } else {
            h / 5..=h / 5 + 1
        };
        body.extend(cols.map(|c| (x + c, y + r)));
    }
    for &(px, py) in &body {
        for ny in py.saturating_sub(1)..=py + 1 {
            for nx in px.saturating_sub(1)..=px + 1 {
                plane[ny * MID_W + nx] = edge;
            }
        }
    }
    for &(px, py) in &body {
        plane[py * MID_W + px] = ink;
    }
}

fn with_arrow(bg: u8, x: usize, y: usize) -> Vec<u8> {
    let mut plane = blank(bg);
    draw_arrow(&mut plane, x, y, 12, INK, 255);
    plane
}

fn track(frames: Vec<Vec<u8>>) -> Vec<Option<Fix>> {
    let mut tracker = Tracker::new(MID_W);
    for frame in frames {
        tracker.observe(frame);
    }
    tracker.finish()
}

fn near(fix: Option<Fix>, x: usize, y: usize) -> bool {
    fix.is_some_and(|f| {
        let (tx, ty) = f.tip();
        tx.abs_diff(x) <= 2 && ty.abs_diff(y) <= 2
    })
}

#[test]
fn a_jump_is_found_where_it_lands_and_where_it_left() {
    let fixes = track(vec![with_arrow(BG, 100, 100), with_arrow(BG, 300, 200)]);
    assert!(near(fixes[1], 300, 200), "{fixes:?}");
    assert_eq!(fixes[1].unwrap().kind, Kind::Arrival);
    assert!(near(fixes[0], 100, 100), "{fixes:?}");
    assert_eq!(fixes[0].unwrap().kind, Kind::Departure);
}

#[test]
fn a_still_cursor_is_carried_and_lost_when_it_goes() {
    let still = with_arrow(BG, 300, 200);
    let fixes = track(vec![
        with_arrow(BG, 100, 100),
        still.clone(),
        still.clone(),
        still,
        blank(BG),
        blank(BG),
    ]);
    let kinds: Vec<Option<Kind>> = fixes.iter().map(|f| f.map(|f| f.kind)).collect();
    assert_eq!(
        kinds,
        vec![
            Some(Kind::Departure),
            Some(Kind::Arrival),
            Some(Kind::Carried),
            Some(Kind::Carried),
            None,
            None
        ]
    );
    assert!(fixes[1..4].iter().all(|&f| near(f, 300, 200)), "{fixes:?}");
}

#[test]
fn a_cursor_that_never_moves_is_never_found() {
    let still = with_arrow(BG, 300, 200);
    assert!(track(vec![still.clone(), still.clone(), still])
        .iter()
        .all(Option::is_none));
}

#[test]
fn a_new_line_of_text_is_not_a_cursor() {
    let mut typed = with_arrow(BG, 300, 200);
    for y in 100..106 {
        typed[y * MID_W + 50..y * MID_W + 250].fill(INK);
    }
    let fixes = track(vec![
        with_arrow(BG, 100, 100),
        with_arrow(BG, 300, 200),
        typed,
    ]);
    assert!(near(fixes[2], 300, 200), "{fixes:?}");
    assert_eq!(fixes[2].unwrap().kind, Kind::Carried);

    let mut bar = blank(BG);
    for y in 100..106 {
        bar[y * MID_W + 50..y * MID_W + 250].fill(INK);
    }
    assert!(track(vec![blank(BG), bar]).iter().all(Option::is_none));
}

#[test]
fn a_light_cursor_with_a_dark_outline_is_found_too() {
    let mut a = blank(30);
    draw_arrow(&mut a, 100, 100, 12, 250, 0);
    let mut b = blank(30);
    draw_arrow(&mut b, 300, 200, 12, 250, 0);
    let fixes = track(vec![a, b]);
    assert!(near(fixes[1], 300, 200), "{fixes:?}");
}

#[test]
fn a_cut_to_another_screen_has_no_cursor() {
    let fixes = track(vec![with_arrow(BG, 300, 200), blank(40)]);
    assert_eq!(fixes, vec![None, None]);
}

#[test]
fn backfill_covers_a_still_stretch_before_the_first_move_in_a_state() {
    let fix = |kind| {
        Some(Fix {
            x: 10,
            y: 10,
            template: 0,
            kind,
        })
    };
    let mut fixes = vec![None, None, fix(Kind::Departure), fix(Kind::Arrival), None];
    backfill(&mut fixes, &[0]);
    let kinds: Vec<Option<Kind>> = fixes.iter().map(|f| f.map(|f| f.kind)).collect();
    assert_eq!(
        kinds,
        vec![
            Some(Kind::Backfilled),
            Some(Kind::Backfilled),
            Some(Kind::Departure),
            Some(Kind::Arrival),
            None
        ]
    );
    // Not across a state boundary, and not before an arrival from off
    // screen (no departure was seen).
    let mut fixes = vec![
        None,
        None,
        fix(Kind::Departure),
        None,
        None,
        fix(Kind::Arrival),
    ];
    backfill(&mut fixes, &[0, 1, 4]);
    let kinds: Vec<Option<Kind>> = fixes.iter().map(|f| f.map(|f| f.kind)).collect();
    assert_eq!(
        kinds,
        vec![
            None,
            Some(Kind::Backfilled),
            Some(Kind::Departure),
            None,
            None,
            Some(Kind::Arrival)
        ]
    );
}

#[test]
fn presence_is_the_share_of_samples_with_a_fix() {
    let fix = Some(Fix {
        x: 0,
        y: 0,
        template: 0,
        kind: Kind::Carried,
    });
    let track = CursorTrack {
        fixes: vec![fix, fix, None, fix, None],
        native: (1280, 720),
    };
    assert!((track.presence(0..4) - 0.75).abs() < f64::EPSILON);
    assert!((track.presence(2..5) - 1.0 / 3.0).abs() < f64::EPSILON);
    assert!(
        track.presence(3..9) < 0.2,
        "samples past the end count as absent"
    );
    assert!(track.presence(5..5).abs() < f64::EPSILON);
    assert_eq!(track.native_tip(0), Some((2, 2)));
    assert_eq!(track.native_tip(2), None);
}
