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

#[test]
fn built_in_workspaces_are_valid_and_have_distinct_panel_arrangements() {
    for preset in WorkspacePreset::ALL {
        let layout = preset.layout();
        layout.validate().unwrap();
        assert_eq!(layout.name, preset.label());
        let restored: Workspace =
            serde_json::from_str(&serde_json::to_string(&layout).unwrap()).unwrap();
        assert_eq!(layout, restored);
    }
    assert_eq!(
        WorkspacePreset::Rigging.layout().timeline.dock,
        Dock::Hidden
    );
    assert_eq!(
        WorkspacePreset::Animation.layout().timeline.dock,
        Dock::Bottom
    );
    assert_eq!(WorkspacePreset::Scene.layout().production.dock, Dock::Left);
    assert!(WorkspacePreset::Animation.layout().timeline.height >= 300.);
}

#[test]
fn navigation_actions_can_be_remapped_without_changing_legacy_shortcuts() {
    let mut bindings = default_bindings();
    for action in [
        Action::FirstFrame,
        Action::LastFrame,
        Action::NextKey,
        Action::PreviousKey,
    ] {
        assert!(bindings.iter().any(|binding| binding.action == action));
    }
    let custom = Binding {
        action: Action::SceneWorkspace,
        key: "F8".into(),
        ctrl: false,
        shift: false,
        alt: false,
    };
    bindings.push(custom.clone());
    validate_bindings(&bindings).unwrap();
    let restored: Binding = serde_json::from_str(&serde_json::to_string(&custom).unwrap()).unwrap();
    assert_eq!(restored, custom);
    let legacy: Binding = serde_json::from_str(
        r#"{"action":"MoveRig","key":"M","ctrl":false,"shift":false,"alt":false}"#,
    )
    .unwrap();
    assert!(bindings.contains(&legacy));
    let mut labels = std::collections::HashSet::new();
    for action in Action::ALL {
        assert!(!action.label().is_empty());
        assert!(labels.insert(action.label()), "duplicate command label");
    }
}

#[test]
fn production_visibility_roundtrips_and_older_workspaces_default_to_closed() {
    let scene = WorkspacePreset::Scene.layout();
    assert!(scene.production_open);
    let restored: Workspace =
        serde_json::from_str(&serde_json::to_string(&scene).unwrap()).unwrap();
    assert_eq!(restored, scene);

    let mut old = serde_json::to_value(&scene).unwrap();
    old.as_object_mut().unwrap().remove("production_open");
    let legacy: Workspace = serde_json::from_value(old).unwrap();
    assert!(!legacy.production_open);
    assert_eq!(legacy.production, scene.production);
    legacy.validate().unwrap();
}

#[test]
fn navigation_shortcut_upgrade_preserves_custom_actions_and_occupied_chords() {
    let mut bindings: Vec<_> = default_bindings()
        .into_iter()
        .filter(|binding| {
            !matches!(
                binding.action,
                Action::FirstFrame | Action::LastFrame | Action::NextKey | Action::PreviousKey
            )
        })
        .collect();
    bindings.push(Binding {
        action: Action::ShowSkeleton,
        key: "Home".into(),
        ctrl: false,
        shift: false,
        alt: false,
    });
    bindings.push(Binding {
        action: Action::NextKey,
        key: "F9".into(),
        ctrl: false,
        shift: false,
        alt: false,
    });
    let original = bindings.clone();
    assert!(upgrade_navigation_bindings(&mut bindings));
    assert_eq!(&bindings[..original.len()], original.as_slice());
    assert_eq!(bindings.len(), original.len() + 2);
    assert!(
        !bindings
            .iter()
            .any(|binding| binding.action == Action::FirstFrame)
    );
    assert!(
        bindings
            .iter()
            .any(|binding| binding.action == Action::LastFrame)
    );
    assert!(
        bindings
            .iter()
            .any(|binding| binding.action == Action::PreviousKey)
    );
    assert_eq!(
        bindings
            .iter()
            .filter(|binding| binding.action == Action::NextKey)
            .count(),
        1
    );
    validate_bindings(&bindings).unwrap();
    assert!(!upgrade_navigation_bindings(&mut bindings));
}
