//! Cross-agent installation of a generated skill package.
//!
//! A skill package is installed **once** — into a canonical root — and
//! every other agent on the machine gets a relative symlink to it. That
//! mirrors what the ecosystem already does on disk (`~/.copilot/skills/x
//! -> ../../.claude/skills/x`) and keeps one source of truth, so an
//! update or a fold is visible to every agent at once.
//!
//! This module is the **pure** half: it turns a set of observed root
//! states into a plan. It never touches the filesystem, so it never
//! needs `$HOME`. [`roots`] observes, [`apply`] writes.

pub mod apply;
pub mod package;
pub mod report;
pub mod roots;

use std::fmt;
use std::path::{Path, PathBuf};

/// Table name of the universal root (`.agents/skills`) — the convention
/// the `npx skills` CLI installs into, and our first choice of canonical.
pub const UNIVERSAL_AGENT: &str = "universal";
/// Second choice of canonical root, for machines predating the
/// universal convention.
pub const CLAUDE_AGENT: &str = "claude";

/// What was observed about one agent's skills root.
///
/// Built by [`roots::detect`] in production and by hand in tests — the
/// planner sees nothing else, which is what keeps it pure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootState {
    /// Table name of the agent, e.g. `"claude"`.
    pub agent: String,
    /// The agent's skills dir, or `None` when it does not exist — its
    /// absence is the signal that the agent is not installed, and we
    /// never create it.
    pub root: Option<PathBuf>,
    /// Whether `<root>/<skill>` already exists (real dir or symlink).
    pub occupied: bool,
    /// Resolved destination of `<root>/<skill>` when it is a symlink.
    pub link_target: Option<PathBuf>,
}

impl RootState {
    /// The agent is not installed: its skills root does not exist.
    #[must_use]
    pub fn missing(agent: &str) -> Self {
        Self {
            agent: agent.to_string(),
            root: None,
            occupied: false,
            link_target: None,
        }
    }

    /// The root exists and has no skill of this name in it.
    #[must_use]
    pub fn vacant(agent: &str, root: impl Into<PathBuf>) -> Self {
        Self {
            agent: agent.to_string(),
            root: Some(root.into()),
            occupied: false,
            link_target: None,
        }
    }

    /// The root exists and already holds a real skill of this name.
    #[must_use]
    pub fn occupied(agent: &str, root: impl Into<PathBuf>) -> Self {
        Self {
            agent: agent.to_string(),
            root: Some(root.into()),
            occupied: true,
            link_target: None,
        }
    }

    /// The root exists and holds a symlink of this name, resolving to
    /// `target`.
    #[must_use]
    pub fn linked(agent: &str, root: impl Into<PathBuf>, target: impl Into<PathBuf>) -> Self {
        Self {
            agent: agent.to_string(),
            root: Some(root.into()),
            occupied: true,
            link_target: Some(target.into()),
        }
    }
}

/// The one real copy of the package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canonical {
    pub agent: String,
    /// `<root>/<skill-name>`.
    pub path: PathBuf,
    /// Something is already there and `--force` will replace it.
    pub replaces: bool,
}

/// A pointer from another agent's root to the canonical package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub agent: String,
    /// `<root>/<skill-name>` — where the link is created.
    pub path: PathBuf,
    /// Link destination, relative to the link's own parent directory.
    pub relative_target: PathBuf,
    /// Something is already there and `--force` will replace it.
    pub replaces: bool,
}

/// A root deliberately left alone, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skip {
    pub agent: String,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The agent's skills root does not exist on this machine.
    NotInstalled,
    /// The root already points at the canonical package — nothing to do.
    AlreadyLinked,
}

/// The full installation plan: one copy, N links, M skips.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub skill_name: String,
    pub canonical: Canonical,
    pub links: Vec<Link>,
    pub skips: Vec<Skip>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// No known agent skills root exists — we refuse to invent one.
    NoAgentsDetected,
    /// A skill of this name is already installed somewhere.
    Conflict { agent: String, path: PathBuf },
}

