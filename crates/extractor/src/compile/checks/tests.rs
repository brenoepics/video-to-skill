use std::path::Path;

use super::{checkable_line, code_span, needs_workspace, strings, substitute_workspace};
use crate::a11y::check::Check;

fn parse(json: &str) -> Check {
    Check::parse(json).unwrap_or_else(|e| panic!("{json} should parse: {e:#}"))
}

#[test]
fn code_spans_outlast_any_backtick_run_inside_them() {
    assert_eq!(code_span("{}"), "`{}`");
    assert_eq!(code_span("a`b"), "``a`b``");
    assert_eq!(code_span("a```b`"), "```` a```b` ````");
}

#[test]
fn the_workspace_is_substituted_in_file_paths_only() {
    let workspace = Path::new("/tmp/vts-work");
    let file = parse(r#"{"check":"file_exists","path":"{workspace}/out/notes.txt"}"#);
    assert!(needs_workspace(&file));

    let substituted = substitute_workspace(&file, workspace).unwrap();

    assert_eq!(
        substituted,
        Check::FileExists {
            path: workspace.join("out/notes.txt").to_str().unwrap().into()
        }
    );
    assert!(!needs_workspace(&substituted));
    let title = parse(r#"{"check":"window_title_contains","text":"{workspace}"}"#);
    assert!(!needs_workspace(&title), "only file paths are substituted");
    assert_eq!(substitute_workspace(&title, workspace).unwrap(), title);
}

#[test]
fn every_string_in_a_check_is_enumerated_once() {
    let check = parse(
        r#"{"check":"element_value_equals","app":"Notes","role":"text field",
            "title":"Name","identifier":"name","value":"Ada"}"#,
    );
    assert_eq!(
        strings(&check),
        [
            ("check app", "Notes"),
            ("check role", "text field"),
            ("check title", "Name"),
            ("check identifier", "name"),
            ("check value", "Ada"),
        ]
    );
    let number =
        parse(r#"{"check":"element_value_equals","role":"slider","title":"Zoom","value":2}"#);
    assert_eq!(
        strings(&number).len(),
        2,
        "a number is not text: {:?}",
        strings(&number)
    );
}

#[test]
fn a_checkable_line_carries_the_exact_json_observe_takes() {
    let json = r#"{"check":"checkbox_state","identifier":"wrap","checked":false}"#;

    let line = checkable_line(&parse(json)).unwrap();

    assert_eq!(
        line,
        format!(
            "Checkable: the checkbox with identifier \"wrap\" in the frontmost application \
             is unchecked — `{json}`"
        )
    );
}
