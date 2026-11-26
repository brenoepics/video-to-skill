use super::{
    control_type_name, describe_failure, element_value, exe_stem, is_process_named, sole_pid,
    toggle_reading, wide_to_string, PatternReadings,
};
use crate::a11y::tree::{normalize_role, Scalar};
use crate::a11y::windows::MESSAGING_TIMEOUT_MS;

#[test]
fn control_types_are_their_programmatic_names() {
    for (id, name) in [
        (50_000, "Button"),
        (50_002, "CheckBox"),
        (50_004, "Edit"),
        (50_009, "Menu"),
        (50_010, "MenuBar"),
        (50_011, "MenuItem"),
        (50_020, "Text"),
        (50_030, "Document"),
        (50_032, "Window"),
        (50_033, "Pane"),
        (50_037, "TitleBar"),
        (50_040, "AppBar"),
    ] {
        assert_eq!(control_type_name(id), name, "{id}");
    }
}

#[test]
fn unknown_control_types_keep_their_id() {
    assert_eq!(control_type_name(49_999), "ControlType49999");
    assert_eq!(control_type_name(50_041), "ControlType50041");
    assert_eq!(
        control_type_name(i32::MIN),
        format!("ControlType{}", i32::MIN)
    );
    assert_eq!(control_type_name(0), "ControlType0");
}

#[test]
fn uia_roles_normalise_to_the_macos_spellings() {
    for (uia, ax) in [
        (50_000, "AXButton"),
        (50_002, "AXCheckBox"),
        (50_011, "AXMenuItem"),
        (50_010, "AXMenuBar"),
        (50_032, "AXWindow"),
    ] {
        assert_eq!(
            normalize_role(&control_type_name(uia)),
            normalize_role(ax),
            "{uia}"
        );
    }
}

#[test]
fn toggle_states_read_as_the_numbers_macos_reports() {
    assert_eq!(toggle_reading(0), Some(Scalar::Number(0.0)));
    assert_eq!(toggle_reading(1), Some(Scalar::Number(1.0)));
    assert_eq!(toggle_reading(2), Some(Scalar::Number(2.0)));
    assert_eq!(toggle_reading(3), None);
    assert_eq!(toggle_reading(-1), None);
    // The evaluator's checkbox rule then reads them like AXValue.
    assert_eq!(toggle_reading(1).and_then(|s| s.as_checked()), Some(true));
    assert_eq!(toggle_reading(0).and_then(|s| s.as_checked()), Some(false));
    assert_eq!(toggle_reading(2).and_then(|s| s.as_checked()), None);
}

#[test]
fn a_toggle_state_wins_over_text_and_range_readings() {
    let readings = PatternReadings {
        toggle: Some(1),
        text: Some("on".into()),
        range: Some(4.0),
    };
    assert_eq!(element_value(readings), Some(Scalar::Number(1.0)));
}

#[test]
fn value_text_wins_over_a_range_and_is_kept_even_when_empty() {
    let readings = PatternReadings {
        toggle: None,
        text: Some("hello".into()),
        range: Some(4.0),
    };
    assert_eq!(element_value(readings), Some(Scalar::Text("hello".into())));
    let empty = PatternReadings {
        text: Some(String::new()),
        ..PatternReadings::default()
    };
    assert_eq!(element_value(empty), Some(Scalar::Text(String::new())));
}

#[test]
fn a_range_reading_is_a_number_when_finite() {
    let slider = PatternReadings {
        range: Some(42.5),
        ..PatternReadings::default()
    };
    assert_eq!(element_value(slider), Some(Scalar::Number(42.5)));
    let broken = PatternReadings {
        range: Some(f64::NAN),
        ..PatternReadings::default()
    };
    assert_eq!(element_value(broken), None);
    assert_eq!(element_value(PatternReadings::default()), None);
}

#[test]
fn an_unknown_toggle_state_falls_through_to_the_other_readings() {
    let readings = PatternReadings {
        toggle: Some(7),
        text: Some("x".into()),
        range: None,
    };
    assert_eq!(element_value(readings), Some(Scalar::Text("x".into())));
}

#[test]
fn exe_suffixes_are_optional_and_case_blind() {
    assert_eq!(exe_stem("Notepad.exe"), "Notepad");
    assert_eq!(exe_stem("notepad.EXE"), "notepad");
    assert_eq!(exe_stem("System"), "System");
    assert_eq!(exe_stem(".exe"), ".exe");
    assert_eq!(exe_stem("ñ.exe"), "ñ");
    assert!(is_process_named("Notepad.exe", "notepad"));
    assert!(is_process_named("Notepad.exe", "NOTEPAD.EXE"));
    assert!(is_process_named("notepad++.exe", "notepad++"));
    assert!(is_process_named("System", "system"));
    assert!(is_process_named("explorer.exe", " explorer "));
    assert!(!is_process_named("notepad++.exe", "notepad"));
    assert!(!is_process_named("Notepad.exe", "Note"));
}

#[test]
fn wide_names_end_at_the_first_nul() {
    let mut buffer = [0_u16; 16];
    for (slot, unit) in buffer.iter_mut().zip("Notepad.exe".encode_utf16()) {
        *slot = unit;
    }
    assert_eq!(wide_to_string(&buffer), "Notepad.exe");
    let full: Vec<u16> = "abc".encode_utf16().collect();
    assert_eq!(wide_to_string(&full), "abc", "no NUL: the whole buffer");
    assert_eq!(wide_to_string(&[]), "");
}

#[test]
fn a_pid_is_reported_only_when_one_process_owns_every_window() {
    assert_eq!(sole_pid(&[42, 42, 42]), Some(42));
    assert_eq!(sole_pid(&[42]), Some(42));
    assert_eq!(sole_pid(&[42, 43]), None);
    assert_eq!(sole_pid(&[]), None);
}

#[test]
fn known_failures_are_explained_with_their_hresult() {
    // UIA_E_ELEMENTNOTAVAILABLE
    let gone = describe_failure("reading Name", 0x8004_0201_u32.cast_signed(), "");
    assert!(gone.starts_with("reading Name: "), "{gone}");
    assert!(gone.contains("did the application quit"), "{gone}");
    assert!(gone.ends_with("(HRESULT 0x80040201)"), "{gone}");
    // UIA_E_TIMEOUT
    let slow = describe_failure("listing windows", 0x8013_1505_u32.cast_signed(), "");
    let within = format!("did not answer within {} s", MESSAGING_TIMEOUT_MS / 1000);
    assert!(
        slow.contains(&within),
        "the wording follows the timeout: {slow}"
    );
    // E_ACCESSDENIED
    let denied = describe_failure("listing windows", 0x8007_0005_u32.cast_signed(), "Denied");
    assert!(denied.contains("elevated"), "{denied}");
    // REGDB_E_CLASSNOTREG
    let missing = describe_failure("creating", 0x8004_0154_u32.cast_signed(), "");
    assert!(missing.contains("not registered"), "{missing}");
}

#[test]
fn unknown_failures_keep_the_system_message() {
    let other = describe_failure(
        "listing windows",
        0x8000_4005_u32.cast_signed(),
        "Unspecified",
    );
    assert_eq!(
        other,
        "listing windows failed: Unspecified (HRESULT 0x80004005)"
    );
    let silent = describe_failure("listing windows", 0x8000_4005_u32.cast_signed(), "  ");
    assert_eq!(silent, "listing windows failed (HRESULT 0x80004005)");
}
