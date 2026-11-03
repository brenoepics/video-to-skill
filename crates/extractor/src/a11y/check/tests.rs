use super::{Check, Target};
use crate::a11y::tree::Scalar;

fn parse(json: &str) -> Check {
    Check::parse(json).unwrap_or_else(|e| panic!("{json} should parse: {e:#}"))
}

#[test]
fn every_check_kind_parses_from_its_tagged_json() {
    assert_eq!(
        parse(r#"{"check":"window_title_contains","app":"TextEdit","text":"Untitled"}"#),
        Check::WindowTitleContains {
            app: Some("TextEdit".into()),
            text: "Untitled".into()
        }
    );
    assert_eq!(
        parse(r#"{"check":"element_exists","role":"AXButton","title":"Save"}"#),
        Check::ElementExists {
            app: None,
            role: "AXButton".into(),
            title: Some("Save".into()),
            identifier: None
        }
    );
    assert_eq!(
        parse(
            r#"{"check":"element_value_equals","role":"AXTextArea","identifier":"body","value":"hello"}"#
        ),
        Check::ElementValueEquals {
            app: None,
            role: "AXTextArea".into(),
            title: None,
            identifier: Some("body".into()),
            value: Scalar::Text("hello".into())
        }
    );
    assert_eq!(
        parse(r#"{"check":"checkbox_state","title":"Wrap lines","checked":true}"#),
        Check::CheckboxState {
            app: None,
            title: Some("Wrap lines".into()),
            identifier: None,
            checked: true
        }
    );
    assert_eq!(
        parse(r#"{"check":"file_exists","path":"/tmp/out.txt"}"#),
        Check::FileExists {
            path: "/tmp/out.txt".into()
        }
    );
    assert_eq!(
        parse(r#"{"check":"menu_item_enabled","app":"TextEdit","menu":["File","Save"]}"#),
        Check::MenuItemEnabled {
            app: Some("TextEdit".into()),
            menu: vec!["File".into(), "Save".into()],
            enabled: true
        }
    );
}

#[test]
fn values_keep_their_json_type() {
    let Check::ElementValueEquals { value, .. } =
        parse(r#"{"check":"element_value_equals","role":"AXSlider","title":"Volume","value":0.5}"#)
    else {
        panic!("wrong variant");
    };
    assert_eq!(value, Scalar::Number(0.5));
    let Check::ElementValueEquals { value, .. } =
        parse(r#"{"check":"element_value_equals","role":"AXCheckBox","title":"On","value":true}"#)
    else {
        panic!("wrong variant");
    };
    assert_eq!(value, Scalar::Bool(true));
}

#[test]
fn unknown_kinds_and_unknown_fields_are_rejected() {
    let unknown_kind = Check::parse(r#"{"check":"click_button","title":"OK"}"#).unwrap_err();
    assert!(
        format!("{unknown_kind:#}").contains("click_button"),
        "{unknown_kind:#}"
    );
    let unknown_field =
        Check::parse(r#"{"check":"file_exists","path":"/x","contents":"y"}"#).unwrap_err();
    assert!(
        format!("{unknown_field:#}").contains("contents"),
        "{unknown_field:#}"
    );
    assert!(Check::parse("not json").is_err());
}

#[test]
fn element_locators_need_a_title_or_an_identifier() {
    let error = Check::parse(r#"{"check":"element_exists","role":"AXButton"}"#).unwrap_err();
    assert!(format!("{error:#}").contains("title"), "{error:#}");
    assert!(format!("{error:#}").contains("identifier"), "{error:#}");
    assert!(Check::parse(r#"{"check":"checkbox_state","checked":true}"#).is_err());
    assert!(
        Check::parse(r#"{"check":"element_value_equals","role":"AXTextField","value":"x"}"#)
            .is_err()
    );
}

#[test]
fn empty_text_paths_and_menu_paths_are_rejected() {
    assert!(Check::parse(r#"{"check":"window_title_contains","text":""}"#).is_err());
    assert!(Check::parse(r#"{"check":"window_title_contains","text":"   "}"#).is_err());
    assert!(Check::parse(r#"{"check":"file_exists","path":""}"#).is_err());
    assert!(Check::parse(r#"{"check":"menu_item_enabled","menu":[]}"#).is_err());
    assert!(Check::parse(r#"{"check":"menu_item_enabled","menu":["File",""]}"#).is_err());
    assert!(Check::parse(r#"{"check":"element_exists","role":"","title":"x"}"#).is_err());
}

#[test]
fn parse_many_accepts_one_object_or_an_array() {
    let one = Check::parse_many(r#"{"check":"file_exists","path":"/a"}"#).unwrap();
    assert_eq!(one.len(), 1);
    let many = Check::parse_many(
        r#"[{"check":"file_exists","path":"/a"},{"check":"window_title_contains","text":"b"}]"#,
    )
    .unwrap();
    assert_eq!(many.len(), 2);
    assert!(
        Check::parse_many("[]").is_err(),
        "an empty list checks nothing"
    );
    assert!(Check::parse_many("42").is_err());
}

#[test]
fn checks_round_trip_through_json() {
    let json = r#"[{"check":"window_title_contains","app":"TextEdit","text":"Untitled"},{"check":"element_exists","role":"AXButton","title":"Save"},{"check":"element_value_equals","role":"AXTextArea","identifier":"body","value":"hello"},{"check":"checkbox_state","title":"Wrap","checked":false},{"check":"file_exists","path":"/tmp/x"},{"check":"menu_item_enabled","menu":["File","Save"],"enabled":false}]"#;
    let checks = Check::parse_many(json).unwrap();
    let again = Check::parse_many(&serde_json::to_string(&checks).unwrap()).unwrap();
    assert_eq!(checks, again);
    let rendered = serde_json::to_value(&checks[1]).unwrap();
    assert_eq!(rendered["check"], "element_exists");
    assert!(
        rendered.get("app").is_none(),
        "absent optionals are not rendered as null: {rendered}"
    );
}

#[test]
fn targets_separate_filesystem_checks_from_gui_checks() {
    let file = parse(r#"{"check":"file_exists","path":"/tmp/out.txt"}"#);
    assert!(matches!(file.target(), Target::File(p) if p.ends_with("out.txt")));
    let gui = parse(r#"{"check":"window_title_contains","app":"TextEdit","text":"x"}"#);
    assert!(matches!(gui.target(), Target::Gui(Some("TextEdit"))));
    let frontmost = parse(r#"{"check":"menu_item_enabled","menu":["File"]}"#);
    assert!(matches!(frontmost.target(), Target::Gui(None)));
    assert_eq!(file.kind(), "file_exists");
    assert_eq!(gui.kind(), "window_title_contains");
}
