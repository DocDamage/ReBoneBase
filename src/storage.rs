use crate::project::{self, Project};
use crate::project_format::MAX_PROJECT_BYTES;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
};

const MAX_RECORDS: usize = 4096;
const MAX_METADATA_BYTES: u64 = 1024 * 1024;
const MAX_TOTAL_METADATA_BYTES: u64 = 8 * 1024 * 1024;
const MAX_RECOVERY_BYTES: u64 = 512 * 1024 * 1024;
const BACKUPS_TO_KEEP: usize = 10;

#[derive(Clone)]
pub struct Store {
    pub root: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recovery {
    pub id: String,
    pub name: String,
    pub original: Option<PathBuf>,
    pub modified: u64,
}
#[derive(Debug, Default)]
pub struct RecoveryReport {
    pub recoveries: Vec<Recovery>,
    pub warnings: Vec<String>,
}
#[derive(Debug, Default)]
pub struct SaveReport {
    pub warnings: Vec<String>,
}
#[derive(Serialize, Deserialize)]
struct RecoveryRecord {
    #[serde(flatten)]
    recovery: Recovery,
    #[serde(default)]
    snapshot: Option<SnapshotFingerprint>,
}
#[derive(PartialEq, Eq, Serialize, Deserialize)]
struct SnapshotFingerprint {
    bytes: u64,
    fnv1a64: String,
}
impl SnapshotFingerprint {
    fn of(bytes: &[u8]) -> Self {
        // This detects interrupted or mismatched sidecars, not malicious tampering.
        let mut hash = 14695981039346656037u64;
        for byte in bytes {
            hash = (hash ^ *byte as u64).wrapping_mul(1099511628211);
        }
        Self {
            bytes: bytes.len() as u64,
            fnv1a64: format!("{hash:016x}"),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LibraryEntry {
    pub id: String,
    pub name: String,
    pub tags: Vec<String>,
    pub favorite: bool,
    pub kind: String,
    pub modified: u64,
}
impl Store {
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }
    pub fn user() -> Self {
        let root = std::env::var_os("REBONEBASE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("APPDATA")
                    .map(PathBuf::from)
                    .unwrap_or_else(std::env::temp_dir)
                    .join("ReBoneBase")
            });
        Self::at(root)
    }
    pub fn safe_id(id: &str) -> Result<()> {
        ensure!(
            !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
            "Invalid storage ID"
        );
        Ok(())
    }
    pub fn folder(&self, name: &str) -> Result<PathBuf> {
        Self::safe_id(name)?;
        let p = self.root.join(name);
        std::fs::create_dir_all(&p)?;
        Ok(p)
    }
    pub fn write_recovery(&self, id: &str, p: &Project, original: Option<PathBuf>) -> Result<()> {
        Self::safe_id(id)?;
        let bytes = project::encode(p)?;
        ensure!(
            original.as_deref().is_none_or(valid_original),
            "Invalid original project path"
        );
        let meta = RecoveryRecord {
            recovery: Recovery {
                id: id.into(),
                name: p.rig.name.clone(),
                original,
                modified: now(),
            },
            snapshot: Some(SnapshotFingerprint::of(&bytes)),
        };
        let metadata = serde_json::to_vec_pretty(&meta)?;
        let dir = self.folder("recovery")?;
        let pending = dir.join(format!("{id}.pending.json"));
        // Stage the sidecar first. Discovery only trusts a sidecar whose fingerprint
        // matches the committed snapshot, whether a crash left it pending or final.
        project::atomic_write(&pending, &metadata)?;
        project::atomic_write(&dir.join(format!("{id}.rebone")), &bytes)?;
        std::fs::rename(&pending, dir.join(format!("{id}.json")))
            .context("Recovery snapshot saved, but its metadata could not be committed")
    }
    pub fn recoveries(&self) -> Result<Vec<Recovery>> {
        Ok(self.recoveries_report()?.recoveries)
    }
    pub fn recoveries_report(&self) -> Result<RecoveryReport> {
        let dir = self.folder("recovery")?;
        let (paths, truncated) = limited_paths(&dir, "rebone", |path| {
            path.file_stem()
                .and_then(|id| id.to_str())
                .is_some_and(|id| Self::safe_id(id).is_ok())
        })?;
        let mut report = RecoveryReport::default();
        if truncated {
            report.warnings.push(format!(
                "Recovery discovery is limited to {MAX_RECORDS} snapshots; additional files remain on disk"
            ));
        }
        let mut project_bytes = 0;
        let mut metadata_bytes = 0;
        let mut budget_warning = false;
        for path in paths {
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if Self::safe_id(id).is_err() {
                continue;
            }
            let metadata = match std::fs::metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    report
                        .warnings
                        .push(format!("Could not inspect recovery {id}: {error}"));
                    continue;
                }
            };
            if metadata.len() > MAX_PROJECT_BYTES {
                report
                    .warnings
                    .push(format!("Recovery {id} exceeds the 96 MiB project limit"));
                continue;
            }
            if metadata.len() > MAX_RECOVERY_BYTES - project_bytes {
                if !budget_warning {
                    report.warnings.push(
                        "Recovery discovery reached its 512 MiB read budget; additional files remain on disk".into(),
                    );
                    budget_warning = true;
                }
                continue;
            }
            let read_limit = MAX_PROJECT_BYTES.min(MAX_RECOVERY_BYTES - project_bytes);
            let bytes = match read_bounded(&path, read_limit) {
                Ok(bytes) => bytes,
                Err(error) => {
                    // A file may grow or fail partway through a read. Conservatively
                    // charge the attempted read so repeated failures stay bounded.
                    project_bytes += read_limit;
                    report
                        .warnings
                        .push(format!("Could not read recovery {id}: {error}"));
                    continue;
                }
            };
            project_bytes += bytes.len() as u64;
            let project = match project::decode(&bytes) {
                Ok(project) => project,
                Err(error) => {
                    report
                        .warnings
                        .push(format!("Ignored invalid recovery {id}: {error}"));
                    continue;
                }
            };
            let fingerprint = SnapshotFingerprint::of(&bytes);
            let committed = read_recovery_record(
                &dir.join(format!("{id}.json")),
                &mut metadata_bytes,
                &mut report.warnings,
            );
            let pending = read_recovery_record(
                &dir.join(format!("{id}.pending.json")),
                &mut metadata_bytes,
                &mut report.warnings,
            );
            let matched = [committed.as_ref(), pending.as_ref()]
                .into_iter()
                .flatten()
                .find(|record| {
                    record.recovery.id == id && record.snapshot.as_ref() == Some(&fingerprint)
                });
            // Legacy sidecars have no content binding. Require their ID and name to
            // agree with the actual project; never use a legacy pending sidecar.
            let legacy = committed.as_ref().filter(|record| {
                record.snapshot.is_none()
                    && record.recovery.id == id
                    && record.recovery.name == project.rig.name
            });
            let trusted = matched.or(legacy);
            if trusted.is_none() {
                report.warnings.push(format!(
                    "Recovered snapshot {id} without matching metadata; its original save location is unknown"
                ));
            }
            report.recoveries.push(Recovery {
                id: id.into(),
                name: project.rig.name,
                original: trusted
                    .and_then(|record| record.recovery.original.clone())
                    .filter(|path| valid_original(path)),
                modified: trusted.map_or_else(
                    || {
                        metadata
                            .modified()
                            .ok()
                            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                            .map_or(0, |duration| duration.as_secs())
                    },
                    |record| record.recovery.modified,
                ),
            });
        }
        report
            .recoveries
            .sort_by(|a, b| b.modified.cmp(&a.modified).then_with(|| a.id.cmp(&b.id)));
        Ok(report)
    }
    pub fn load_recovery(&self, id: &str) -> Result<Project> {
        Self::safe_id(id)?;
        project::load(&self.root.join("recovery").join(format!("{id}.rebone")))
    }
    pub fn clear_recovery(&self, id: &str) -> Result<()> {
        Self::safe_id(id)?;
        for ext in ["json", "pending.json", "rebone"] {
            let p = self.root.join("recovery").join(format!("{id}.{ext}"));
            if p.is_file() {
                std::fs::remove_file(p)?;
            }
        }
        Ok(())
    }
    pub fn save_with_backup(&self, path: &Path, p: &Project) -> Result<()> {
        self.save_with_backup_report(path, p).map(|_| ())
    }
    pub fn save_with_backup_report(&self, path: &Path, p: &Project) -> Result<SaveReport> {
        // Serialization can fail after validation (for example, the output budget).
        // Complete it before creating backups or changing the destination.
        let bytes = project::encode(p)?;
        let existing = match std::fs::metadata(path) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if let Some(metadata) = &existing {
            ensure!(
                metadata.is_file(),
                "The project destination is not a regular file"
            );
            ensure!(
                metadata.len() <= MAX_PROJECT_BYTES,
                "Existing project exceeds backup budget"
            );
            let previous = read_bounded(path, MAX_PROJECT_BYTES)
                .context("Could not read the existing project for backup")?;
            let dir = self.backup_dir(path)?;
            static BACKUP_SEQUENCE: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(0);
            let sequence = BACKUP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let backup = dir.join(format!(
                "{}-{}-{sequence:020}.rebone",
                unique(),
                std::process::id()
            ));
            project::atomic_write(&backup, &previous)
                .context("Could not create project backup; the project was not overwritten")?;
        }
        project::atomic_write(path, &bytes)?;
        let mut report = SaveReport::default();
        if existing.is_some() {
            // A successfully committed save remains successful if retention fails.
            match self.backup_candidates(path) {
                Ok(backups) => {
                    let mut kept = 0;
                    for old in backups {
                        if std::fs::symlink_metadata(&old).is_ok_and(|meta| meta.is_file())
                            && kept < BACKUPS_TO_KEEP
                        {
                            kept += 1;
                            continue;
                        }
                        if let Err(error) = std::fs::remove_file(&old) {
                            report.warnings.push(format!(
                                "Project saved, but could not remove old backup {}: {error}",
                                old.display()
                            ));
                        }
                    }
                }
                Err(error) => report.warnings.push(format!(
                    "Project saved, but backup retention could not run: {error}"
                )),
            }
        }
        Ok(report)
    }
    fn backup_dir(&self, path: &Path) -> Result<PathBuf> {
        let absolute = std::path::absolute(path)?;
        let mut hash = 14695981039346656037u64;
        for b in absolute.to_string_lossy().to_lowercase().bytes() {
            hash = (hash ^ b as u64).wrapping_mul(1099511628211);
        }
        let dir = self.folder("backups")?.join(format!("{hash:016x}"));
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }
    pub fn backups(&self, path: &Path) -> Result<Vec<PathBuf>> {
        Ok(self
            .backup_candidates(path)?
            .into_iter()
            .filter(|path| std::fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file()))
            .collect())
    }
    fn backup_candidates(&self, path: &Path) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(self.backup_dir(path)?)? {
            let path = entry?.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "rebone")
            {
                paths.push(path);
            }
        }
        paths.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
        Ok(paths)
    }
    pub fn save_library(
        &self,
        name: &str,
        p: &Project,
        tags: Vec<String>,
        favorite: bool,
    ) -> Result<String> {
        self.save_library_kind(name, p, tags, favorite, "Rig")
    }
    pub fn save_library_kind(
        &self,
        name: &str,
        p: &Project,
        tags: Vec<String>,
        favorite: bool,
        kind: &str,
    ) -> Result<String> {
        ensure!(
            !name.trim().is_empty()
                && name.len() <= 1024
                && tags.len() <= 32
                && tags.iter().all(|t| t.len() <= 128),
            "Invalid library metadata"
        );
        let id = format!("entry-{}", unique());
        let dir = self.folder("library")?;
        project::save(&dir.join(format!("{id}.rebone")), p)?;
        let pose = if kind == "Pose" {
            p.poses
                .first()
                .map(|saved| saved.pose.clone())
                .unwrap_or_else(|| p.pose_at(1.))
        } else {
            p.pose_at(1.)
        };
        let image = crate::skin::render(&p.rig, Some(&pose));
        let thumb = image::imageops::thumbnail(&image, 128, 128);
        thumb.save(dir.join(format!("{id}.png")))?;
        let entry = LibraryEntry {
            id: id.clone(),
            name: name.into(),
            tags,
            favorite,
            kind: kind.into(),
            modified: now(),
        };
        self.write_json(&dir.join(format!("{id}.json")), &entry)?;
        Ok(id)
    }
    pub fn search_library(&self, query: &str, favorites_only: bool) -> Result<Vec<LibraryEntry>> {
        let q = query.to_lowercase();
        let mut entries = self.read_records::<LibraryEntry>("library")?;
        entries.retain(|e| {
            (!favorites_only || e.favorite)
                && (e.name.to_lowercase().contains(&q)
                    || e.tags.iter().any(|t| t.to_lowercase().contains(&q)))
        });
        entries.sort_by_key(|e| std::cmp::Reverse(e.modified));
        Ok(entries)
    }
    pub fn load_library(&self, id: &str) -> Result<Project> {
        Self::safe_id(id)?;
        project::load(&self.root.join("library").join(format!("{id}.rebone")))
    }
    pub fn update_entry(&self, entry: &LibraryEntry) -> Result<()> {
        Self::safe_id(&entry.id)?;
        ensure!(
            entry.name.len() <= 1024
                && entry.tags.len() <= 32
                && entry.tags.iter().all(|t| t.len() <= 128),
            "Invalid library metadata"
        );
        self.write_json(
            &self.root.join("library").join(format!("{}.json", entry.id)),
            entry,
        )
    }
    pub fn read_records<T: serde::de::DeserializeOwned>(&self, folder: &str) -> Result<Vec<T>> {
        let dir = self.folder(folder)?;
        let mut all = vec![];
        let mut total = 0u64;
        // Ignore thumbnails and project files before applying the record cap.
        for p in limited_paths(&dir, "json", |_| true)?.0 {
            let size = std::fs::metadata(&p)?.len();
            if size > MAX_METADATA_BYTES {
                continue;
            }
            let bytes = read_bounded(&p, MAX_METADATA_BYTES)?;
            total += bytes.len() as u64;
            ensure!(
                total <= MAX_TOTAL_METADATA_BYTES,
                "Storage metadata exceeds 8 MB"
            );
            if let Ok(record) = serde_json::from_slice::<T>(&bytes) {
                all.push(record);
            }
        }
        Ok(all)
    }
    pub fn write_json<T: Serialize>(&self, path: &Path, value: &T) -> Result<()> {
        project::atomic_write(path, &serde_json::to_vec_pretty(value)?)
    }
}
fn valid_original(path: &Path) -> bool {
    path.file_name().is_some()
        && path.as_os_str().len() <= 32 * 1024
        && !path.to_string_lossy().contains('\0')
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= limit, "File exceeds its read budget");
    Ok(bytes)
}

