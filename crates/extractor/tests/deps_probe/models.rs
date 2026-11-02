//! Model files (whisper weights, the VAD model): detected by a verified
//! copy in the data dir whose marker names the currently pinned digest,
//! never by executability or PATH.

use super::*;

#[test]
fn whisper_model_is_detected_by_verified_file_not_executability() {
    let bin = scratch("model-bin");
    let data = scratch("model-data");
    let models = data.join("models");
    fs::create_dir_all(&models).unwrap();
    fs::write(models.join("ggml-base.bin"), b"weights").unwrap();
    write_current_marker(Tool::WhisperModel, &models.join("ggml-base.bin"));

    let statuses = probe(&env_with(vec![bin], data.clone()));

    let model = statuses
        .iter()
        .find(|s| s.tool == Tool::WhisperModel)
        .unwrap();
    assert_eq!(model.presence, Presence::Installed);
    assert_eq!(model.location, Some(models.join("ggml-base.bin")));
}

#[test]
fn vad_model_is_detected_beside_the_whisper_weights() {
    let bin = scratch("vad-bin");
    let data = scratch("vad-data");
    let models = data.join("models");
    fs::create_dir_all(&models).unwrap();
    fs::write(models.join("ggml-silero-v5.1.2.bin"), b"vad weights").unwrap();
    write_current_marker(Tool::WhisperVad, &models.join("ggml-silero-v5.1.2.bin"));

    let statuses = probe(&env_with(vec![bin], data.clone()));

    let vad = statuses
        .iter()
        .find(|s| s.tool == Tool::WhisperVad)
        .unwrap();
    assert_eq!(vad.presence, Presence::Installed);
    assert_eq!(vad.location, Some(models.join("ggml-silero-v5.1.2.bin")));
    assert!(
        !Tool::WhisperVad.is_executable(),
        "model files are data, never run"
    );
}

#[test]
fn stale_managed_marker_is_not_reported_ready() {
    let data = scratch("stale-managed-marker");
    let model = data.join("models").join("ggml-base.bin");
    fs::create_dir_all(model.parent().unwrap()).unwrap();
    fs::write(&model, b"old weights").unwrap();
    fs::write(model.with_file_name("ggml-base.bin.verified"), "sha256:old").unwrap();

    let statuses = probe(&env_with(vec![], data));

    let model = statuses
        .iter()
        .find(|status| status.tool == Tool::WhisperModel)
        .unwrap();
    assert_eq!(model.presence, Presence::Missing);
    assert_eq!(model.location, None);
}
