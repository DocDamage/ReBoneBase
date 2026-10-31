use rebonebase::{
    model::Rig,
    production::SceneActor,
    project::{self, Project},
    project_format::{CURRENT_VERSION, decode_with_report},
};
use serde_json::{Value, json};

fn document() -> Project {
    Project::new(Rig::new("Compatibility", 16, 16))
}

fn legacy(project: &Project) -> Value {
    let mut value = serde_json::to_value(project).unwrap();
    value["version"] = json!(1);
    value.as_object_mut().unwrap().remove("compatibility");
    value
}

#[test]
fn legacy_document_migrates_without_changing_art_or_animation() {
    let original = document();
    let bytes = serde_json::to_vec(&legacy(&original)).unwrap();
    let report = decode_with_report(&bytes).unwrap();
    assert_eq!(report.migrated_projects, 1);
    assert_eq!(report.project.version, CURRENT_VERSION);
    assert_eq!(report.project.rig, original.rig);
    assert_eq!(report.project.render_at(1.), original.render_at(1.));
    assert_eq!(
        project::decode(&project::encode(&report.project).unwrap()).unwrap(),
        report.project
    );
}

#[test]
fn legacy_scene_actors_migrate_with_the_parent_project() {
    let mut original = document();
    original.extras.production.actors.push(SceneActor {
        id: 1,
        project: Box::new(Project::new(Rig::new("Actor", 8, 8))),
        ..Default::default()
    });
    let mut raw = legacy(&original);
    raw["extras"]["production"]["actors"][0]["project"] =
        legacy(&original.extras.production.actors[0].project);
    let report = decode_with_report(&serde_json::to_vec(&raw).unwrap()).unwrap();
    assert_eq!(report.migrated_projects, 2);
    assert_eq!(report.project, original);
}

#[test]
fn current_document_roundtrips_without_a_migration() {
    let p = document();
    let report = decode_with_report(&project::encode(&p).unwrap()).unwrap();
    assert_eq!(report.migrated_projects, 0);
    assert_eq!(report.project, p);
}

#[test]
fn malformed_source_arithmetic_is_rejected_without_panicking() {
    let mut raw = serde_json::to_value(document()).unwrap();
    raw["rig"]["clips"][0]["length"] = json!(u32::MAX);
    raw["rig"]["clips"][0]["source"] = json!({
        "origin": 1,
        "length": 12,
        "end": u32::MAX,
        "looped": false,
        "durations": {},
        "time_offset_ms": 0
    });
    assert!(project::decode(&serde_json::to_vec(&raw).unwrap()).is_err());
}

#[test]
fn unsupported_versions_features_and_missing_headers_are_rejected() {
    let original = serde_json::to_value(document()).unwrap();
    let mut future = original.clone();
    future["version"] = json!(CURRENT_VERSION + 1);
    assert!(
        project::decode(&serde_json::to_vec(&future).unwrap())
            .unwrap_err()
            .to_string()
            .contains("Unsupported project schema")
    );
    let mut future = original.clone();
    future["compatibility"]["minimum_reader"] = json!(CURRENT_VERSION + 1);
    assert!(project::decode(&serde_json::to_vec(&future).unwrap()).is_err());
    let mut future = original.clone();
    future["compatibility"]["required_features"] = json!(["future-cloth-simulation"]);
    assert!(
        project::decode(&serde_json::to_vec(&future).unwrap())
            .unwrap_err()
            .to_string()
            .contains("future-cloth-simulation")
    );
    let mut missing = original;
    missing.as_object_mut().unwrap().remove("compatibility");
    assert!(project::decode(&serde_json::to_vec(&missing).unwrap()).is_err());
}

#[test]
fn unknown_nested_fields_never_disappear_silently_on_load() {
    let mut raw = serde_json::to_value(document()).unwrap();
    raw["extras"]["production"]["future_track"] = json!({"frames": [1, 2]});
    let error = project::decode(&serde_json::to_vec(&raw).unwrap()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("project.extras.production.future_track")
    );
    let mut raw = legacy(&document());
    raw["future_data"] = json!(["keep me"]);
    assert!(project::decode(&serde_json::to_vec(&raw).unwrap()).is_err());
    let mut raw = serde_json::to_value(document()).unwrap();
    raw["extras"]["future_optional"] = Value::Null;
    assert!(project::decode(&serde_json::to_vec(&raw).unwrap()).is_err());
    let mut raw = serde_json::to_value(document()).unwrap();
    raw["rig"]["clips"][0]["source"] = Value::Null;
    assert!(project::decode(&serde_json::to_vec(&raw).unwrap()).is_ok());
}

#[test]
fn legacy_loop_alias_is_understood_without_false_unknown_field_errors() {
    let mut raw = legacy(&document());
    let clip = raw["rig"]["clips"][0].as_object_mut().unwrap();
    let looped = clip.remove("loop").unwrap();
    clip.insert("looped".into(), looped);
    assert!(
        project::decode(&serde_json::to_vec(&raw).unwrap())
            .unwrap()
            .rig
            .clips[0]
            .looped
    );
}

#[test]
fn migration_never_rewrites_the_original_until_an_explicit_save() {
    let directory = std::env::temp_dir().join(format!(
        "rebone-migration-{}",
        rebonebase::storage::unique()
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("legacy.rebone");
    let old = serde_json::to_vec_pretty(&legacy(&document())).unwrap();
    std::fs::write(&path, &old).unwrap();
    let upgraded = project::load(&path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), old);
    assert_eq!(upgraded.version, CURRENT_VERSION);
    project::save(&path, &upgraded).unwrap();
    let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["version"], CURRENT_VERSION);
    let mut invalid = upgraded;
    invalid
        .compatibility
        .required_features
        .push("unsupported".into());
    let before = std::fs::read(&path).unwrap();
    assert!(project::save(&path, &invalid).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::remove_dir_all(directory).unwrap();
}
