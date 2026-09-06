//! Deterministic native executable resolution and Windows argv quoting.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

const MAX_COMMAND_LINE_UNITS: usize = 32_767;

pub(super) struct ResolvedCommand {
    executable: PathBuf,
    application_name: Vec<u16>,
    command_line: Vec<u16>,
}

impl ResolvedCommand {
    pub(super) fn resolve(workspace: &Path, command: &[String]) -> Result<Self> {
        let requested = command
            .first()
            .filter(|value| !value.is_empty())
            .context("no command given to run under the sandbox")?;
        let executable = resolve_executable(workspace, requested)?;
        let application_name = terminated(executable.as_os_str(), "executable path")?;

        let mut command_line = Vec::new();
        append_argument(&mut command_line, executable.as_os_str())?;
        for argument in &command[1..] {
            command_line.push(u16::from(b' '));
            append_argument(&mut command_line, OsStr::new(argument))?;
        }
        command_line.push(0);
        if command_line.len() > MAX_COMMAND_LINE_UNITS {
            bail!("Windows command line exceeds {MAX_COMMAND_LINE_UNITS} UTF-16 code units");
        }

        Ok(Self {
            executable,
            application_name,
            command_line,
        })
    }

    pub(super) fn executable(&self) -> &Path {
        &self.executable
    }

    pub(super) fn application_name(&self) -> *const u16 {
        self.application_name.as_ptr()
    }

    pub(super) fn command_line_mut(&mut self) -> *mut u16 {
        self.command_line.as_mut_ptr()
    }
}

fn resolve_executable(workspace: &Path, requested: &str) -> Result<PathBuf> {
    let requested = Path::new(requested);
    let candidate = if requested.is_absolute() || requested.components().count() > 1 {
        if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            workspace.join(requested)
        }
    } else {
        resolve_from_path(requested)?
    };
    let resolved = candidate
        .canonicalize()
        .with_context(|| format!("resolving executable {}", candidate.display()))?;
    if !resolved.is_file() {
        bail!("resolved command is not a file: {}", resolved.display());
    }
    ensure_exe(&resolved)?;
    super::path::local_disk(&resolved, "executable")
}

fn resolve_from_path(name: &Path) -> Result<PathBuf> {
    let name = executable_name(name)?;
    let search_path = std::env::var_os("PATH").context("PATH is unavailable")?;
    std::env::split_paths(&search_path)
        .map(|directory| directory.join(&name))
        .find(|candidate| candidate.is_file())
        .with_context(|| format!("native executable {} was not found in PATH", name.display()))
}

fn executable_name(name: &Path) -> Result<std::ffi::OsString> {
    match name.extension().and_then(OsStr::to_str) {
        None => {
            let mut value = name.as_os_str().to_os_string();
            value.push(".exe");
            Ok(value)
        }
        Some(extension) if extension.eq_ignore_ascii_case("exe") => {
            Ok(name.as_os_str().to_os_string())
        }
        _ => bail!("only native .exe commands may run in the Windows sandbox"),
    }
}

fn ensure_exe(path: &Path) -> Result<()> {
    if path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
    {
        return Ok(());
    }
    bail!("only native .exe commands may run in the Windows sandbox")
}

fn append_argument(line: &mut Vec<u16>, argument: &OsStr) -> Result<()> {
    let encoded: Vec<_> = argument.encode_wide().collect();
    if encoded.contains(&0) {
        bail!("Windows command argument contains a nul character");
    }
    let quote = encoded.is_empty()
        || encoded
            .iter()
            .any(|unit| matches!(*unit, 0x09 | 0x20 | 0x22));
    if !quote {
        line.extend(encoded);
        return Ok(());
    }

    line.push(u16::from(b'"'));
    let mut backslashes = 0;
    for unit in encoded {
        if unit == u16::from(b'\\') {
            backslashes += 1;
        } else if unit == u16::from(b'"') {
            line.extend(std::iter::repeat_n(u16::from(b'\\'), backslashes * 2 + 1));
            line.push(unit);
            backslashes = 0;
        } else {
            line.extend(std::iter::repeat_n(u16::from(b'\\'), backslashes));
            line.push(unit);
            backslashes = 0;
        }
    }
    line.extend(std::iter::repeat_n(u16::from(b'\\'), backslashes * 2));
    line.push(u16::from(b'"'));
    Ok(())
}

fn terminated(value: &OsStr, label: &str) -> Result<Vec<u16>> {
    let mut encoded: Vec<_> = value.encode_wide().collect();
    if encoded.contains(&0) {
        bail!("{label} contains a nul character");
    }
    encoded.push(0);
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_spaces_quotes_and_trailing_backslashes() {
        let mut line = Vec::new();
        append_argument(&mut line, OsStr::new("a b\\\"c\\")).unwrap();
        let rendered = String::from_utf16(&line).unwrap();
        assert_eq!(rendered, r#""a b\\\"c\\""#);
    }

    #[test]
    fn rejects_script_extensions() {
        let error = executable_name(Path::new("tool.cmd")).unwrap_err();
        assert!(error.to_string().contains("native .exe"));
    }
}
