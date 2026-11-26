//! Follow the arrow pointer from sample to sample on a mid-resolution
//! gray plane. A pointer that moved leaves two small groups of changed
//! pixels, where it was and where it is; the one that now looks like an
//! arrow ([`Template`]) is the new fix, and the other, checked against
//! the previous frame, says where it was. A pointer that did not move
//! leaves no trace, so the last fix is kept while the arrow still
//! scores there, and dropped when it does not (hidden, or the screen is
//! another one). All classical: no model, a handful of rendered sizes.

use super::arrow::Template;
use super::blobs::{self, Blob};

/// Gray-level difference above which a pixel changed (codec noise and
/// anti-aliasing stay below it).
const DIFF_THRESHOLD: u8 = 40;
/// Body heights of the rendered arrows, about 15 % apart: from a 1x
/// pointer on a 1440p recording to a pointer enlarged 3x on 720p, at
/// [`super::MID_W`] wide.
const BANK_HEIGHTS: [usize; 11] = [6, 7, 8, 9, 10, 12, 14, 16, 18, 21, 24];
/// Score at which a changed group is the arrow.
const MATCH: f64 = 0.6;
/// Score at which a still arrow is still where it was.
const CARRY: f64 = 0.5;
/// More changed pixels than this share is a new screen: nothing small
/// in it is a pointer move.
const CUT_SHARE: f64 = 0.5;
/// Changed groups looked at per sample, in scan order.
const MAX_CANDIDATES: usize = 48;
/// A changed group this large is not a pointer.
const MAX_BLOB_W: usize = 32;
const MAX_BLOB_H: usize = 40;
/// Placements tried around a group's corner, or around the last fix.
const JITTER: usize = 2;

/// How a sample's fix was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The arrow landed here between this sample and the previous one.
    Arrival,
    /// The arrow left from here before the next sample.
    Departure,
    /// The arrow stayed where the previous sample had it.
    Carried,
    /// Assumed from a later departure: the arrow had not yet moved.
    Backfilled,
}

/// The arrow's place on the plane: its template's top-left corner and
/// which size of the bank matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fix {
    pub x: usize,
    pub y: usize,
    pub template: usize,
    pub kind: Kind,
}

impl Fix {
    /// The pointer's tip (the template's corner is its outline).
    #[must_use]
    pub fn tip(&self) -> (usize, usize) {
        (self.x + 1, self.y + 1)
    }
}

pub struct Tracker {
    width: usize,
    bank: Vec<Template>,
    prev: Option<Vec<u8>>,
    last: Option<Fix>,
    fixes: Vec<Option<Fix>>,
}

impl Tracker {
    /// A tracker for `width`-wide gray planes.
    #[must_use]
    pub fn new(width: usize) -> Tracker {
        Tracker {
            width,
            bank: BANK_HEIGHTS.iter().map(|&h| Template::render(h)).collect(),
            prev: None,
            last: None,
            fixes: Vec::new(),
        }
    }

    /// Take the next sample's plane; its fix (and possibly a departure
    /// fix for the previous sample) lands in [`Tracker::finish`].
    pub fn observe(&mut self, frame: Vec<u8>) {
        let index = self.fixes.len();
        let Some(prev) = self.prev.take() else {
            self.fixes.push(None);
            self.prev = Some(frame);
            return;
        };
        let candidates = self.candidates(&prev, &frame);
        let arrival = candidates
            .iter()
            .filter_map(|blob| self.best_match(&frame, blob, Kind::Arrival))
            .max_by(|a, b| a.0.total_cmp(&b.0));
        let carried = self.last.and_then(|last| self.rescore(&frame, last));
        let fix = match (arrival, carried) {
            (Some((arrived, a)), Some((stayed, c))) => Some(if arrived > stayed { a } else { c }),
            (Some((_, a)), None) => Some(a),
            (None, Some((_, c))) => Some(c),
            (None, None) => None,
        };
        if fix.is_some_and(|f| f.kind == Kind::Arrival) && self.fixes[index - 1].is_none() {
            self.fixes[index - 1] = candidates
                .iter()
                .filter_map(|blob| self.best_match(&prev, blob, Kind::Departure))
                .max_by(|a, b| a.0.total_cmp(&b.0))
                .map(|(_, departure)| departure);
        }
        self.fixes.push(fix);
        self.last = fix;
        self.prev = Some(frame);
    }

    /// One entry per observed sample.
    #[must_use]
    pub fn finish(self) -> Vec<Option<Fix>> {
        self.fixes
    }

    /// Pointer-sized groups of changed pixels, none when the screen
    /// itself changed.
    fn candidates(&self, prev: &[u8], frame: &[u8]) -> Vec<Blob> {
        let mask = blobs::changed_pixels(prev, frame, DIFF_THRESHOLD);
        let changed = mask.iter().filter(|&&c| c).count();
        #[allow(clippy::cast_precision_loss)] // pixel counts of one plane
        if changed as f64 / mask.len() as f64 >= CUT_SHARE {
            return Vec::new();
        }
        blobs::blobs(&mask, self.width)
            .into_iter()
            .filter(|b| {
                (2..=MAX_BLOB_W).contains(&b.width())
                    && (3..=MAX_BLOB_H).contains(&b.height())
                    && b.pixels >= 3
            })
            .take(MAX_CANDIDATES)
            .collect()
    }

    /// The best-scoring arrow of a fitting size placed around the
    /// group's top-left corner, if it scores as a match.
    fn best_match(&self, plane: &[u8], blob: &Blob, kind: Kind) -> Option<(f64, Fix)> {
        let fits =
            |t: &Template| 5 * t.height >= 3 * blob.height() && 5 * t.height <= 8 * blob.height();
        let corner = (blob.left.saturating_sub(1), blob.top.saturating_sub(1));
        self.bank
            .iter()
            .enumerate()
            .filter(|(_, t)| fits(t))
            .filter_map(|(template, t)| {
                let (score, x, y) = best_around(t, plane, self.width, corner, JITTER);
                (score >= MATCH).then_some((
                    score,
                    Fix {
                        x,
                        y,
                        template,
                        kind,
                    },
                ))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
    }

    /// The last fix re-scored on this plane, if the arrow is still there.
    fn rescore(&self, plane: &[u8], last: Fix) -> Option<(f64, Fix)> {
        let template = &self.bank[last.template];
        let (score, x, y) = best_around(template, plane, self.width, (last.x, last.y), 1);
        (score >= CARRY).then_some((
            score,
            Fix {
                x,
                y,
                template: last.template,
                kind: Kind::Carried,
            },
        ))
    }
}

/// The best score of `template` with its top-left within `jitter` of
/// `(x, y)`, and where (the nearest-to-origin placement on a tie).
fn best_around(
    template: &Template,
    plane: &[u8],
    width: usize,
    (x, y): (usize, usize),
    jitter: usize,
) -> (f64, usize, usize) {
    (y.saturating_sub(jitter)..=y + jitter)
        .flat_map(|py| (x.saturating_sub(jitter)..=x + jitter).map(move |px| (px, py)))
        .map(|(px, py)| (template.score(plane, width, px, py), px, py))
        .fold(
            (0.0, x, y),
            |best, cur| if cur.0 > best.0 { cur } else { best },
        )
}
