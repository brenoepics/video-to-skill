//! Task-graph folding, continued: contradictory orders, packages from
//! before task graphs, and the recompiled step index.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

#[path = "merge_graph/vim_basics.rs"]
mod vim_basics;

use std::fs;
use vim_basics::{compiled_a, fold_a_b, ids, scratch, video_a, video_b, video_c};
use vts_extract::compile::graph::START;
use vts_extract::compile::{compile, ProcedureIr};
use vts_extract::merge::merge;

#[test]
fn a_contradictory_order_keeps_the_heavier_direction_and_cites_the_other() {
    let once = merge(&compiled_a("contradict"), &video_c(), "2026-11-02").merged;

    assert_eq!(
        ids(&once),
        [
            "01-open-file",
            "02-insert-text",
            "03-save-file",
            "04-quit-vim"
        ],
        "one against one: the existing order stands"
    );
    assert_eq!(once.task_graph.count("02-insert-text", "03-save-file"), 1);
    assert_eq!(once.task_graph.count("03-save-file", "02-insert-text"), 1);
    assert_eq!(
        once.history[0].order_variants,
        ["03-save-file before 02-insert-text (1 vs 1 videos)"]
    );

    let twice = merge(&once, &video_c(), "2026-11-03").merged;

    assert_eq!(
        ids(&twice),
        [
            "01-open-file",
            "03-save-file",
            "02-insert-text",
            "04-quit-vim"
        ],
        "two against one: the heavier direction orders the steps"
    );
    assert_eq!(
        twice.history[1].order_variants,
        ["02-insert-text before 03-save-file (1 vs 2 videos)"]
    );
}

#[test]
fn a_package_without_a_graph_derives_one_from_its_step_order_first() {
    // A v1 package, compiled before task graphs existed.
    let legacy = video_a();
    assert!(legacy.task_graph.is_empty());

    let graph = merge(&legacy, &video_b(), "2026-11-02").merged.task_graph;

    assert_eq!(graph.count(START, "01-open-file"), 2);
    assert_eq!(graph.count("02-insert-text", "03-save-file"), 1);
    assert_eq!(graph.count("03-undo", "03-save-file"), 1);
}

#[test]
fn a_compiled_merge_keeps_its_graph_and_reorders_the_step_index() {
    let dir = scratch("recompile");
    let bundle = dir.join("bundle");
    fs::create_dir_all(&bundle).unwrap();
    let merged = fold_a_b("recompile-fold").merged;
    let out = dir.join("vim-basics");

    compile(&merged, &bundle, &out).unwrap();

    let provenance: ProcedureIr =
        serde_json::from_str(&fs::read_to_string(out.join("provenance.json")).unwrap()).unwrap();
    assert_eq!(provenance.task_graph, merged.task_graph);
    let skill_md = fs::read_to_string(out.join("SKILL.md")).unwrap();
    let undo = skill_md.find("steps/03-undo.md").unwrap();
    let save = skill_md.find("steps/03-save-file.md").unwrap();
    assert!(undo < save, "the index follows the graph order: {skill_md}");
}
