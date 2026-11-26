use super::{
    describe_remote, is_app_named, reads_text, role, state, text_value, toggle_value, States,
    MAX_TEXT_CHARS,
};
use crate::a11y::tree::{normalize_role, Scalar};

fn states(bits: &[u32]) -> States {
    let mut words = [0u32; 2];
    for &bit in bits {
        words[usize::try_from(bit / 32).unwrap()] |= 1 << (bit % 32);
    }
    States::from_words(&words)
}

#[test]
fn state_words_are_read_as_one_64_bit_set() {
    let set = states(&[state::CHECKED, state::INDETERMINATE]);
    assert!(set.has(state::CHECKED));
    assert!(
        set.has(state::INDETERMINATE),
        "bit 32 lives in the second word"
    );
    assert!(!set.has(state::ACTIVE));
    assert_eq!(States::from_words(&[]), States::default());
}

#[test]
fn enabled_means_sensitive_or_enabled() {
    // GTK drops ENABLED from the items of a closed menu but keeps
    // SENSITIVE; an insensitive widget has neither.
    assert!(states(&[state::SENSITIVE]).enabled());
    assert!(states(&[state::ENABLED, state::SENSITIVE]).enabled());
    assert!(!states(&[state::VISIBLE]).enabled());
    assert_eq!(states(&[state::VISIBLE]).enabled_reading(), Some(false));
    assert_eq!(
        States::default().enabled_reading(),
        None,
        "an empty set (an application's root) says nothing either way"
    );
}

#[test]
fn roles_take_the_spellings_the_other_backends_use() {
    let plain = States::default();
    let cases = [
        ("frame", None, "Window"),
        ("dialog", None, "Window"),
        ("push button", None, "Button"),
        ("check box", None, "CheckBox"),
        ("menu bar", None, "MenuBar"),
        ("menu", Some("menu bar"), "MenuBarItem"),
        ("menu", Some("menu"), "MenuItem"),
        ("menu", Some("window"), "Menu"),
        ("check menu item", Some("menu"), "MenuItem"),
        ("label", None, "StaticText"),
        ("text", None, "TextField"),
        ("entry", None, "TextField"),
        ("scroll pane", None, "ScrollPane"),
        ("application", None, "Application"),
    ];
    for (atspi, parent, expected) in cases {
        assert_eq!(
            role(atspi, parent, plain),
            expected,
            "{atspi} under {parent:?}"
        );
    }
    assert_eq!(role("text", None, states(&[state::MULTI_LINE])), "TextArea");
    // A check written against macOS names finds the Linux element.
    assert_eq!(
        normalize_role(&role("push button", None, plain)),
        normalize_role("AXButton")
    );
    assert_eq!(
        role("password text", None, plain),
        "PasswordText",
        "kept distinct"
    );
    assert_eq!(role("", None, plain), "Unknown");
}

#[test]
fn toggles_report_the_numbers_the_other_backends_report() {
    assert_eq!(
        toggle_value("check box", states(&[state::CHECKED])),
        Some(Scalar::Number(1.0))
    );
    assert_eq!(
        toggle_value("check box", States::default()),
        Some(Scalar::Number(0.0))
    );
    assert_eq!(
        toggle_value("check box", states(&[state::CHECKED, state::INDETERMINATE])),
        Some(Scalar::Number(2.0)),
        "mixed wins"
    );
    assert_eq!(
        toggle_value("toggle button", states(&[state::PRESSED])),
        Some(Scalar::Number(1.0))
    );
    assert_eq!(
        toggle_value("radio menu item", states(&[state::CHECKED])),
        Some(Scalar::Number(1.0))
    );
    assert_eq!(toggle_value("push button", states(&[state::CHECKED])), None);
}

#[test]
fn text_values_are_kept_only_when_complete() {
    assert_eq!(
        text_value("hello".into()),
        Some(Scalar::Text("hello".into()))
    );
    let long = "x".repeat(MAX_TEXT_CHARS + 1);
    assert_eq!(text_value(long), None, "a cut-off text is no reading");
    assert_eq!(
        text_value("é".repeat(MAX_TEXT_CHARS)),
        Some(Scalar::Text("é".repeat(MAX_TEXT_CHARS))),
        "the limit counts characters, not bytes"
    );
}

#[test]
fn applications_match_by_process_or_accessible_name_ignoring_case() {
    let names = ["python3.12", "app.py", "Text Editor"];
    assert!(is_app_named("python3.12", &names));
    assert!(is_app_named("text editor", &names));
    assert!(is_app_named(" app.py ", &names));
    assert!(!is_app_named("python3", &names));
    assert!(!is_app_named("", &[""]));
}

#[test]
fn remote_errors_say_what_they_mean_for_the_verifier() {
    let gone = describe_remote(
        "reading gedit",
        "org.freedesktop.DBus.Error.ServiceUnknown",
        "The name :1.9 was not provided by any .service files",
    );
    assert!(gone.starts_with("reading gedit: "), "{gone}");
    assert!(gone.contains("did it quit"), "{gone}");
    assert!(
        gone.ends_with("(org.freedesktop.DBus.Error.ServiceUnknown)"),
        "{gone}"
    );

    let slow = describe_remote("reading x", "org.freedesktop.DBus.Error.NoReply", "");
    assert!(slow.contains("did not answer"), "{slow}");

    let other = describe_remote("reading x", "com.example.Odd", "it broke");
    assert_eq!(other, "reading x failed: it broke (com.example.Odd)");
}

#[test]
fn text_is_read_only_from_a_known_role_that_is_not_a_password() {
    assert!(reads_text(Some("text")));
    assert!(reads_text(Some("label")));
    assert!(!reads_text(Some("password text")));
    // A role that could not be read might be a password field: fail closed.
    assert!(!reads_text(None));
    assert!(!reads_text(Some("")));
    assert!(!reads_text(Some("unknown")));
}
