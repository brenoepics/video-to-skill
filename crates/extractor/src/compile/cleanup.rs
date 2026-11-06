//! Remove only obsolete generated files recorded by the previous provenance.

use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::ProcedureIr;

pub(super) fn obsolete_files(ir: &ProcedureIr, out_dir: &Path) -> Result<Vec<PathBuf>> {
    let previous = match fs::read(out_dir.join("provenance.json")) {
        Ok(bytes) => serde_json::from_slice::<ProcedureIr>(&bytes)
            .context("cannot read the previous package's provenance for recompilation")?,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(vec![]),
        Err(error) => return Err(error).context("reading the previous package's provenance"),
    };
    let current = owned_files(ir)?;
    let obsolete: Vec<_> = owned_files(&previous)?
        .difference(&current)
        .cloned()
        .collect();
    // Check before emission too: a symlinked directory must not let cleanup
    // address files outside this package, even with hand-edited provenance.
    for path in &obsolete {
        check_parents(out_dir, path)?;
    }
    Ok(obsolete)
}

/// Match exactly the step files, scripts and frames written by `emit`.
/// Old provenance is input too: validate its names before constructing paths.
pub(super) fn owned_files(ir: &ProcedureIr) -> Result<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    for step in &ir.steps {
        crate::safe_path::validate_step_id(&step.id)?;
        files.insert(Path::new("steps").join(format!("{}.md", step.id)));
        for script in &step.scripts {
            crate::safe_path::validate_script_name(&script.name)?;
            files.insert(Path::new("scripts").join(&script.name));
        }
    }
    let frames = ir
        .steps
        .iter()
        .flat_map(|step| &step.evidence)
        .filter_map(|evidence| evidence.frame.as_deref())
        .chain(ir.artifacts.iter().filter_map(|a| a.frame.as_deref()));
    for frame in frames {
        let name = crate::safe_path::portable_basename(frame)?;
        files.insert(Path::new("references/frames").join(name));
    }
    Ok(files)
}

pub(super) fn remove_obsolete(out_dir: &Path, obsolete: &[PathBuf]) -> Result<()> {
    for path in obsolete {
        check_parents(out_dir, path)?;
        match fs::remove_file(out_dir.join(path)) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("removing obsolete generated file {}", path.display())
                });
            }
        }
    }
    Ok(())
}

pub(super) fn check_parents(out_dir: &Path, relative: &Path) -> Result<()> {
    if !check_directory(out_dir)? {
        return Ok(());
    }
    let mut parent = out_dir.to_path_buf();
    for component in relative.parent().into_iter().flat_map(Path::components) {
        parent.push(component);
        if !check_directory(&parent)? {
            break;
        }
    }
    Ok(())
}

fn check_directory(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            bail!(
                "refusing cleanup through non-directory or symlink {}",
                path.display()
            );
        }
        Ok(_) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context("checking package directory"),
    }
}
