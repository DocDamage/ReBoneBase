use rebonebase::workspace::*;
#[test]
fn saved_layouts_and_shortcuts_roundtrip_and_reject_collisions() {
    let mut workspace = Workspace::default();
    workspace.skeleton.dock = Dock::Right;
    workspace.timeline.dock = Dock::Floating;
    workspace.validate().unwrap();
    let restored: Workspace =
        serde_json::from_str(&serde_json::to_string(&workspace).unwrap()).unwrap();
    assert_eq!(workspace, restored);
    let mut bindings = default_bindings();
    validate_bindings(&bindings).unwrap();
    let mut duplicate = bindings[0].clone();
    duplicate.action = Action::Production;
    bindings.push(duplicate);
    assert!(validate_bindings(&bindings).is_err());
}
