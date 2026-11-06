//! The two-video vim-basics example shared by the task-graph folding
//! tests: video A (open, insert, save, quit), video B (the same opening,
//! a new undo step, a differently worded save, a conflicting quit) and
//! video C (saves before it inserts).

// Shared by two test crates; each uses a subset of these helpers.
#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;
use vts_extract::compile::{
    compile, Evidence, ProcedureIr, SourceRef, Step, TaskGraph, SCHEMA_VERSION,
};
use vts_extract::merge::{merge, MergeResult};

pub const A: &str = "https://videos.example/vim-basics-a";
pub const B: &str = "https://videos.example/vim-basics-b";
pub const C: &str = "https://videos.example/vim-basics-c";

pub fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("vts-merge-graph-tests")
        .join(format!("{name}-{}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir).unwrap();
    }
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub fn step(id: &str, goal: &str, actions: &str) -> Step {
    Step {
        id: id.into(),
        goal: goal.into(),
        actions: actions.into(),
        success_criteria: "it worked".into(),
        confidence: "medium".into(),
        caveats: None,
        evidence: vec![Evidence {
            timestamp_secs: 10.0,
            frame: None,
            quote: Some("seen".into()),
            source: None,
        }],
        scripts: vec![],
        variants: vec![],
        preconditions: vec![],
        parameters: vec![],
        postcondition: None,
        checks: vec![],
        failure_notes: vec![],
        usage_count: 0,
        verdict: None,
    }
}

pub fn video(url: &str, steps: Vec<Step>) -> ProcedureIr {
    ProcedureIr {
        schema_version: SCHEMA_VERSION,
        skill_name: "vim-basics".into(),
        title: "Vim Basics".into(),
        description: "Use when editing files with Vim.".into(),
        license: None,
        compatibility: None,
        overview: "Vim fundamentals.".into(),
        genre: "screencast".into(),
        source: SourceRef {
            original: url.into(),
            title: None,
            duration_secs: 300.0,
            sha256: None,
        },
        steps,
        artifacts: vec![],
        gaps: vec![],
        history: vec![],
        task_graph: TaskGraph::default(),
    }
}

/// The first video: open, insert, save, quit.
pub fn video_a() -> ProcedureIr {
    video(
        A,
        vec![
            step("01-open-file", "Open a file in Vim", "Run `vim notes.txt`"),
            step(
                "02-insert-text",
                "Enter insert mode and type text",
                "Press `i`, type, then `Esc`",
            ),
            step("03-save-file", "Save the file", "Type `:w` and press Enter"),
            step("04-quit-vim", "Quit Vim", "Type `:q` and press Enter"),
        ],
    )
}

/// The second video: the same opening, a new undo step, a save step
/// whose goal is worded differently, and a conflicting quit.
pub fn video_b() -> ProcedureIr {
    video(
        B,
        vec![
            step("01-open", "Open a file in Vim", "Run `vim notes.txt`"),
            step(
                "02-insert",
                "Enter insert mode and type text",
                "Press `i`, type, then `Esc`",
            ),
            step(
                "03-undo",
                "Undo the last change",
                "Press `u` in normal mode",
            ),
            step(
                "04-write",
                "Write the buffer to disk",
                "Type `:w` and press Enter",
            ),
            step("05-quit", "Quit Vim", "Type `:q!` to quit without saving"),
        ],
    )
}

/// A third video that saves before it inserts.
pub fn video_c() -> ProcedureIr {
    video(
        C,
        vec![
            step("01-open", "Open a file in Vim", "Run `vim notes.txt`"),
            step("02-save", "Save the file", "Type `:w` and press Enter"),
            step(
                "03-insert",
                "Enter insert mode and type text",
                "Press `i`, type, then `Esc`",
            ),
            step("04-quit", "Quit Vim", "Type `:q` and press Enter"),
        ],
    )
}

/// Video A as its compiled package records it (provenance.json).
pub fn compiled_a(name: &str) -> ProcedureIr {
    let dir = scratch(name);
    let bundle = dir.join("bundle");
    fs::create_dir_all(&bundle).unwrap();
    let out = dir.join("vim-basics");
    compile(&video_a(), &bundle, &out).unwrap();
    serde_json::from_str(&fs::read_to_string(out.join("provenance.json")).unwrap()).unwrap()
}

pub fn ids(ir: &ProcedureIr) -> Vec<&str> {
    ir.steps.iter().map(|s| s.id.as_str()).collect()
}

pub fn fold_a_b(name: &str) -> MergeResult {
    merge(&compiled_a(name), &video_b(), "2026-11-02")
}
