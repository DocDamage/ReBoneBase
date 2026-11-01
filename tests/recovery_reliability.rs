use rebonebase::{
    model::Rig,
    project::{self, Project},
    storage::{LibraryEntry, Recovery, Store},
};
use std::{
    fs,
    path::{Path, PathBuf},
};

struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        // A timestamp alone can repeat across parallel tests on coarse clocks.
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "rebone-recovery-{}-{}-{}",
            std::process::id(),
            rebonebase::storage::unique(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn store(&self) -> Store {
        Store::at(self.0.join("store"))
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn project(name: &str) -> Project {
    Project::new(Rig::new(name, 8, 8))
}

fn backup_dir(store: &Store, path: &Path) -> PathBuf {
    store.backups(path).unwrap();
    fs::read_dir(store.root.join("backups"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
}

#[test]
fn valid_orphans_survive_missing_corrupt_and_mismatched_sidecars() {
    let dir = TestDir::new();
    let store = dir.store();
    let recovery = store.folder("recovery").unwrap();
    let original = dir.0.join("original.rebone");
    for id in ["missing", "corrupt", "stale", "wrong-id"] {
        store
            .write_recovery(id, &project(id), Some(original.clone()))
            .unwrap();
    }
    fs::remove_file(recovery.join("missing.json")).unwrap();
    fs::write(recovery.join("corrupt.json"), b"{interrupted").unwrap();
    // A valid newer project with the same name cannot inherit the stale sidecar's path.
    let mut changed = project("stale");
    changed.rig.width = 16;
    project::save(&recovery.join("stale.rebone"), &changed).unwrap();
    let meta_path = recovery.join("wrong-id.json");
    let mut meta: serde_json::Value =
        serde_json::from_slice(&fs::read(&meta_path).unwrap()).unwrap();
    meta["id"] = "different-session".into();
    fs::write(meta_path, serde_json::to_vec(&meta).unwrap()).unwrap();
    let before: Vec<_> = ["missing", "corrupt", "stale", "wrong-id"]
        .map(|id| {
            let path = recovery.join(format!("{id}.rebone"));
            (path.clone(), fs::read(path).unwrap())
        })
        .into();

    let report = store.recoveries_report().unwrap();
    assert_eq!(report.recoveries.len(), 4);
    for entry in &report.recoveries {
        assert_eq!(entry.name, entry.id);
        assert!(entry.original.is_none());
        assert_eq!(store.load_recovery(&entry.id).unwrap().rig.name, entry.id);
    }
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("original save location is unknown"))
    );
    for (path, bytes) in before {
        assert_eq!(
            fs::read(path).unwrap(),
            bytes,
            "discovery must not rewrite or discard orphans"
        );
    }
}

#[test]
fn matching_and_legacy_sidecars_preserve_original_paths_and_ties_sort_by_id() {
    let dir = TestDir::new();
    let store = dir.store();
    let recovery = store.folder("recovery").unwrap();
    let original = dir.0.join("not-currently-present.rebone");
    for id in ["zeta", "alpha"] {
        store
            .write_recovery(id, &project(id), Some(original.clone()))
            .unwrap();
        let path = recovery.join(format!("{id}.json"));
        let mut record: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        record["modified"] = 42.into();
        fs::write(path, serde_json::to_vec(&record).unwrap()).unwrap();
    }
    project::save(&recovery.join("legacy.rebone"), &project("legacy")).unwrap();
    store
        .write_json(
            &recovery.join("legacy.json"),
            &Recovery {
                id: "legacy".into(),
                name: "legacy".into(),
                original: Some(original.clone()),
                modified: 42,
            },
        )
        .unwrap();

    let report = store.recoveries_report().unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(
        report
            .recoveries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>(),
        ["alpha", "legacy", "zeta"]
    );
    assert!(
        report
            .recoveries
            .iter()
            .all(|entry| entry.original.as_ref() == Some(&original))
    );
}

#[test]
fn legacy_metadata_with_a_different_project_name_cannot_supply_an_original_path() {
    let dir = TestDir::new();
    let store = dir.store();
    let recovery = store.folder("recovery").unwrap();
    project::save(&recovery.join("legacy.rebone"), &project("actual project")).unwrap();
    store
        .write_json(
            &recovery.join("legacy.json"),
            &Recovery {
                id: "legacy".into(),
                name: "stale project".into(),
                original: Some(dir.0.join("stale.rebone")),
                modified: u64::MAX,
            },
        )
        .unwrap();
    let records = store.recoveries().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].name, "actual project");
    assert!(records[0].original.is_none());
    assert_ne!(records[0].modified, u64::MAX);
}

