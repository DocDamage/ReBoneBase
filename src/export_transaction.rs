//! Recoverable publication of exactly the files in an export plan.
//!
//! This is not a filesystem-wide atomic switch: readers may see individual
//! files change while publishing. An I/O failure rolls the whole plan back.
//! A synced journal, retained new-file links and original-file backups also
//! allow the next export to recover an interrupted process.
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
};

const TRANSACTION: &str = ".rebone-export-transaction";
const JOURNAL: &str = "journal.json";
const READY: &str = "READY";
const COMMITTED: &str = "COMMITTED";
const LOCK: &str = "ownership.lock";
const MAGIC: &str = "ReBoneBase export transaction";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    path: PathBuf,
    original: Option<Digest>,
    replacement: Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Digest {
    length: u64,
    checksum: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    format: String,
    version: u32,
    root: PathBuf,
    entries: Vec<Entry>,
    created_directories: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug)]
// The production hook is a no-op; tests inspect checkpoint indices to cause
// real I/O failures and interruptions after specific publication operations.
#[cfg_attr(not(test), allow(dead_code))]
enum Step {
    CheckLinks,
    Install(usize),
    Restore(usize),
}

pub(crate) fn publish(
    folder: &Path,
    files: Vec<(PathBuf, Vec<u8>)>,
    overwrite: bool,
) -> Result<Vec<PathBuf>> {
    publish_with(folder, files, overwrite, &mut |_, _| Ok(()))
}

