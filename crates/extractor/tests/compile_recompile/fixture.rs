use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;
use vts_extract::compile::ProcedureIr;

pub fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-recompile-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn procedure() -> ProcedureIr {
    let step = |id: &str, asset: &str| {
        json!({
            "id": id,
            "goal": "Read a local note",
            "actions": "Read the note.",
            "success_criteria": "The note is visible.",
            "confidence": "high",
            "caveats": null,
            "evidence": [{
                "timestamp_secs": 1.0,
                "frame": format!("{asset}.jpg"),
                "quote": "Read the note"
            }],
            "scripts": [{
                "name": format!("{asset}.sh"),
                "contents": "#!/bin/sh\n# Read a note.\n"
            }]
        })
    };
    serde_json::from_value(json!({
        "schema_version": 2,
        "skill_name": "read-notes",
        "title": "Read notes",
        "description": "Use when reading local notes.",
        "overview": "Read local notes.",
        "genre": "screencast",
        "source": {
            "original": "notes.mp4",
            "title": "Notes",
            "duration_secs": 10.0
        },
        "steps": [step("01-keep", "keep"), step("02-remove", "old")]
    }))
    .unwrap()
}

pub fn bundle(dir: &Path) -> PathBuf {
    let bundle = dir.join("bundle");
    fs::create_dir_all(&bundle).unwrap();
    for name in ["keep.jpg", "old.jpg"] {
        fs::write(bundle.join(name), name).unwrap();
    }
    bundle
}

pub fn files(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root).unwrap().into(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(dir, dir, &mut out);
    out
}
