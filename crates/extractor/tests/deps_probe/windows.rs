use std::fs;

use vts_extract::deps::{probe, Presence, Tool};

use super::{env_with, fake_executable, scratch, FFMPEG_FILE_NAME};

#[test]
fn invalid_exe_on_path_is_not_reported_ready() {
    let bin = scratch("invalid-exe-bin");
    let data = scratch("invalid-exe-data");
    fs::write(bin.join(FFMPEG_FILE_NAME), "not a Windows executable").unwrap();

    let statuses = probe(&env_with(vec![bin], data));

    let ffmpeg = statuses.iter().find(|s| s.tool == Tool::Ffmpeg).unwrap();
    assert_eq!(ffmpeg.presence, Presence::Missing);
    assert_eq!(ffmpeg.location, None);
}

#[test]
fn path_lookup_requires_the_native_exe_name() {
    let bin = scratch("cmd-shim-bin");
    let data = scratch("cmd-shim-data");
    fake_executable(&bin, "ffmpeg.cmd");

    let statuses = probe(&env_with(vec![bin], data));

    let ffmpeg = statuses.iter().find(|s| s.tool == Tool::Ffmpeg).unwrap();
    assert_eq!(ffmpeg.presence, Presence::Missing);
}
