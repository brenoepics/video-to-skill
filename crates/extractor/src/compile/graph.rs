//! The task graph: keysteps as nodes, observed transitions as weighted
//! edges, pooled across every folded video (Video-Mined Task Graphs,
//! Ashutosh et al., 2023). Folding becomes counting: a skill's
//! step order is the heaviest walk through the graph, and a step whose
//! text matches nothing confidently is placed by where it sits between
//! steps that do (see `merge`).

use std::cmp::Reverse;

use serde::{Deserialize, Serialize};

/// Sentinel node every video's walk starts from.
pub const START: &str = "START";
/// Sentinel node every video's walk ends in.
pub const END: &str = "END";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskGraph {
    /// Keystep vocabulary: step ids, in the order they were first seen.
    #[serde(default)]
    pub nodes: Vec<String>,
    /// Consecutive-step observations, pooled across videos; `from` and
    /// `to` are node ids or the [`START`] / [`END`] sentinels.
    #[serde(default)]
    pub transitions: Vec<Transition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Transition {
    pub from: String,
    pub to: String,
    pub count: u32,
}

impl TaskGraph {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.transitions.is_empty()
    }

    /// The graph of one video: a chain through `ids` with unit counts.
    #[must_use]
    pub fn from_sequence(ids: &[String]) -> Self {
        let mut graph = Self::default();
        graph.record_walk(ids);
        graph
    }

    /// Count one more video that performed `ids` in this order
    /// (START → ids… → END); ids not yet known join the vocabulary.
    pub fn record_walk(&mut self, ids: &[String]) {
        for id in ids {
            if !self.nodes.contains(id) {
                self.nodes.push(id.clone());
            }
        }
        let walk: Vec<&str> = std::iter::once(START)
            .chain(ids.iter().map(String::as_str))
            .chain(std::iter::once(END))
            .collect();
        for pair in walk.windows(2) {
            self.bump(pair[0], pair[1]);
        }
    }

    fn bump(&mut self, from: &str, to: &str) {
        match self
            .transitions
            .iter_mut()
            .find(|t| t.from == from && t.to == to)
        {
            Some(transition) => transition.count += 1,
            None => self.transitions.push(Transition {
                from: from.to_string(),
                to: to.to_string(),
                count: 1,
            }),
        }
    }

    /// How many videos performed `to` right after `from`.
    #[must_use]
    pub fn count(&self, from: &str, to: &str) -> u32 {
        self.transitions
            .iter()
            .find(|t| t.from == from && t.to == to)
            .map_or(0, |t| t.count)
    }

    /// Whether some video-observed path leads from `from` to `to`.
    #[must_use]
    pub fn reaches(&self, from: &str, to: &str) -> bool {
        let mut seen = vec![from];
        let mut frontier = vec![from];
        while let Some(node) = frontier.pop() {
            for transition in self.transitions.iter().filter(|t| t.from == node) {
                if transition.to == to {
                    return true;
                }
                if !seen.contains(&transition.to.as_str()) {
                    seen.push(&transition.to);
                    frontier.push(&transition.to);
                }
            }
        }
        false
    }

    /// The step order the graph supports: a topological order over the
    /// dominant direction of every edge, following the heaviest kept
    /// transition out of the last placed step, ties broken by `rank` (the
    /// order the package already had, new steps after it). Also returns
    /// the edges that lost — the lighter direction of each contradiction
    /// — as `(from, to)`.
    #[must_use]
    pub fn order(&self, rank: &[String]) -> (Vec<String>, Vec<(String, String)>) {
        let rank_of = |id: &str| {
            rank.iter().position(|r| r == id).unwrap_or_else(|| {
                rank.len() + self.nodes.iter().position(|n| n == id).unwrap_or(0)
            })
        };
        let is_node = |id: &str| id != START && id != END;
        let mut dropped: Vec<(String, String)> = Vec::new();
        let kept: Vec<&Transition> = self
            .transitions
            .iter()
            .filter(|t| is_node(&t.from) && is_node(&t.to) && t.count > 0)
            .filter(|t| {
                let reverse = self.count(&t.to, &t.from);
                let wins =
                    t.count > reverse || (t.count == reverse && rank_of(&t.from) < rank_of(&t.to));
                if reverse > 0 && !wins {
                    dropped.push((t.from.clone(), t.to.clone()));
                }
                reverse == 0 || wins
            })
            .collect();

        let mut remaining: Vec<&str> = self.nodes.iter().map(String::as_str).collect();
        let mut order: Vec<String> = Vec::with_capacity(remaining.len());
        let mut last = START;
        while !remaining.is_empty() {
            let available: Vec<&str> = remaining
                .iter()
                .copied()
                .filter(|node| {
                    !kept
                        .iter()
                        .any(|t| t.to == *node && remaining.contains(&t.from.as_str()))
                })
                .collect();
            let pick = if available.is_empty() {
                // A longer cycle: place the best-ranked node and let the
                // edges into it from unplaced nodes lose.
                let node = remaining.iter().copied().min_by_key(|n| rank_of(n));
                let Some(node) = node else { break };
                for t in kept
                    .iter()
                    .filter(|t| t.to == node && remaining.contains(&t.from.as_str()))
                {
                    dropped.push((t.from.clone(), t.to.clone()));
                }
                node
            } else {
                let weight = |node: &str| {
                    kept.iter()
                        .find(|t| t.from == last && t.to == node)
                        .map_or(0, |t| t.count)
                };
                available
                    .iter()
                    .copied()
                    .max_by_key(|node| (weight(node), Reverse(rank_of(node))))
                    .unwrap_or(available[0])
            };
            order.push(pick.to_string());
            remaining.retain(|n| *n != pick);
            last = pick;
        }
        (order, dropped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_sequence_is_a_chain_with_sentinels() {
        let graph = TaskGraph::from_sequence(&ids(&["a", "b"]));
        assert_eq!(graph.nodes, ["a", "b"]);
        assert_eq!(graph.count(START, "a"), 1);
        assert_eq!(graph.count("a", "b"), 1);
        assert_eq!(graph.count("b", END), 1);
        assert_eq!(graph.count("b", "a"), 0);
        assert!(graph.reaches(START, END) && !graph.reaches("b", "a"));
    }

    #[test]
    fn walks_pool_counts_and_grow_the_vocabulary() {
        let mut graph = TaskGraph::from_sequence(&ids(&["a", "b"]));
        graph.record_walk(&ids(&["a", "c", "b"]));
        assert_eq!(graph.nodes, ["a", "b", "c"]);
        assert_eq!(graph.count(START, "a"), 2);
        assert_eq!(graph.count("a", "c"), 1);
        assert_eq!(
            graph.order(&ids(&["a", "b", "c"])),
            (ids(&["a", "c", "b"]), vec![])
        );
    }

    #[test]
    fn contradictions_follow_the_heavier_direction_and_report_the_lighter() {
        let mut graph = TaskGraph::from_sequence(&ids(&["a", "b", "c"]));
        graph.record_walk(&ids(&["a", "c", "b"]));
        let rank = ids(&["a", "b", "c"]);
        assert_eq!(
            graph.order(&rank),
            (ids(&["a", "b", "c"]), vec![("c".into(), "b".into())]),
            "a tie keeps the existing order"
        );
        graph.record_walk(&ids(&["a", "c", "b"]));
        assert_eq!(
            graph.order(&rank),
            (ids(&["a", "c", "b"]), vec![("b".into(), "c".into())])
        );
    }
}
