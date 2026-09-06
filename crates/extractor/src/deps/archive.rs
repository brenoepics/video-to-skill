//! Native ZIP extraction with strict validation for downloaded tools.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use anyhow::{bail, Context, Result};
use zip::ZipArchive;

use super::bootstrap::{make_executable, remove_file_if_exists, temporary_path, ArchiveMember};
use super::transaction::StagedTool;

const MAX_ENTRIES: usize = 4_096;
const MAX_MEMBER_BYTES: u64 = 512 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 1024 * 1024 * 1024;
const EOCD_MIN_BYTES: usize = 22;
const ZIP_COMMENT_MAX_BYTES: usize = u16::MAX as usize;

pub(crate) fn extract(
    archive_path: &Path,
    members: &[ArchiveMember],
    data_dir: &Path,
) -> Result<Vec<StagedTool>> {
    let file = File::open(archive_path).context("opening downloaded ZIP")?;
    let mut archive = ZipArchive::new(file).context("reading downloaded ZIP")?;
    validate_archive(archive_path, &mut archive)?;

    let mut staged = Vec::with_capacity(members.len());
    for member in members {
        match extract_member(&mut archive, member, data_dir) {
            Ok(item) => staged.push(item),
            Err(error) => {
                cleanup(&staged);
                return Err(error);
            }
        }
    }
    Ok(staged)
}

fn validate_archive(path: &Path, archive: &mut ZipArchive<File>) -> Result<()> {
    if archive.is_empty() {
        bail!("downloaded ZIP is empty");
    }
    if archive.len() > MAX_ENTRIES {
        bail!("downloaded ZIP has too many entries");
    }
    let declared = declared_entry_count(path)?;
    if declared != archive.len() {
        bail!("downloaded ZIP contains duplicate file names");
    }

    let mut total = 0_u64;
    let mut names = HashSet::with_capacity(archive.len());
    for index in 0..archive.len() {
        let entry = archive.by_index(index).context("reading ZIP entry")?;
        if !safe_name(&entry) {
            bail!("unsafe ZIP path: {}", entry.name());
        }
        if entry.encrypted() {
            bail!("encrypted ZIP entries are not supported");
        }
        if entry.is_symlink() {
            bail!("ZIP symbolic links are not allowed");
        }
        if !names.insert(entry.name().to_ascii_lowercase()) {
            bail!("downloaded ZIP contains duplicate file names");
        }
        total = total
            .checked_add(entry.size())
            .context("ZIP expanded size overflow")?;
        if entry.size() > MAX_MEMBER_BYTES || total > MAX_TOTAL_BYTES {
            bail!("downloaded ZIP exceeds the extraction size limit");
        }
    }
    Ok(())
}

fn safe_name<R: Read>(entry: &zip::read::ZipFile<'_, R>) -> bool {
    let name = entry.name();
    if name.is_empty()
        || entry.enclosed_name().is_none()
        || name.contains('\\')
        || name.starts_with('/')
    {
        return false;
    }
    let path = name.strip_suffix('/').unwrap_or(name);
    path.split('/').all(|component| {
        !component.is_empty() && component != "." && component != ".." && !component.contains(':')
    })
}

fn extract_member(
    archive: &mut ZipArchive<File>,
    member: &ArchiveMember,
    data_dir: &Path,
) -> Result<StagedTool> {
    let mut entry = archive
        .by_name(&member.path)
        .with_context(|| format!("ZIP member not found: {}", member.path))?;
    if !entry.is_file() {
        bail!("ZIP member is not a regular file: {}", member.path);
    }

    let destination = member.tool.install_path(data_dir);
    let destination_dir = destination
        .parent()
        .context("archive member install path has no parent")?;
    fs::create_dir_all(destination_dir)?;
    let staged_path = temporary_path(&destination, "prepared");
    remove_file_if_exists(&staged_path)?;
    let result = write_limited(&mut entry, &staged_path).and_then(|()| {
        if member.tool.is_executable() {
            make_executable(&staged_path)?;
        }
        Ok(())
    });
    if result.is_err() {
        let _ = remove_file_if_exists(&staged_path);
    }
    result?;
    Ok(StagedTool {
        tool: member.tool,
        path: staged_path,
    })
}

fn write_limited(reader: &mut impl Read, destination: &Path) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let copied = std::io::copy(&mut reader.take(MAX_MEMBER_BYTES + 1), &mut file)?;
    if copied > MAX_MEMBER_BYTES {
        bail!("ZIP member exceeds the extraction size limit");
    }
    file.flush()?;
    file.sync_all()?;
    Ok(())
}

fn cleanup(staged: &[StagedTool]) {
    for item in staged {
        let _ = remove_file_if_exists(&item.path);
    }
}

fn declared_entry_count(path: &Path) -> Result<usize> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    let maximum_tail = u64::try_from(EOCD_MIN_BYTES + ZIP_COMMENT_MAX_BYTES)?;
    let tail_len = usize::try_from(length.min(maximum_tail))?;
    if tail_len < EOCD_MIN_BYTES {
        bail!("downloaded ZIP is truncated");
    }
    let tail_offset = i64::try_from(tail_len)?;
    file.seek(SeekFrom::End(-tail_offset))?;
    let mut tail = vec![0_u8; tail_len];
    file.read_exact(&mut tail)?;

    for index in (0..=tail_len.saturating_sub(EOCD_MIN_BYTES)).rev() {
        if tail.get(index..index + 4) != Some(b"PK\x05\x06") {
            continue;
        }
        let comment_len = little_u16(&tail[index + 20..index + 22]) as usize;
        if index + EOCD_MIN_BYTES + comment_len != tail.len() {
            continue;
        }
        let disk = little_u16(&tail[index + 4..index + 6]);
        let central_disk = little_u16(&tail[index + 6..index + 8]);
        let disk_entries = little_u16(&tail[index + 8..index + 10]);
        let total_entries = little_u16(&tail[index + 10..index + 12]);
        if disk != 0 || central_disk != 0 || disk_entries != total_entries {
            bail!("multi-disk ZIP archives are not supported");
        }
        if total_entries == u16::MAX {
            bail!("ZIP64 archives are not supported for tool downloads");
        }
        return Ok(usize::from(total_entries));
    }
    bail!("ZIP end-of-central-directory record not found")
}

fn little_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}