impl fmt::Display for PlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAgentsDetected => write!(
                f,
                "no agent skill directory found on this machine — install an agent first \
                 (a root dir is never created for you, since its absence means the agent is absent)"
            ),
            Self::Conflict { agent, path } => write!(
                f,
                "a skill is already installed for {agent} at {} — pass --force to replace it",
                path.display()
            ),
        }
    }
}

impl std::error::Error for PlanError {}

/// Plan the installation of `skill_name` across the observed `roots`.
///
/// The canonical root is the universal `.agents/skills` when present,
/// else Claude's, else the first detected root; every other detected
/// root is linked to it. Roots that already point at the canonical
/// package are skipped, and any other pre-existing skill of the same
/// name is a conflict unless `force` is set.
pub fn plan_install(skill_name: &str, roots: &[RootState], force: bool) -> Result<Plan, PlanError> {
    let canonical_index = pick_canonical(roots).ok_or(PlanError::NoAgentsDetected)?;
    let canonical_root = root_of(&roots[canonical_index]);
    let canonical_path = canonical_root.join(skill_name);

    let canonical_state = &roots[canonical_index];
    if canonical_state.occupied && !force {
        return Err(PlanError::Conflict {
            agent: canonical_state.agent.clone(),
            path: canonical_path,
        });
    }

    let mut links = Vec::new();
    let mut skips = Vec::new();
    for (index, state) in roots.iter().enumerate() {
        if index == canonical_index {
            continue;
        }
        let Some(root) = state.root.as_deref() else {
            skips.push(Skip {
                agent: state.agent.clone(),
                reason: SkipReason::NotInstalled,
            });
            continue;
        };
        let path = root.join(skill_name);
        if state.link_target.as_deref() == Some(canonical_path.as_path()) {
            skips.push(Skip {
                agent: state.agent.clone(),
                reason: SkipReason::AlreadyLinked,
            });
            continue;
        }
        if state.occupied && !force {
            return Err(PlanError::Conflict {
                agent: state.agent.clone(),
                path,
            });
        }
        links.push(Link {
            agent: state.agent.clone(),
            relative_target: relative_target(root, &canonical_path),
            path,
            replaces: state.occupied,
        });
    }

    Ok(Plan {
        skill_name: skill_name.to_string(),
        canonical: Canonical {
            agent: canonical_state.agent.clone(),
            path: canonical_path,
            replaces: canonical_state.occupied,
        },
        links,
        skips,
    })
}

/// Index of the root that gets the real copy: universal, else Claude,
/// else the first one that exists.
fn pick_canonical(roots: &[RootState]) -> Option<usize> {
    let existing = |name: &str| {
        roots
            .iter()
            .position(|r| r.root.is_some() && r.agent == name)
    };
    existing(UNIVERSAL_AGENT)
        .or_else(|| existing(CLAUDE_AGENT))
        .or_else(|| roots.iter().position(|r| r.root.is_some()))
}

fn root_of(state: &RootState) -> &Path {
    state.root.as_deref().unwrap_or(Path::new(""))
}

/// Path to `target` as walked from inside `from` (a directory).
///
/// Falls back to the absolute target when the two share no prefix, as
/// happens across Windows drives.
fn relative_target(from: &Path, target: &Path) -> PathBuf {
    let mut from_parts = from.components().peekable();
    let mut target_parts = target.components().peekable();
    let mut shared = 0usize;
    while from_parts.peek().is_some() && from_parts.peek() == target_parts.peek() {
        from_parts.next();
        target_parts.next();
        shared += 1;
    }
    if shared == 0 {
        return target.to_path_buf();
    }
    let ups = from_parts.count();
    let rest: PathBuf = target_parts.collect();
    if ups == 0 && rest.as_os_str().is_empty() {
        return PathBuf::from(".");
    }
    let mut relative = PathBuf::new();
    for _ in 0..ups {
        relative.push("..");
    }
    relative.push(rest);
    relative
}
