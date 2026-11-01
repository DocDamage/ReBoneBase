use super::*;
use rebonebase::{editing::KeySelection, features::ReferenceSequence, project::Reference};
use std::{collections::BTreeSet, sync::Arc};

pub(super) fn panel(
    app: &mut ReboneApp,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    tab: studio::Tab,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(1400., 1500.))),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| match tab {
                studio::Tab::Animation => app.studio_animation(ui),
                studio::Tab::Appearance => app.studio_appearance(ui),
                studio::Tab::Library => app.studio_library(ui, ctx),
                studio::Tab::References => app.studio_references(ui),
                studio::Tab::Files => app.studio_files(ui),
                studio::Tab::Export => app.studio_export(ui),
                studio::Tab::Rig => app.studio_rig(ui),
            });
        },
    )
}
pub(super) fn click(app: &mut ReboneApp, ctx: &egui::Context, tab: studio::Tab, label: &str) {
    let out = panel(app, ctx, vec![], tab);
    let pos = out
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                let center = text.pos + text.galley.size() * 0.5;
                s.clip_rect.contains(center).then_some(center)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("visible control missing: {label}"));
    panel(
        app,
        ctx,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            },
        ],
        tab,
    );
    panel(
        app,
        ctx,
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }],
        tab,
    );
}
fn cleanup(app: &ReboneApp) {
    let root = app.studio.store.root.canonicalize().unwrap();
    assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn animation_buttons_complete_a_clip_and_key_editing_workflow() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let id = app
        .project
        .rig
        .bones
        .iter()
        .find(|b| b.name == "forearm_L")
        .unwrap()
        .id;
    app.select_bone(id, false);
    let tab = studio::Tab::Animation;
    app.project
        .rig
        .active_clip_mut()
        .set_key(id, Channel::Rotation, 1., 0.)
        .unwrap();
    app.project
        .rig
        .active_clip_mut()
        .set_key(id, Channel::Rotation, 5., 40.)
        .unwrap();
    click(&mut app, &ctx, tab, "Select all keys");
    assert_eq!(app.studio.keys.len(), 2);
    click(&mut app, &ctx, tab, "Copy");
    app.frame = 8.;
    click(&mut app, &ctx, tab, "Paste at playhead");
    assert_eq!(app.project.rig.active_clip().tracks[0].keys.len(), 4);
    app.studio.key_offset = 1.;
    app.studio.key_scale = 1.;
    click(&mut app, &ctx, tab, "Retime selection");
    assert_eq!(app.studio.keys[0].frame, 2.);
    click(&mut app, &ctx, tab, "Snap keys");
    assert!(app.studio.keys.is_empty());
    click(&mut app, &ctx, tab, "Select all keys");
    click(&mut app, &ctx, tab, "Delete keys");
    assert!(
        app.project
            .rig
            .active_clip()
            .tracks
            .iter()
            .all(|t| t.keys.is_empty())
    );
    click(&mut app, &ctx, tab, "+ Key this channel at playhead");
    assert_eq!(app.studio.keys.len(), 1);
    click(&mut app, &ctx, tab, "Fit values");
    click(&mut app, &ctx, tab, "Clear key selection");
    click(&mut app, &ctx, tab, "Duplicate clip");
    assert_eq!(app.project.rig.clips.len(), 2);
    click(&mut app, &ctx, tab, "Move left");
    assert_eq!(app.project.rig.active_clip, 0);
    click(&mut app, &ctx, tab, "Move right");
    assert_eq!(app.project.rig.active_clip, 1);
    app.studio.trim_start = 2;
    app.studio.trim_end = 9;
    click(&mut app, &ctx, tab, "Trim to range");
    assert_eq!(app.project.rig.active_clip().length, 8);
    app.frame = 4.;
    click(&mut app, &ctx, tab, "Split at current frame");
    assert_eq!(app.project.rig.clips.len(), 3);
    click(&mut app, &ctx, tab, "Delete clip");
    assert_eq!(app.project.rig.clips.len(), 2);
    app.project.rig.active_clip_mut().durations.insert(1, 500);
    app.frame = 1.;
    click(&mut app, &ctx, tab, "Reset frame hold");
    assert!(app.project.rig.active_clip().durations.is_empty());
    click(&mut app, &ctx, tab, "Show root-motion path");
    assert!(app.studio.show_path);
    click(&mut app, &ctx, tab, "Convert clip to in-place");
    app.project.validate().unwrap();
}

