//! The schema v2 step fields the safety walk scans, kept apart so the
//! enumeration in `mod.rs` stays the readable list of what a step is.

use super::Field;
use crate::compile::Step;

/// Prose v2 added to a step; all of it lands in the step file.
pub(super) fn v2_prose(step: &Step) -> Vec<Field<'_>> {
    let at = || format!("step '{}'", step.id);
    let mut out: Vec<Field<'_>> = Vec::new();
    out.extend(
        step.preconditions
            .iter()
            .map(|p| (at(), "preconditions", p.as_str())),
    );
    for parameter in &step.parameters {
        out.push((at(), "parameter description", &parameter.description));
        out.extend(
            parameter
                .example
                .iter()
                .map(|e| (at(), "parameter example", e.as_str())),
        );
    }
    out.extend(
        step.postcondition
            .iter()
            .map(|p| (at(), "postcondition", p.as_str())),
    );
    out.extend(
        step.failure_notes
            .iter()
            .map(|n| (at(), "failure notes", n.as_str())),
    );
    out
}

/// What v2 added that gets run: the verifier executes the postcondition.
pub(super) fn v2_executable(step: &Step) -> Vec<Field<'_>> {
    step.postcondition
        .iter()
        .map(|p| (format!("step '{}'", step.id), "postcondition", p.as_str()))
        .collect()
}
