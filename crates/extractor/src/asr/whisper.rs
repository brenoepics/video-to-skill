//! Production engine: whisper.cpp via the whisper-rs bindings
//! (Metal-accelerated on Apple Silicon bare metal).
//!
//! Inside a virtual machine the only GPU is a paravirtual device whose
//! Metal lacks simdgroup reductions and matrix multiply; ggml's fallback
//! kernels there run tens of times slower than the CPU path, so the
//! engine stays on the CPU whenever the host looks virtualized.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use super::segments::{segments_for, Token};
use super::vad::{self, TimeMap};
use super::{EngineOutput, TranscriptionEngine};
use crate::host;
use crate::tools::Toolchain;

pub struct WhisperEngine {
    model_path: PathBuf,
    use_gpu: bool,
    vad_model: Option<PathBuf>,
}

impl WhisperEngine {
    /// Engine over `model_path`, on the GPU unless the host is a virtual
    /// machine (see the module docs).
    #[must_use]
    pub fn new(model_path: PathBuf) -> WhisperEngine {
        WhisperEngine::with_gpu(model_path, !host::is_virtual_machine())
    }

    /// Engine with the compute device chosen explicitly.
    #[must_use]
    pub fn with_gpu(model_path: PathBuf, use_gpu: bool) -> WhisperEngine {
        // Route whisper.cpp/ggml chatter through the `log` crate; with no
        // logger installed it is silenced instead of spamming stderr.
        whisper_rs::install_logging_hooks();
        WhisperEngine {
            model_path,
            use_gpu,
            vad_model: None,
        }
    }

    /// Skip non-speech audio using these Silero VAD weights. `None` keeps
    /// the plain behaviour: every second of audio is decoded.
    #[must_use]
    pub fn with_vad_model(mut self, vad_model: Option<PathBuf>) -> WhisperEngine {
        self.vad_model = vad_model;
        self
    }

    /// The engine a resolved toolchain affords: `None` without whisper
    /// weights; VAD on whenever its model is installed alongside them.
    #[must_use]
    pub fn from_toolchain(tools: &Toolchain) -> Option<WhisperEngine> {
        let model = tools.whisper_model.clone()?;
        Some(WhisperEngine::new(model).with_vad_model(tools.whisper_vad.clone()))
    }

    /// Where inference runs: "gpu" or "cpu".
    #[must_use]
    pub fn device(&self) -> &'static str {
        if self.use_gpu {
            "gpu"
        } else {
            "cpu"
        }
    }

    #[must_use]
    pub fn vad_model(&self) -> Option<&Path> {
        self.vad_model.as_deref()
    }
}

impl TranscriptionEngine for WhisperEngine {
    fn transcribe(&self, wav: &Path, progress: &mut dyn FnMut(u8)) -> Result<EngineOutput> {
        let samples = read_wav_mono_f32(wav)?;
        progress(0);
        let (samples, times) = match &self.vad_model {
            Some(vad_model) => {
                let regions = vad::detect_speech(vad_model, &samples)?;
                let (speech, times) = vad::keep_speech(&samples, &regions);
                if speech.is_empty() {
                    // Nothing to decode — and no language to claim.
                    progress(100);
                    return Ok(EngineOutput {
                        language: None,
                        segments: Vec::new(),
                    });
                }
                (speech, times)
            }
            None => (samples, TimeMap::identity()),
        };

        let mut ctx_params = WhisperContextParameters::default();
        ctx_params.use_gpu(self.use_gpu);
        let ctx = WhisperContext::new_with_params(&self.model_path, ctx_params)
            .context("loading whisper model")?;
        let mut state = ctx.create_state().context("creating whisper state")?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("auto"));
        params.set_token_timestamps(true);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        // whisper-rs progress callbacks require 'static captures; report
        // coarse start/end until streaming progress is worth the plumbing.
        state.full(params, &samples).context("running whisper")?;
        progress(100);

        let eot = ctx.token_eot();
        let mut segments = Vec::new();
        for whisper_segment in state.as_iter() {
            let text = whisper_segment.to_str_lossy()?;
            if text.trim().is_empty() {
                continue;
            }
            let mut tokens = Vec::new();
            for t in 0..whisper_segment.n_tokens() {
                let Some(token) = whisper_segment.get_token(t) else {
                    continue;
                };
                let data = token.token_data();
                tokens.push(Token {
                    text: token.to_str_lossy()?.to_string(),
                    t0: data.t0,
                    t1: data.t1,
                    special: token.token_id() >= eot,
                });
            }
            segments.extend(segments_for(
                &text,
                whisper_segment.start_timestamp(),
                whisper_segment.end_timestamp(),
                &tokens,
                &times,
            ));
        }
        let language = whisper_rs::get_lang_str(state.full_lang_id_from_state())
            .map(std::string::ToString::to_string);
        Ok(EngineOutput { language, segments })
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tools(model: Option<&str>, vad: Option<&str>) -> Toolchain {
        Toolchain {
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
            whisper_model: model.map(PathBuf::from),
            whisper_vad: vad.map(PathBuf::from),
        }
    }

    #[test]
    fn no_whisper_weights_means_no_engine_even_with_vad() {
        assert!(WhisperEngine::from_toolchain(&tools(None, Some("vad.bin"))).is_none());
    }

    #[test]
    fn the_toolchain_decides_whether_vad_is_on() {
        let plain = WhisperEngine::from_toolchain(&tools(Some("base.bin"), None)).unwrap();
        assert_eq!(plain.vad_model(), None);

        let vad = WhisperEngine::from_toolchain(&tools(Some("base.bin"), Some("vad.bin"))).unwrap();
        assert_eq!(vad.vad_model(), Some(Path::new("vad.bin")));
    }
}
