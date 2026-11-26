//! Which of whisper.cpp's alignment-head presets fits a model file, so
//! DTW token timestamps follow a future model bump without a code change.

use std::path::Path;

use whisper_rs::DtwModelPreset;

/// The preset for a ggml model file (`ggml-<model>[-q5_1].bin`), or
/// `None` for weights whose alignment heads whisper.cpp does not know —
/// DTW then stays off and word times fall back to whisper's heuristic.
#[must_use]
pub fn preset_for(model: &Path) -> Option<DtwModelPreset> {
    let file_name = model.file_name()?.to_str()?;
    let name = file_name.strip_prefix("ggml-")?.strip_suffix(".bin")?;
    let preset = match without_quantization(name) {
        "tiny.en" => DtwModelPreset::TinyEn,
        "tiny" => DtwModelPreset::Tiny,
        "base.en" => DtwModelPreset::BaseEn,
        "base" => DtwModelPreset::Base,
        "small.en" => DtwModelPreset::SmallEn,
        "small" => DtwModelPreset::Small,
        "medium.en" => DtwModelPreset::MediumEn,
        "medium" => DtwModelPreset::Medium,
        "large-v1" => DtwModelPreset::LargeV1,
        "large-v2" => DtwModelPreset::LargeV2,
        "large-v3" => DtwModelPreset::LargeV3,
        "large-v3-turbo" => DtwModelPreset::LargeV3Turbo,
        _ => return None,
    };
    Some(preset)
}

/// `large-v3-turbo-q5_0` → `large-v3-turbo`: quantization changes the
/// weights' precision, not which heads align text to audio.
fn without_quantization(name: &str) -> &str {
    match name.rsplit_once('-') {
        Some((base, suffix)) if is_quantization(suffix) => base,
        _ => name,
    }
}

fn is_quantization(suffix: &str) -> bool {
    matches!(
        suffix.as_bytes(),
        [b'q', bits, b'_', variant] if bits.is_ascii_digit() && variant.is_ascii_digit()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(file_name: &str) -> Option<DtwModelPreset> {
        preset_for(&Path::new("/models").join(file_name))
    }

    #[test]
    fn the_preset_follows_the_ggml_model_file_name() {
        assert!(matches!(
            preset("ggml-base.bin"),
            Some(DtwModelPreset::Base)
        ));
        assert!(matches!(
            preset("ggml-base.en.bin"),
            Some(DtwModelPreset::BaseEn)
        ));
        assert!(matches!(
            preset("ggml-large-v3-turbo.bin"),
            Some(DtwModelPreset::LargeV3Turbo)
        ));
    }

    #[test]
    fn quantized_weights_share_their_model_s_alignment_heads() {
        assert!(matches!(
            preset("ggml-small-q5_1.bin"),
            Some(DtwModelPreset::Small)
        ));
        assert!(matches!(
            preset("ggml-large-v3-turbo-q8_0.bin"),
            Some(DtwModelPreset::LargeV3Turbo)
        ));
    }

    #[test]
    fn unknown_weights_leave_dtw_off() {
        assert!(
            preset("ggml-large.bin").is_none(),
            "a bare `large` is ambiguous across model versions"
        );
        assert!(preset("custom.bin").is_none());
        assert!(preset("ggml-base").is_none());
    }
}
