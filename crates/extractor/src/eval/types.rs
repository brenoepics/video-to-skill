//! The eval report and the counts behind it, serialised as the JSON
//! artifact and the committed baseline.

use serde::{Deserialize, Serialize};

use crate::bench::Machine;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalReport {
    pub schema_version: u32,
    pub version: String,
    pub generated_on: String,
    pub machine: Machine,
    pub tolerance_secs: f64,
    pub iou_threshold: f64,
    pub gold_set: GoldSummary,
    pub metrics: Vec<Metric>,
    pub videos: Vec<VideoReport>,
    #[serde(default)]
    pub notes: Vec<String>,
}

impl EvalReport {
    #[must_use]
    pub fn metric(&self, name: &str) -> Option<&Metric> {
        self.metrics.iter().find(|m| m.name == name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct GoldSummary {
    pub videos: u64,
    pub steps: u64,
    pub commands: u64,
    pub changes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metric {
    pub name: String,
    #[serde(flatten)]
    pub outcome: Outcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Scored {
        value: f64,
        /// Wilson 95 % lower bound (for F1: harmonic mean of the
        /// precision and recall bounds).
        lower_bound: f64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        numerator: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        denominator: Option<u64>,
    },
    NotScored {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoReport {
    pub id: String,
    pub title: String,
    pub duration_secs: f64,
    pub segments: u64,
    pub transcript_segments: Option<u64>,
    pub counts: Counts,
    #[serde(default)]
    pub packages_scored: u64,
    #[serde(default)]
    pub verifications_scored: u64,
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Raw counts behind every metric; summed across videos before scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Counts {
    pub boundaries_predicted: u64,
    pub boundaries_gold: u64,
    pub boundaries_matched: u64,
    pub steps_gold: u64,
    pub steps_recovered: u64,
    pub segments_total: u64,
    pub segments_clean: u64,
    pub commands_gold: u64,
    pub commands_spoken: u64,
    pub package_steps: u64,
    pub package_matched: u64,
    pub commands_exact: u64,
    pub verification_steps: u64,
    pub verification_passed: u64,
}

impl Counts {
    pub fn add(&mut self, other: &Counts) {
        self.boundaries_predicted += other.boundaries_predicted;
        self.boundaries_gold += other.boundaries_gold;
        self.boundaries_matched += other.boundaries_matched;
        self.steps_gold += other.steps_gold;
        self.steps_recovered += other.steps_recovered;
        self.segments_total += other.segments_total;
        self.segments_clean += other.segments_clean;
        self.commands_gold += other.commands_gold;
        self.commands_spoken += other.commands_spoken;
        self.package_steps += other.package_steps;
        self.package_matched += other.package_matched;
        self.commands_exact += other.commands_exact;
        self.verification_steps += other.verification_steps;
        self.verification_passed += other.verification_passed;
    }
}