fn read_recovery_record(
    path: &Path,
    total: &mut u64,
    warnings: &mut Vec<String>,
) -> Option<RecoveryRecord> {
    let result = (|| -> Result<Option<RecoveryRecord>> {
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        ensure!(metadata.is_file(), "Metadata is not a regular file");
        ensure!(
            metadata.len() <= MAX_METADATA_BYTES,
            "Metadata exceeds 1 MiB"
        );
        ensure!(
            metadata.len() <= MAX_TOTAL_METADATA_BYTES - *total,
            "Recovery metadata exceeds 8 MiB"
        );
        let limit = MAX_METADATA_BYTES.min(MAX_TOTAL_METADATA_BYTES - *total);
        let bytes = match read_bounded(path, limit) {
            Ok(bytes) => bytes,
            Err(error) => {
                *total += limit;
                return Err(error);
            }
        };
        *total += bytes.len() as u64;
        Ok(Some(serde_json::from_slice(&bytes)?))
    })();
    match result {
        Ok(record) => record,
        Err(error) => {
            warnings.push(format!(
                "Ignored recovery metadata {}: {error}",
                path.display()
            ));
            None
        }
    }
}

fn limited_paths(
    dir: &Path,
    extension: &str,
    accept: impl Fn(&Path) -> bool,
) -> Result<(Vec<PathBuf>, bool)> {
    // Bound the retained names as well as the number of files opened, while making
    // selection independent of the filesystem's directory iteration order.
    let mut paths = BTreeSet::new();
    let mut truncated = false;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == extension)
            && accept(&path)
            && entry.file_type()?.is_file()
        {
            paths.insert(path);
            if paths.len() > MAX_RECORDS {
                paths.pop_last();
                truncated = true;
            }
        }
    }
    Ok((paths.into_iter().collect(), truncated))
}
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn unique() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