fn publish_with(
    folder: &Path,
    files: Vec<(PathBuf, Vec<u8>)>,
    overwrite: bool,
    hook: &mut impl FnMut(Step, &Path) -> Result<()>,
) -> Result<Vec<PathBuf>> {
    let root = absolute_directory(folder)?;
    let relative: Vec<_> = files.iter().map(|(path, _)| path.clone()).collect();
    validate_paths(&relative)?;
    check_directory(&root)?;
    recover_pending(&root)?;

    // Validate every collision and boundary before creating any export file.
    let mut entries = Vec::with_capacity(files.len());
    let mut missing_directories = BTreeSet::new();
    for (path, bytes) in &files {
        let target = root.join(path);
        for parent in target.parent().unwrap().ancestors() {
            if !parent.starts_with(&root) {
                break;
            }
            if !check_directory(parent)? {
                missing_directories.insert(parent.to_path_buf());
            }
        }
        let original = regular_file(&target)?;
        ensure!(
            overwrite || original.is_none(),
            "Export would replace {}; enable Replace existing files or choose a new directory",
            target.display()
        );
        #[cfg(windows)]
        if let Some(metadata) = &original {
            ensure!(
                !metadata.permissions().readonly(),
                "Export destination is read-only: {}. Make it writable or choose another directory",
                target.display()
            );
        }
        entries.push(Entry {
            path: path.clone(),
            original: original.map(|_| file_digest(&target)).transpose()?,
            replacement: bytes_digest(bytes),
        });
    }
    if files.is_empty() {
        return Ok(Vec::new());
    }

    create_directories(&root)?;
    let staging = root.join(TRANSACTION);
    fs::create_dir(&staging).with_context(|| {
        format!(
            "Cannot start export transaction at {}; another export or recovery may be pending",
            staging.display()
        )
    })?;
    // The operating system releases this lock if the process exits. A second
    // exporter may recover an interrupted transaction, but never a live one.
    let ownership = create_lock(&staging).with_context(|| {
        format!(
            "Could not initialize export transaction at {}; previous export files were not changed. Inspect this staging directory before retrying",
            staging.display()
        )
    })?;
    let journal = Journal {
        format: MAGIC.into(),
        version: 1,
        root: root.clone(),
        entries,
        created_directories: missing_directories
            .into_iter()
            .filter(|path| path != &root)
            .map(|path| path.strip_prefix(&root).unwrap().to_path_buf())
            .collect(),
    };
    let prepared = (|| -> Result<()> {
        write_new(
            &staging.join("journal.part"),
            &serde_json::to_vec_pretty(&journal)?,
        )?;
        fs::rename(staging.join("journal.part"), staging.join(JOURNAL))?;
        sync_directory(&staging)?;
        sync_directory(&root)?;
        for (i, (_, bytes)) in files.iter().enumerate() {
            write_new(&new_path(&staging, i), bytes).with_context(|| {
                format!("Could not stage {}", journal.entries[i].path.display())
            })?;
        }
        for path in &journal.created_directories {
            create_directories(&root.join(path))?;
        }
        // FAT/exFAT and some remote volumes cannot create hard links. Discover
        // that before moving any original; rollback needs this operation too.
        hook(Step::CheckLinks, &staging)?;
        fs::hard_link(new_path(&staging, 0), staging.join("link-probe")).context(
            "The export filesystem cannot create the atomic file links required for safe publication; choose a different destination",
        )?;
        fs::remove_file(staging.join("link-probe"))?;
        write_new(&staging.join(READY), MAGIC.as_bytes())?;
        sync_directory(&staging)?;
        Ok(())
    })();
    if let Err(error) = prepared {
        // No target file is touched before READY. Keep any unrecognizable
        // staging area instead of guessing what is safe to delete.
        return Err(failed_transaction(
            error, &root, &staging, &journal, ownership, hook,
        ));
    }

    let published = (|| -> Result<()> {
        for (i, entry) in journal.entries.iter().enumerate() {
            let target = root.join(&entry.path);
            check_directory(target.parent().unwrap())?;
            ensure!(
                regular_file(&target)?
                    .map(|_| file_digest(&target))
                    .transpose()?
                    == entry.original,
                "Export destination changed after preflight: {}",
                target.display()
            );
            if entry.original.is_some() {
                fs::rename(&target, old_path(&staging, i)).with_context(|| {
                    format!("Could not preserve previous export {}", target.display())
                })?;
                sync_directory(&staging)?;
                sync_directory(target.parent().unwrap())?;
            }
            hook(Step::Install(i), &staging)?;
            ensure!(
                file_digest(&new_path(&staging, i))? == entry.replacement,
                "Staged export changed before publication: {}",
                entry.path.display()
            );
            // A create-only link cannot silently replace a file that appeared
            // since preflight. Retaining the staged link identifies our file
            // during recovery, including a crash immediately after this call.
            fs::hard_link(new_path(&staging, i), &target)
                .with_context(|| format!("Could not publish {}", target.display()))?;
            sync_directory(target.parent().unwrap())?;
        }
        Ok(())
    })();
    if let Err(error) = published {
        return Err(failed_transaction(
            error, &root, &staging, &journal, ownership, hook,
        ));
    }
    // Once COMMITTED exists, recovery must retain the new package. Build and
    // sync its contents before the rename, so a failed write cannot masquerade
    // as a committed transaction. No rollback begins after that rename.
    let commit = (|| -> Result<()> {
        write_new(&staging.join("commit.part"), MAGIC.as_bytes())?;
        fs::rename(staging.join("commit.part"), staging.join(COMMITTED))?;
        Ok(())
    })();
    if let Err(error) = commit {
        return Err(failed_transaction(
            error, &root, &staging, &journal, ownership, hook,
        ));
    }
    sync_directory(&staging).with_context(|| {
        format!(
            "Export files were published, but the commit marker could not be synced. Recovery files remain at {}; retry the export after fixing this I/O error",
            staging.display()
        )
    })?;
    cleanup(&staging, &journal, ownership).with_context(|| {
        format!(
            "Export files were published, but transaction cleanup failed at {}. The next export will retry cleanup; keep this recovery directory",
            staging.display()
        )
    })?;
    Ok(relative.into_iter().map(|path| folder.join(path)).collect())
}

