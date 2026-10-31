use super::*;
use rebonebase::{
    production::{self, SpriteAsset},
    project::Reference,
};
use std::sync::Arc;
#[test]
fn imported_3d_character_controls_and_move_rig_preserve_pose_and_undo() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let target = app.project.extras.canvas_origin
        + Vec2::new(
            app.project.rig.width as f32 * 0.5,
            app.project.rig.height as f32 * 0.5,
        );
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/character3d/fixtures/textured_skin.glb");
    let c = rebonebase::character3d::load(&path).unwrap();
    app.add_character3d(c).unwrap();
    let (min, max) = app.project.extras.characters3d[0].bounds(0.).unwrap();
    assert!(
        ((min + max) * 0.5 - target).length() < 1.,
        "Imported model should be centered in the original canvas: {:?} vs {:?}",
        (min + max) * 0.5,
        target
    );
    app.production.tab = production_ui::Tab::Characters;
    assert!(!app.project.extras.show_2d);
    assert_eq!(app.project.extras.characters3d.len(), 1);
    app.project.extras.characters3d[0]
        .joints
        .insert(1, [0., 0., 35.]);
    let before = app.project.clone();
    click(&mut app, &ctx, "Reset all 3D joint poses");
    assert!(app.project.extras.characters3d[0].joints.is_empty());
    assert!(app.history.undo(&mut app.project));
    assert_eq!(app.project, before);
    let start = Vec2::new(10., 10.);
    app.gesture = Some(Gesture {
        base: before.clone(),
        start,
        id: None,
        tail: false,
        tool: Tool::MoveRig,
        recorded: false,
    });
    app.drag(start + Vec2::new(4., 2.), false);
    assert_eq!(
        app.project.extras.characters3d[0].position,
        before.extras.characters3d[0].position + Vec2::new(4., 2.)
    );
    app.gesture = None;
    assert!(app.history.undo(&mut app.project));
    assert_eq!(app.project, before);
}
#[test]
fn visible_animation_templates_add_an_editable_clip_in_one_undo() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let before = app.project.clone();
    click(&mut app, &ctx, "Add animation template");
    assert_eq!(app.project.rig.clips.len(), before.rig.clips.len() + 1);
    assert_eq!(app.project.rig.active_clip().name, "Idle");
    assert!(app.history.undo(&mut app.project));
    assert_eq!(app.project, before);
}
#[test]
fn rejected_canvas_edits_leave_the_project_and_history_unchanged() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.project
        .extras
        .production
        .actors
        .push(production::SceneActor {
            id: 1,
            offset: Vec2::new(100000., 0.),
            ..Default::default()
        });
    app.production.actor = Some(1);
    let before = app.project.clone();
    app.gesture = Some(Gesture {
        base: before.clone(),
        start: Vec2::default(),
        id: None,
        tail: false,
        tool: Tool::MoveRig,
        recorded: false,
    });
    app.drag(Vec2::new(1., 0.), false);
    assert!(app.project == before, "out-of-bounds actor must not commit");
    assert!(!app.history.undo(&mut app.project));
    app.gesture = None;
    app.project.extras.production.actors.clear();
    for id in 1..=128 {
        app.project
            .extras
            .production
            .boxes
            .push(production::BoxTrack {
                id,
                ..Default::default()
            });
    }
    let before = app.project.clone();
    app.draw_collision(Vec2::new(1., 1.), Vec2::new(8., 8.));
    assert!(
        app.project == before,
        "129th region must be rejected before commit"
    );
    assert!(!app.history.undo(&mut app.project));
}
fn panel(app: &mut ReboneApp, ctx: &egui::Context, events: Vec<egui::Event>) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(1800., 2600.))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| app.production_ui(ui));
        },
    )
}
fn click(app: &mut ReboneApp, ctx: &egui::Context, label: &str) {
    let out = panel(app, ctx, vec![]);
    let pos = out
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.text() == label => {
                let p = t.pos + t.galley.size() * 0.5;
                s.clip_rect.contains(p).then_some(p)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing control {label}"));
    for pressed in [true, false] {
        panel(
            app,
            ctx,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ],
        );
    }
}
fn select(app: &mut ReboneApp, name: &str) {
    let id = app
        .project
        .rig
        .bones
        .iter()
        .find(|b| b.name == name)
        .unwrap()
        .id;
    app.select_bone(id, false);
}
fn seed_art(app: &mut ReboneApp) {
    for (id, color) in [(1, [255, 0, 0, 255]), (2, [0, 0, 255, 255])] {
        app.project.extras.production.assets.push(SpriteAsset {
            id,
            name: format!("Art{id}"),
            image: Reference {
                width: 2,
                height: 2,
                pixels: Arc::new(color.repeat(4)),
                x: 0.,
                y: 0.,
                scale: 1.,
                opacity: 1.,
            },
        });
    }
    app.production.asset = Some(1);
}
#[test]
fn choosing_a_prop_parent_waits_for_the_preserving_switch_action() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    seed_art(&mut app);
    select(&mut app, "forearm_L");
    app.production.tab = production_ui::Tab::Props;
    click(&mut app, &ctx, "Attach artwork to selected bone");
    let before = app.project.render_at(1.);
    click(&mut app, &ctx, "forearm_L");
    click(&mut app, &ctx, "World");
    assert!(
        app.project.render_at(1.).as_raw() == before.as_raw(),
        "choosing a proposed parent must not move artwork"
    );
    click(&mut app, &ctx, "Switch to parent without a jump");
    assert!(
        app.project.render_at(1.).as_raw() == before.as_raw(),
        "switching parent must retain the world position"
    );
}
#[test]
fn motion_tools_complete_a_controller_gait_and_transition_workflow() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    select(&mut app, "forearm_L");
    app.production.tab = production_ui::Tab::Motion;
    click(&mut app, &ctx, "Hold selected limb here (IK)");
    assert_eq!(app.project.extras.production.motion.contacts.len(), 1);
    app.frame = 5.;
    click(&mut app, &ctx, "Release selected contact here");
    assert_eq!(
        app.project.extras.production.motion.contacts[0].keys.len(),
        2
    );
    click(&mut app, &ctx, "Create chain from selected tip");
    assert_eq!(app.project.extras.production.motion.chains.len(), 1);
    click(&mut app, &ctx, "Add layer from clip");
    assert_eq!(app.project.extras.production.motion.layers.len(), 1);
    click(&mut app, &ctx, "Mask selected bones");
    click(&mut app, &ctx, "Add springs to selected bones");
    assert_eq!(app.project.extras.production.motion.springs.len(), 1);
    click(&mut app, &ctx, "Bake all clips and switch to FK");
    assert_eq!(app.project.extras.production.motion, Default::default());
    assert!(!app.project.rig.active_clip().tracks.is_empty());
    click(&mut app, &ctx, "Generate editable gait clip");
    assert_eq!(app.project.rig.clips.len(), 2, "{}", app.status);
    click(&mut app, &ctx, "Clean active loop");
    click(&mut app, &ctx, "Mirror entire clip");
    assert_eq!(app.project.rig.clips.len(), 3);
    app.production.transition_to = 0;
    click(&mut app, &ctx, "Create transition clip");
    assert_eq!(app.project.rig.clips.len(), 4);
    app.project.validate().unwrap();
}
#[test]
fn artwork_outfit_face_scene_and_regions_work_together_and_undo_pixel_edits() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    seed_art(&mut app);
    select(&mut app, "forearm_L");
    app.production.tab = production_ui::Tab::Props;
    click(&mut app, &ctx, "Attach artwork to selected bone");
    assert_eq!(app.project.extras.production.attachments.len(), 1);
    click(&mut app, &ctx, "Key prop state here");
    assert_eq!(
        production::prop_keys(
            &app.project.extras.production.attachments[0],
            app.project.rig.active_clip
        )
        .len(),
        1
    );
    app.production.tab = production_ui::Tab::Meshes;
    click(&mut app, &ctx, "Create weighted mesh from artwork");
    assert_eq!(app.project.extras.production.meshes.len(), 1);
    click(&mut app, &ctx, "Bind vertex to selected bones");
    app.production.tab = production_ui::Tab::Looks;
    click(&mut app, &ctx, "Capture current outfit");
    assert_eq!(app.project.extras.production.looks.len(), 1);
    select(&mut app, "head");
    app.production.tab = production_ui::Tab::Face;
    click(&mut app, &ctx, "Add facial sprite track");
    app.production.asset = Some(2);
    app.frame = 2.;
    click(&mut app, &ctx, "Add blink with chosen sprite");
    assert_eq!(app.project.extras.production.faces[0].keys.len(), 4);
    app.production.tab = production_ui::Tab::Scene;
    click(&mut app, &ctx, "Duplicate active character as actor");
    assert_eq!(app.project.extras.production.actors.len(), 1);
    click(&mut app, &ctx, "Key actor placement here");
    assert_eq!(app.project.extras.production.actors[0].keys.len(), 1);
    app.production.tab = production_ui::Tab::PixelsBoxes;
    click(&mut app, &ctx, "Add default collision region");
    click(&mut app, &ctx, "Key region here");
    assert_eq!(production::collision_shapes(&app.project, 2.).len(), 1);
    let before = app.project.clone();
    app.production.pixel_color = [255, 0, 255, 255];
    app.paint_stroke(Vec2::new(1., 1.), Vec2::new(2., 1.), true);
    assert_eq!(
        app.project
            .render_at(2.)
            .get_pixel(
                (1. - app.project.extras.canvas_origin.x) as u32,
                (1. - app.project.extras.canvas_origin.y) as u32
            )
            .0,
        [255, 0, 255, 255]
    );
    app.undo();
    assert_eq!(app.project, before);
    app.project.validate().unwrap();
}
#[test]
fn audio_markers_workspaces_and_custom_shortcuts_are_persistent_and_editable() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.production.tab = production_ui::Tab::AudioEvents;
    let wav = production::audio::encode_wav(&vec![1000; 8000], 8000, 1);
    app.project.extras.production.audio = Some(production::audio::decode_wav(&wav).unwrap());
    click(&mut app, &ctx, "Mark current frame");
    assert_eq!(app.project.extras.production.events.len(), 1);
    click(&mut app, &ctx, "Go to event");
    click(&mut app, &ctx, "Remove event");
    assert!(app.project.extras.production.events.is_empty());
    click(&mut app, &ctx, "Remove audio");
    assert!(app.project.extras.production.audio.is_none());
    app.production.tab = production_ui::Tab::Workspace;
    app.studio.preferences.workspace.name = "My layout".into();
    click(&mut app, &ctx, "Save current layout");
    assert_eq!(app.studio.preferences.workspaces.len(), 2);
    app.production.binding_key = "F8".into();
    click(&mut app, &ctx, "Apply shortcut");
    assert_eq!(
        app.studio
            .preferences
            .bindings
            .iter()
            .find(|b| b.action == rebonebase::workspace::Action::MoveRig)
            .unwrap()
            .key,
        "F8"
    );
    app.tool = Tool::Select;
    let _ = ctx.run(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::F8,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            ..Default::default()
        },
        |ctx| app.keyboard(ctx),
    );
    assert!(app.tool == Tool::MoveRig);
    click(&mut app, &ctx, "Restore default shortcuts");
    click(&mut app, &ctx, "Reset panel layout");
    click(&mut app, &ctx, "Load saved layout");
    let root = app.studio.store.root.canonicalize().unwrap();
    assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn production_panels_render_with_backward_compatible_defaults() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    for tab in production_ui::Tab::ALL {
        app.production.tab = tab;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(1400., 1200.))),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| app.production_ui(ui));
            },
        );
        app.project.validate().unwrap();
    }
}
#[test]
fn docking_layouts_and_command_palette_preserve_the_document() {
    use rebonebase::workspace::{Action, Dock};
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let before = app.project.clone();
    app.production.open = true;
    for dock in [
        Dock::Left,
        Dock::Right,
        Dock::Bottom,
        Dock::Floating,
        Dock::Hidden,
    ] {
        app.studio.preferences.workspace.production.dock = dock;
        app.studio.preferences.workspace.skeleton.dock = if dock == Dock::Left {
            Dock::Right
        } else {
            dock
        };
        app.production.apply_layout = true;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(1600., 1100.))),
                ..Default::default()
            },
            |ctx| app.show(ctx),
        );
        assert_eq!(app.project, before);
    }
    app.run_action(Action::CommandPalette);
    app.production.palette_query = "Move Rig".into();
    let _ = ctx.run(
        egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            ..Default::default()
        },
        |ctx| app.command_palette(ctx),
    );
    assert!(app.tool == Tool::MoveRig);
    assert!(!app.production.palette_open);
}
#[test]
fn remapped_move_rig_command_and_palette_keep_root_protection() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.run_action(rebonebase::workspace::Action::MoveRig);
    assert!(app.tool == Tool::MoveRig);
    app.run_action(rebonebase::workspace::Action::CommandPalette);
    assert!(app.production.palette_open);
    app.run_action(rebonebase::workspace::Action::Paint);
    assert!(app.tool == Tool::Pixel);
}
#[test]
fn pixel_strokes_are_undoable_and_collisions_export_as_metadata() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let before = app.project.clone();
    app.tool = Tool::Pixel;
    app.production.pixel_color = [255, 0, 255, 255];
    app.paint_stroke(Vec2::new(1., 1.), Vec2::new(4., 1.), true);
    assert_eq!(
        app.project
            .render_at(1.)
            .get_pixel(
                (3. - app.project.extras.canvas_origin.x) as u32,
                (1. - app.project.extras.canvas_origin.y) as u32
            )
            .0,
        [255, 0, 255, 255]
    );
    app.undo();
    assert_eq!(app.project, before);
}
