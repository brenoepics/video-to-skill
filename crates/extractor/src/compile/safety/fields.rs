//! The schema v2 step fields the safety walk scans, kept apart so the
//! enumeration in `mod.rs` stays the readable list of what a step is.
//! Each field carries the scope that decides how a hit is treated.

use super::{field, Field, Scope};
use crate::compile::Step;

/// What v2 added to a step. Preconditions and parameters are prose the
/// skill tells the agent; the postcondition is something the verifier
/// runs; failure notes describe what a repair fixed, so like caveats
/// they may quote a danger without being refused for it.
pub(super) fn v2(step: &Step) -> Vec<Field<'_>> {
    use Scope::{Executable, Prose, Warning};
    let at = || format!("step '{}'", step.id);
    let mut out: Vec<Field<'_>> = Vec::new();
    out.extend(
        step.preconditions
            .iter()
            .map(|p| field(at(), "preconditions", p, Prose)),
    );
    for parameter in &step.parameters {
        out.push(field(
            at(),
            "parameter description",
            &parameter.description,
            Prose,
        ));
        out.extend(
            parameter
                .example
                .iter()
                .map(|e| field(at(), "parameter example", e, Prose)),
        );
    }
    out.extend(
        step.postcondition
            .iter()
            .map(|p| field(at(), "postcondition", p, Executable)),
    );
    out.extend(
        step.failure_notes
            .iter()
            .map(|n| field(at(), "failure notes", n, Warning)),
    );
    out
}
