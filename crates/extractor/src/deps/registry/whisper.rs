//! The whisper models: which one a machine gets, and where it comes from.
//!
//! The choice follows the compute device (`host::whisper_device`):
//! `large-v3-turbo-q5_0` where Metal keeps it well above realtime,
//! `small-q8_0` where whisper runs on the CPU. Both are multilingual
//! quantized builds from ggerganov/whisper.cpp; `base` was retired because
//! it misheard several times more words on tutorial narration.

use crate::deps::bootstrap::{Archive, ToolSpec};
use crate::deps::Tool;
use crate::host::{self, WhisperDevice};

const WHISPER_CPP_REVISION: &str = "5359861c739e955e79d9a303bcbc70fb988958b1";

/// A pinned whisper model file from ggerganov/whisper.cpp on Hugging Face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhisperModel {
    /// Upstream model name, e.g. `small-q8_0`.
    pub name: &'static str,
    /// `ggml-<name>.bin`, the file name inside the models dir.
    pub file_name: &'static str,
    /// From the file's git-lfs pointer, re-verified against a download.
    pub sha256: &'static str,
}

/// 574 MB. Tracks large-v3 on the Open ASR leaderboard; on Metal it still
/// transcribes several times faster than realtime.
const WHISPER_LARGE_V3_TURBO_Q5_0: WhisperModel = WhisperModel {
    name: "large-v3-turbo-q5_0",
    file_name: "ggml-large-v3-turbo-q5_0.bin",
    sha256: "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
};

/// 264 MB. The big accuracy step up from `base` (Whisper paper, Table 16)
/// at a CPU cost a laptop still absorbs.
const WHISPER_SMALL_Q8_0: WhisperModel = WhisperModel {
    name: "small-q8_0",
    file_name: "ggml-small-q8_0.bin",
    sha256: "49c8fb02b65e6049d5fa6c04f81f53b867b5ec9540406812c643f177317f779f",
};

/// The default whisper model for a compute device.
#[must_use]
pub fn whisper_model_for(device: WhisperDevice) -> WhisperModel {
    match device {
        WhisperDevice::Metal => WHISPER_LARGE_V3_TURBO_Q5_0,
        WhisperDevice::Cpu => WHISPER_SMALL_Q8_0,
    }
}

/// The whisper model this machine installs and loads.
#[must_use]
pub fn default_whisper_model() -> WhisperModel {
    whisper_model_for(host::whisper_device())
}

/// The pinned download for a whisper model. Platform-independent: the
/// ggerganov/whisper.cpp repo on Hugging Face is public and ungated.
#[must_use]
pub fn whisper_model_spec(model: WhisperModel) -> ToolSpec {
    ToolSpec {
        tool: Tool::WhisperModel,
        url: format!(
            "https://huggingface.co/ggerganov/whisper.cpp/resolve/{WHISPER_CPP_REVISION}/{}",
            model.file_name
        ),
        sha256: model.sha256.into(),
        archive: Archive::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_whisper_models_are_pinned_from_the_whisper_cpp_repo() {
        // Digests from the files' git-lfs pointers, re-verified by
        // downloading each once and hashing it.
        let pins = [
            (
                whisper_model_for(WhisperDevice::Metal),
                "large-v3-turbo-q5_0",
                "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
            ),
            (
                whisper_model_for(WhisperDevice::Cpu),
                "small-q8_0",
                "49c8fb02b65e6049d5fa6c04f81f53b867b5ec9540406812c643f177317f779f",
            ),
        ];
        for (model, name, sha256) in pins {
            assert_eq!(model.name, name);
            assert_eq!(model.sha256, sha256);
            let spec = whisper_model_spec(model);
            assert_eq!(
                spec.url,
                format!(
                    "https://huggingface.co/ggerganov/whisper.cpp/resolve/\
                     5359861c739e955e79d9a303bcbc70fb988958b1/ggml-{name}.bin"
                )
            );
            assert_eq!(spec.sha256, sha256);
            assert_eq!(spec.archive, Archive::None);
            assert_eq!(spec.tool, Tool::WhisperModel);
        }
    }
}
