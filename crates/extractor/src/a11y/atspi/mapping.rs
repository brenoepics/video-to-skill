//! The pure half of the AT-SPI backend: how AT-SPI readings become the
//! neutral tree's roles and values, how applications match, and how a
//! failed call is worded. No bus is touched here, so these rules are
//! tested on every OS, not only on Linux.

use crate::a11y::tree::Scalar;

#[cfg(test)]
mod tests;

/// `AtspiStateType` bit numbers (at-spi2-core `atspi-constants.h`).
pub mod state {
    pub const ACTIVE: u32 = 1;
    pub const CHECKED: u32 = 4;
    pub const ENABLED: u32 = 8;
    pub const MULTI_LINE: u32 = 17;
    pub const PRESSED: u32 = 20;
    pub const SENSITIVE: u32 = 24;
    pub const VISIBLE: u32 = 30;
    pub const INDETERMINATE: u32 = 32;
}

/// The longest text value kept, in characters. A longer text (a whole
/// document) is not reported as a value at all: a cut-off string could
/// never honestly equal what a check expects.
pub const MAX_TEXT_CHARS: usize = 16_384;

/// An element's state set, as `GetState` returns it (two `u32` words).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct States(u64);

impl States {
    #[must_use]
    pub fn from_words(words: &[u32]) -> Self {
        let low = words.first().copied().unwrap_or(0);
        let high = words.get(1).copied().unwrap_or(0);
        Self(u64::from(high) << 32 | u64::from(low))
    }

    #[must_use]
    pub fn has(self, bit: u32) -> bool {
        bit < 64 && self.0 & (1 << bit) != 0
    }

    /// GTK removes `ENABLED` from the entries of a closed menu while
    /// keeping `SENSITIVE`, and removes both from an insensitive widget,
    /// so either one means the element can be used.
    #[must_use]
    pub fn enabled(self) -> bool {
        self.has(state::ENABLED) || self.has(state::SENSITIVE)
    }

    /// [`Self::enabled`] as the tree's `enabled` field: `None` for an
    /// empty set, which an application's root reports and which says
    /// nothing either way.
    #[must_use]
    pub fn enabled_reading(self) -> Option<bool> {
        (self.0 != 0).then(|| self.enabled())
    }
}

/// The neutral role for an AT-SPI role name (`GetRoleName`, which is not
/// localised). Roles with a counterpart on the other backends take its
/// spelling — `push button` is `Button`, `frame` and `dialog` are
/// `Window`, `text` is `TextField` — so one check reads the same
/// everywhere; the rest are the AT-SPI name in CamelCase (`scroll pane`
/// → `ScrollPane`). A GTK menu bar entry and a submenu are both `menu`
/// in AT-SPI; under a menu bar or a menu it is a menu entry, as on macOS.
#[must_use]
pub fn role(atspi: &str, parent: Option<&str>, states: States) -> String {
    let mapped = match (atspi, parent) {
        ("frame" | "window" | "dialog" | "alert", _) => "Window",
        ("push button", _) => "Button",
        ("check box", _) => "CheckBox",
        ("radio button", _) => "RadioButton",
        ("menu bar", _) => "MenuBar",
        ("menu", Some("menu bar")) => "MenuBarItem",
        ("menu", Some("menu"))
        | ("menu item" | "check menu item" | "radio menu item" | "tearoff menu item", _) => {
            "MenuItem"
        }
        ("label", _) => "StaticText",
        ("text" | "entry", _) if states.has(state::MULTI_LINE) => "TextArea",
        ("text" | "entry", _) => "TextField",
        _ => return camel_case(atspi),
    };
    mapped.to_string()
}

fn camel_case(name: &str) -> String {
    let out: String = name
        .split([' ', '_', '-'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().chain(chars).collect::<String>()
            })
        })
        .collect();
    if out.is_empty() {
        "Unknown".to_string()
    } else {
        out
    }
}

/// A toggle's state as the numbers macOS and UI Automation report — 0
/// off, 1 on, 2 mixed — so `checkbox_state` reads the same everywhere.
/// `None` for roles that do not toggle.
#[must_use]
pub fn toggle_value(atspi: &str, states: States) -> Option<Scalar> {
    let toggles = matches!(
        atspi,
        "check box"
            | "toggle button"
            | "radio button"
            | "check menu item"
            | "radio menu item"
            | "switch"
    );
    if !toggles {
        return None;
    }
    let on = states.has(state::CHECKED) || (atspi == "toggle button" && states.has(state::PRESSED));
    let reading = if states.has(state::INDETERMINATE) {
        2.0
    } else if on {
        1.0
    } else {
        0.0
    };
    Some(Scalar::Number(reading))
}

/// Whether an element's text may be read as its value: only when its
/// role was read and is neither a password field nor unknown. A role
/// that could not be read might be a password field, so this fails
/// closed.
#[must_use]
pub fn reads_text(atspi_role: Option<&str>) -> bool {
    atspi_role
        .is_some_and(|role| !matches!(role.trim(), "" | "unknown" | "invalid" | "password text"))
}

/// A text read with `GetText(0, MAX_TEXT_CHARS + 1)`: kept only when
/// it is the whole text.
#[must_use]
pub fn text_value(text: String) -> Option<Scalar> {
    (text.chars().count() <= MAX_TEXT_CHARS).then_some(Scalar::Text(text))
}

/// Whether an application known by `names` (its process name, the
/// basename of its `argv[0]`, its accessible name) is the one a check
/// names: case-insensitive, surrounding spaces in `wanted` ignored.
#[must_use]
pub fn is_app_named(wanted: &str, names: &[&str]) -> bool {
    let wanted = wanted.trim().to_lowercase();
    !wanted.is_empty() && names.iter().any(|name| name.to_lowercase() == wanted)
}

fn remote_meaning(name: &str) -> Option<&'static str> {
    let suffix = name.strip_prefix("org.freedesktop.DBus.Error.")?;
    Some(match suffix {
        "ServiceUnknown" | "NameHasNoOwner" => {
            "the application left the accessibility bus (did it quit?)"
        }
        "NoReply" | "Timeout" | "TimedOut" => {
            "the application did not answer in time or does not support AT-SPI"
        }
        "UnknownObject" | "UnknownMethod" | "UnknownInterface" => {
            "the element is gone or does not implement AT-SPI"
        }
        "AccessDenied" => "the bus policy denied the request",
        _ => return None,
    })
}

/// The sentence `observe` prints when an AT-SPI call fails while
/// `doing` something: a known error is explained, any other keeps the
/// application's message; both end with the error name.
#[must_use]
pub fn describe_remote(doing: &str, name: &str, message: &str) -> String {
    match remote_meaning(name) {
        Some(meaning) => format!("{doing}: {meaning} ({name})"),
        None if message.trim().is_empty() => format!("{doing} failed ({name})"),
        None => format!("{doing} failed: {} ({name})", message.trim()),
    }
}
