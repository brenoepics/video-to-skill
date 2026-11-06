//! The pure half of the Windows backend: how UI Automation readings
//! become the neutral tree's roles and values, how process names match,
//! and how a failed call is worded. No Windows API is touched here, so
//! these rules are tested on every OS, not only on a Windows runner.

use crate::a11y::tree::Scalar;

#[cfg(test)]
#[cfg(windows)]
mod sdk;
#[cfg(test)]
mod tests;

/// The first UI Automation control type id (`UIA_ButtonControlTypeId`).
const FIRST_CONTROL_TYPE: i32 = 50_000;

/// Programmatic control type names in id order, 50000 (`Button`) to
/// 50040 (`AppBar`), as the SDK's `UIA_*ControlTypeId` constants spell
/// them. The programmatic name — not `LocalizedControlType` — is the role:
/// it is the same in every display language, and it already normalises
/// against the macOS spellings (`CheckBox` ↔ `AXCheckBox`).
const CONTROL_TYPES: [&str; 41] = [
    "Button",
    "Calendar",
    "CheckBox",
    "ComboBox",
    "Edit",
    "Hyperlink",
    "Image",
    "ListItem",
    "List",
    "Menu",
    "MenuBar",
    "MenuItem",
    "ProgressBar",
    "RadioButton",
    "ScrollBar",
    "Slider",
    "Spinner",
    "StatusBar",
    "Tab",
    "TabItem",
    "Text",
    "ToolBar",
    "ToolTip",
    "Tree",
    "TreeItem",
    "Custom",
    "Group",
    "Thumb",
    "DataGrid",
    "DataItem",
    "Document",
    "SplitButton",
    "Window",
    "Pane",
    "Header",
    "HeaderItem",
    "Table",
    "TitleBar",
    "Separator",
    "SemanticZoom",
    "AppBar",
];

/// The role for a `ControlType` id; an id outside the table keeps its
/// number so a dump still says what was reported.
#[must_use]
pub fn control_type_name(id: i32) -> String {
    id.checked_sub(FIRST_CONTROL_TYPE)
        .and_then(|offset| usize::try_from(offset).ok())
        .and_then(|index| CONTROL_TYPES.get(index))
        .map_or_else(|| format!("ControlType{id}"), |name| (*name).to_string())
}

/// `ToggleState_Off` (0), `_On` (1) and `_Indeterminate` (2) as the
/// numbers macOS reports for a checkbox's `AXValue`, so one
/// `checkbox_state` or `element_value_equals` check reads the same on
/// both platforms; any other state is no reading.
#[must_use]
pub fn toggle_reading(state: i32) -> Option<Scalar> {
    (0..=2)
        .contains(&state)
        .then(|| Scalar::Number(f64::from(state)))
}

/// What the cache holds for one element's value-bearing patterns.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct PatternReadings {
    /// `Toggle.ToggleState`, when the element supports the pattern.
    pub toggle: Option<i32>,
    /// `Value.Value` (text, possibly empty).
    pub text: Option<String>,
    /// `RangeValue.Value` (sliders, progress bars, spinners).
    pub range: Option<f64>,
}

/// One value per element: a toggle state wins (a checkbox's state is its
/// value, as on macOS), then the `Value` pattern's text, then a finite
/// `RangeValue` number.
#[must_use]
pub fn element_value(readings: PatternReadings) -> Option<Scalar> {
    readings
        .toggle
        .and_then(toggle_reading)
        .or_else(|| readings.text.map(Scalar::Text))
        .or_else(|| {
            readings
                .range
                .filter(|number| number.is_finite())
                .map(Scalar::Number)
        })
}

/// `Notepad.exe` → `Notepad`; a name without the suffix is unchanged.
#[must_use]
pub fn exe_stem(name: &str) -> &str {
    const SUFFIX: &str = ".exe";
    let cut = name.len().saturating_sub(SUFFIX.len());
    match (name.get(..cut), name.get(cut..)) {
        (Some(stem), Some(suffix)) if !stem.is_empty() && suffix.eq_ignore_ascii_case(SUFFIX) => {
            stem
        }
        _ => name,
    }
}

/// Whether the executable `exe` (as Toolhelp32 reports it) is the
/// application a check names: case-insensitive, `.exe` optional on
/// either side, surrounding spaces in `wanted` ignored.
#[must_use]
pub fn is_process_named(exe: &str, wanted: &str) -> bool {
    exe_stem(exe).to_lowercase() == exe_stem(wanted.trim()).to_lowercase()
}

/// A NUL-terminated UTF-16 buffer (`PROCESSENTRY32W::szExeFile`) as text.
#[must_use]
pub fn wide_to_string(units: &[u16]) -> String {
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}

/// The pid a snapshot reports: the one process that owns every window
/// read, or `None` when they came from several same-named processes.
#[must_use]
pub fn sole_pid(owners: &[i32]) -> Option<i32> {
    let (first, rest) = owners.split_first()?;
    rest.iter().all(|pid| pid == first).then_some(*first)
}

/// What the HRESULTs an observation can meet mean for the verifier.
fn hresult_meaning(code: u32) -> Option<&'static str> {
    Some(match code {
        // UIA_E_ELEMENTNOTAVAILABLE
        0x8004_0201 => "the element is gone (did the application quit or close the window?)",
        // UIA_E_TIMEOUT
        0x8013_1505 => {
            "the application did not answer within 2 s or does not support UI Automation"
        }
        // E_ACCESSDENIED
        0x8007_0005 => {
            "access denied — the application probably runs elevated and this process does not; \
             run vts-extract at the application's integrity level"
        }
        // REGDB_E_CLASSNOTREG
        0x8004_0154 => "UI Automation is not registered on this system",
        // CO_E_NOTINITIALIZED
        0x8004_01F0 => "COM is not initialised on this thread",
        _ => return None,
    })
}

/// The sentence `observe` prints when a UI Automation call fails while
/// `doing` something: a known HRESULT is explained, any other keeps the
/// system's message; both end with the code.
#[must_use]
pub fn describe_failure(doing: &str, code: i32, message: &str) -> String {
    let code = code.cast_unsigned();
    match hresult_meaning(code) {
        Some(meaning) => format!("{doing}: {meaning} (HRESULT 0x{code:08X})"),
        None if message.trim().is_empty() => format!("{doing} failed (HRESULT 0x{code:08X})"),
        None => format!("{doing} failed: {} (HRESULT 0x{code:08X})", message.trim()),
    }
}
