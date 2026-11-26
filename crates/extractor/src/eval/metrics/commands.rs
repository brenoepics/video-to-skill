//! Command matching. Gold commands are the backtick spans of a step's
//! `actions`; the transcript proxy asks whether the narration carries
//! each command, comparing lowercase alphanumerics only so "NPM
//! install" and `npm install` agree while `ls -la` stays distinct from
//! `ls`. Spoken punctuation ("notes dot txt") is dropped from the
//! transcript side, since the gold side loses its punctuation too.

use crate::bundle::Segment;

/// Every backtick-delimited span in `markdown`, in order, trimmed.
#[must_use]
pub fn backtick_commands(markdown: &str) -> Vec<String> {
    markdown
        .split('`')
        .skip(1)
        .step_by(2)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Lowercase letters and digits only.
#[must_use]
pub fn normalize(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Words a narrator says for punctuation the gold command spells.
const SPOKEN_PUNCTUATION: &[&str] = &[
    "dot",
    "dash",
    "minus",
    "slash",
    "backslash",
    "underscore",
    "tilde",
];

/// Transcript text reduced to the form gold commands take: spoken
/// punctuation words removed, then [`normalize`]d.
#[must_use]
pub fn spoken_form(text: &str) -> String {
    text.split_whitespace()
        .filter(|word| {
            let bare: String = word.chars().filter(char::is_ascii_alphanumeric).collect();
            !SPOKEN_PUNCTUATION.contains(&bare.to_ascii_lowercase().as_str())
        })
        .map(normalize)
        .collect()
}

/// Whether `command` is spoken within the `[from_secs, to_secs]` window:
/// word timings bound the search when a segment carries them, otherwise
/// the whole overlapping segment counts.
#[must_use]
pub fn spoken_in_window(
    command: &str,
    transcript: &[Segment],
    from_secs: f64,
    to_secs: f64,
) -> bool {
    let needle = normalize(command);
    if needle.len() < 2 {
        return false;
    }
    let haystack: String = transcript
        .iter()
        .filter(|s| s.end_secs >= from_secs && s.start_secs <= to_secs)
        .map(|s| {
            if s.words.is_empty() {
                spoken_form(&s.text)
            } else {
                let inside: Vec<&str> = s
                    .words
                    .iter()
                    .filter(|w| w.start_secs >= from_secs && w.start_secs <= to_secs)
                    .map(|w| w.text.as_str())
                    .collect();
                spoken_form(&inside.join(" "))
            }
        })
        .collect();
    haystack.contains(&needle)
}

/// Whether `command` appears verbatim (after trimming) in `candidates`.
#[must_use]
pub fn exact_match(command: &str, candidates: &[String]) -> bool {
    candidates.iter().any(|c| c.trim() == command.trim())
}

#[cfg(test)]
mod tests {
    use super::{backtick_commands, exact_match, normalize, spoken_form, spoken_in_window};
    use crate::bundle::{Segment, Word};

    fn seg(start: f64, end: f64, text: &str) -> Segment {
        Segment {
            start_secs: start,
            end_secs: end,
            text: text.into(),
            words: vec![],
        }
    }

    #[test]
    fn backtick_spans_are_extracted_in_order() {
        assert_eq!(
            backtick_commands("Run `git init`, then ` ls -la ` and finish."),
            vec!["git init", "ls -la"]
        );
        assert_eq!(backtick_commands("no commands here"), Vec::<String>::new());
    }

    #[test]
    fn normalization_drops_case_spacing_and_punctuation() {
        assert_eq!(normalize("NPM install"), "npminstall");
        assert_eq!(normalize("ls -la"), "lsla");
    }

    #[test]
    fn spoken_commands_are_found_only_inside_their_window() {
        let transcript = [
            seg(1.0, 4.0, "First, run Git init to create it."),
            seg(10.0, 13.0, "Then NPM install pulls dependencies."),
        ];
        assert!(spoken_in_window("git init", &transcript, 0.0, 6.0));
        assert!(!spoken_in_window("npm install", &transcript, 0.0, 6.0));
        assert!(spoken_in_window("npm install", &transcript, 9.0, 15.0));
    }

    #[test]
    fn spoken_punctuation_is_dropped_from_the_transcript_side() {
        assert_eq!(
            spoken_form("Edit notes dot txt, then app dash py."),
            "editnotestxtthenapppy"
        );
        let transcript = [seg(0.0, 3.0, "edit notes dot txt opens it")];
        assert!(spoken_in_window("edit notes.txt", &transcript, 0.0, 3.0));
    }

    #[test]
    fn word_timings_bound_the_window_when_present() {
        let words = |pairs: &[(f64, &str)]| -> Vec<Word> {
            pairs
                .iter()
                .map(|(t, w)| Word {
                    start_secs: *t,
                    end_secs: t + 0.3,
                    text: (*w).to_string(),
                })
                .collect()
        };
        let mut long = seg(4.0, 30.0, "stage it with git add, then git log shows it");
        long.words = words(&[
            (4.0, "stage"),
            (4.5, "it"),
            (5.0, "with"),
            (5.3, "git"),
            (5.6, "add"),
            (20.0, "git"),
            (20.4, "log"),
        ]);
        let transcript = [long];
        assert!(spoken_in_window("git add", &transcript, 2.0, 10.0));
        assert!(!spoken_in_window("git log", &transcript, 2.0, 10.0));
        assert!(spoken_in_window("git log", &transcript, 18.0, 24.0));
    }

    #[test]
    fn single_character_commands_are_never_matched_by_accident() {
        let transcript = [seg(0.0, 2.0, "anything with an l in it")];
        assert!(!spoken_in_window("l", &transcript, 0.0, 2.0));
    }

    #[test]
    fn exact_match_trims_but_keeps_everything_else() {
        let candidates = vec!["git init ".to_string(), "ls".to_string()];
        assert!(exact_match("git init", &candidates));
        assert!(!exact_match("ls -la", &candidates));
    }
}
