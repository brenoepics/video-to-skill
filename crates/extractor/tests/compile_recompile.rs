//! Recompilation removes only obsolete files owned by the previous package.

#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::Path;

use vts_extract::compile::compile;

#[path = "compile_recompile/fixture.rs"]
mod fixture;
use fixture::{bundle, files, procedure, scratch};

#[test]
fn recompilation_removes_obsolete_generated_files_and_preserves_unrelated_files() {
    let dir = scratch("remove-obsolete");
    let bundle = bundle(&dir);
    let reused = dir.join("reused/read-notes");
    let fresh = dir.join("fresh/read-notes");
    let mut ir = procedure();
    compile(&ir, &bundle, &reused).unwrap();
    let user_files = [
        "notes.txt",
        "steps/manual.md",
        "scripts/manual.sh",
        "references/frames/manual.jpg",
    ];
    for name in user_files {
        fs::write(reused.join(name), b"user-owned").unwrap();
    }

    ir.steps.pop();
    compile(&ir, &bundle, &reused).unwrap();
    compile(&ir, &bundle, &fresh).unwrap();

    for obsolete in [
        "steps/02-remove.md",
        "scripts/old.sh",
        "references/frames/old.jpg",
    ] {
        assert!(
            !reused.join(obsolete).exists(),
            "obsolete file survived: {obsolete}"
        );
    }
    let mut actual = files(&reused);
    for name in user_files {
        assert_eq!(actual.remove(Path::new(name)).unwrap(), b"user-owned");
    }
    assert_eq!(
        actual,
        files(&fresh),
        "recompilation must match a fresh compile"
    );
}

#[test]
fn invalid_ir_leaves_the_previous_package_unchanged() {
    let dir = scratch("invalid-ir");
    let bundle = bundle(&dir);
    let out = dir.join("read-notes");
    let mut ir = procedure();
    compile(&ir, &bundle, &out).unwrap();
    let before = files(&out);

    ir.steps.truncate(1);
    ir.steps[0].evidence.clear();

    assert!(compile(&ir, &bundle, &out).is_err());
    assert_eq!(files(&out), before);
}

#[test]
fn removing_a_step_preserves_scripts_and_frames_shared_with_a_retained_step() {
    let dir = scratch("shared-assets");
    let bundle = bundle(&dir);
    let out = dir.join("read-notes");
    let mut ir = procedure();
    ir.steps[1].scripts = ir.steps[0].scripts.clone();
    ir.steps[1].evidence = ir.steps[0].evidence.clone();
    compile(&ir, &bundle, &out).unwrap();

    ir.steps.truncate(1);
    compile(&ir, &bundle, &out).unwrap();

    assert_eq!(
        fs::read(out.join("references/frames/keep.jpg")).unwrap(),
        b"keep.jpg"
    );
    assert_eq!(
        fs::read_to_string(out.join("scripts/keep.sh")).unwrap(),
        ir.steps[0].scripts[0].contents
    );
}

#[test]
fn a_frame_still_used_by_an_artifact_survives_removal_of_its_step() {
    let dir = scratch("artifact-frame");
    let bundle = bundle(&dir);
    let out = dir.join("read-notes");
    let mut ir = procedure();
    compile(&ir, &bundle, &out).unwrap();

    ir.steps.truncate(1);
    ir.steps[0].scripts.clear();
    ir.steps[0].evidence[0].frame = None;
    ir.artifacts.push(vts_extract::compile::OnScreenArtifact {
        text: "A local note".into(),
        timestamp_secs: 1.0,
        frame: Some("old.jpg".into()),
    });
    compile(&ir, &bundle, &out).unwrap();

    assert_eq!(
        fs::read(out.join("references/frames/old.jpg")).unwrap(),
        b"old.jpg"
    );
    assert!(!out.join("references/frames/keep.jpg").exists());
    assert!(fs::read_dir(out.join("scripts")).unwrap().next().is_none());
}

