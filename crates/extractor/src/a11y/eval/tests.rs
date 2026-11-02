use std::fs;

use super::evaluate;
use crate::a11y::check::Check;
use crate::a11y::tree::{Node, Scalar, Snapshot};

/// A TextEdit-shaped tree: menu bar, one document window with a toolbar
/// button, a checkbox and a text area.
fn text_edit() -> Snapshot {
    let file_menu = Node::new("AXMenuBarItem").with_title("File").with_child(
        Node::new("AXMenu")
            .with_child(Node::new("AXMenuItem").with_title("New").with_enabled(true))
            .with_child(
                Node::new("AXMenuItem")
                    .with_title("Close")
                    .with_enabled(true),
            )
            .with_child(
                Node::new("AXMenuItem")
                    .with_title("Save")
                    .with_enabled(false),
            )
            .with_child(Node::new("AXMenuItem").with_title("Export")),
    );
    let window = Node::new("AXWindow")
        .with_title("Untitled 2")
        .with_child(Node::new("AXButton").with_title("Save").with_enabled(true))
        .with_child(
            Node::new("AXCheckBox")
                .with_title("Wrap lines")
                .with_value(Scalar::Number(1.0)),
        )
        .with_child(
            Node::new("AXCheckBox")
                .with_title("Mixed")
                .with_value(Scalar::Number(2.0)),
        )
        .with_child(
            Node::new("AXScrollArea").with_child(
                Node::new("AXTextArea")
                    .with_identifier("body")
                    .with_value(Scalar::Text("hello".into())),
            ),
        );
    Snapshot {
        app: "TextEdit".into(),
        pid: Some(4242),
        truncated: false,
        root: Node::new("AXApplication")
            .with_title("TextEdit")
            .with_child(Node::new("AXMenuBar").with_child(file_menu))
            .with_child(window),
    }
}

fn check(json: &str) -> Check {
    Check::parse(json).unwrap()
}