/// Called before every new export. Only a recognized, path-validated journal
/// can authorize recovery; unknown contents are retained for manual review.
fn recover_pending(root: &Path) -> Result<()> {
    let staging = root.join(TRANSACTION);
    let Some(metadata) = fs_metadata(&staging)? else {
        return Ok(());
    };
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "Export recovery path is not a regular directory: {}",
        staging.display()
    );
    if regular_file(&staging.join(JOURNAL))?.is_none() {
        return cleanup_orphan(&staging).with_context(|| {
            format!(
                "Cannot safely clean up export staging directory {}; unrecognized files were left untouched. Wait for any running export, or inspect this directory before moving it aside",
                staging.display()
            )
        });
    }
    let ownership = acquire_lock(&staging).with_context(|| {
        format!(
            "Cannot access export transaction at {}; its files were left untouched. Wait for any running export to finish, or inspect the retained recovery directory before moving it aside",
            staging.display()
        )
    })?;
    let journal = (|| -> Result<Journal> {
        let journal_path = staging.join(JOURNAL);
        let metadata = regular_file(&journal_path)?.context("Export recovery journal is missing")?;
        ensure!(metadata.len() <= 4 * 1024 * 1024, "Export recovery journal is too large");
        let bytes = fs::read(&journal_path)?;
        let journal: Journal = serde_json::from_slice(&bytes)?;
        ensure!(
            journal.format == MAGIC && journal.version == 1 && journal.root == root,
            "Unrecognized export recovery journal"
        );
        validate_paths(&journal.entries.iter().map(|entry| entry.path.clone()).collect::<Vec<_>>())?;
        let mut directories = BTreeSet::new();
        for path in &journal.created_directories {
            validate_paths(std::slice::from_ref(path))?;
            ensure!(
                directories.insert(path)
                    && journal.entries.iter().any(|entry| entry.path.parent().is_some_and(|parent| parent.starts_with(path))),
                "Invalid export recovery directory list"
            );
        }
        validate_staging(&staging, &journal)?;
        Ok(journal)
    })().with_context(|| {
        format!(
            "Cannot safely recover export staging directory {}. Its files were left untouched; inspect it and move it aside before retrying",
            staging.display()
        )
    })?;
    if regular_file(&staging.join(COMMITTED))?.is_none() {
        rollback(root, &staging, &journal, &mut |_, _| Ok(())).with_context(|| {
            format!(
                "Interrupted export recovery is incomplete. Backups and journal remain at {}; resolve the reported conflict and retry the export",
                staging.display()
            )
        })?;
    }
    cleanup(&staging, &journal, ownership).with_context(|| {
        format!(
            "Could not clean up recovered export at {}; retry after fixing the I/O error",
            staging.display()
        )
    })
}

fn failed_transaction(
    error: anyhow::Error,
    root: &Path,
    staging: &Path,
    journal: &Journal,
    ownership: File,
    hook: &mut impl FnMut(Step, &Path) -> Result<()>,
) -> anyhow::Error {
    match rollback(root, staging, journal, hook) {
        Ok(()) => match cleanup(staging, journal, ownership) {
            Ok(()) => anyhow::anyhow!("Export failed; previous files were restored: {error:#}"),
            Err(cleanup) => anyhow::anyhow!(
                "Export failed; previous files were restored: {error:#}. Recovery cleanup also failed: {cleanup:#}. Retained recovery directory: {}. Retry the export after fixing this I/O error",
                staging.display()
            ),
        },
        Err(rollback) => anyhow::anyhow!(
            "Export failed: {error:#}. Rollback is incomplete: {rollback:#}. Backups and journal are retained at {}. Resolve the reported conflict and retry the export to recover; do not delete the recovery directory",
            staging.display()
        ),
    }
}