#[test]
fn metadata_commit_failure_leaves_a_valid_snapshot_and_trustworthy_pending_sidecar() {
    let dir = TestDir::new();
    let store = dir.store();
    let recovery = store.folder("recovery").unwrap();
    let blocked = recovery.join("session.json");
    fs::create_dir(&blocked).unwrap();
    fs::write(blocked.join("keep"), b"occupied").unwrap();
    let original = dir.0.join("work.rebone");
    let candidate = project("work after interruption");

    assert!(
        store
            .write_recovery("session", &candidate, Some(original.clone()))
            .is_err()
    );
    assert_eq!(store.load_recovery("session").unwrap(), candidate);
    assert!(recovery.join("session.pending.json").is_file());
    let records = store.recoveries().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].original.as_ref(), Some(&original));
    assert_eq!(records[0].name, candidate.rig.name);
    assert_eq!(fs::read(blocked.join("keep")).unwrap(), b"occupied");

    fs::remove_dir_all(blocked).unwrap();
    store
        .write_recovery("session", &candidate, Some(original))
        .unwrap();
    assert!(!recovery.join("session.pending.json").exists());
    assert!(store.recoveries_report().unwrap().warnings.is_empty());
}

#[test]
fn interrupted_project_commit_does_not_apply_a_new_pending_sidecar_to_old_data() {
    let dir = TestDir::new();
    let store = dir.store();
    let previous = project("previous project");
    let previous_path = dir.0.join("previous.rebone");
    store
        .write_recovery("session", &previous, Some(previous_path.clone()))
        .unwrap();
    let recovery = store.folder("recovery").unwrap();
    let snapshot = recovery.join("session.rebone");
    let preserved = dir.0.join("preserved.rebone");
    fs::rename(&snapshot, &preserved).unwrap();
    fs::create_dir(&snapshot).unwrap();
    fs::write(snapshot.join("keep"), b"occupied").unwrap();

    assert!(
        store
            .write_recovery(
                "session",
                &project("next project"),
                Some(dir.0.join("next.rebone"))
            )
            .is_err()
    );
    assert!(recovery.join("session.pending.json").is_file());
    assert_eq!(fs::read(snapshot.join("keep")).unwrap(), b"occupied");
    fs::remove_dir_all(&snapshot).unwrap();
    fs::rename(preserved, snapshot).unwrap();

    assert_eq!(store.load_recovery("session").unwrap(), previous);
    let records = store.recoveries().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].original.as_ref(), Some(&previous_path));
    assert_eq!(records[0].name, previous.rig.name);
    store.clear_recovery("session").unwrap();
    assert!(!recovery.join("session.pending.json").exists());
    assert!(store.recoveries().unwrap().is_empty());
}

#[test]
fn invalid_recovery_candidate_changes_neither_snapshot_nor_sidecar() {
    let dir = TestDir::new();
    let store = dir.store();
    store
        .write_recovery("session", &project("valid"), None)
        .unwrap();
    let recovery = store.folder("recovery").unwrap();
    let previous = fs::read(recovery.join("session.rebone")).unwrap();
    let metadata = fs::read(recovery.join("session.json")).unwrap();
    let mut invalid = project("invalid");
    invalid.rig.width = 0;

    assert!(store.write_recovery("session", &invalid, None).is_err());
    assert_eq!(fs::read(recovery.join("session.rebone")).unwrap(), previous);
    assert_eq!(fs::read(recovery.join("session.json")).unwrap(), metadata);
    assert!(!recovery.join("session.pending.json").exists());
}

