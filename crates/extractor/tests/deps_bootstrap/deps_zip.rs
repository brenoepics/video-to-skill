use std::io::{Cursor, Write};

use super::*;
use vts_extract::deps::bootstrap::ArchiveMember;
use zip::write::SimpleFileOptions;

fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn archive_member(tool: Tool, path: &str) -> ArchiveMember {
    ArchiveMember {
        tool,
        path: path.into(),
    }
}

fn zip_with_duplicate_names() -> Vec<u8> {
    let mut bytes = zip_bytes(&[("first", b"one"), ("other", b"two")]);
    let mut replacements = 0;
    for index in 0..=bytes.len() - 5 {
        if &bytes[index..index + 5] == b"other" {
            bytes[index..index + 5].copy_from_slice(b"first");
            replacements += 1;
        }
    }
    assert_eq!(
        replacements, 2,
        "local and central ZIP names must be patched"
    );
    bytes
}

fn zip_with_oversized_metadata(name: &str) -> Vec<u8> {
    let mut bytes = zip_bytes(&[(name, b"tiny")]);
    let central = bytes
        .windows(4)
        .position(|window| window == b"PK\x01\x02")
        .unwrap();
    let oversized = (512_u32 * 1024 * 1024 + 1).to_le_bytes();
    bytes[central + 24..central + 28].copy_from_slice(&oversized);
    bytes
}

#[test]
fn zipped_artifact_is_extracted_to_the_bare_tool() {
    let data = scratch("zip");
    let member_path = format!("bundle/bin/{}", Tool::Ffmpeg.file_name());
    let archive = zip_bytes(&[(&member_path, PAYLOAD)]);
    let spec = ToolSpec {
        tool: Tool::Ffmpeg,
        url: "https://example.invalid/ffmpeg.zip".into(),
        sha256: vts_extract::hashing::sha256_hex(&archive),
        archive: Archive::Zip {
            members: vec![archive_member(Tool::Ffmpeg, &member_path)],
        },
    };

    let installed = install(&spec, &FakeFetcher::serving(&archive), &data).unwrap();

    assert_eq!(fs::read(&installed).unwrap(), PAYLOAD);
    assert!(verification_marker(&installed).is_file());
}

#[test]
fn ffmpeg_bundle_installs_both_tools_and_is_fetched_once() {
    let data = scratch("ffmpeg-bundle");
    let ffmpeg_member = format!("bundle/bin/{}", Tool::Ffmpeg.file_name());
    let ffprobe_member = format!("bundle/bin/{}", Tool::Ffprobe.file_name());
    let archive = zip_bytes(&[
        (&ffmpeg_member, b"ffmpeg bytes"),
        (&ffprobe_member, b"ffprobe bytes"),
    ]);
    let sha256 = vts_extract::hashing::sha256_hex(&archive);
    let members = vec![
        archive_member(Tool::Ffmpeg, &ffmpeg_member),
        archive_member(Tool::Ffprobe, &ffprobe_member),
    ];
    let ffmpeg_spec = bundle_spec(Tool::Ffmpeg, &sha256, &members);
    let ffprobe_spec = bundle_spec(Tool::Ffprobe, &sha256, &members);
    let fetcher = FakeFetcher::serving(&archive);

    let ffmpeg = install(&ffmpeg_spec, &fetcher, &data).unwrap();
    let ffprobe = install(&ffprobe_spec, &fetcher, &data).unwrap();

    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
    let artifact_dir = data.join("tools").join("artifacts").join(&sha256);
    assert_eq!(ffmpeg, artifact_dir.join(Tool::Ffmpeg.file_name()));
    assert_eq!(ffprobe, artifact_dir.join(Tool::Ffprobe.file_name()));
    for installed in [ffmpeg, ffprobe] {
        assert!(installed.is_file());
    }
    assert!(artifact_dir.join("artifact.verified").is_file());
}