fn rollback(
    root: &Path,
    staging: &Path,
    journal: &Journal,
    hook: &mut impl FnMut(Step, &Path) -> Result<()>,
) -> Result<()> {
    validate_staging(staging, journal)?;
    let ready = regular_file(&staging.join(READY))?.is_some();
    let mut errors = Vec::new();
    for (i, entry) in journal.entries.iter().enumerate().rev().filter(|_| ready) {
        let attempt = (|| -> Result<()> {
            hook(Step::Restore(i), staging)?;
            let target = root.join(&entry.path);
            check_directory(target.parent().unwrap())?;
            let current = regular_file(&target)?;
            let backup = old_path(staging, i);
            if regular_file(&backup)?.is_some() {
                ensure!(entry.original.is_some(), "Unexpected original-file backup");
                ensure!(
                    Some(file_digest(&backup)?) == entry.original,
                    "Original-file backup changed"
                );
                if current.is_some() {
                    if files_equal(&target, &backup)? {
                        // A prior recovery may have installed this file but
                        // failed to sync its directory before returning.
                        sync_directory(target.parent().unwrap())?;
                        return Ok(()); // An earlier recovery already restored this entry.
                    }
                    ensure!(
                        regular_file(&new_path(staging, i))?.is_some()
                            && file_digest(&new_path(staging, i))? == entry.replacement
                            && files_equal(&target, &new_path(staging, i))?,
                        "Destination changed during export; retaining its backup instead of replacing it"
                    );
                    fs::remove_file(&target)?;
                }
                restore_backup(staging, i, &backup, &target)?;
            } else if let Some(original) = entry.original {
                ensure!(
                    current.is_some() && file_digest(&target)? == original,
                    "Previous file is missing or changed, and its backup is unavailable"
                );
            } else if current.is_some() {
                ensure!(
                    regular_file(&new_path(staging, i))?.is_some()
                        && file_digest(&new_path(staging, i))? == entry.replacement
                        && files_equal(&target, &new_path(staging, i))?,
                    "New destination contains an unrelated file; it was left untouched"
                );
                fs::remove_file(&target)?;
            }
            sync_directory(target.parent().unwrap())?;
            Ok(())
        })();
        if let Err(error) = attempt {
            errors.push(format!("{}: {error:#}", entry.path.display()));
        }
    }
    if !errors.is_empty() {
        bail!("{}", errors.join("; "));
    }
    for path in journal.created_directories.iter().rev() {
        let path = root.join(path);
        if check_directory(&path)? && fs::read_dir(&path)?.next().is_none() {
            fs::remove_dir(&path)?;
        }
    }
    Ok(())
}

fn validate_paths(paths: &[PathBuf]) -> Result<()> {
    ensure!(paths.len() <= 100_000, "Too many export paths");
    let mut used = BTreeSet::new();
    for path in paths {
        let mut parts = Vec::new();
        for component in path.components() {
            let Component::Normal(part) = component else {
                bail!(
                    "Export paths must stay inside the chosen directory: {}",
                    path.display()
                );
            };
            let part = part.to_str().context("Export paths must be valid text")?;
            ensure!(
                !part.is_empty()
                    && !part.ends_with(['.', ' '])
                    && !part
                        .chars()
                        .any(|c| c.is_control() || matches!(c, ':' | '\\')),
                "Unsafe export path: {}",
                path.display()
            );
            let stem = part.split('.').next().unwrap().to_ascii_uppercase();
            ensure!(
                !matches!(
                    stem.as_str(),
                    "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
                ) && !(stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && matches!(stem.as_bytes()[3], b'1'..=b'9')),
                "Reserved export filename: {}",
                path.display()
            );
            parts.push(part.to_lowercase());
        }
        ensure!(
            !parts.is_empty() && parts[0] != TRANSACTION,
            "Invalid or reserved export path: {}",
            path.display()
        );
        let key = parts.join("/");
        ensure!(
            used.insert(key),
            "Export filenames collide: {}",
            path.display()
        );
    }
    for path in &used {
        let mut parent = path.as_str();
        while let Some((prefix, _)) = parent.rsplit_once('/') {
            ensure!(
                !used.contains(prefix),
                "Export file and directory paths collide: {path}"
            );
            parent = prefix;
        }
    }
    Ok(())
}

fn validate_staging(staging: &Path, journal: &Journal) -> Result<()> {
    ensure!(
        check_directory(staging)?,
        "Export staging directory is missing"
    );
    let mut expected = BTreeSet::from([
        JOURNAL.to_string(),
        "journal.part".to_string(),
        "commit.part".to_string(),
        "link-probe".to_string(),
        LOCK.to_string(),
        READY.to_string(),
        COMMITTED.to_string(),
    ]);
    for i in 0..journal.entries.len() {
        expected.insert(format!("new-{i}"));
        expected.insert(format!("old-{i}"));
        expected.insert(format!("restore-{i}"));
    }
    for entry in fs::read_dir(staging)? {
        let entry = entry?;
        ensure!(
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| expected.contains(name)),
            "Unrecognized file in export recovery directory: {}",
            entry.path().display()
        );
        regular_file(&entry.path())?;
    }
    for marker in [READY, COMMITTED] {
        let path = staging.join(marker);
        if regular_file(&path)?.is_some() {
            ensure!(
                fs::read(path)? == MAGIC.as_bytes(),
                "Unrecognized export transaction marker"
            );
        }
    }
    Ok(())
}

