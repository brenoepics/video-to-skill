//! Secret and fetch-exec detection: the regex layer.
//!
//! Motivated by a security audit finding (Snyk HIGH W007): steps are
//! transcribed "verbatim from pixels", so a credential visible on
//! screen in the source video would otherwise be compiled straight
//! into an installed skill. The walk over the IR lives in the parent
//! module; this file only decides whether a given string is dangerous.

use anyhow::Result;
use regex::Regex;

/// Secret classes scanned wherever video-derived text lands.
const SECRET_CLASSES: &[(&str, &str)] = &[
    ("aws access key id", r"\bAKIA[0-9A-Z]{16}\b"),
    ("github token", r"\bgh[pousr]_[A-Za-z0-9]{20,}\b"),
    ("slack token", r"\bxox[baprs]-[A-Za-z0-9-]{10,}"),
    ("openai-style api key", r"\bsk-[A-Za-z0-9_-]{20,}\b"),
    ("stripe live key", r"\b[srp]k_live_[0-9A-Za-z]{16,}\b"),
    ("google api key", r"\bAIza[0-9A-Za-z_-]{35}\b"),
    ("gitlab token", r"\bglpat-[A-Za-z0-9_-]{16,}\b"),
    ("npm token", r"\bnpm_[A-Za-z0-9]{30,}\b"),
    (
        "sendgrid key",
        r"\bSG\.[A-Za-z0-9_-]{16,}\.[A-Za-z0-9_-]{16,}\b",
    ),
    (
        "json web token",
        r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b",
    ),
    ("private key block", r"-----BEGIN .*PRIVATE KEY-----"),
    (
        "bearer token",
        r"(?i)\bbearer\s+[A-Za-z0-9._~+/-]{20,}={0,2}",
    ),
    // postgres://user:hunter2@host — credentials inside a URL
    (
        "credentials in a url",
        r"\b[a-zA-Z][a-zA-Z0-9+.-]*://[^\s/:@]+:[^\s/@]{4,}@",
    ),
];

/// Quoted credential assignment, e.g. `password = "hunter2hunter2"`.
/// The value is captured so redaction placeholders can be exempted.
const QUOTED_ASSIGNMENT: &str = r#"(?i)\b(?:password|passwd|secret|api[_-]?key|token|access[_-]?key)\s*[=:]\s*['"]([^'"]{8,})['"]"#;

/// Bare assignment, e.g. `export API_KEY=AbC123...`. Prose like
/// "token: see the docs" must not match, so the value has to look like
/// a credential: long, unbroken, and mixing letters with digits.
const BARE_ASSIGNMENT: &str = r"(?i)\b(?:password|passwd|secret|api[_-]?key|token|access[_-]?key)\s*[=:]\s*([A-Za-z0-9_\-./+]{12,})";

/// A download piped straight into an interpreter, e.g. `curl … | bash`.
const FETCH_EXEC: &str = r"(?:curl|wget)[^|;\n]*\|\s*(?:sh|bash|zsh|python[0-9.]*)\b";

pub(super) struct Patterns {
    secrets: Vec<(&'static str, Regex)>,
    quoted: Regex,
    bare: Regex,
    pub(super) fetch_exec: Regex,
}

impl Patterns {
    pub(super) fn new() -> Result<Self> {
        let mut secrets = Vec::with_capacity(SECRET_CLASSES.len());
        for &(class, pattern) in SECRET_CLASSES {
            secrets.push((class, Regex::new(pattern)?));
        }
        Ok(Self {
            secrets,
            quoted: Regex::new(QUOTED_ASSIGNMENT)?,
            bare: Regex::new(BARE_ASSIGNMENT)?,
            fetch_exec: Regex::new(FETCH_EXEC)?,
        })
    }