#[test]
fn appearance_library_and_presets_are_usable_across_projects() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.init_services();
    let id = app
        .project
        .rig
        .bones
        .iter()
        .find(|b| b.name == "forearm_L")
        .unwrap()
        .id;
    app.select_bone(id, false);
    let count = app.project.extras.palette.len();
    click(
        &mut app,
        &ctx,
        studio::Tab::Appearance,
        "Capture group colors",
    );
    assert!(app.project.extras.palette.len() > count);
    click(&mut app, &ctx, studio::Tab::Appearance, "Add swatch");
    click(&mut app, &ctx, studio::Tab::Appearance, "Remove last");
    let color = app.project.extras.palette[0];
    click(&mut app, &ctx, studio::Tab::Appearance, "Apply 1");
    let part = app.project.rig.bone(id).unwrap().part;
    assert_eq!(
        app.project
            .rig
            .parts
            .iter()
            .find(|p| p.id == part)
            .unwrap()
            .color,
        color
    );
    app.studio.library_name = "Reusable pose".into();
    app.studio.library_tags = "walk, humanoid".into();
    app.project
        .rig
        .active_clip_mut()
        .set_key(id, Channel::Rotation, 1., 33.)
        .unwrap();
    click(
        &mut app,
        &ctx,
        studio::Tab::Library,
        "Save current pose to library",
    );
    assert_eq!(app.studio.library.len(), 1);
    click(&mut app, &ctx, studio::Tab::Library, "Favorite");
    assert!(app.studio.library[0].favorite);
    app.project.rig.active_clip_mut().tracks.clear();
    click(
        &mut app,
        &ctx,
        studio::Tab::Library,
        "Apply pose by bone name",
    );
    assert_eq!(app.project.rig.active_clip().sample(1.)[&id].rotation, 33.);
    click(&mut app, &ctx, studio::Tab::Library, "Refresh");
    app.studio.search = "absent".into();
    panel(&mut app, &ctx, vec![], studio::Tab::Library);
    app.studio.search.clear();
    app.studio.library_name = "Reusable rig".into();
    click(&mut app, &ctx, studio::Tab::Library, "Save rig to library");
    assert_eq!(app.studio.library.len(), 2);
    app.studio.batch.name = "Pixel game".into();
    click(&mut app, &ctx, studio::Tab::Export, "Save / replace preset");
    assert_eq!(app.studio.preferences.presets.len(), 2);
    app.studio.batch.columns = 4;
    click(&mut app, &ctx, studio::Tab::Export, "Save / replace preset");
    assert_eq!(app.studio.preferences.presets[1].columns, 4);
    click(
        &mut app,
        &ctx,
        studio::Tab::Export,
        "Pack frames into atlas pages",
    );
    assert!(app.studio.batch.atlas.packed);
    click(
        &mut app,
        &ctx,
        studio::Tab::Export,
        "Reuse identical frames",
    );
    assert!(!app.studio.batch.atlas.reuse);
    app.studio.batch.atlas.max_size = 512;
    click(&mut app, &ctx, studio::Tab::Export, "Save / replace preset");
    let saved = app.studio.preferences.presets[1].atlas;
    assert!(saved.packed && !saved.reuse && saved.max_size == 512);
    let reloaded: studio::Preferences =
        serde_json::from_value(serde_json::to_value(&app.studio.preferences).unwrap()).unwrap();
    assert_eq!(reloaded.presets[1].atlas, saved);
    app.studio.preferences.engine_atlas.packed = true;
    match app.export_target(export_ui::Kind::Engine) {
        rebonebase::export::Target::Engine(_, layout) => assert!(layout.packed),
        other => panic!("unexpected target {other:?}"),
    }
    click(
        &mut app,
        &ctx,
        studio::Tab::Export,
        "Reset to game-asset preset",
    );
    assert_eq!(app.studio.batch.columns, 8);
    click(&mut app, &ctx, studio::Tab::Files, "Snapshot now");
    click(
        &mut app,
        &ctx,
        studio::Tab::Files,
        "Refresh recovery snapshots",
    );
    assert!(!app.studio.recoveries.is_empty());
    click(&mut app, &ctx, studio::Tab::Files, "Save settings");
    cleanup(&app);
}

fn render(app: &mut ReboneApp, ctx: &egui::Context) {
    let _ = ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(1400., 1000.))),
            ..Default::default()
        },
        |ctx| app.show(ctx),
    );
}
fn reference() -> Reference {
    Reference {
        width: 1,
        height: 1,
        pixels: Arc::new(vec![255, 0, 0, 255]),
        x: 0.,
        y: 0.,
        scale: 1.,
        opacity: 0.5,
    }
}

