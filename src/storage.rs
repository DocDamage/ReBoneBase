use crate::project::{self, Project};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
        let dir = self.folder("recovery")?;
        project::save(&dir.join(format!("{id}.rebone")), p)?;
        let meta = Recovery {
            id: id.into(),
            name: p.rig.name.clone(),
            original,
            modified: now(),
        };
        self.write_json(&dir.join(format!("{id}.json")), &meta)
    }
    pub fn recoveries(&self) -> Result<Vec<Recovery>> {
        let mut all = self.read_records::<Recovery>("recovery")?;
        all.sort_by_key(|r| std::cmp::Reverse(r.modified));
        Ok(all)
    }
    pub fn load_recovery(&self, id: &str) -> Result<Project> {
        Self::safe_id(id)?;
        project::load(&self.root.join("recovery").join(format!("{id}.rebone")))
    }
    pub fn clear_recovery(&self, id: &str) -> Result<()> {
        Self::safe_id(id)?;
        for ext in ["json", "rebone"] {
            let p = self.root.join("recovery").join(format!("{id}.{ext}"));
            if p.is_file() {
                std::fs::remove_file(p)?;
            }
        }
        Ok(())
    }
    pub fn save_with_backup(&self, path: &Path, p: &Project) -> Result<()> {
        p.validate()?;
        if path.is_file() {
            ensure!(
                std::fs::metadata(path)?.len() <= 96 * 1024 * 1024,
                "Existing project exceeds backup budget"
            );
            let dir = self.backup_dir(path)?;
            std::fs::copy(path, dir.join(format!("{}.rebone", unique())))?;
            let backups = self.backups(path)?;
            for old in backups.into_iter().skip(10) {
                std::fs::remove_file(old)?;
            }
        }
        project::save(path, p)
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
        let mut paths: Vec<_> = std::fs::read_dir(self.backup_dir(path)?)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "rebone"))
            .collect();
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
        for f in std::fs::read_dir(dir)?.take(4096) {
            let p = f?.path();
            if p.extension().is_some_and(|s| s == "json") {
                let size = std::fs::metadata(&p)?.len();
                if size > 1024 * 1024 {
                    continue;
                }
                total += size;
                ensure!(total <= 8 * 1024 * 1024, "Storage metadata exceeds 8 MB");
                if let Ok(record) = serde_json::from_slice::<T>(&std::fs::read(p)?) {
                    all.push(record);
                }
            }
        }
        Ok(all)
    }
    pub fn write_json<T: Serialize>(&self, path: &Path, value: &T) -> Result<()> {
        project::atomic_write(path, &serde_json::to_vec_pretty(value)?)
    }
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
