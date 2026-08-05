//! Marker-preserving splice: replace the region between two markers in a
//! document, keeping the markers so the splice can be re-run.

use anyhow::{bail, Result};

/// Replace everything between `begin_marker` and `end_marker` with
/// `replacement`, preserving both markers. Errors name the missing
/// marker, or report markers appearing in the wrong order.
pub fn splice(
    document: &str,
    begin_marker: &str,
    end_marker: &str,
    replacement: &str,
) -> Result<String> {
    let Some(begin_at) = document.find(begin_marker) else {
        bail!("missing marker '{begin_marker}'");
    };
    let head_end = begin_at + begin_marker.len();
    let Some(end_offset) = document[head_end..].find(end_marker) else {
        if document.contains(end_marker) {
            bail!("markers out of order: '{end_marker}' appears before '{begin_marker}'");
        }
        bail!("missing marker '{end_marker}'");
    };
    Ok(format!(
        "{}\n{}\n{}",
        &document[..head_end],
        replacement.trim_matches('\n'),
        &document[head_end + end_offset..]
    ))
}

#[cfg(test)]
mod tests {
    use super::splice;

    const BEGIN: &str = "<!-- bench-results:start -->";
    const END: &str = "<!-- bench-results:end -->";

    fn doc() -> String {
        format!("# Title\n\n{BEGIN}\nstale content\n{END}\n\ntail\n")
    }

    #[test]
    fn replaces_content_between_markers_preserving_them() {
        let spliced = splice(&doc(), BEGIN, END, "fresh content").unwrap();

        assert_eq!(
            spliced,
            format!("# Title\n\n{BEGIN}\nfresh content\n{END}\n\ntail\n")
        );
    }

    #[test]
    fn resplicing_is_stable_and_replaces_previous_content() {
        let once = splice(&doc(), BEGIN, END, "v1").unwrap();

        let again = splice(&once, BEGIN, END, "v1").unwrap();
        assert_eq!(again, once, "same replacement must be a fixed point");

        let updated = splice(&once, BEGIN, END, "v2").unwrap();
        assert!(updated.contains("v2"));
        assert!(!updated.contains("v1"));
    }

    #[test]
    fn missing_begin_marker_is_named_in_the_error() {
        let err = splice("no markers here", BEGIN, END, "x").unwrap_err();

        assert!(err.to_string().contains(BEGIN), "got: {err}");
    }

    #[test]
    fn missing_end_marker_is_named_in_the_error() {
        let document = format!("intro\n{BEGIN}\nbody\n");

        let err = splice(&document, BEGIN, END, "x").unwrap_err();

        assert!(err.to_string().contains(END), "got: {err}");
    }

    #[test]
    fn markers_out_of_order_are_rejected() {
        let document = format!("{END}\nmiddle\n{BEGIN}\n");

        let err = splice(&document, BEGIN, END, "x").unwrap_err();

        assert!(err.to_string().contains("out of order"), "got: {err}");
    }
}
