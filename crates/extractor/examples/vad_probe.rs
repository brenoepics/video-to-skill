//! Scratch probe: VAD wall time versus thread count. Not committed.
#![allow(clippy::unwrap_used, clippy::cast_precision_loss)]
use std::time::Instant;
use whisper_rs::{WhisperVadContext, WhisperVadContextParams, WhisperVadParams};

fn main() {
    whisper_rs::install_logging_hooks();
    let args: Vec<String> = std::env::args().collect();
    let (model, wav) = (&args[1], &args[2]);
    let mut reader = hound::WavReader::open(wav).unwrap();
    let samples: Vec<f32> = reader
        .samples::<i16>()
        .map(|s| f32::from(s.unwrap()) / 32_768.0)
        .collect();
    let secs = samples.len() as f32 / 16_000.0;
    for n in args[3..].iter().map(|s| s.parse::<i32>().unwrap()) {
        let mut params = WhisperVadContextParams::default();
        params.set_n_threads(n);
        let mut ctx = WhisperVadContext::new(model, params).unwrap();
        let started = Instant::now();
        let segments = ctx
            .segments_from_samples(WhisperVadParams::default(), &samples)
            .unwrap();
        println!(
            "{wav}: threads={n} audio={secs:.1}s vad={:.3}s segments={}",
            started.elapsed().as_secs_f64(),
            segments.num_segments()
        );
    }
}
