//! Unicode environment block built from an explicit, secret-free allowlist.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::OsStrExt;
use std::path::{Component, Path};

use anyhow::{bail, Context, Result};

pub(super) struct EnvironmentBlock(Vec<u16>);

impl EnvironmentBlock {
    pub(super) fn build(workspace: &Path, executable: &Path) -> Result<Self> {
        let system_root = super::path::windows_directory()?;
        let system32 = system_root.join("System32");
        let command_processor = system32.join("cmd.exe");
        if !command_processor.is_file() {
            bail!(
                "trusted Windows command processor is missing: {}",
                command_processor.display()
            );
        }

        let private = workspace.join(".vts-lpac");
        let roaming = private.join("AppData").join("Roaming");
        let local = private.join("AppData").join("Local");
        let temp = local.join("Temp");
        std::fs::create_dir_all(&roaming)
            .with_context(|| format!("creating private profile at {}", roaming.display()))?;
        std::fs::create_dir_all(&temp)
            .with_context(|| format!("creating private temp at {}", temp.display()))?;

        let executable_dir = executable
            .parent()
            .context("resolved executable has no parent directory")?;
        let path = std::env::join_paths([executable_dir, system32.as_path()])
            .context("building the sandbox PATH")?;
        let system_drive = drive_prefix(&system_root)?;

        let mut values = BTreeMap::<&str, OsString>::new();
        values.insert("APPDATA", roaming.into_os_string());
        values.insert("COMSPEC", command_processor.into_os_string());
        values.insert("HOME", private.clone().into_os_string());
        values.insert("LOCALAPPDATA", local.into_os_string());
        values.insert("PATH", path);
        values.insert("PATHEXT", OsString::from(".EXE"));
        values.insert("SystemDrive", system_drive);
        values.insert("SystemRoot", system_root.clone().into_os_string());
        values.insert("TEMP", temp.clone().into_os_string());
        values.insert("TMP", temp.into_os_string());
        values.insert("USERPROFILE", private.into_os_string());
        values.insert("WINDIR", system_root.into_os_string());

        let mut block = Vec::new();
        for (name, value) in values {
            append_entry(&mut block, name, &value)?;
        }
        block.push(0);
        Ok(Self(block))
    }

    pub(super) fn as_ptr(&self) -> *const u16 {
        self.0.as_ptr()
    }
}

fn drive_prefix(root: &Path) -> Result<OsString> {
    root.components()
        .find_map(|component| match component {
            Component::Prefix(_) => Some(component.as_os_str().to_os_string()),
            _ => None,
        })
        .context("SystemRoot does not contain a Windows drive or UNC prefix")
}

fn append_entry(block: &mut Vec<u16>, name: &str, value: &OsStr) -> Result<()> {
    let mut entry: Vec<_> = OsString::from(format!("{name}="))
        .encode_wide()
        .chain(value.encode_wide())
        .collect();
    if entry.contains(&0) {
        bail!("sandbox environment value for {name} contains a nul character");
    }
    block.append(&mut entry);
    block.push(0);
    Ok(())
}