#[test]
fn window_title_contains_reads_every_window() {
    let tree = text_edit();
    let hit = evaluate(
        &check(r#"{"check":"window_title_contains","text":"Untitled"}"#),
        Some(&tree),
    );
    assert!(hit.passed, "{}", hit.observed);
    assert!(hit.observed.contains("Untitled 2"));
    let miss = evaluate(
        &check(r#"{"check":"window_title_contains","text":"Report.txt"}"#),
        Some(&tree),
    );
    assert!(!miss.passed);
    assert!(
        miss.observed.contains("Untitled 2"),
        "a miss still lists what was seen: {}",
        miss.observed
    );
    assert_ne!(hit.not_proved, "");
}

#[test]
fn element_exists_matches_role_plus_title_or_identifier() {
    let tree = text_edit();
    let by_title = check(r#"{"check":"element_exists","role":"AXButton","title":"Save"}"#);
    assert!(evaluate(&by_title, Some(&tree)).passed);
    let by_id = check(r#"{"check":"element_exists","role":"AXTextArea","identifier":"body"}"#);
    assert!(evaluate(&by_id, Some(&tree)).passed);
    let wrong_role =
        check(r#"{"check":"element_exists","role":"AXMenuItem","title":"Wrap lines"}"#);
    let verdict = evaluate(&wrong_role, Some(&tree));
    assert!(!verdict.passed);
    assert!(
        verdict.observed.contains("Close"),
        "lists same-role candidates: {}",
        verdict.observed
    );
}

#[test]
fn roles_match_without_the_platform_prefix_and_ignoring_case() {
    let tree = text_edit();
    for role in ["button", "Button", "AXBUTTON", "push_button"] {
        let c = check(&format!(
            r#"{{"check":"element_exists","role":"{role}","title":"Save"}}"#
        ));
        let verdict = evaluate(&c, Some(&tree));
        assert_eq!(
            verdict.passed,
            role != "push_button",
            "{role}: {}",
            verdict.observed
        );
    }
}

#[test]
fn element_value_equals_compares_typed_values() {
    let tree = text_edit();
    let text = check(
        r#"{"check":"element_value_equals","role":"AXTextArea","identifier":"body","value":"hello"}"#,
    );
    assert!(evaluate(&text, Some(&tree)).passed);
    let other = check(
        r#"{"check":"element_value_equals","role":"AXTextArea","identifier":"body","value":"bye"}"#,
    );
    let verdict = evaluate(&other, Some(&tree));
    assert!(!verdict.passed);
    assert!(verdict.observed.contains("hello"), "{}", verdict.observed);
    let number = check(
        r#"{"check":"element_value_equals","role":"AXCheckBox","title":"Wrap lines","value":1}"#,
    );
    assert!(evaluate(&number, Some(&tree)).passed);
    let typed = check(
        r#"{"check":"element_value_equals","role":"AXCheckBox","title":"Wrap lines","value":"1"}"#,
    );
    assert!(
        !evaluate(&typed, Some(&tree)).passed,
        "a text \"1\" is not the number 1"
    );
    let missing = check(
        r#"{"check":"element_value_equals","role":"AXTextArea","identifier":"nope","value":"x"}"#,
    );
    assert!(!evaluate(&missing, Some(&tree)).passed);
}

#[test]
fn checkbox_state_reads_checked_unchecked_and_refuses_mixed() {
    let tree = text_edit();
    let on = check(r#"{"check":"checkbox_state","title":"Wrap lines","checked":true}"#);
    assert!(evaluate(&on, Some(&tree)).passed);
    let off = check(r#"{"check":"checkbox_state","title":"Wrap lines","checked":false}"#);
    assert!(!evaluate(&off, Some(&tree)).passed);
    let mixed = check(r#"{"check":"checkbox_state","title":"Mixed","checked":false}"#);
    let verdict = evaluate(&mixed, Some(&tree));
    assert!(!verdict.passed);
    assert!(verdict.observed.contains("mixed"), "{}", verdict.observed);
}

#[test]
fn menu_item_enabled_walks_the_menu_path() {
    let tree = text_edit();
    let close = check(r#"{"check":"menu_item_enabled","menu":["File","Close"]}"#);
    assert!(evaluate(&close, Some(&tree)).passed);
    let save = check(r#"{"check":"menu_item_enabled","menu":["File","Save"]}"#);
    assert!(!evaluate(&save, Some(&tree)).passed);
    let save_disabled =
        check(r#"{"check":"menu_item_enabled","menu":["File","Save"],"enabled":false}"#);
    assert!(evaluate(&save_disabled, Some(&tree)).passed);
    let unreported = check(r#"{"check":"menu_item_enabled","menu":["File","Export"]}"#);
    let verdict = evaluate(&unreported, Some(&tree));
    assert!(!verdict.passed);
    assert!(
        verdict.observed.contains("not reported"),
        "{}",
        verdict.observed
    );
    let missing = check(r#"{"check":"menu_item_enabled","menu":["Edit","Copy"]}"#);
    let verdict = evaluate(&missing, Some(&tree));
    assert!(!verdict.passed);
    assert!(verdict.observed.contains("Edit"), "{}", verdict.observed);
    assert!(
        verdict.observed.contains("File"),
        "names the entries that exist: {}",
        verdict.observed
    );
}

#[test]
fn file_exists_needs_no_snapshot() {
    let dir = std::env::temp_dir().join(format!("vts-a11y-eval-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let present = dir.join("out.txt");
    fs::write(&present, "x").unwrap();
    let hit = check(&serde_json::json!({"check":"file_exists","path":present}).to_string());
    assert!(evaluate(&hit, None).passed);
    let missing = dir.join("absent.txt");
    let miss = check(&serde_json::json!({"check":"file_exists","path":missing}).to_string());
    let verdict = evaluate(&miss, None);
    assert!(!verdict.passed);
    assert!(verdict.observed.contains("absent.txt"));
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_gui_check_without_a_snapshot_fails_loudly_instead_of_passing() {
    let verdict = evaluate(
        &check(r#"{"check":"window_title_contains","text":"x"}"#),
        None,
    );
    assert!(!verdict.passed);
    assert!(
        verdict.observed.contains("snapshot"),
        "{}",
        verdict.observed
    );
}
