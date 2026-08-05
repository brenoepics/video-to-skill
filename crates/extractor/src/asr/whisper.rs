//! Production engine: whisper.cpp via the whisper-rs bindings
//! (Metal-accelerated on Apple Silicon).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use super::{EngineOutput, TranscriptionEngine};
use crate::bundle::{Segment, Word};

pub struct WhisperEngine {
    model_path: PathBuf,
}

impl WhisperEngine {
    #[must_use]
    pub fn new(model_path: PathBuf) -> WhisperEngine {
        // Route whisper.cpp/ggml chatter through the `log` crate; with no
        // logger installed it is silenced instead of spamming stderr.
        whisper_rs::install_logging_hooks();
        WhisperEngine { model_path }
    }
}

impl TranscriptionEngine for WhisperEngine {
    fn transcribe(&self, wav: &Path, progress: &mut dyn FnMut(u8)) -> Result<EngineOutput> {
        let ctx = WhisperContext::new_with_params(
            &self.model_path.to_string_lossy(),
            WhisperContextParameters::default(),
        )
        .context("loading whisper model")?;
        let mut state = ctx.create_state().context("creating whisper state")?;

        let samples = read_wav_mono_f32(wav)?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("auto"));
        params.set_token_timestamps(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        // whisper-rs progress callbacks require 'static captures; report
        // coarse start/end until streaming progress is worth the plumbing.
        progress(0);
        state.full(params, &samples).context("running whisper")?;
        progress(100);

        let mut segments = Vec::new();
        for whisper_segment in state.as_iter() {
            let text = whisper_segment.to_str_lossy()?.trim().to_string();
            if text.is_empty() {
                continue;
            }
            let mut words = Vec::new();
            for t in 0..whisper_segment.n_tokens() {
                let Some(token) = whisper_segment.get_token(t) else {
                    continue;
                };
                let token_text = token.to_str_lossy()?.to_string();
                // Skip special tokens like [_BEG_] / <|en|>.
                if token_text.starts_with('[') || token_text.starts_with('<') {
                    continue;
                }
                let data = token.token_data();
                words.push(Word {
                    start_secs: centis(data.t0),
                    end_secs: centis(data.t1),
                    text: token_text,
                });
            }
            segments.push(Segment {
                start_secs: centis(whisper_segment.start_timestamp()),
                end_secs: centis(whisper_segment.end_timestamp()),
                text,
                words,
            });
        }
        let language = whisper_rs::get_lang_str(state.full_lang_id_from_state())
            .map(std::string::ToString::to_string);
        Ok(EngineOutput { language, segments })
    }
}

/// whisper timestamps are in centiseconds.
// Timestamps are far below f64's 2^52 exact-integer range.
#[allow(clippy::cast_precision_loss)]
fn centis(t: i64) -> f64 {
    t as f64 / 100.0
}

fn read_wav_mono_f32(wav: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(wav).context("opening demuxed wav")?;
    let spec = reader.spec();
    anyhow::ensure!(
        spec.channels == 1 && spec.sample_rate == 16_000,
        "demux must produce mono 16kHz wav (got {}ch {}Hz)",
        spec.channels,
        spec.sample_rate
    );
    let samples: Result<Vec<i16>, _> = reader.samples::<i16>().collect();
    // i16 fits f32's mantissa exactly; this is the standard PCM conversion.
    #[allow(clippy::cast_possible_truncation)]
    Ok(samples
        .context("reading wav samples")?
        .into_iter()
        .map(|s| f32::from(s) / 32_768.0)
        .collect())
}
