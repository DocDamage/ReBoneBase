use rebonebase::{
    model::Rig,
    production::{
        BoxTrack, EventMarker,
        engine_export::{EngineTarget, export_package},
    },
    project::Project,
};
#[test]
fn engine_animation_names_are_unique_even_for_duplicate_or_empty_clip_labels() {
    let mut p = Project::new(Rig::new("Names", 4, 4));
    p.rig.clips[0].name = "Walk".into();
    p.rig.clips.push(p.rig.clips[0].clone());
    p.rig.clips.push(rebonebase::animation::Clip {
        name: String::new(),
        ..Default::default()
    });
    let root = std::env::temp_dir().join(format!(
        "rebone-engine-names-{}",
        rebonebase::storage::unique()
    ));
    export_package(&p, &root, EngineTarget::Godot, false).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("package.json")).unwrap()).unwrap();
    let clips = manifest["clips"].as_array().unwrap();
    let mut names = std::collections::HashSet::new();
    for clip in clips {
        let name = clip["animation"]
            .as_str()
            .expect("export must provide an engine animation identifier");
        assert!(!name.is_empty() && names.insert(name));
        assert!(clip["id"].as_str().unwrap().starts_with("clip-"));
    }
    assert_eq!(clips[1]["name"], "Walk");
    assert!(
        root.canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap())
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn game_packages_include_timing_events_collision_and_engine_loaders() {
    let mut p = Project::new(Rig::new("Actor", 8, 8));
    p.extras.production.events.push(EventMarker {
        clip: 0,
        frame: 2.,
        name: "footstep".into(),
        payload: "left".into(),
    });
    p.extras.production.boxes.push(BoxTrack {
        id: 1,
        ..Default::default()
    });
    let root =
        std::env::temp_dir().join(format!("rebone-package-{}", rebonebase::storage::unique()));
    for engine in [
        EngineTarget::Godot,
        EngineTarget::Unity,
        EngineTarget::Generic,
    ] {
        let dir = root.join(format!("{engine:?}"));
        let report = export_package(&p, &dir, engine, false).unwrap();
        assert!(!report.is_empty());
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("package.json")).unwrap()).unwrap();
        assert_eq!(
            json["clips"][0]["frames"][1]["events"][0]["name"],
            "footstep"
        );
        assert_eq!(
            json["clips"][0]["frames"][1]["boxes"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(image::open(dir.join("clip-01.png")).is_ok());
        assert!(export_package(&p, &dir, engine, false).is_err());
    }
    assert!(
        root.canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap())
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unreal_packages_ship_the_importer_and_plugin_sources() {
    use rebonebase::{jobs::Progress, production::engine_export::plan_package};
    let p = Project::new(Rig::new("Unreal", 8, 8));
    let files: std::collections::HashMap<_, _> =
        plan_package(&p, EngineTarget::Unreal, &Progress::default())
            .unwrap()
            .into_iter()
            .collect();
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("integrations/unreal");
    let shipped = |path: &str| &files[&std::path::PathBuf::from(path)];
    assert_eq!(
        shipped("ReBoneImport.py"),
        &std::fs::read(repo.join("ReBoneImport.py")).unwrap()
    );
    let mut sources = 0;
    for entry in walk(&repo.join("ReBoneBase")) {
        let relative = entry
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        assert_eq!(
            shipped(&format!("Plugin/{relative}")),
            &std::fs::read(&entry).unwrap(),
            "{relative} is shipped unchanged"
        );
        sources += 1;
    }
    assert!(sources >= 12, "every plugin file is shipped ({sources})");
    let readme = String::from_utf8(shipped("README.txt").clone()).unwrap();
    assert!(readme.contains("5.8") && readme.contains("ReBoneImport.py"));
    assert!(files.contains_key(&std::path::PathBuf::from("package.json")));
}
fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = vec![];
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if !["Binaries", "Intermediate"]
                .iter()
                .any(|d| path.ends_with(d))
            {
                out.extend(walk(&path));
            }
        } else {
            out.push(path);
        }
    }
    out
}
