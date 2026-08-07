//! What an install did, in a form both a human and the orchestrating
//! agent can read back.

use std::fmt;
use std::path::PathBuf;

use super::Skip;
use super::SkipReason;

/// How a non-canonical root ended up pointing at the package.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// A relative symlink to the canonical package — the default.
    Symlink,
    /// A full recursive copy, used when symlinks are unavailable.
    Copy,
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Symlink => write!(f, "link"),
            Self::Copy => write!(f, "copy"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkOutcome {
    pub agent: String,
    pub path: PathBuf,
    /// Destination relative to the link's parent (symlinks only).
    pub relative_target: PathBuf,
    pub method: Method,
    /// An older skill of the same name was replaced here.
    pub replaced: bool,
}

/// The outcome of one `vts-extract install` run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub skill_name: String,
    pub canonical_agent: String,
    /// The single real copy every other root points at.
    pub canonical: PathBuf,
    /// An older canonical package was replaced.
    pub replaced: bool,
    pub links: Vec<LinkOutcome>,
    pub skips: Vec<Skip>,
    /// Nothing was written; this is the plan only.
    pub dry_run: bool,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.dry_run {
            writeln!(f, "dry run — nothing written.")?;
        }
        writeln!(f, "skill:     {}", self.skill_name)?;
        writeln!(
            f,
            "canonical: {} ({}{})",
            self.canonical.display(),
            self.canonical_agent,
            if self.replaced { ", replaced" } else { "" }
        )?;
        for link in &self.links {
            writeln!(
                f,
                "  {:<4} {:<9} {}{}",
                link.method.to_string(),
                link.agent,
                link.path.display(),
                match link.method {
                    Method::Symlink => format!(" -> {}", link.relative_target.display()),
                    Method::Copy => " (symlink unavailable — full copy)".to_string(),
                }
            )?;
            if link.replaced {
                writeln!(f, "       {:<9} replaced an existing skill", "")?;
            }
        }
        for skip in &self.skips {
            writeln!(
                f,
                "  skip {:<9} {}",
                skip.agent,
                match skip.reason {
                    SkipReason::NotInstalled => "not installed on this machine",
                    SkipReason::AlreadyLinked => "already points at the canonical package",
                }
            )?;
        }
        write!(f, "invoke it with /{} in any agent above", self.skill_name)
    }
}