fn cleanup(staging: &Path, journal: &Journal, ownership: File) -> Result<()> {
    validate_staging(staging, journal)?;
    for i in 0..journal.entries.len() {
        for path in [
            new_path(staging, i),
            old_path(staging, i),
            restore_path(staging, i),
        ] {
            if regular_file(&path)?.is_some() {
                fs::remove_file(path)?;
            }
        }
    }
    for name in [
        "journal.part",
        "commit.part",
        "link-probe",
        READY,
        JOURNAL,
        COMMITTED,
    ] {
        let path = staging.join(name);
        if regular_file(&path)?.is_some() {
            fs::remove_file(path)?;
        }
    }
    // The journal is gone before ownership is released. A concurrent caller
    // cannot mistake this last cleanup window for a recoverable transaction.
    drop(ownership);
    fs::remove_file(staging.join(LOCK))?;
    fs::remove_dir(staging)?;
    sync_directory(staging.parent().unwrap())
}

fn cleanup_orphan(staging: &Path) -> Result<()> {
    // A crash may happen after the final journal unlink but before the final
    // directory unlink. At that point only the recognized commit/lock markers
    // (or an empty directory) can remain. No export target needs touching.
    let mut has_lock = false;
    for entry in fs::read_dir(staging)? {
        let entry = entry?;
        let name = entry.file_name();
        ensure!(
            name == LOCK || name == COMMITTED,
            "An export journal is missing, and the directory contains {}",
            entry.path().display()
        );
        regular_file(&entry.path())?;
        if name == LOCK {
            has_lock = true;
        } else {
            ensure!(
                fs::read(entry.path())? == MAGIC.as_bytes(),
                "Unrecognized export commit marker"
            );
        }
    }
    let ownership = if has_lock {
        Some(acquire_lock(staging)?)
    } else {
        None
    };
    remove_if_present(&staging.join(COMMITTED))?;
    drop(ownership);
    remove_if_present(&staging.join(LOCK))?;
    match fs::remove_dir(staging) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    sync_directory(staging.parent().unwrap())
}

fn remove_if_present(path: &Path) -> Result<()> {
    if regular_file(path)?.is_some() {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn create_lock(staging: &Path) -> Result<File> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(staging.join(LOCK))?;
    file.try_lock().map_err(|error| {
        anyhow::anyhow!(
            "Cannot acquire export ownership at {}: {error}",
            staging.display()
        )
    })?;
    file.write_all(MAGIC.as_bytes())?;
    file.sync_all()?;
    Ok(file)
}

fn acquire_lock(staging: &Path) -> Result<File> {
    let path = staging.join(LOCK);
    let metadata = regular_file(&path)?.context("Export ownership file is missing")?;
    ensure!(
        metadata.len() == MAGIC.len() as u64,
        "Unrecognized export ownership file"
    );
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    file.try_lock().map_err(|error| {
        anyhow::anyhow!(
            "Another export is in progress, or ownership is unavailable at {}: {error}",
            staging.display()
        )
    })?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    ensure!(
        bytes == MAGIC.as_bytes(),
        "Unrecognized export ownership file"
    );
    Ok(file)
}

fn absolute_directory(path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                ensure!(out.pop(), "Invalid export directory");
            }
            _ => out.push(component.as_os_str()),
        }
    }
    Ok(out)
}

fn fs_metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Could not inspect {}", path.display())),
    }
}

fn regular_file(path: &Path) -> Result<Option<fs::Metadata>> {
    let metadata = fs_metadata(path)?;
    if let Some(metadata) = &metadata {
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "Export target is not a regular file: {}",
            path.display()
        );
    }
    Ok(metadata)
}

