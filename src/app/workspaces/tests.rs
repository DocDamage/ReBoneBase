use super::*;

fn key(key: egui::Key) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

fn palette_pass(
    app: &mut ReboneApp,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(900., 700.))),
            events,
            ..Default::default()
        },
        |ctx| app.command_palette(ctx),
    )
}

fn apply_layout(app: &mut ReboneApp, ctx: &egui::Context) {
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(900., 700.))),
            ..Default::default()
        },
        |ctx| app.apply_pending_workspace_layout(ctx),
    );
}

#[test]
fn workspace_and_tool_navigation_preserve_document_and_saved_layouts() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let before = app.project.clone();
    let saved = app.studio.preferences.workspaces.clone();

    app.run_action(Action::RiggingWorkspace);
    assert_eq!(app.studio.preferences.workspace.timeline.dock, Dock::Hidden);
    app.run_action(Action::AnimationWorkspace);
    assert_eq!(app.studio.preferences.workspace.timeline.dock, Dock::Bottom);
    app.run_action(Action::SceneWorkspace);
    assert!(app.production.open);
    assert_eq!(app.production.tab, production_ui::Tab::Scene);
    app.run_action(Action::AnimationTools);
    assert!(app.studio.open && app.studio.tab == studio::Tab::Animation);
    app.run_action(Action::CharacterTools);
    assert_eq!(app.production.tab, production_ui::Tab::Characters);

    assert_eq!(app.project, before);
    assert_eq!(app.studio.preferences.workspaces, saved);
    assert!(!app.dirty());
    assert!(!app.history.undo(&mut app.project));
}

#[test]
fn palette_arrows_choose_a_destination_and_search_resets_the_highlight() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let before = app.project.clone();
    app.run_action(Action::CommandPalette);
    app.production.palette_query = "workspace to".into();
    palette_pass(&mut app, &ctx, vec![]);
    palette_pass(&mut app, &ctx, vec![key(egui::Key::ArrowDown)]);
    assert_eq!(app.workspace_ui.palette_index, 1);
    palette_pass(&mut app, &ctx, vec![key(egui::Key::ArrowDown)]);
    assert_eq!(app.workspace_ui.palette_index, 2);
    palette_pass(&mut app, &ctx, vec![key(egui::Key::ArrowUp)]);
    assert_eq!(app.workspace_ui.palette_index, 1);
    palette_pass(&mut app, &ctx, vec![key(egui::Key::Enter)]);
    assert_eq!(app.studio.preferences.workspace.name, "Animation");
    assert!(!app.production.palette_open);

    app.run_action(Action::CommandPalette);
    app.production.palette_query = "workspace to".into();
    palette_pass(&mut app, &ctx, vec![key(egui::Key::ArrowDown)]);
    app.production.palette_query = "Studio Rig pins".into();
    palette_pass(&mut app, &ctx, vec![]);
    assert_eq!(app.workspace_ui.palette_index, 0);
    palette_pass(&mut app, &ctx, vec![key(egui::Key::Enter)]);
    assert!(app.studio.open && app.studio.tab == studio::Tab::Rig);
    assert_eq!(app.project, before);
    assert!(!app.history.undo(&mut app.project));
}

#[test]
fn empty_palette_results_keep_the_document_and_wait_for_a_new_query() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let before = app.project.clone();
    app.run_action(Action::CommandPalette);
    app.production.palette_query = "no-matching-command-739".into();
    // A new egui window needs its sizing pass before asserting painted text.
    palette_pass(&mut app, &ctx, vec![]);
    let output = palette_pass(&mut app, &ctx, vec![key(egui::Key::Enter)]);
    assert!(app.production.palette_open);
    assert!(output.shapes.iter().any(|shape| matches!(
        &shape.shape,
        egui::Shape::Text(text) if text.galley.text() == "No commands match your search."
    )));
    palette_pass(&mut app, &ctx, vec![key(egui::Key::Escape)]);
    assert!(!app.production.palette_open);
    assert_eq!(app.project, before);
    assert!(!app.history.undo(&mut app.project));
}