#[test]
fn discovery_skips_invalid_projects_and_bounds_project_and_metadata_reads() {
    let dir = TestDir::new();
    let store = dir.store();
    let recovery = store.folder("recovery").unwrap();
    project::save(&recovery.join("valid.rebone"), &project("valid orphan")).unwrap();
    fs::File::create(recovery.join("valid.json"))
        .unwrap()
        .set_len(1024 * 1024 + 1)
        .unwrap();
    fs::File::create(recovery.join("oversize.rebone"))
        .unwrap()
        .set_len(96 * 1024 * 1024 + 1)
        .unwrap();
    fs::write(recovery.join("partial.rebone"), b"{\"version\":").unwrap();
    let mut invalid = project("invalid dimensions");
    invalid.rig.width = 0;
    fs::write(
        recovery.join("invalid.rebone"),
        serde_json::to_vec(&invalid).unwrap(),
    )
    .unwrap();
    let mut future = serde_json::to_value(project("future")).unwrap();
    future["version"] = (Project::new(Rig::new("current", 8, 8)).version + 1).into();
    fs::write(
        recovery.join("future.rebone"),
        serde_json::to_vec(&future).unwrap(),
    )
    .unwrap();
    fs::create_dir(recovery.join("directory.rebone")).unwrap();
    project::save(&recovery.join("unsafe.name.rebone"), &project("unsafe ID")).unwrap();

    let report = store.recoveries_report().unwrap();
    assert_eq!(report.recoveries.len(), 1);
    assert_eq!(report.recoveries[0].id, "valid");
    assert_eq!(report.recoveries[0].name, "valid orphan");
    assert!(report.recoveries[0].original.is_none());
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("96 MiB"))
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("Metadata exceeds 1 MiB"))
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("invalid recovery partial"))
    );
    assert!(recovery.join("partial.rebone").exists());
}

#[cfg(unix)]
#[test]
fn discovery_does_not_follow_snapshot_symlinks() {
    let dir = TestDir::new();
    let store = dir.store();
    let original = dir.0.join("original.rebone");
    project::save(&original, &project("outside recovery")).unwrap();
    std::os::unix::fs::symlink(
        &original,
        store.folder("recovery").unwrap().join("linked.rebone"),
    )
    .unwrap();
    assert!(store.recoveries().unwrap().is_empty());
}

#[test]
fn recovery_file_limit_uses_deterministic_names_and_leaves_extra_snapshots_intact() {
    let dir = TestDir::new();
    let store = dir.store();
    let recovery = store.folder("recovery").unwrap();
    // Creation order deliberately differs from the lexical selection order.
    project::save(&recovery.join("z-extra.rebone"), &project("extra")).unwrap();
    for index in (0..4095).rev() {
        fs::write(recovery.join(format!("m-{index:04}.rebone")), b"invalid").unwrap();
    }
    project::save(&recovery.join("a-first.rebone"), &project("first")).unwrap();
    let report = store.recoveries_report().unwrap();
    assert_eq!(report.recoveries.len(), 1);
    assert_eq!(report.recoveries[0].id, "a-first");
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("limited to 4096 snapshots"))
    );
    assert_eq!(
        project::load(&recovery.join("z-extra.rebone"))
            .unwrap()
            .rig
            .name,
        "extra"
    );
}

#[test]
fn library_scan_does_not_spend_record_limit_on_thumbnails_and_projects() {
    let dir = TestDir::new();
    let store = dir.store();
    let library = store.folder("library").unwrap();
    for index in 0..4100 {
        fs::write(library.join(format!("asset-{index:04}.png")), b"").unwrap();
    }
    store
        .write_json(
            &library.join("actual.json"),
            &LibraryEntry {
                id: "actual".into(),
                name: "Searchable project".into(),
                tags: vec!["robot".into()],
                favorite: true,
                kind: "Rig".into(),
                modified: 1,
            },
        )
        .unwrap();
    assert_eq!(store.search_library("robot", true).unwrap().len(), 1);
}

#[test]
fn successful_save_keeps_exact_old_bytes_and_only_ten_regular_backups() {
    let dir = TestDir::new();
    let store = dir.store();
    let path = dir.0.join("work.rebone");
    let mut previous = project::encode(&project("previous")).unwrap();
    previous.extend_from_slice(b"\n\n");
    fs::write(&path, &previous).unwrap();
    let backups = backup_dir(&store, &path);
    for index in 0..12 {
        fs::write(
            backups.join(format!("000-{index:02}.rebone")),
            b"older backup",
        )
        .unwrap();
    }
    let candidate = project("new work");

    let report = store.save_with_backup_report(&path, &candidate).unwrap();
    assert!(report.warnings.is_empty());
    assert_eq!(project::load(&path).unwrap(), candidate);
    let retained = store.backups(&path).unwrap();
    assert_eq!(retained.len(), 10);
    assert_eq!(fs::read(&retained[0]).unwrap(), previous);
    assert!(!backups.join("000-00.rebone").exists());
    assert!(!backups.join("000-01.rebone").exists());
    assert!(!backups.join("000-02.rebone").exists());
}

