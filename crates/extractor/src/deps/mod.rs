//! Local dependency management: detection and zero-setup bootstrap.
//!
//! Layout inside the app data dir: `tools/` for static binaries,
//! `models/` for whisper weights. A file is only counted present when a
//! `.verified` marker sits beside it — the marker is written exclusively
//! after a checksum-verified install, so an interrupted download can never
//! masquerade as a working tool.

pub mod bootstrap;
pub mod fetch;
pub mod registry;

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Ffmpeg,
    Ffprobe,
    Ytdlp,
    WhisperModel,
}

impl Tool {
    pub const ALL: [Tool; 4] = [Tool::Ffmpeg, Tool::Ffprobe, Tool::Ytdlp, Tool::WhisperModel];

    /// File name inside the data dir (and looked up on PATH for executables).
    #[must_use]
    pub fn file_name(self) -> &'static str {
        match self {
            Tool::Ffmpeg => "ffmpeg",
            Tool::Ffprobe => "ffprobe",
            Tool::Ytdlp => "yt-dlp",
            // Default model: multilingual `base` — 142 MB, fast on Apple
            // Silicon, good enough for tutorial narration; configurable later.
            Tool::WhisperModel => "ggml-base.bin",
        }
    }

    /// Subdirectory of the data dir this tool installs into.
    #[must_use]
    pub fn data_subdir(self) -> &'static str {
        match self {
            Tool::Ffmpeg | Tool::Ffprobe | Tool::Ytdlp => "tools",
            Tool::WhisperModel => "models",
        }
    }

    /// Executables are also searched on PATH; model files are data-dir only.
    #[must_use]
    pub fn is_executable(self) -> bool {
        !matches!(self, Tool::WhisperModel)
    }

    /// Where this tool lives inside `data_dir`.
    #[must_use]
    pub fn install_path(self, data_dir: &Path) -> PathBuf {
        data_dir.join(self.data_subdir()).join(self.file_name())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// Found on the user's PATH (their own install; we leave it alone).
    OnPath,
    /// Checksum-verified copy in the app data dir.
    Installed,
    Missing,
}

#[derive(Debug, Clone)]
pub struct ToolStatus {
    pub tool: Tool,
    pub presence: Presence,
    pub location: Option<PathBuf>,
}

/// The probe's view of the machine — injectable so tests never touch the
/// real PATH or home directory.
#[derive(Debug, Clone)]
pub struct Env {
    pub path_dirs: Vec<PathBuf>,
    pub data_dir: PathBuf,
}

impl Env {
    /// Real environment: PATH entries plus the app data dir
    /// (`$VTS_DATA_DIR` override, else the platform data dir).
    #[must_use]
    pub fn from_system() -> Env {
        let path_dirs = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        let data_dir = std::env::var_os("VTS_DATA_DIR").map_or_else(
            || {
                dirs::data_dir()
                    .unwrap_or_else(std::env::temp_dir)
                    .join("video-to-skill")
            },
            PathBuf::from,
        );
        Env {
            path_dirs,
            data_dir,
        }
    }
}

/// Report the presence of every managed tool.
#[must_use]
pub fn probe(env: &Env) -> Vec<ToolStatus> {
    Tool::ALL.iter().map(|&tool| status_of(tool, env)).collect()
}

fn status_of(tool: Tool, env: &Env) -> ToolStatus {
    if tool.is_executable() {
        if let Some(path) = find_on_path(tool.file_name(), &env.path_dirs) {
            return ToolStatus {
                tool,
                presence: Presence::OnPath,
                location: Some(path),
            };
        }
    }
    let installed = tool.install_path(&env.data_dir);
    if installed.is_file() && verification_marker(&installed).is_file() {
        return ToolStatus {
            tool,
            presence: Presence::Installed,
            location: Some(installed),
        };
    }
    ToolStatus {
        tool,
        presence: Presence::Missing,
        location: None,
    }
}

/// Marker written beside a file after checksum verification.
#[must_use]
pub fn verification_marker(installed: &Path) -> PathBuf {
    let mut name = installed
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.push_str(".verified");
    installed.with_file_name(name)
}

fn find_on_path(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter()
        .map(|d| d.join(name))
        .find(|p| p.is_file() && is_executable(p))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(_path: &Path) -> bool {
    true
}