#[test]
fn studio_tabs_render_full_project_without_spurious_edits() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.init_services();
    let id = app
        .project
        .rig
        .bones
        .iter()
        .find(|b| b.name == "forearm_L")
        .unwrap()
        .id;
    app.select_bone(id, false);
    app.add_pin(id, false);
    app.project.extras.references = Some(ReferenceSequence {
        frames: vec![reference(), reference()],
        fps: 12.,
        start_ms: 0,
        looped: true,
    });
    app.project.capture_pose("Stored".into(), 1.).unwrap();
    app.project
        .rig
        .active_clip_mut()
        .set_key(id, Channel::Rotation, 1., 10.)
        .unwrap();
    app.project
        .rig
        .active_clip_mut()
        .set_key(id, Channel::Rotation, 8., 30.)
        .unwrap();
    app.studio.keys = vec![KeySelection {
        bone: id,
        channel: Channel::Rotation,
        frame: 1.,
    }];
    app.studio.show_path = true;
    app.onion = true;
    app.frame = 4.;
    app.studio.open = true;
    let entry = app
        .studio
        .store
        .save_library("Humanoid pose", &app.project, vec!["walk".into()], true)
        .unwrap();
    app.refresh_library();
    app.studio
        .store
        .write_recovery("previous-session", &app.project, None)
        .unwrap();
    app.studio.recoveries = app.studio.store.recoveries().unwrap();
    let before = app.project.clone();
    for tab in [
        studio::Tab::Rig,
        studio::Tab::Animation,
        studio::Tab::Appearance,
        studio::Tab::Library,
        studio::Tab::References,
        studio::Tab::Files,
        studio::Tab::Export,
    ] {
        app.studio.tab = tab;
        render(&mut app, &ctx);
        render(&mut app, &ctx);
        assert_eq!(app.project, before, "tab changed project");
    }
    assert!(app.studio.thumbnails.contains_key(&entry));
    assert!(app.skin_tex.is_some());
    assert!(app.onion_tex.is_some());
    for channel in [
        Channel::X,
        Channel::Y,
        Channel::Scale,
        Channel::Z,
        Channel::Visibility,
    ] {
        app.studio.channel = channel;
        app.studio.tab = studio::Tab::Animation;
        render(&mut app, &ctx);
    }
    std::fs::remove_dir_all(&app.studio.store.root).unwrap();
}

#[test]
fn key_clipboard_and_group_rotation_have_transactional_undo() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let ids: Vec<_> = app
        .project
        .rig
        .bones
        .iter()
        .filter(|b| b.name == "upper_arm_L" || b.name == "upper_arm_R")
        .map(|b| b.id)
        .collect();
    assert_eq!(ids.len(), 2);
    app.studio.selection = BTreeSet::from_iter(ids.clone());
    app.selected = Some(ids[0]);
    let baseline = app.project.clone();
    app.record();
    studio_rig::rotate_selection(&mut app.project.rig, &ids, 20., 1., Mode::Pose).unwrap();
    let posed = app.project.clone();
    app.studio.keys = ids
        .iter()
        .map(|id| KeySelection {
            bone: *id,
            channel: Channel::Rotation,
            frame: 1.,
        })
        .collect();
    app.copy_timeline_keys();
    app.frame = 5.;
    app.paste_timeline_keys(true);
    assert!(
        app.project
            .rig
            .active_clip()
            .tracks
            .iter()
            .all(|t| t.keys.iter().any(|k| k.frame == 5.))
    );
    app.undo();
    assert_eq!(app.project, posed);
    app.undo();
    assert_eq!(app.project, baseline);
}

