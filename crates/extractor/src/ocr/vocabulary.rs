//! Words an engine should prefer when a glyph is ambiguous: a small
//! built-in CLI vocabulary plus whatever the video's title names.

/// Tokens that turn up in tutorial terminals and that a lexicon built
/// from prose does not know.
pub const BUILTIN: &[&str] = &[
    "git",
    "npm",
    "npx",
    "pnpm",
    "yarn",
    "node",
    "cargo",
    "rustup",
    "pip",
    "python",
    "uv",
    "docker",
    "kubectl",
    "helm",
    "terraform",
    "brew",
    "apt",
    "sudo",
    "curl",
    "wget",
    "ssh",
    "chmod",
    "mkdir",
    "grep",
    "sed",
    "awk",
    "systemctl",
    "localhost",
    "README.md",
    "package.json",
    "Cargo.toml",
    "tsconfig.json",
    "Dockerfile",
    ".env",
    "--help",
    "--version",
    "--release",
    "--force",
    "--global",
    "-rf",
    "&&",
    "||",
];

/// Characters a title token is trimmed of before it counts as a word.
const EDGE_PUNCTUATION: &[char] = &[
    '.', ',', ';', ':', '!', '?', '(', ')', '[', ']', '{', '}', '"', '\'', '|', '#', '«', '»', '“',
    '”', '‘', '’',
];

/// The built-in vocabulary followed by the title's own tokens (tool
/// names, versions, flags), deduplicated in first-seen order.
#[must_use]
pub fn custom_words(title: Option<&str>) -> Vec<String> {
    let mut words: Vec<String> = BUILTIN.iter().map(|w| (*w).to_string()).collect();
    for token in title.unwrap_or_default().split_whitespace() {
        let word = token.trim_matches(EDGE_PUNCTUATION);
        let is_word = word.chars().count() >= 2 && word.chars().any(char::is_alphanumeric);
        if is_word && !words.iter().any(|w| w == word) {
            words.push(word.to_string());
        }
    }
    words
}

#[cfg(test)]
mod tests {
    use super::{custom_words, BUILTIN};

    #[test]
    fn without_a_title_the_vocabulary_is_the_built_in_list() {
        assert_eq!(custom_words(None), BUILTIN);
    }

    #[test]
    fn title_tokens_follow_the_built_ins_without_punctuation_or_repeats() {
        let words = custom_words(Some(
            "Deploying FastAPI with Docker — docker, uvicorn & (v0.2):",
        ));

        let extra: Vec<&str> = words[BUILTIN.len()..].iter().map(String::as_str).collect();
        assert_eq!(
            extra,
            ["Deploying", "FastAPI", "with", "Docker", "uvicorn", "v0.2"],
            "'docker' repeats a built-in, '—' and '&' are not words"
        );
    }
}