#[test]
fn case_only_asset_renames_do_not_delete_the_current_files() {
    let dir = scratch("case-only");
    let bundle = bundle(&dir);
    let out = dir.join("read-notes");
    let mut ir = procedure();
    compile(&ir, &bundle, &out).unwrap();
    let case_insensitive = out.join("scripts/KEEP.sh").exists();

    ir.steps.truncate(1);
    ir.steps[0].scripts[0].name = "KEEP.sh".into();
    ir.steps[0].evidence[0].frame = Some("KEEP.jpg".into());
    fs::write(bundle.join("KEEP.jpg"), b"new frame").unwrap();
    compile(&ir, &bundle, &out).unwrap();

    assert_eq!(
        fs::read(out.join("references/frames/KEEP.jpg")).unwrap(),
        b"new frame"
    );
    assert!(out.join("scripts/KEEP.sh").is_file());
    if !case_insensitive {
        assert!(!out.join("scripts/keep.sh").exists());
        assert!(!out.join("references/frames/keep.jpg").exists());
    }
}

#[test]
fn paths_in_old_provenance_cannot_remove_files_outside_the_package() {
    let dir = scratch("invalid-old-path");
    let bundle = bundle(&dir);
    let out = dir.join("read-notes");
    let ir = procedure();
    compile(&ir, &bundle, &out).unwrap();
    fs::write(dir.join("outside.md"), b"unrelated").unwrap();
    let mut previous = ir.clone();
    previous.steps[1].id = "../../outside".into();
    fs::write(
        out.join("provenance.json"),
        serde_json::to_vec(&previous).unwrap(),
    )
    .unwrap();
    let before = files(&out);

    assert!(compile(&ir, &bundle, &out).is_err());
    assert_eq!(fs::read(dir.join("outside.md")).unwrap(), b"unrelated");
    assert_eq!(files(&out), before);
}

#[test]
fn cleanup_refuses_symlinked_directories_before_changing_the_package() {
    let dir = scratch("symlink-parent");
    let bundle = bundle(&dir);
    let out = dir.join("read-notes");
    let outside = dir.join("outside");
    let mut ir = procedure();
    compile(&ir, &bundle, &out).unwrap();
    fs::rename(out.join("scripts"), &outside).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, out.join("scripts")).unwrap();
    #[cfg(windows)]
    if let Err(error) = std::os::windows::fs::symlink_dir(&outside, out.join("scripts")) {
        eprintln!("directory symlink unavailable: {error}");
        return;
    }
    let before = fs::read(out.join("provenance.json")).unwrap();
    let outside_before = files(&outside);
    ir.steps.truncate(1);
    ir.steps[0].scripts.clear();

    assert!(compile(&ir, &bundle, &out).is_err());
    assert_eq!(fs::read(out.join("provenance.json")).unwrap(), before);
    assert!(out.join("steps/02-remove.md").is_file());
    assert_eq!(files(&outside), outside_before);
}

#[cfg(unix)]
#[test]
fn cleanup_refuses_a_symlinked_package_root() {
    let dir = scratch("symlink-root");
    let bundle = bundle(&dir);
    let real = dir.join("real/read-notes");
    let link = dir.join("read-notes");
    let mut ir = procedure();
    compile(&ir, &bundle, &real).unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let before = files(&real);
    ir.steps.truncate(1);

    assert!(compile(&ir, &bundle, &link).is_err());
    assert_eq!(files(&real), before);
}

#[test]
fn recompilation_can_use_frames_from_the_existing_package() {
    let dir = scratch("package-as-bundle");
    let bundle = bundle(&dir);
    let out = dir.join("read-notes");
    let mut ir = procedure();
    compile(&ir, &bundle, &out).unwrap();
    ir.steps.truncate(1);
    ir.steps[0].evidence[0].frame = Some("references/frames/keep.jpg".into());

    compile(&ir, &out, &out).unwrap();

    assert_eq!(
        fs::read(out.join("references/frames/keep.jpg")).unwrap(),
        b"keep.jpg"
    );
    assert!(!out.join("steps/02-remove.md").exists());
}
