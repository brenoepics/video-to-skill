//! Repairs recorded in a step's caveats must cite the external signal
//! that motivated them. Intrinsic self-correction — a verifier changing
//! a step because it "looks wrong" — degrades skills; a repair earns its
//! place only by pointing at what the environment said: an exit code,
//! output, or a file/state check (references/verification-protocol.md).

/// The signal kinds a repair may cite: what the text after `Signal:`
/// must begin with (case-insensitive, followed by `:`, whitespace or the
/// end). `exit code` must be followed by the number.
pub const SIGNAL_KINDS: &[&str] = &[
    "exit code",
    "stderr",
    "stdout",
    "output diff",
    "file",
    "state",
];

/// The external signals cited by the repairs recorded in `caveats`, one
/// per `Repair …:` line, in order. `Err` carries the first repair line
/// that cites none.
pub fn cited_signals(caveats: &str) -> Result<Vec<String>, String> {
    let mut signals = Vec::new();
    for line in caveats.lines().map(str::trim).filter(|l| is_repair_line(l)) {
        match signal_of(line) {
            Some(signal) => signals.push(signal),
            None => return Err(line.to_string()),
        }
    }
    Ok(signals)
}

/// `Repair:`, `Repair 2:`, `repair #1:` … — a repair record, as opposed
/// to prose that merely mentions repairing something.
fn is_repair_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("repair") else {
        return false;
    };
    rest.trim_start()
        .trim_start_matches(|c: char| c.is_ascii_digit() || c == '#')
        .trim_start()
        .starts_with(':')
}

/// The text after `Signal:` when it names one of [`SIGNAL_KINDS`].
fn signal_of(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();
    let at = lower.find("signal:")? + "signal:".len();
    // ASCII lowercasing keeps byte offsets, so `at` indexes `line` too.
    let cited = line[at..].trim();
    let cited_lower = cited.to_ascii_lowercase();
    let kind = SIGNAL_KINDS.iter().find(|kind| {
        cited_lower.strip_prefix(*kind).is_some_and(|after| {
            after.is_empty() || after.starts_with(':') || after.starts_with(char::is_whitespace)
        })
    })?;
    if *kind == "exit code"
        && !cited_lower[kind.len()..]
            .trim_start()
            .starts_with(|c: char| c.is_ascii_digit())
    {
        return None;
    }
    Some(cited.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repair_records_are_recognised_and_prose_is_not() {
        for line in ["Repair: x", "Repair 2: x", "repair #1: x", "REPAIR 10 : x"] {
            assert!(is_repair_line(line), "{line:?}");
        }
        for line in [
            "Repairs to the swap file are in :help recover",
            "Do not repair the config by hand",
            "The repair: never mind",
        ] {
            assert!(!is_repair_line(line), "{line:?}");
        }
    }

    #[test]
    fn signals_need_a_known_kind_and_exit_codes_need_a_number() {
        assert_eq!(
            signal_of("Repair 1: x. Signal: exit code 1 — E492").as_deref(),
            Some("exit code 1 — E492")
        );
        assert_eq!(
            signal_of("Repair 1: x. signal: File: notes.txt missing").as_deref(),
            Some("File: notes.txt missing")
        );
        assert!(signal_of("Repair 1: x. Signal: exit code").is_none());
        assert!(signal_of("Repair 1: x. Signal: statement of belief").is_none());
        assert!(signal_of("Repair 1: x, no signal at all").is_none());
    }

    #[test]
    fn cited_signals_collects_every_repair_line_in_order() {
        let caveats = "Keys unverified.\nRepair 1: a. Signal: stderr: E492\n\
                       Repair 2: b. Signal: state: still modified";
        assert_eq!(
            cited_signals(caveats).unwrap(),
            vec!["stderr: E492", "state: still modified"]
        );
        assert_eq!(
            cited_signals("Repair 1: looked wrong").unwrap_err(),
            "Repair 1: looked wrong"
        );
        assert_eq!(
            cited_signals("no repairs here").unwrap(),
            Vec::<String>::new()
        );
    }
}
