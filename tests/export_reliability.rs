use rebonebase::{
    aseprite,
    batch::{self, ExportFormat, ExportPreset},
    character3d::Character,
    model::Rig,
    production::{
        Attachment, SceneActor,
        engine_export::{self, EngineTarget},
    },
    project::{self, Project},
};

fn broken_character() -> Project {
    let mut project = Project::new(Rig::new("Render errors", 16, 16));
    project.rig.clips[0].name = "Walk cycle".into();
    project.rig.clips[0].length = 3;
    project.extras.auto_canvas = false;
    project.extras.characters3d.push(Character {
        name: "Armored hero".into(),
        scale: f32::NAN,
        ..Default::default()
    });
    project
}

#[test]
fn checked_render_identifies_character_clip_and_frame_instead_of_omitting_the_model() {
    let project = broken_character();
    let message = format!("{:#}", project.try_render_at(2.).unwrap_err());
    assert!(message.contains("Armored hero"), "{message}");
    assert!(message.contains("Walk cycle"), "{message}");
    assert!(message.contains("frame 2"), "{message}");
    assert!(message.contains("pose/camera"), "{message}");

    let legacy = project.render_at(2.);
    assert_eq!(legacy.dimensions(), (16, 16));
    assert!(legacy.pixels().all(|pixel| pixel[3] == 255));
    assert_ne!(legacy.get_pixel(0, 0), legacy.get_pixel(8, 0));
}

#[test]
fn nested_actor_failures_include_the_actor_and_child_character_context() {
    let mut scene = Project::new(Rig::new("Scene", 16, 16));
    scene.rig.clips[0].name = "Entrance".into();
    scene.extras.production.actors.push(SceneActor {
        id: 4,
        name: "Lead actor".into(),
        project: Box::new(broken_character()),
        ..Default::default()
    });
    let message = format!("{:#}", scene.try_render_at(2.).unwrap_err());
    for context in [
        "Lead actor",
        "Entrance",
        "Armored hero",
        "Walk cycle",
        "frame 2",
    ] {
        assert!(message.contains(context), "Missing {context}: {message}");
    }
}

#[test]
fn missing_visible_art_assets_are_reported_by_checked_rendering() {
    let mut project = Project::new(Rig::new("Missing art", 16, 16));
    project.extras.production.attachments.push(Attachment {
        id: 2,
        name: "Shield".into(),
        asset: 999,
        ..Default::default()
    });
    let message = format!("{:#}", project.try_render_at(1.).unwrap_err());
    assert!(message.contains("Shield") && message.contains("999") && message.contains("frame 1"));
}

#[test]
fn every_project_export_rejects_failed_components_and_preserves_previous_output() {
    let project = broken_character();
    assert!(project::sheet_project(&project, 2).is_err());
    assert!(project::gif_project(&project).is_err());
    assert!(aseprite::encode_project(&project, false).is_err());
    let root = std::env::temp_dir().join(format!(
        "rebone-export-errors-{}",
        rebonebase::storage::unique()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("atlas.json"), b"existing atlas").unwrap();
    std::fs::write(root.join("package.json"), b"existing package").unwrap();
    for format in [
        ExportFormat::Sheet,
        ExportFormat::IndexedSheet,
        ExportFormat::Gif,
        ExportFormat::Aseprite,
        ExportFormat::Frames,
    ] {
        assert!(
            batch::export(
                &project,
                &root,
                &ExportPreset {
                    format,
                    ..Default::default()
                },
                true
            )
            .is_err()
        );
    }
    for target in [
        EngineTarget::Generic,
        EngineTarget::Godot,
        EngineTarget::Unity,
    ] {
        assert!(engine_export::export_package(&project, &root, target, true).is_err());
    }
    assert_eq!(
        std::fs::read(root.join("atlas.json")).unwrap(),
        b"existing atlas"
    );
    assert_eq!(
        std::fs::read(root.join("package.json")).unwrap(),
        b"existing package"
    );
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn package_preflight_checks_late_nested_collisions_before_replacing_earlier_files() {
    let mut project = Project::new(Rig::new("Package", 4, 4));
    project.rig.clips[0].length = 1;
    let root = std::env::temp_dir().join(format!(
        "rebone-export-collision-{}",
        rebonebase::storage::unique()
    ));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("clip-01.png"), b"previous sheet").unwrap();
    std::fs::write(root.join("package.json"), b"previous manifest").unwrap();
    std::fs::write(root.join("Editor"), b"unrelated existing file").unwrap();
    assert!(engine_export::export_package(&project, &root, EngineTarget::Unity, true).is_err());
    assert_eq!(
        std::fs::read(root.join("clip-01.png")).unwrap(),
        b"previous sheet"
    );
    assert_eq!(
        std::fs::read(root.join("package.json")).unwrap(),
        b"previous manifest"
    );
    assert_eq!(
        std::fs::read(root.join("Editor")).unwrap(),
        b"unrelated existing file"
    );
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 3);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn exported_frame_metadata_retains_source_time_for_trimmed_audio_and_references() {
    let mut project = Project::new(Rig::new("Trimmed timing", 4, 4));
    project.rig.clips[0].length = 4;
    project.rig.clips[0].durations.insert(1, 50);
    project.rig.clips[0].durations.insert(2, 125);
    project.rig.clips[0] = project.rig.clips[0].windowed(3, 4).unwrap();
    let root = std::env::temp_dir().join(format!(
        "rebone-export-source-time-{}",
        rebonebase::storage::unique()
    ));
    batch::export(&project, &root, &ExportPreset::default(), false).unwrap();
    engine_export::export_package(&project, &root, EngineTarget::Generic, false).unwrap();
    for file in ["atlas.json", "package.json"] {
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join(file)).unwrap()).unwrap();
        let frame = &manifest["clips"][0]["frames"][0];
        assert_eq!(frame["source_frame"], 1);
        assert_eq!(frame["source_time_ms"], 175);
    }
    std::fs::remove_dir_all(root).unwrap();
}