/// Checks every existing component, including parents of the output root.
fn check_directory(path: &Path) -> Result<bool> {
    let mut current = PathBuf::new();
    let mut exists = true;
    for component in path.components() {
        current.push(component.as_os_str());
        if let Some(metadata) = fs_metadata(&current)? {
            ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "Export directory crosses a symlink or non-directory: {}",
                current.display()
            );
        } else {
            exists = false;
        }
    }
    Ok(exists)
}

fn create_directories(path: &Path) -> Result<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if !check_directory(&current)? {
            fs::create_dir(&current).with_context(|| {
                format!("Could not create export directory {}", current.display())
            })?;
            if let Some(parent) = current.parent() {
                sync_directory(parent)?;
            }
        }
    }
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(_: &Path) -> Result<()> {
    // Standard-library directory handles are not portable to Windows. File
    // contents and journals are still synced before publication.
    Ok(())
}

fn new_path(staging: &Path, i: usize) -> PathBuf {
    staging.join(format!("new-{i}"))
}
fn old_path(staging: &Path, i: usize) -> PathBuf {
    staging.join(format!("old-{i}"))
}
fn restore_path(staging: &Path, i: usize) -> PathBuf {
    staging.join(format!("restore-{i}"))
}

fn restore_backup(staging: &Path, i: usize, backup: &Path, target: &Path) -> Result<()> {
    // Retained backups must not share an inode with the restored destination:
    // another application may edit it while a different entry still needs
    // recovery. Use a separate synced copy for the create-only installation.
    let restored = restore_path(staging, i);
    if regular_file(&restored)?.is_some() {
        fs::remove_file(&restored)?;
    }
    let mut source = File::open(backup)?;
    let mut copy = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&restored)?;
    std::io::copy(&mut source, &mut copy)?;
    copy.set_permissions(source.metadata()?.permissions())?;
    copy.sync_all()?;
    drop(copy);
    fs::hard_link(restored, target)?;
    Ok(())
}

fn checksum(mut state: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        state ^= u64::from(*byte);
        state = state.wrapping_mul(0x100000001b3);
    }
    state
}

fn bytes_digest(bytes: &[u8]) -> Digest {
    Digest {
        length: bytes.len() as u64,
        checksum: checksum(0xcbf29ce484222325, bytes),
    }
}

fn file_digest(path: &Path) -> Result<Digest> {
    let mut file = File::open(path)?;
    let mut digest = bytes_digest(&[]);
    let mut buffer = [0; 8192];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.length += count as u64;
        digest.checksum = checksum(digest.checksum, &buffer[..count]);
    }
    Ok(digest)
}

