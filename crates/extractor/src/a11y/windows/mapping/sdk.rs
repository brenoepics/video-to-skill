//! On Windows, the hand-written tables in `mapping` are checked against
//! the SDK's own constants, so a typo in an id cannot pass on macOS and
//! Linux alone.

use ::windows::Win32::Foundation::{CO_E_NOTINITIALIZED, E_ACCESSDENIED, REGDB_E_CLASSNOTREG};
use ::windows::Win32::UI::Accessibility as uia;

use super::{control_type_name, describe_failure, toggle_reading};
use crate::a11y::tree::Scalar;

#[test]
fn every_control_type_name_matches_its_sdk_id() {
    for (id, name) in [
        (uia::UIA_ButtonControlTypeId, "Button"),
        (uia::UIA_CalendarControlTypeId, "Calendar"),
        (uia::UIA_CheckBoxControlTypeId, "CheckBox"),
        (uia::UIA_ComboBoxControlTypeId, "ComboBox"),
        (uia::UIA_EditControlTypeId, "Edit"),
        (uia::UIA_HyperlinkControlTypeId, "Hyperlink"),
        (uia::UIA_ImageControlTypeId, "Image"),
        (uia::UIA_ListItemControlTypeId, "ListItem"),
        (uia::UIA_ListControlTypeId, "List"),
        (uia::UIA_MenuControlTypeId, "Menu"),
        (uia::UIA_MenuBarControlTypeId, "MenuBar"),
        (uia::UIA_MenuItemControlTypeId, "MenuItem"),
        (uia::UIA_ProgressBarControlTypeId, "ProgressBar"),
        (uia::UIA_RadioButtonControlTypeId, "RadioButton"),
        (uia::UIA_ScrollBarControlTypeId, "ScrollBar"),
        (uia::UIA_SliderControlTypeId, "Slider"),
        (uia::UIA_SpinnerControlTypeId, "Spinner"),
        (uia::UIA_StatusBarControlTypeId, "StatusBar"),
        (uia::UIA_TabControlTypeId, "Tab"),
        (uia::UIA_TabItemControlTypeId, "TabItem"),
        (uia::UIA_TextControlTypeId, "Text"),
        (uia::UIA_ToolBarControlTypeId, "ToolBar"),
        (uia::UIA_ToolTipControlTypeId, "ToolTip"),
        (uia::UIA_TreeControlTypeId, "Tree"),
        (uia::UIA_TreeItemControlTypeId, "TreeItem"),
        (uia::UIA_CustomControlTypeId, "Custom"),
        (uia::UIA_GroupControlTypeId, "Group"),
        (uia::UIA_ThumbControlTypeId, "Thumb"),
        (uia::UIA_DataGridControlTypeId, "DataGrid"),
        (uia::UIA_DataItemControlTypeId, "DataItem"),
        (uia::UIA_DocumentControlTypeId, "Document"),
        (uia::UIA_SplitButtonControlTypeId, "SplitButton"),
        (uia::UIA_WindowControlTypeId, "Window"),
        (uia::UIA_PaneControlTypeId, "Pane"),
        (uia::UIA_HeaderControlTypeId, "Header"),
        (uia::UIA_HeaderItemControlTypeId, "HeaderItem"),
        (uia::UIA_TableControlTypeId, "Table"),
        (uia::UIA_TitleBarControlTypeId, "TitleBar"),
        (uia::UIA_SeparatorControlTypeId, "Separator"),
        (uia::UIA_SemanticZoomControlTypeId, "SemanticZoom"),
        (uia::UIA_AppBarControlTypeId, "AppBar"),
    ] {
        assert_eq!(control_type_name(id.0), name, "{}", id.0);
    }
}

#[test]
fn toggle_states_match_the_sdk() {
    assert_eq!(
        toggle_reading(uia::ToggleState_Off.0),
        Some(Scalar::Number(0.0))
    );
    assert_eq!(
        toggle_reading(uia::ToggleState_On.0),
        Some(Scalar::Number(1.0))
    );
    assert_eq!(
        toggle_reading(uia::ToggleState_Indeterminate.0),
        Some(Scalar::Number(2.0))
    );
}

#[test]
fn every_explained_hresult_is_the_sdk_code() {
    for code in [
        uia::UIA_E_ELEMENTNOTAVAILABLE.cast_signed(),
        uia::UIA_E_TIMEOUT.cast_signed(),
        E_ACCESSDENIED.0,
        REGDB_E_CLASSNOTREG.0,
        CO_E_NOTINITIALIZED.0,
    ] {
        let text = describe_failure("x", code, "system text");
        assert!(!text.contains("system text"), "explained: {text}");
    }
}
