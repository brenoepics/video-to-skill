//! The glossary whisper's decoder is primed with: what the media is called
//! and, later, the words seen on screen. Whisper reads the prompt as the
//! transcript so far, so it picks up spellings and names from it.

/// Whisper's prompt holds at most 224 tokens, and `n_max_text_ctx` in
/// `whisper.rs` keeps 64 of context; at GPT-2 BPE's ~4 characters per
/// English token this is ~40, so the glossary survives whole.
pub const MAX_PROMPT_CHARS: usize = 160;

/// The prompt for media called `title`, followed by `terms` worth
/// spelling right (none yet; on-screen text is to come): tidied of
/// control characters and stray whitespace, cut between words under
/// `MAX_PROMPT_CHARS`, and punctuated as the transcript should be.
/// `None` when there is nothing to say.
#[must_use]
pub fn glossary(title: Option<&str>, terms: &[&str]) -> Option<String> {
    let mut prompt = String::new();
    if let Some(title) = title.map(clean).filter(|t| !t.is_empty()) {
        prompt.push_str(&title);
        if !title.ends_with(['.', '!', '?']) {
            prompt.push('.');
        }
    }
    let terms: Vec<String> = terms
        .iter()
        .map(|t| clean(t))
        .filter(|t| !t.is_empty())
        .collect();
    if !terms.is_empty() {
        if !prompt.is_empty() {
            prompt.push(' ');
        }
        prompt.push_str(&terms.join(", "));
        prompt.push('.');
    }
    if prompt.is_empty() {
        None
    } else {
        Some(capped(&prompt))
    }
}

/// Control characters dropped (a NUL would make whisper-rs panic),
/// whitespace runs collapsed to one space.
fn clean(text: &str) -> String {
    let printable: String = text
        .chars()
        .filter(|c| c.is_whitespace() || !c.is_control())
        .collect();
    printable.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `prompt` cut to `MAX_PROMPT_CHARS` with its closing period, at the
/// last space before the cap when there is one.
fn capped(prompt: &str) -> String {
    if prompt.chars().count() <= MAX_PROMPT_CHARS {
        return prompt.to_string();
    }
    let head: String = prompt.chars().take(MAX_PROMPT_CHARS - 1).collect();
    let mut cut = match head.rfind(' ') {
        Some(space) => head[..space].to_string(),
        None => head,
    };
    while cut.ends_with([',', '.', ' ']) {
        cut.pop();
    }
    cut.push('.');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_to_say_is_no_prompt() {
        assert_eq!(glossary(None, &[]), None);
        assert_eq!(glossary(Some(""), &[]), None);
        assert_eq!(glossary(Some(" \n\t "), &["", "  "]), None);
    }

    #[test]
    fn the_title_reads_as_a_sentence() {
        assert_eq!(
            glossary(Some("Learn Mac Terminal Basics"), &[]).as_deref(),
            Some("Learn Mac Terminal Basics.")
        );
        assert_eq!(
            glossary(Some("What is Rust?"), &[]).as_deref(),
            Some("What is Rust?"),
            "a title already punctuated keeps its own"
        );
    }

    #[test]
    fn terms_follow_the_title_as_a_list() {
        assert_eq!(
            glossary(Some("Bash in 100 Seconds"), &["grep", "awk"]).as_deref(),
            Some("Bash in 100 Seconds. grep, awk.")
        );
        assert_eq!(
            glossary(None, &["Neovim"]).as_deref(),
            Some("Neovim."),
            "terms alone are a prompt too"
        );
    }

    #[test]
    fn whitespace_and_control_characters_are_tidied() {
        let title = "  Rust\tfor\n\nBeginners\u{0} — Part 2: \"Ownership\"  ";

        let prompt = glossary(Some(title), &[]).unwrap();

        assert_eq!(prompt, "Rust for Beginners — Part 2: \"Ownership\".");
        assert!(!prompt.contains('\u{0}'), "whisper-rs panics on a NUL byte");
    }

    #[test]
    fn a_long_title_is_cut_at_a_word_boundary_under_the_cap() {
        let title = "Débuter avec Rust ".repeat(40);

        let prompt = glossary(Some(&title), &[]).unwrap();

        assert!(
            prompt.chars().count() <= MAX_PROMPT_CHARS,
            "{} chars: {prompt}",
            prompt.chars().count()
        );
        let kept = prompt.strip_suffix('.').unwrap();
        assert!(title.starts_with(kept), "{prompt}");
        assert!(
            title[kept.len()..].starts_with(' '),
            "cut between words: {prompt}"
        );
        assert!(prompt.chars().count() > MAX_PROMPT_CHARS / 2, "{prompt}");
    }

    #[test]
    fn a_single_overlong_word_is_cut_hard() {
        let title = "x".repeat(MAX_PROMPT_CHARS * 2);

        let prompt = glossary(Some(&title), &[]).unwrap();

        assert_eq!(prompt.chars().count(), MAX_PROMPT_CHARS);
    }
}