fn files_equal(left: &Path, right: &Path) -> Result<bool> {
    if fs::metadata(left)?.len() != fs::metadata(right)?.len() {
        return Ok(false);
    }
    let mut left = File::open(left)?;
    let mut right = File::open(right)?;
    let mut a = [0; 8192];
    let mut b = [0; 8192];
    loop {
        let count = left.read(&mut a)?;
        if count == 0 {
            return Ok(true);
        }
        right.read_exact(&mut b[..count])?;
        if a[..count] != b[..count] {
            return Ok(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rebone-transaction-{label}-{}",
            crate::storage::unique()
        ))
    }
    fn plan() -> Vec<(PathBuf, Vec<u8>)> {
        vec![
            ("first.png".into(), b"new first".to_vec()),
            ("nested/second.png".into(), b"new second".to_vec()),
            ("atlas.json".into(), b"new manifest".to_vec()),
        ]
    }
    fn existing(root: &Path) {
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("first.png"), b"old first").unwrap();
        fs::write(root.join("atlas.json"), b"old manifest").unwrap();
        fs::write(root.join("unrelated.txt"), b"keep me").unwrap();
    }
    fn assert_restored(root: &Path) {
        assert_eq!(fs::read(root.join("first.png")).unwrap(), b"old first");
        assert_eq!(fs::read(root.join("atlas.json")).unwrap(), b"old manifest");
        assert_eq!(fs::read(root.join("unrelated.txt")).unwrap(), b"keep me");
        assert!(!root.join("nested/second.png").exists());
    }

    #[test]
    fn publish_replaces_only_planned_files_and_cleans_up() {
        let root = folder("success");
        existing(&root);
        let files = publish(&root, plan(), true).unwrap();
        assert_eq!(files.len(), 3);
        assert_eq!(fs::read(root.join("first.png")).unwrap(), b"new first");
        assert_eq!(fs::read(root.join("atlas.json")).unwrap(), b"new manifest");
        assert_eq!(fs::read(root.join("unrelated.txt")).unwrap(), b"keep me");
        assert!(!root.join(TRANSACTION).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_live_export_cannot_be_mistaken_for_an_interrupted_transaction() {
        let root = folder("concurrent");
        existing(&root);
        let mut checked = false;
        publish_with(&root, plan(), true, &mut |step, _| {
            if matches!(step, Step::Install(1)) {
                let error = publish(&root, plan(), true).unwrap_err();
                assert!(format!("{error:#}").contains("Another export is in progress"));
                assert_eq!(fs::read(root.join("first.png"))?, b"new first");
                checked = true;
            }
            Ok(())
        })
        .unwrap();
        assert!(checked);
        assert_eq!(fs::read(root.join("atlas.json")).unwrap(), b"new manifest");
        assert!(!root.join(TRANSACTION).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hard_link_preparation_failure_leaves_every_original_in_place() {
        let root = folder("link-preparation");
        existing(&root);
        let error = publish_with(&root, plan(), true, &mut |step, staging| {
            if matches!(step, Step::CheckLinks) {
                fs::remove_file(new_path(staging, 0))?;
            }
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains("atomic file links"));
        assert_restored(&root);
        assert!(!root.join(TRANSACTION).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interruptions_after_journal_cleanup_are_finished_on_the_next_export() {
        for state in 0..3 {
            let root = folder("cleanup-interruption");
            existing(&root);
            let staging = root.join(TRANSACTION);
            fs::create_dir(&staging).unwrap();
            if state > 0 {
                drop(create_lock(&staging).unwrap());
            }
            if state > 1 {
                write_new(&staging.join(COMMITTED), MAGIC.as_bytes()).unwrap();
            }
            assert!(publish(&root, plan(), false).is_err());
            assert_restored(&root);
            assert!(!staging.exists());
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn io_failure_after_multiple_installs_restores_old_files_and_removes_new_files() {
        let root = folder("rollback");
        existing(&root);
        let mut installed = false;
        let error = publish_with(&root, plan(), true, &mut |step, staging| {
            if matches!(step, Step::Install(2)) {
                assert_eq!(fs::read(root.join("first.png"))?, b"new first");
                assert_eq!(fs::read(root.join("nested/second.png"))?, b"new second");
                installed = true;
                // Cause a genuine filesystem failure on the next staging read.
                fs::remove_file(new_path(staging, 2))?;
            }
            Ok(())
        })
        .unwrap_err();
        assert!(installed);
        assert!(error.to_string().contains("previous files were restored"));
        assert_restored(&root);
        assert!(!root.join(TRANSACTION).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_failure_retains_backups_and_next_export_recovers_before_preflight() {
        let root = folder("recovery");
        existing(&root);
        let error = publish_with(&root, plan(), true, &mut |step, staging| {
            if matches!(step, Step::Install(2)) {
                fs::remove_file(new_path(staging, 2))?;
            }
            if matches!(step, Step::Restore(0)) {
                bail!("injected sharing violation while restoring first.png");
            }
            Ok(())
        })
        .unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("Rollback is incomplete"));
        assert!(message.contains("sharing violation"));
        assert!(message.contains(&root.join(TRANSACTION).display().to_string()));
        assert_eq!(
            fs::read(old_path(&root.join(TRANSACTION), 0)).unwrap(),
            b"old first"
        );
        // A restored target must not share storage with the retained backup.
        fs::write(
            root.join("atlas.json"),
            b"user edited the restored manifest",
        )
        .unwrap();
        assert_eq!(
            fs::read(old_path(&root.join(TRANSACTION), 2)).unwrap(),
            b"old manifest"
        );
        fs::rename(root.join("atlas.json"), root.join("user-manifest.json")).unwrap();
        // Recovery runs even when the new operation will reject overwrite.
        assert!(publish(&root, plan(), false).is_err());
        assert_restored(&root);
        assert_eq!(
            fs::read(root.join("user-manifest.json")).unwrap(),
            b"user edited the restored manifest"
        );
        assert!(!root.join(TRANSACTION).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rollback_preserves_external_edits_and_keeps_the_previous_file_for_recovery() {
        let root = folder("external-edit");
        existing(&root);
        let error = publish_with(&root, plan(), true, &mut |step, staging| {
            if matches!(step, Step::Install(2)) {
                fs::write(root.join("first.png"), b"edited by another application")?;
                fs::remove_file(new_path(staging, 2))?;
            }
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains("Rollback is incomplete"));
        assert_eq!(
            fs::read(root.join("first.png")).unwrap(),
            b"edited by another application"
        );
        assert_eq!(
            fs::read(old_path(&root.join(TRANSACTION), 0)).unwrap(),
            b"old first"
        );
        fs::rename(root.join("first.png"), root.join("user-edit.png")).unwrap();
        assert!(publish(&root, plan(), false).is_err());
        assert_restored(&root);
        assert_eq!(
            fs::read(root.join("user-edit.png")).unwrap(),
            b"edited by another application"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_publish_recovers_from_journal_before_the_next_export() {
        let root = folder("interruption");
        existing(&root);
        let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = publish_with(&root, plan(), true, &mut |step, _| {
                if matches!(step, Step::Install(2)) {
                    // Simulate process interruption after earlier installs and
                    // after moving this entry's original, before its new link.
                    panic!("simulated interruption");
                }
                Ok(())
            });
        }));
        assert!(interrupted.is_err());
        assert_eq!(fs::read(root.join("first.png")).unwrap(), b"new first");
        assert!(!root.join("atlas.json").exists());
        assert!(publish(&root, plan(), false).is_err());
        assert_restored(&root);
        assert!(!root.join(TRANSACTION).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unrecognized_recovery_contents_are_retained_without_touching_exports() {
        let root = folder("unrecognized");
        existing(&root);
        fs::create_dir(root.join(TRANSACTION)).unwrap();
        fs::write(root.join(TRANSACTION).join("notes.txt"), b"user notes").unwrap();
        let error = publish(&root, plan(), true).unwrap_err();
        assert!(format!("{error:#}").contains("left untouched"));
        assert_restored(&root);
        assert_eq!(
            fs::read(root.join(TRANSACTION).join("notes.txt")).unwrap(),
            b"user notes"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn paths_collisions_and_late_directory_conflicts_are_rejected_before_any_changes() {
        let root = folder("preflight");
        existing(&root);
        for paths in [
            vec!["../escape.png", "atlas.json"],
            vec!["same.png", "SAME.png"],
            vec!["item", "item/child.png"],
            vec!["/absolute.png"],
            vec!["C:\\escape.png"],
            vec!["NUL.png"],
        ] {
            let files = paths
                .into_iter()
                .map(|path| (PathBuf::from(path), vec![1]))
                .collect();
            assert!(publish(&root, files, true).is_err());
            assert_restored(&root);
            assert!(!root.join(TRANSACTION).exists());
        }
        fs::create_dir(root.join("atlas.json.dir")).unwrap();
        let mut files = plan();
        files.push(("atlas.json.dir".into(), vec![1]));
        assert!(publish(&root, files, true).is_err());
        assert_restored(&root);
        assert!(!root.join(TRANSACTION).exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlink_targets_and_parent_boundaries_are_never_followed() {
        use std::os::unix::fs::symlink;
        let root = folder("symlinks");
        let outside = folder("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret"), b"untouched").unwrap();
        symlink(outside.join("secret"), root.join("file.png")).unwrap();
        symlink(&outside, root.join("folder")).unwrap();
        for path in ["file.png", "folder/secret"] {
            assert!(publish(&root, vec![(path.into(), vec![9])], true).is_err());
        }
        assert_eq!(fs::read(outside.join("secret")).unwrap(), b"untouched");
        assert!(!root.join(TRANSACTION).exists());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }
}
