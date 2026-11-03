//! Building the track: which image each evidence keyframe is read from,
//! reading them a few at a time, and recording the outcome.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use anyhow::{Context, Result};

use super::{save_screen_text, OcrEngine, Recognized, ScreenTextLine, ScreenTextTrack};
use crate::bundle::{load_manifest, save_manifest};
use crate::frames::export::worker_count;
use crate::frames::{load_frame_track, FrameTrack};

/// One keyframe to read: the image handed to the engine and the
/// agent-facing path the lines are reported under.
#[derive(Debug, Clone, PartialEq)]
struct Read {
    timestamp_secs: f64,
    frame: String,
    image: String,
}

/// Read every evidence keyframe in `frames.json` and write the bundle's
/// `screen_text.json`.
pub fn extract_text_track(bundle_dir: &Path, engine: &dyn OcrEngine) -> Result<ScreenTextTrack> {
    let manifest = load_manifest(bundle_dir)?;
    let frames = load_frame_track(bundle_dir)?;
    let media = (manifest.media.width, manifest.media.height);
    let reads = plan_reads(&frames, media, engine.max_dimension());
    let recognised = read_all(bundle_dir, engine, &reads)?;
    let lines = reads
        .iter()
        .zip(recognised)
        .flat_map(|(read, lines)| {
            lines.into_iter().map(move |line| ScreenTextLine {
                timestamp_secs: read.timestamp_secs,
                frame: read.frame.clone(),
                bbox: line.bbox,
                text: line.text,
                confidence: line.confidence,
                engine: engine.name().to_string(),
            })
        })
        .collect();
    let track = ScreenTextTrack {
        schema_version: 1,
        frames_read: reads.len(),
        lines,
    };
    save_screen_text(bundle_dir, &track)?;
    Ok(track)
}

/// Record in the manifest why the bundle has no screen text track;
/// returns the note. Idempotent.
pub fn record_unavailable(bundle_dir: &Path, reason: &str) -> Result<String> {
    let mut manifest = load_manifest(bundle_dir)?;
    let note = format!("screen_text: {reason}");
    if !manifest.notes.contains(&note) {
        manifest.notes.push(note.clone());
        save_manifest(bundle_dir, &manifest)?;
    }
    Ok(note)
}

/// The native still carries the most legible text, so it is read when
/// the engine can take the media's full size; interior keyframes only
/// exist width-bounded.
fn plan_reads(frames: &FrameTrack, (width, height): (u32, u32), limit: Option<u32>) -> Vec<Read> {
    let native_fits = limit.is_none_or(|limit| width.max(height) <= limit);
    let mut reads = Vec::new();
    for shot in &frames.shots {
        let keyframe = &shot.keyframe;
        let image = match &keyframe.native_path {
            Some(native) if native_fits => native.clone(),
            _ => keyframe.path.clone(),
        };
        reads.push(Read {
            timestamp_secs: keyframe.timestamp_secs,
            frame: keyframe.path.clone(),
            image,
        });
        reads.extend(shot.interior_keyframes.iter().map(|kf| Read {
            timestamp_secs: kf.timestamp_secs,
            frame: kf.path.clone(),
            image: kf.path.clone(),
        }));
    }
    reads
}

/// Read every planned image, several at a time (one engine call is
/// mostly model time, and the platform engines scale across threads).
/// Results come back in plan order; the error reported is the earliest
/// failing read in that order, independent of scheduling.
fn read_all(
    bundle_dir: &Path,
    engine: &dyn OcrEngine,
    reads: &[Read],
) -> Result<Vec<Vec<Recognized>>> {
    let parallelism = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let workers = worker_count(parallelism, reads.len());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let outcomes: Mutex<Vec<Option<Result<Vec<Recognized>>>>> =
        Mutex::new((0..reads.len()).map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(read) = reads.get(index) else {
                    break;
                };
                let image: PathBuf = bundle_dir.join(&read.image);
                let outcome = engine
                    .read(&image)
                    .with_context(|| format!("reading on-screen text in {}", read.image));
                outcomes.lock().unwrap_or_else(PoisonError::into_inner)[index] = Some(outcome);
            });
        }
    });
    outcomes
        .into_inner()
        .unwrap_or_else(PoisonError::into_inner)
        .into_iter()
        .map(|outcome| outcome.context("a keyframe was never read")?)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::plan_reads;
    use crate::frames::{FrameTrack, Keyframe, Shot};

    fn keyframe(timestamp_secs: f64, name: &str, native: bool) -> Keyframe {
        Keyframe {
            timestamp_secs,
            path: format!("frames/{name}.jpg"),
            native_path: native.then(|| format!("frames/{name}-native.jpg")),
            hash: "0".repeat(16),
        }
    }

    fn track() -> FrameTrack {
        FrameTrack {
            schema_version: 1,
            shots: vec![Shot {
                index: 0,
                start_secs: 0.0,
                end_secs: 30.0,
                motion_density: 0.0,
                keyframe: keyframe(15.0, "shot000", true),
                interior_keyframes: vec![keyframe(20.0, "shot000-i01", false)],
                changed_region: None,
                cursor_presence: None,
                cursor: None,
            }],
        }
    }

    #[test]
    fn native_stills_are_read_when_the_engine_takes_the_media_size() {
        let reads = plan_reads(&track(), (1920, 1080), Some(2600));

        let images: Vec<&str> = reads.iter().map(|r| r.image.as_str()).collect();
        assert_eq!(
            images,
            ["frames/shot000-native.jpg", "frames/shot000-i01.jpg"]
        );
        assert_eq!(reads[0].frame, "frames/shot000.jpg");
        assert!((reads[1].timestamp_secs - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn bounded_keyframes_are_read_when_the_native_still_exceeds_the_limit() {
        let reads = plan_reads(&track(), (3840, 2160), Some(2600));

        assert_eq!(reads[0].image, "frames/shot000.jpg");
    }
}