#[test]
fn invalid_candidate_cannot_create_a_backup_or_run_retention() {
    let dir = TestDir::new();
    let store = dir.store();
    let path = dir.0.join("work.rebone");
    project::save(&path, &project("previous")).unwrap();
    let previous = fs::read(&path).unwrap();
    let backups = backup_dir(&store, &path);
    for index in 0..11 {
        fs::write(
            backups.join(format!("000-{index:02}.rebone")),
            b"older backup",
        )
        .unwrap();
    }
    let before = store.backups(&path).unwrap();
    let mut invalid = project("invalid");
    invalid.rig.width = 0;

    assert!(store.save_with_backup_report(&path, &invalid).is_err());
    assert_eq!(fs::read(&path).unwrap(), previous);
    assert_eq!(store.backups(&path).unwrap(), before);
}

#[test]
fn backup_creation_failure_does_not_overwrite_the_existing_project() {
    let dir = TestDir::new();
    let store = dir.store();
    fs::create_dir_all(&store.root).unwrap();
    fs::write(store.root.join("backups"), b"occupied by a file").unwrap();
    let path = dir.0.join("work.rebone");
    project::save(&path, &project("previous")).unwrap();
    let previous = fs::read(&path).unwrap();

    assert!(
        store
            .save_with_backup_report(&path, &project("new work"))
            .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), previous);
    assert_eq!(
        fs::read(store.root.join("backups")).unwrap(),
        b"occupied by a file"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn main_commit_failure_keeps_the_original_and_does_not_prune_old_backups() {
    use std::os::fd::AsRawFd;
    let dir = TestDir::new();
    let store = dir.store();
    let original = dir.0.join("original.rebone");
    project::save(&original, &project("previous")).unwrap();
    let previous = fs::read(&original).unwrap();
    let open = fs::File::open(&original).unwrap();
    // The descriptor can be read as a normal file, but procfs cannot accept a
    // same-directory temporary file. This fails even when tests run as root.
    let destination = PathBuf::from(format!("/proc/self/fd/{}", open.as_raw_fd()));
    let backups = backup_dir(&store, &destination);
    for index in 0..11 {
        fs::write(
            backups.join(format!("000-{index:02}.rebone")),
            b"older backup",
        )
        .unwrap();
    }
    let before = store.backups(&destination).unwrap();

    assert!(
        store
            .save_with_backup_report(&destination, &project("new work"))
            .is_err()
    );
    assert_eq!(fs::read(&original).unwrap(), previous);
    for old in before {
        assert_eq!(fs::read(old).unwrap(), b"older backup");
    }
    let retained = store.backups(&destination).unwrap();
    assert_eq!(
        retained.len(),
        12,
        "the just-created backup and all older backups survive"
    );
    assert_eq!(fs::read(&retained[0]).unwrap(), previous);
}

#[test]
fn retention_failure_reports_a_warning_after_a_successful_main_commit() {
    let dir = TestDir::new();
    let store = dir.store();
    let path = dir.0.join("work.rebone");
    project::save(&path, &project("previous")).unwrap();
    let backups = backup_dir(&store, &path);
    for index in 0..10 {
        fs::write(
            backups.join(format!("000-{index:02}.rebone")),
            b"older backup",
        )
        .unwrap();
    }
    let blocked = backups.join("000-blocked.rebone");
    fs::create_dir(&blocked).unwrap();
    fs::write(blocked.join("keep"), b"unrelated directory contents").unwrap();
    let candidate = project("new work");

    let report = store.save_with_backup_report(&path, &candidate).unwrap();
    assert_eq!(project::load(&path).unwrap(), candidate);
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("could not remove old backup"))
    );
    assert_eq!(store.backups(&path).unwrap().len(), 10);
    assert_eq!(
        fs::read(blocked.join("keep")).unwrap(),
        b"unrelated directory contents"
    );
    // Existing callers still receive success for a committed save with cleanup warnings.
    store
        .save_with_backup(&path, &project("another save"))
        .unwrap();
    assert_eq!(project::load(&path).unwrap().rig.name, "another save");
}

#[test]
fn oversized_existing_file_is_preserved_without_creating_any_backup() {
    let dir = TestDir::new();
    let store = dir.store();
    let path = dir.0.join("oversized.rebone");
    let size = 96 * 1024 * 1024 + 1;
    fs::File::create(&path).unwrap().set_len(size).unwrap();
    assert!(
        store
            .save_with_backup_report(&path, &project("new work"))
            .is_err()
    );
    assert_eq!(fs::metadata(&path).unwrap().len(), size);
    assert!(!store.root.join("backups").exists());
}
