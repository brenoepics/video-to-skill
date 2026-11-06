use super::refuse_unobservable;
use crate::a11y::check::Check;

fn checks(json: &str) -> Vec<Check> {
    Check::parse_many(json).unwrap()
}

#[test]
fn a_submenu_path_is_refused_as_not_observable() {
    let error = refuse_unobservable(&checks(
        r#"{"check":"menu_item_enabled","app":"Notepad","menu":["File","Save"]}"#,
    ))
    .unwrap_err();
    let text = format!("{error:#}");
    assert!(
        text.starts_with("menu_item_enabled: could not observe Notepad: "),
        "{text}"
    );
    assert!(text.contains("File > Save"), "{text}");
    assert!(text.contains("not observable on Windows"), "{text}");
    assert!(text.contains("only while"), "says why: {text}");
    assert!(text.contains("unverifiable"), "says what to record: {text}");
}

#[test]
fn the_refusal_names_the_frontmost_application_when_no_app_is_given() {
    let error = refuse_unobservable(&checks(
        r#"{"check":"menu_item_enabled","menu":["Edit","Undo","Again"],"enabled":false}"#,
    ))
    .unwrap_err();
    assert!(
        format!("{error:#}").contains("could not observe the frontmost application"),
        "{error:#}"
    );
}

#[test]
fn a_menu_bar_entry_and_every_other_check_are_observable() {
    let all = checks(
        r#"[
            {"check":"menu_item_enabled","app":"Notepad","menu":["File"]},
            {"check":"window_title_contains","app":"Notepad","text":"Untitled"},
            {"check":"element_exists","role":"Button","title":"Save"},
            {"check":"element_value_equals","role":"Edit","identifier":"15","value":"x"},
            {"check":"checkbox_state","title":"Word wrap","checked":true},
            {"check":"file_exists","path":"C:\\out.txt"}
        ]"#,
    );
    refuse_unobservable(&all).unwrap();
}

#[test]
fn one_unobservable_check_refuses_the_whole_observation() {
    let mixed = checks(
        r#"[
            {"check":"window_title_contains","app":"Notepad","text":"Untitled"},
            {"check":"menu_item_enabled","app":"Notepad","menu":["File","Save"]}
        ]"#,
    );
    assert!(refuse_unobservable(&mixed).is_err());
}