#[test]
fn hidden_panels_restore_their_dock_and_floating_recovery_applies_once() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.studio.preferences.workspace.inspector = PanelLayout {
        dock: Dock::Floating,
        x: 9000.,
        y: -3000.,
        width: 1100.,
        height: 900.,
    };
    app.set_panel_visible(Panel::Inspector, false);
    assert_eq!(
        app.studio.preferences.workspace.inspector.dock,
        Dock::Hidden
    );
    app.run_action(Action::ShowInspector);
    assert_eq!(
        app.studio.preferences.workspace.inspector.dock,
        Dock::Floating
    );
    apply_layout(&mut app, &ctx);
    let layout = &app.studio.preferences.workspace.inspector;
    assert!(layout.x >= 8. && layout.x + layout.width <= 892.);
    assert!(layout.y >= 8. && layout.y + layout.height <= 692.);
    assert!(!app.workspace_ui.pending_layout);

    app.production.apply_layout = false;
    app.studio.preferences.workspace.inspector.x = 123.;
    apply_layout(&mut app, &ctx);
    assert_eq!(app.studio.preferences.workspace.inspector.x, 123.);
    assert!(!app.production.apply_layout);

    app.studio.preferences.workspace.production.dock = Dock::Hidden;
    app.production.open = false;
    app.open_production_tab(production_ui::Tab::Workspace);
    assert!(app.production.open);
    assert_eq!(
        app.studio.preferences.workspace.production.dock,
        Dock::Floating
    );
    assert!(app.workspace_ui.pending_layout);
}

#[test]
fn saved_workspace_palette_destination_preserves_its_original_geometry() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let mut saved = Workspace {
        name: "My drawing desk".into(),
        ..Workspace::default()
    };
    saved.inspector.dock = Dock::Floating;
    saved.inspector.x = 9000.;
    app.studio.preferences.workspaces.push(saved.clone());
    app.run_action(Action::CommandPalette);
    app.production.palette_query = "saved drawing desk".into();
    palette_pass(&mut app, &ctx, vec![key(egui::Key::Enter)]);
    assert_eq!(app.studio.preferences.workspace, saved);
    assert_eq!(app.production.layout_index, 1);
    assert!(app.workspace_ui.pending_layout);
    apply_layout(&mut app, &ctx);
    assert!(app.studio.preferences.workspace.inspector.x < 9000.);
    assert_eq!(app.studio.preferences.workspaces[1], saved);
    assert!(!app.history.undo(&mut app.project));
}

#[test]
fn palette_and_menu_shortcut_labels_follow_user_bindings() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let binding = app
        .studio
        .preferences
        .bindings
        .iter_mut()
        .find(|binding| binding.action == Action::MoveRig)
        .unwrap();
    binding.key = "F8".into();
    assert_eq!(app.action_shortcut(Action::MoveRig), "F8");
    assert!(app.action_shortcut(Action::SceneTools).is_empty());
    app.production.palette_query = "F8".into();
    let entries = app.palette_commands();
    assert_eq!(entries.len(), 1);
    assert!(matches!(
        entries[0].command,
        PaletteCommand::Action(Action::MoveRig)
    ));
    assert_eq!(entries[0].shortcut, "F8");
}

#[test]
fn loading_a_saved_scene_layout_restores_production_visibility() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let before = app.project.clone();
    let scene = WorkspacePreset::Scene.layout();
    let restored: Workspace =
        serde_json::from_str(&serde_json::to_string(&scene).unwrap()).unwrap();
    app.studio.preferences.workspaces.push(restored);
    assert!(!app.production.open);
    app.load_saved_workspace(1);
    assert!(app.production.open);
    assert!(app.studio.preferences.workspace.production_open);
    assert_eq!(app.studio.preferences.workspace.production.dock, Dock::Left);

    app.set_panel_visible(Panel::Production, false);
    assert!(!app.studio.preferences.workspace.production_open);
    app.open_production_tab(production_ui::Tab::Scene);
    assert!(app.studio.preferences.workspace.production_open);
    app.apply_workspace_layout(Workspace::default());
    assert!(!app.production.open);
    assert!(!app.studio.preferences.workspace.production_open);
    assert_eq!(app.project, before);
    assert!(!app.history.undo(&mut app.project));
}