#[test]
fn incomplete_content_addressed_bundle_is_quarantined_and_repaired() {
    let data = scratch("ffmpeg-bundle-repair");
    let ffmpeg_member = format!("bundle/bin/{}", Tool::Ffmpeg.file_name());
    let ffprobe_member = format!("bundle/bin/{}", Tool::Ffprobe.file_name());
    let archive = zip_bytes(&[
        (&ffmpeg_member, b"ffmpeg bytes"),
        (&ffprobe_member, b"ffprobe bytes"),
    ]);
    let sha256 = vts_extract::hashing::sha256_hex(&archive);
    let members = vec![
        archive_member(Tool::Ffmpeg, &ffmpeg_member),
        archive_member(Tool::Ffprobe, &ffprobe_member),
    ];
    let artifact_dir = data.join("tools").join("artifacts").join(&sha256);
    fs::create_dir_all(&artifact_dir).unwrap();
    fs::write(artifact_dir.join(Tool::Ffmpeg.file_name()), b"partial").unwrap();

    let fetcher = FakeFetcher::failing_once_then(&archive);
    let spec = bundle_spec(Tool::Ffmpeg, &sha256, &members);
    let error = install(&spec, &fetcher, &data).unwrap_err();
    let error = format!("{error:#}");
    assert!(error.contains("simulated network drop"), "{error}");
    assert!(
        !artifact_dir.exists(),
        "a failed repair must discard the corrupt quarantine"
    );

    let installed = install(&spec, &fetcher, &data).unwrap();

    assert_eq!(installed, artifact_dir.join(Tool::Ffmpeg.file_name()));
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        fs::read(artifact_dir.join(Tool::Ffmpeg.file_name())).unwrap(),
        b"ffmpeg bytes"
    );
    assert_eq!(
        fs::read(artifact_dir.join(Tool::Ffprobe.file_name())).unwrap(),
        b"ffprobe bytes"
    );
    assert!(artifact_dir.join("artifact.verified").exists());
    assert_eq!(
        fs::read_dir(artifact_dir.parent().unwrap())
            .unwrap()
            .count(),
        1,
        "successful repair must remove its quarantine"
    );
}

fn bundle_spec(tool: Tool, sha256: &str, members: &[ArchiveMember]) -> ToolSpec {
    ToolSpec {
        tool,
        url: "https://example.invalid/ffmpeg.zip".into(),
        sha256: sha256.into(),
        archive: Archive::Zip {
            members: members.to_vec(),
        },
    }
}

#[test]
fn unsafe_zip_path_rejects_the_whole_install() {
    let data = scratch("zip-traversal");
    let member_path = format!("bundle/bin/{}", Tool::Ffmpeg.file_name());
    let archive = zip_bytes(&[("../../escape", b"bad"), (&member_path, PAYLOAD)]);
    let spec = bundle_spec(
        Tool::Ffmpeg,
        &vts_extract::hashing::sha256_hex(&archive),
        &[archive_member(Tool::Ffmpeg, &member_path)],
    );

    let error = install(&spec, &FakeFetcher::serving(&archive), &data).unwrap_err();

    assert!(
        error.to_string().contains("unsafe ZIP path"),
        "got: {error:#}"
    );
    assert!(!Tool::Ffmpeg.install_path(&data).exists());
}

#[test]
fn zip_with_duplicate_file_names_is_rejected() {
    let data = scratch("zip-duplicate");
    let archive = zip_with_duplicate_names();
    let spec = bundle_spec(
        Tool::Ffmpeg,
        &vts_extract::hashing::sha256_hex(&archive),
        &[archive_member(Tool::Ffmpeg, "first")],
    );

    let error = install(&spec, &FakeFetcher::serving(&archive), &data).unwrap_err();

    assert!(
        error.to_string().contains("duplicate file names"),
        "got: {error:#}"
    );
}

#[test]
fn zip_expansion_over_the_limit_is_rejected_before_extraction() {
    let data = scratch("zip-size-limit");
    let member_path = format!("bundle/bin/{}", Tool::Ffmpeg.file_name());
    let archive = zip_with_oversized_metadata(&member_path);
    let spec = bundle_spec(
        Tool::Ffmpeg,
        &vts_extract::hashing::sha256_hex(&archive),
        &[archive_member(Tool::Ffmpeg, &member_path)],
    );

    let error = install(&spec, &FakeFetcher::serving(&archive), &data).unwrap_err();

    assert!(error.to_string().contains("size limit"), "got: {error:#}");
    assert!(!Tool::Ffmpeg.install_path(&data).exists());
}
