//! The changed region on drawn window maps: each expectation follows
//! from which windows were set, never from the implementation.

use super::super::ssim::WINDOWS;
use super::*;

/// Per-window flags with the luma windows set where `f(row, col)`.
fn windows(f: impl Fn(usize, usize) -> bool) -> Vec<bool> {
    let mut flags = vec![false; WINDOWS];
    for row in 0..LUMA_ROWS {
        for col in 0..LUMA_COLS {
            flags[row * LUMA_COLS + col] = f(row, col);
        }
    }
    flags
}

fn none_live() -> Vec<bool> {
    vec![false; WINDOWS]
}

fn contains(region: Region, x: usize, y: usize) -> bool {
    (region.x..region.x + region.width).contains(&x)
        && (region.y..region.y + region.height).contains(&y)
}

#[test]
fn nothing_changed_is_no_region() {
    assert_eq!(changed_region(&windows(|_, _| false), &none_live()), None);
}

#[test]
fn a_block_of_changed_windows_is_boxed_with_padding() {
    // Windows rows 10..=11, columns 5..=14: luma x 20..64, y 40..52.
    let flags = windows(|r, c| (10..=11).contains(&r) && (5..=14).contains(&c));
    let region = changed_region(&flags, &none_live()).unwrap();
    assert_eq!(
        region,
        Region {
            x: 16,
            y: 36,
            width: 52,
            height: 20
        }
    );
}

#[test]
fn a_lone_window_is_widened_to_the_minimum_size_around_itself() {
    let flags = windows(|r, c| r == 10 && c == 20);
    let region = changed_region(&flags, &none_live()).unwrap();
    // Padded, one window is 16 rows tall; it is widened to the minimum.
    assert_eq!((region.width, region.height), (MIN_WIDTH, WINDOW + 2 * PAD));
    assert!(contains(region, 80, 40) && contains(region, 87, 47));
    assert!(region.x + region.width / 2 >= 80 && region.y + region.height / 2 >= 40);
}

#[test]
fn the_largest_group_wins_over_a_pointer_elsewhere() {
    let flags =
        windows(|r, c| ((10..=11).contains(&r) && (5..=14).contains(&c)) || (r == 2 && c == 35));
    let region = changed_region(&flags, &none_live()).unwrap();
    assert!(
        contains(region, 20, 40) && !contains(region, 140, 8),
        "{region:?}"
    );
}

#[test]
fn diagonal_neighbours_belong_to_the_same_group() {
    let staircase = windows(|r, c| (4..10).contains(&r) && c == r + 10);
    let region = changed_region(&staircase, &none_live()).unwrap();
    assert!(
        contains(region, 56, 16) && contains(region, 83, 43),
        "{region:?}"
    );
}

#[test]
fn live_windows_never_count() {
    let flags = windows(|r, c| r >= 14 && c >= 28);
    let live = flags.clone();
    assert_eq!(changed_region(&flags, &live), None);
    let also_a_line = windows(|r, c| (r >= 14 && c >= 28) || (r == 3 && c < 20));
    let region = changed_region(&also_a_line, &live).unwrap();
    assert!(
        contains(region, 0, 12) && !contains(region, 150, 80),
        "{region:?}"
    );
}

#[test]
fn a_change_of_the_whole_screen_is_no_region() {
    assert_eq!(changed_region(&windows(|_, _| true), &none_live()), None);
    let most = windows(|r, c| r >= 2 && c >= 4);
    assert_eq!(changed_region(&most, &none_live()), None);
}

#[test]
fn a_cut_is_a_cut_even_where_a_live_band_splits_it() {
    // A scroll left the middle columns live; then the screen was
    // replaced. Neither half of the rest is "the" changed region.
    let everything = windows(|_, _| true);
    let band = windows(|_, c| (14..24).contains(&c));
    assert_eq!(changed_region(&everything, &band), None);
}

#[test]
fn a_region_stays_inside_the_thumbnail() {
    let corner = windows(|r, c| r == LUMA_ROWS - 1 && c == LUMA_COLS - 1);
    let region = changed_region(&corner, &none_live()).unwrap();
    assert!(region.x + region.width <= THUMB_W && region.y + region.height <= THUMB_H);
    assert!(contains(region, THUMB_W - 1, THUMB_H - 1), "{region:?}");
}

#[test]
fn native_rectangles_scale_up_and_land_on_even_edges() {
    let region = Region {
        x: 3,
        y: 61,
        width: 77,
        height: 23,
    };
    let rect = region.to_native(1920, 1080);
    assert_eq!((rect.x, rect.y), (36, 732));
    assert_eq!((rect.width, rect.height), (960 - 36, 1008 - 732));
    let odd = Region {
        x: 1,
        y: 1,
        width: 1,
        height: 1,
    }
    .to_native(1280, 720);
    assert_eq!((odd.x, odd.y, odd.width, odd.height), (8, 8, 8, 8));
    let edge = Region {
        x: 120,
        y: 78,
        width: 40,
        height: 12,
    }
    .to_native(1920, 1080);
    assert_eq!((edge.x + edge.width, edge.y + edge.height), (1920, 1080));
}