    /// The class and truncated snippet of the first secret in `text`.
    pub(super) fn find_secret(&self, text: &str) -> Option<(&'static str, String)> {
        for (class, regex) in &self.secrets {
            if let Some(found) = regex.find(text) {
                return Some((class, truncate(found.as_str())));
            }
        }
        for regex in [&self.quoted, &self.bare] {
            let hit = regex
                .captures_iter(text)
                .find(|caps| !is_placeholder(&caps[1]) && looks_secret(&caps[1]));
            if let Some(caps) = hit {
                return Some(("credential assignment", truncate(&caps[0])));
            }
        }
        None
    }
}

/// `<REDACTED-…>`-style placeholders are the *fix* for a flagged
/// secret and must never themselves be flagged.
fn is_placeholder(value: &str) -> bool {
    let trimmed = value.trim();
    (trimmed.starts_with('<') && trimmed.ends_with('>'))
        || (trimmed.starts_with("${") && trimmed.ends_with('}'))
        || trimmed.contains("REDACTED")
}

/// A quoted value is taken at face value; a bare one must mix letters
/// and digits before it is called a credential, so that documentation
/// like `api_key: your_api_key_here` compiles.
fn looks_secret(value: &str) -> bool {
    let has_digit = value.chars().any(|c| c.is_ascii_digit());
    let has_alpha = value.chars().any(|c| c.is_ascii_alphabetic());
    has_digit && has_alpha || value.len() >= 8 && !value.contains('_') && !has_alpha
}

/// First six characters of a match plus an ellipsis: enough to locate
/// the offending text without echoing the secret back in full.
pub(super) fn truncate(matched: &str) -> String {
    let head: String = matched.chars().take(6).collect();
    if head.len() < matched.len() {
        format!("{head}…")
    } else {
        head
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secret_class(text: &str) -> Option<&'static str> {
        Patterns::new().unwrap().find_secret(text).map(|(c, _)| c)
    }

    fn fetch_exec(text: &str) -> bool {
        Patterns::new().unwrap().fetch_exec.is_match(text)
    }

    #[test]
    fn aws_access_key_ids_are_flagged() {
        assert_eq!(
            secret_class("enter AKIAIOSFODNN7EXAMPLE here"),
            Some("aws access key id")
        );
        assert_eq!(secret_class("AKIA is the AWS key prefix"), None);
    }

    #[test]
    fn github_tokens_are_flagged() {
        assert_eq!(
            secret_class("ghp_AbCdEfGhIjKlMnOpQrStUvWxYz012345"),
            Some("github token")
        );
        assert_eq!(secret_class("ghp_short"), None);
    }

    #[test]
    fn slack_tokens_are_flagged() {
        assert_eq!(secret_class("xoxb-1234567890-abcDEF"), Some("slack token"));
        assert_eq!(secret_class("xoxq-1234567890-abcDEF"), None);
    }

    #[test]
    fn openai_style_keys_are_flagged() {
        assert_eq!(
            secret_class("sk-AbCd1234EfGh5678IjKl9012"),
            Some("openai-style api key")
        );
        // "sk-" mid-word must not match: \b guards the prefix.
        assert_eq!(secret_class("task-management-systems-overview"), None);
    }

    #[test]
    fn vendor_key_shapes_are_flagged() {
        // Assembled at runtime rather than written out: a literal
        // Stripe key here trips GitHub's own push protection, which is
        // a fair verdict on the fixture and a decent sanity check on
        // the pattern. It just cannot be committed in one piece.
        let stripe = format!("sk{}live_4eC39HqLyjWDarjtT1zdp7dc", "_");
        assert_eq!(secret_class(&stripe), Some("stripe live key"));
        assert_eq!(
            secret_class("AIzaSyD-1234567890abcdefghijklmnopqrstu"),
            Some("google api key")
        );
        assert_eq!(
            secret_class("glpat-ABCdef1234567890xyz"),
            Some("gitlab token")
        );
        assert_eq!(
            secret_class("npm_012345678901234567890123456789abcdef"),
            Some("npm token")
        );
    }

    #[test]
    fn jwts_and_bearer_headers_are_flagged() {
        assert_eq!(
            secret_class("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N"),
            Some("json web token")
        );
        assert_eq!(
            secret_class("Authorization: Bearer AbCdEf0123456789GhIjKlMn"),
            Some("bearer token")
        );
        assert_eq!(secret_class("pass a bearer token in the header"), None);
    }

    #[test]
    fn credentials_inside_a_url_are_flagged() {
        assert_eq!(
            secret_class("psql postgres://admin:s3cretpw@db.example.com:5432/app"),
            Some("credentials in a url")
        );
        // a plain URL with a port is not a credential
        assert_eq!(secret_class("open http://localhost:3000/admin"), None);
    }

    #[test]
    fn private_key_blocks_are_flagged() {
        assert_eq!(
            secret_class("-----BEGIN OPENSSH PRIVATE KEY-----"),
            Some("private key block")
        );
        assert_eq!(secret_class("-----BEGIN CERTIFICATE-----"), None);
    }

    #[test]
    fn quoted_credential_assignments_are_flagged() {
        assert_eq!(
            secret_class(r#"password = "hunter2hunter2""#),
            Some("credential assignment")
        );
        assert_eq!(
            secret_class("api_key: 'abcdef0123456789'"),
            Some("credential assignment")
        );
        assert_eq!(
            secret_class("send the token in the Authorization header"),
            None
        );
    }

    #[test]
    fn bare_assignments_are_flagged_when_the_value_looks_secret() {
        assert_eq!(
            secret_class("export API_KEY=AbCd1234EfGh5678"),
            Some("credential assignment")
        );
        // documentation placeholders must still compile
        assert_eq!(secret_class("api_key: your_api_key_here"), None);
        assert_eq!(secret_class("token: see the deployment docs"), None);
    }

    #[test]
    fn redaction_placeholders_pass() {
        assert_eq!(secret_class(r#"api_key: "<REDACTED-API-KEY>""#), None);
        assert_eq!(secret_class("export TOKEN=<REDACTED-TOKEN>"), None);
        assert_eq!(secret_class("export TOKEN=${MY_TOKEN}"), None);
    }

    #[test]
    fn download_piped_to_interpreter_is_flagged() {
        assert!(fetch_exec("curl -fsSL https://get.example.sh | bash"));
        assert!(fetch_exec("wget -qO- https://x.example/i.py | python3"));
        assert!(!fetch_exec("curl -O https://x.example/release.tar.gz"));
        assert!(!fetch_exec("curl https://api.example/v1 | jq .name"));
    }

    #[test]
    fn matches_are_truncated_to_six_chars() {
        assert_eq!(truncate("AKIAIOSFODNN7EXAMPLE"), "AKIAIO…");
        assert_eq!(truncate("short"), "short");
    }
}
