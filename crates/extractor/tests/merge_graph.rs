//! Task-graph folding on the two-video vim-basics example: keysteps are
//! graph nodes, each fold adds one walk of transition counts, the merged
//! order follows the graph, low-confidence steps are relabelled by the
//! path between confident anchors, and the confirm/variant/add/keep
//! summary users already see is unchanged.

// Integration-test file: everything here is test code, where unwrap is fine.
#![allow(clippy::unwrap_used)]

#[path = "merge_graph/vim_basics.rs"]
mod vim_basics;

use vim_basics::{compiled_a, fold_a_b, ids, B};
use vts_extract::compile::graph::{END, START};
use vts_extract::merge::PlanAction;

#[test]
fn a_single_video_compiles_to_a_linear_chain_with_unit_counts() {
    let graph = compiled_a("chain").task_graph;

    assert_eq!(
        graph.nodes,
        [
            "01-open-file",
            "02-insert-text",
            "03-save-file",
            "04-quit-vim"
        ]
    );
    assert_eq!(graph.transitions.len(), 5);
    for (from, to) in [
        (START, "01-open-file"),
        ("01-open-file", "02-insert-text"),
        ("02-insert-text", "03-save-file"),
        ("03-save-file", "04-quit-vim"),
        ("04-quit-vim", END),
    ] {
        assert_eq!(graph.count(from, to), 1, "{from} -> {to}");
    }
}

#[test]
fn folding_pools_transition_counts_over_both_videos() {
    let graph = fold_a_b("pool").merged.task_graph;

    assert_eq!(graph.nodes.len(), 5, "{:?}", graph.nodes);
    for (from, to, count) in [
        (START, "01-open-file", 2),
        ("01-open-file", "02-insert-text", 2),
        ("02-insert-text", "03-save-file", 1),
        ("02-insert-text", "03-undo", 1),
        ("03-undo", "03-save-file", 1),
        ("03-save-file", "04-quit-vim", 2),
        ("04-quit-vim", END, 2),
    ] {
        assert_eq!(graph.count(from, to), count, "{from} -> {to}");
    }
}

#[test]
fn the_merged_order_follows_the_graph_instead_of_appending() {
    let merged = fold_a_b("order").merged;

    assert_eq!(
        ids(&merged),
        [
            "01-open-file",
            "02-insert-text",
            "03-undo",
            "03-save-file",
            "04-quit-vim"
        ],
        "the new step lands where the video performed it"
    );
}

#[test]
fn low_confidence_steps_are_relabelled_by_the_path_between_anchors() {
    let result = fold_a_b("relabel");

    let write = result
        .plan
        .iter()
        .find(|e| e.incoming_id.as_deref() == Some("04-write"))
        .unwrap();
    assert_eq!(write.existing_id.as_deref(), Some("03-save-file"));
    assert_eq!(write.action, PlanAction::Confirm);
    assert!(
        write.relabelled,
        "matched through the graph, not by goal words"
    );
    let save = result
        .merged
        .steps
        .iter()
        .find(|s| s.id == "03-save-file")
        .unwrap();
    assert_eq!(save.evidence.len(), 2, "confirmed: second provenance added");
    assert_eq!(save.evidence[1].source.as_deref(), Some(B));
}

#[test]
fn the_user_facing_summary_is_preserved() {
    let result = fold_a_b("summary");

    let action_of = |incoming: &str| {
        result
            .plan
            .iter()
            .find(|e| e.incoming_id.as_deref() == Some(incoming))
            .map(|e| e.action.clone())
            .unwrap()
    };
    assert_eq!(action_of("01-open"), PlanAction::Confirm);
    assert_eq!(action_of("02-insert"), PlanAction::Confirm);
    assert_eq!(action_of("03-undo"), PlanAction::Add);
    assert_eq!(action_of("04-write"), PlanAction::Confirm);
    assert_eq!(action_of("05-quit"), PlanAction::Variant);
    assert!(
        !result
            .plan
            .iter()
            .any(|e| e.action == PlanAction::Unconfirmed),
        "every existing step was covered"
    );
    let record = &result.merged.history[0];
    assert_eq!(
        record.confirmed,
        ["01-open-file", "02-insert-text", "03-save-file"]
    );
    assert_eq!(record.added, ["03-undo"]);
    assert_eq!(record.variants, ["04-quit-vim"]);
    assert!(
        record.order_variants.is_empty(),
        "{:?}",
        record.order_variants
    );
}
