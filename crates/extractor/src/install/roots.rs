//! The agent table and read-only detection of which agents are present.
//!
//! Presence of a skills root is the signal that an agent is installed —
//! this module never creates one. Adding support for a new agent means
//! adding one row to [`AGENT_ROOTS`] and nothing else.

use std::fs;
use std::path::{Path, PathBuf};

use super::RootState;

/// One agent and the base-relative dirs its skills may live in.
///
/// Multiple candidates exist where the ecosystem never settled on one
/// path (Amp); the first that exists wins.
pub struct AgentRoots {
    pub agent: &'static str,
    pub dirs: &'static [&'static str],
}

/// Every agent we know how to install for. Ordered by canonical
/// preference: the universal root first, Claude second (see
/// [`super::plan_install`]).
pub const AGENT_ROOTS: &[AgentRoots] = &[
    // The convention the `npx skills` CLI writes to; agents increasingly
    // read it directly, which is why it is our source of truth.
    AgentRoots {
        agent: super::UNIVERSAL_AGENT,
        dirs: &[".agents/skills"],
    },
    AgentRoots {
        agent: super::CLAUDE_AGENT,
        dirs: &[".claude/skills"],
    },
    AgentRoots {
        agent: "codex",
        dirs: &[".codex/skills"],
    },
    AgentRoots {
        agent: "copilot",
        dirs: &[".copilot/skills"],
    },
    AgentRoots {
        agent: "gemini",
        dirs: &[".gemini/skills"],
    },
    AgentRoots {
        agent: "cursor",
        dirs: &[".cursor/skills"],
    },
    AgentRoots {
        agent: "opencode",
        dirs: &[".opencode/skills"],
    },
    AgentRoots {
        agent: "amp",
        dirs: &[".config/amp/skills", ".amp/skills"],
    },
];

/// Names accepted by `--only`, in canonical-preference order.
#[must_use]
pub fn known_agents() -> Vec<&'static str> {
    AGENT_ROOTS.iter().map(|a| a.agent).collect()
}

/// Names in `only` that no table row answers to.
#[must_use]
pub fn unknown_agents(only: &[String]) -> Vec<String> {
    only.iter()
        .filter(|name| !AGENT_ROOTS.iter().any(|a| a.agent == name.as_str()))
        .cloned()
        .collect()
}

/// Observe every known agent under `base` (a home dir for global scope,
/// a project dir for project scope), reporting whether its root exists
/// and what already sits at `<root>/<skill_name>`.
///
/// An empty `only` means every agent. Nothing is written or created.
#[must_use]
pub fn detect(base: &Path, skill_name: &str, only: &[String]) -> Vec<RootState> {
    // Resolve the base once so root paths and symlink targets — which
    // are canonical — can be compared for equality by the planner.
    let base = fs::canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
    AGENT_ROOTS
        .iter()
        .filter(|a| only.is_empty() || only.iter().any(|n| n == a.agent))
        .map(|a| observe(a, &base, skill_name))
        .collect()
}

fn observe(agent: &AgentRoots, base: &Path, skill_name: &str) -> RootState {
    let Some(root) = agent
        .dirs
        .iter()
        .map(|dir| base.join(dir))
        .find(|path| path.is_dir())
    else {
        return RootState::missing(agent.agent);
    };
    let entry = root.join(skill_name);
    // symlink_metadata, not exists(): a broken symlink still occupies
    // the name and must not be silently clobbered.
    let Ok(metadata) = fs::symlink_metadata(&entry) else {
        return RootState::vacant(agent.agent, root);
    };
    let target = if metadata.file_type().is_symlink() {
        fs::canonicalize(&entry).ok()
    } else {
        None
    };
    match target {
        Some(target) => RootState::linked(agent.agent, root, target),
        None => RootState::occupied(agent.agent, root),
    }
}

/// Canonicalize a root path the same way [`detect`] does, so callers
/// comparing against detected roots see matching paths.
#[must_use]
pub fn canonical_base(base: &Path) -> PathBuf {
    fs::canonicalize(base).unwrap_or_else(|_| base.to_path_buf())
}