#[test]
fn autosave_startup_recovery_and_background_generation_are_safe() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.project.rig.name = "Recover me".into();
    app.studio.last_autosave = Instant::now() - Duration::from_secs(601);
    app.tick_services(&ctx);
    assert_eq!(app.studio.store.recoveries().unwrap().len(), 1);
    app.init_services();
    assert!(app.studio.open);
    assert!(app.studio.tab == studio::Tab::Files);
    let finish = |app: &mut ReboneApp, generation: u64| {
        app.studio.job = Some(studio::Job {
            job: rebonebase::jobs::Job::spawn("references", |_| {
                Ok(ReferenceSequence {
                    frames: vec![reference()],
                    fps: 12.,
                    start_ms: 0,
                    looped: true,
                })
            }),
            generation,
        });
        let started = Instant::now();
        while app.studio.job.is_some() && started.elapsed() < Duration::from_secs(10) {
            app.tick_services(&ctx);
        }
        assert!(app.studio.job.is_none());
    };
    let generation = app.studio.generation;
    finish(&mut app, generation + 1);
    assert!(app.project.extras.references.is_none());
    finish(&mut app, generation);
    assert!(app.project.extras.references.is_some());
    app.undo();
    assert!(app.project.extras.references.is_none());
    app.save_preferences();
    std::fs::remove_dir_all(&app.studio.store.root).unwrap();
}

#[test]
fn persistent_pin_drag_and_move_rig_keep_targets_consistent() {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let id = app
        .project
        .rig
        .bones
        .iter()
        .find(|b| b.name == "forearm_L")
        .unwrap()
        .id;
    app.add_pin(id, false);
    let original = app.project.clone();
    let start = app.project.extras.pins[0].target;
    let target = start + Vec2::new(-3., -4.);
    app.gesture = Some(Gesture {
        base: original.clone(),
        start,
        id: Some(id),
        tail: true,
        tool: Tool::IK,
        recorded: false,
    });
    app.drag(target, false);
    assert_eq!(app.project.extras.pins[0].target, target);
    app.undo();
    assert_eq!(app.project, original);
    app.mode = Mode::Rest;
    app.project.extras.ground = Some(60.);
    let baseline = app.project.clone();
    app.gesture = Some(Gesture {
        base: baseline.clone(),
        start: Vec2::default(),
        id: None,
        tail: false,
        tool: Tool::MoveRig,
        recorded: false,
    });
    app.drag(Vec2::new(2., 3.), false);
    assert_eq!(app.project.extras.pins[0].target, start + Vec2::new(2., 3.));
    assert_eq!(app.project.extras.ground, Some(63.));
    app.undo();
    assert_eq!(app.project, baseline);
}

fn export_window(
    app: &mut ReboneApp,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(1400., 1500.))),
            events,
            ..Default::default()
        },
        |ctx| app.export_window(ctx),
    )
}
fn click_export(app: &mut ReboneApp, ctx: &egui::Context, label: &str) {
    let out = export_window(app, ctx, vec![]);
    let pos = out
        .shapes
        .iter()
        .find_map(|s| match &s.shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                let center = text.pos + text.galley.size() * 0.5;
                s.clip_rect.contains(center).then_some(center)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("visible control missing: {label}"));
    let press = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    export_window(app, ctx, vec![egui::Event::PointerMoved(pos), press(true)]);
    export_window(app, ctx, vec![press(false)]);
}

#[test]
fn directional_sets_are_configured_in_the_export_window_without_editing_the_project() {
    use rebonebase::directions::{Count, Slot, Source};
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    app.init_services();
    let before = app.project.clone();
    app.open_export(export_ui::Kind::Directions);
    export_window(&mut app, &ctx, vec![]);
    // A flat project starts with a 2D clip assignment.
    let Source::Clips { slots, .. } = &app.exports.directions.set.source else {
        panic!("2D projects use authored clips");
    };
    assert_eq!(slots.len(), 4);
    click_export(&mut app, &ctx, "8");
    export_window(&mut app, &ctx, vec![]);
    assert_eq!(app.exports.directions.set.count, Count::Eight);
    let Source::Clips { slots, .. } = &mut app.exports.directions.set.source else {
        unreachable!()
    };
    assert_eq!(slots.len(), 8);
    for (i, slot) in slots.iter_mut().enumerate() {
        *slot = if i < 4 {
            Slot::Clip(0)
        } else {
            Slot::Mirror(i - 4)
        };
    }
    click_export(&mut app, &ctx, "Preview orientations");
    assert_eq!(
        app.exports.directions.previews.len(),
        8,
        "{:?}",
        app.exports.directions.preview_error
    );
    match app.export_target(export_ui::Kind::Directions) {
        rebonebase::export::Target::Directions(d) => {
            assert_eq!(d.set.count, Count::Eight);
            assert_eq!(
                rebonebase::directions::frame_count(&app.project, &d),
                8 * app.project.rig.active_clip().frame_sequence().len() as u64
            );
        }
        other => panic!("unexpected target {other:?}"),
    }
    assert_eq!(
        app.project, before,
        "configuring directions never edits the project"
    );
}
