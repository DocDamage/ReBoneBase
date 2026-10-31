use super::*;
use rebonebase::animation::Playback;

fn app() -> (ReboneApp, egui::Context, u32) {
    let ctx = egui::Context::default();
    let mut app = ReboneApp::with_context(&ctx);
    let id = app.project.rig.bones.iter().find(|b| b.name == "forearm_L").unwrap().id;
    app.project.rig.active_clip_mut().tracks.clear();
    app.select_bone(id, false);
    (app, ctx, id)
}
#[derive(Clone, Copy)]
enum Host { Dope, TimelineCurves, StudioCurves, Timeline, Advanced }
fn pass(app: &mut ReboneApp, ctx: &egui::Context, events: Vec<egui::Event>, host: Host) -> egui::FullOutput {
    ctx.run(egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(1000., 700.))),
        events,
        ..Default::default()
    }, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| match host {
            Host::Dope => app.editable_dope(ui),
            Host::TimelineCurves => app.timeline_curves(ui),
            Host::Timeline => app.timeline_ui(ui),
            Host::Advanced => app.studio_animation(ui),
            Host::StudioCurves => {
                let before = app.project.clone();
                app.studio.curve_history_frame = false;
                ui.push_id("studio_curves", |ui| app.curve_editor(ui));
                app.commit_studio_edit(before);
            }
        });
    })
}
fn press(pos: Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
}
fn text_rect(out: &egui::FullOutput, name: &str) -> Option<Rect> {
    out.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(text) if text.galley.job.text == name => Some(Rect::from_min_size(text.pos, text.galley.size())),
        _ => None,
    })
}
fn diamond(out: &egui::FullOutput) -> Pos2 {
    out.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Path(path) if path.closed && path.points.len() == 4 && path.fill == ACCENT => {
            Some((path.points.iter().fold(EVec2::ZERO, |sum, p| sum + p.to_vec2()) / 4.).to_pos2())
        }
        _ => None,
    }).unwrap()
}
#[test]
fn seek_stops_playback_and_audio_and_synchronizes_variable_holds() {
    let (mut app, ctx, _) = app();
    let clip = app.project.rig.active_clip_mut();
    clip.length = 12;
    clip.fps = 10.;
    clip.durations.insert(1, 250);
    clip.durations.insert(2, 150);
    app.playing = true;
    app.production.audio_until = Some(Instant::now() + Duration::from_secs(10));
    app.production.audio_error = true;
    app.studio.playback_ms = 900.;
    app.skin_tex = Some(ctx.load_texture("seek-cache", egui::ColorImage::new([1, 1], vec![Color32::WHITE]), Default::default()));
    let project = app.project.clone();
    app.seek_frame(3.);
    assert!(!app.playing);
    assert!(app.production.audio_until.is_none());
    assert!(!app.production.audio_error);
    assert!(app.skin_tex.is_none());
    assert_eq!(app.frame, 3.);
    assert!(app.mode == Mode::Pose);
    assert_eq!(app.studio.playback_ms, 400.);
    assert_eq!(app.project, project);
    assert!(!app.history.can_undo());
    app.seek_frame(-5.);
    assert_eq!(app.frame, 1.);
    app.seek_frame(500.);
    assert_eq!(app.frame, 12.);
    app.seek_frame(f32::NAN);
    assert_eq!(app.frame, 1.);
    app.project.rig.active_clip_mut().playback = Playback::Reverse;
    app.seek_frame(3.);
    assert_eq!(app.studio.playback_ms, 900.);
    assert_eq!(app.project.rig.active_clip().frame_at_ms(app.studio.playback_ms).0, 3);
}
#[test]
fn switching_clips_clears_active_gestures_and_key_selection() {
    let (mut app, _, id) = app();
    let selected = KeySelection { bone: id, channel: Channel::Rotation, frame: 1. };
    app.project.rig.clips.push(Clip { length: 4, name: "Second".into(), ..Default::default() });
    app.studio.keys = vec![selected];
    app.studio.key_drag = Some(studio::KeyDrag { base: app.project.clone(), selected: vec![selected], start: 1., offset: 2., recorded: true });
    app.studio.curve_drag = Some(studio::CurveDrag { base: app.project.clone(), key: selected, handle: 0, recorded: true });
    app.playing = true;
    app.frame = 11.;
    app.production.audio_until = Some(Instant::now() + Duration::from_secs(1));
    app.activate_clip(1);
    assert_eq!(app.project.rig.active_clip, 1);
    assert_eq!(app.frame, 1.);
    assert_eq!(app.studio.playback_ms, 0.);
    assert!(!app.playing);
    assert!(app.production.audio_until.is_none());
    assert!(app.studio.keys.is_empty());
    assert!(app.studio.key_drag.is_none());
    assert!(app.studio.curve_drag.is_none());
    assert_eq!(app.selected, Some(id), "Changing clips retains the bone being animated");
}
#[test]
fn zoomed_scrolled_key_drag_uses_the_same_frame_coordinates() {
    let (mut app, ctx, id) = app();
    app.project.rig.active_clip_mut().length = 60;
    app.project.rig.active_clip_mut().set_key(id, Channel::Rotation, 10., 20.).unwrap();
    app.timeline.zoom = 2.;
    app.timeline.scroll_x = 200.;
    let before = app.project.clone();
    pass(&mut app, &ctx, vec![], Host::Dope);
    let center = diamond(&pass(&mut app, &ctx, vec![], Host::Dope));
    pass(&mut app, &ctx, vec![egui::Event::PointerMoved(center), press(center, true)], Host::Dope);
    let end = center + EVec2::new(96., 0.);
    pass(&mut app, &ctx, vec![egui::Event::PointerMoved(end)], Host::Dope);
    pass(&mut app, &ctx, vec![press(end, false)], Host::Dope);
    assert_eq!(app.project.rig.active_clip().tracks[0].keys[0].frame, 12.);
    app.undo();
    assert_eq!(app.project, before);
    assert!(!app.history.can_undo(), "One key drag should create one undo step");
}
#[test]
fn row_labels_stay_fixed_while_scrolling_and_select_bones() {
    let (mut app, ctx, _) = app();
    app.project.rig.active_clip_mut().length = 80;
    let bone = app.project.rig.bones[0].clone();
    let before = app.project.clone();
    let out = pass(&mut app, &ctx, vec![], Host::Dope);
    let initial = text_rect(&out, &bone.name).unwrap();
    app.timeline.scroll_x = 280.;
    let out = pass(&mut app, &ctx, vec![], Host::Dope);
    let scrolled = text_rect(&out, &bone.name).unwrap();
    assert!((initial.left() - scrolled.left()).abs() < 0.1);
    let pos = scrolled.center();
    pass(&mut app, &ctx, vec![egui::Event::PointerMoved(pos), press(pos, true)], Host::Dope);
    pass(&mut app, &ctx, vec![press(pos, false)], Host::Dope);
    assert_eq!(app.selected, Some(bone.id));
    assert!(app.studio.selection.contains(&bone.id));
    assert_eq!(app.project, before);
}
#[test]
fn track_filters_are_explicit_and_search_is_case_insensitive() {
    let (mut app, ctx, keyed) = app();
    let other = app.project.rig.bones[0].clone();
    let keyed_name = app.project.rig.bone(keyed).unwrap().name.clone();
    app.project.rig.active_clip_mut().set_key(keyed, Channel::Rotation, 1., 10.).unwrap();
    let out = pass(&mut app, &ctx, vec![], Host::Dope);
    assert!(text_rect(&out, &other.name).is_some(), "Selecting a bone must not implicitly filter tracks");
    app.timeline.filter = TrackFilter::Selected;
    let out = pass(&mut app, &ctx, vec![], Host::Dope);
    assert!(text_rect(&out, &other.name).is_none());
    assert!(text_rect(&out, &keyed_name).is_some());
    app.select_bone(other.id, false);
    app.timeline.filter = TrackFilter::Animated;
    let out = pass(&mut app, &ctx, vec![], Host::Dope);
    assert!(text_rect(&out, &other.name).is_none());
    assert!(text_rect(&out, &keyed_name).is_some());
    app.timeline.filter = TrackFilter::All;
    app.timeline.search = keyed_name.to_uppercase();
    let out = pass(&mut app, &ctx, vec![], Host::Dope);
    assert!(text_rect(&out, &other.name).is_none());
    assert!(text_rect(&out, &keyed_name).is_some());
}
#[test]
fn each_curve_host_validates_and_records_a_drag_once() {
    for host in [Host::TimelineCurves, Host::StudioCurves] {
        let (mut app, ctx, id) = app();
        app.project.rig.active_clip_mut().set_key(id, Channel::Rotation, 2., 0.).unwrap();
        app.project.rig.active_clip_mut().set_key(id, Channel::Rotation, 10., 30.).unwrap();
        let before = app.project.clone();
        let out = pass(&mut app, &ctx, vec![], host);
        let center = out.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Circle(circle) if circle.fill == ACCENT && circle.radius == 4. => Some(circle.center),
            _ => None,
        }).unwrap();
        pass(&mut app, &ctx, vec![egui::Event::PointerMoved(center), press(center, true)], host);
        let end = center + EVec2::new(80., -30.);
        pass(&mut app, &ctx, vec![egui::Event::PointerMoved(center + EVec2::new(35., -10.))], host);
        pass(&mut app, &ctx, vec![egui::Event::PointerMoved(end)], host);
        pass(&mut app, &ctx, vec![press(end, false)], host);
        assert_ne!(app.project, before);
        app.project.validate().unwrap();
        app.undo();
        assert_eq!(app.project, before);
        assert!(!app.history.can_undo(), "The curve host must not duplicate the drag’s undo record");
    }
}
#[test]
fn curve_host_records_key_creation_and_rendering_does_not_change_mode() {
    let (mut app, ctx, _) = app();
    for host in [Host::Timeline, Host::TimelineCurves, Host::Advanced] {
        app.mode = Mode::Rest;
        pass(&mut app, &ctx, vec![], host);
        assert!(app.mode == Mode::Rest, "Opening an editor must not change the document mode");
    }
    let before = app.project.clone();
    let out = pass(&mut app, &ctx, vec![], Host::TimelineCurves);
    let pos = text_rect(&out, "+ Key this channel at playhead").unwrap().center();
    pass(&mut app, &ctx, vec![egui::Event::PointerMoved(pos), press(pos, true)], Host::TimelineCurves);
    pass(&mut app, &ctx, vec![press(pos, false)], Host::TimelineCurves);
    assert_ne!(app.project, before);
    app.project.validate().unwrap();
    app.undo();
    assert_eq!(app.project, before);
    assert!(!app.history.can_undo());
}
#[test]
fn retained_source_keys_are_hidden_and_excluded_from_navigation_and_selection() {
    let (mut app, ctx, id) = app();
    app.project.rig.active_clip_mut().length = 12;
    for frame in [1., 6., 10.] { app.project.rig.active_clip_mut().set_key(id, Channel::Rotation, frame, frame).unwrap(); }
    rebonebase::production::edit_clip(&mut app.project, rebonebase::production::ClipEdit::Trim { index: 0, start: 3, end: 7 }).unwrap();
    assert_eq!(app.project.rig.active_clip().tracks[0].keys.len(), 3);
    app.activate_clip(0);
    let out = pass(&mut app, &ctx, vec![], Host::Dope);
    let diamonds = out.shapes.iter().filter(|shape| matches!(&shape.shape, egui::Shape::Path(path) if path.closed && path.points.len() == 4 && path.fill == ACCENT)).count();
    assert_eq!(diamonds, 1);
    let out = pass(&mut app, &ctx, vec![], Host::TimelineCurves);
    let points = out.shapes.iter().filter(|shape| matches!(&shape.shape, egui::Shape::Circle(circle) if circle.radius == 4. && circle.fill == ACCENT)).count();
    assert_eq!(points, 1);
    app.select_all_timeline_keys();
    assert_eq!(app.studio.keys.len(), 1);
    app.seek_key(true);
    assert_eq!(app.frame, 4.);
    app.seek_key(true);
    assert_eq!(app.frame, 4., "The retained future key must not become a navigation target");
    app.seek_key(false);
    assert_eq!(app.frame, 4., "The retained past key must not become a navigation target");
}
#[test]
fn animated_filter_retains_source_motion_without_visible_key_endpoints() {
    let (mut app, ctx, id) = app();
    let name = app.project.rig.bone(id).unwrap().name.clone();
    app.project.rig.active_clip_mut().length = 12;
    app.project.rig.active_clip_mut().set_key(id, Channel::Rotation, 1., 0.).unwrap();
    app.project.rig.active_clip_mut().set_key(id, Channel::Rotation, 12., 110.).unwrap();
    rebonebase::production::edit_clip(&mut app.project, rebonebase::production::ClipEdit::Trim { index: 0, start: 4, end: 8 }).unwrap();
    let clip = app.project.rig.active_clip();
    assert!(clip.tracks[0].keys.iter().all(|key| !(1. ..=clip.length as f32).contains(&key.frame)));
    assert_ne!(clip.sample(1.)[&id].rotation, clip.sample(5.)[&id].rotation, "The retained source endpoints still animate the visible window");
    app.timeline.filter = TrackFilter::Animated;
    let out = pass(&mut app, &ctx, vec![], Host::Dope);
    assert!(text_rect(&out, &name).is_some(), "Animated must retain a moving source track even when its endpoints are hidden");
    let diamonds = out.shapes.iter().filter(|shape| matches!(&shape.shape, egui::Shape::Path(path) if path.closed && path.points.len() == 4 && path.fill == ACCENT)).count();
    assert_eq!(diamonds, 0, "The source endpoints must remain hidden");
    app.select_all_timeline_keys();
    assert!(app.studio.keys.is_empty());
}
#[test]
fn fit_shows_both_clip_endpoints_in_a_narrow_panel() {
    let (mut app, ctx, id) = app();
    app.project.rig.active_clip_mut().length = 240;
    app.project.rig.active_clip_mut().set_key(id, Channel::Rotation, 1., 0.).unwrap();
    app.project.rig.active_clip_mut().set_key(id, Channel::Rotation, 240., 30.).unwrap();
    app.timeline.fit = true;
    let out = ctx.run(egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(320., 700.))),
        ..Default::default()
    }, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| app.editable_dope(ui));
    });
    let diamonds = out.shapes.iter().filter(|shape| matches!(&shape.shape, egui::Shape::Path(path) if path.closed && path.points.len() == 4 && path.fill == ACCENT)).count();
    assert_eq!(diamonds, 2, "Fit must draw both frame 1 and frame 240 inside the narrow viewport");
    assert!(app.timeline.zoom * 24. < 1., "Fitting this panel requires subpixel frame spacing");
    assert!(app.timeline.scroll_x.abs() < 0.01);
}
#[test]
fn each_curve_host_records_a_numeric_field_drag_once() {
    for host in [Host::TimelineCurves, Host::StudioCurves] {
        let (mut app, ctx, id) = app();
        app.project.rig.active_clip_mut().set_key(id, Channel::Rotation, 2., 0.).unwrap();
        app.studio.keys = vec![KeySelection { bone: id, channel: Channel::Rotation, frame: 2. }];
        let before = app.project.clone();
        let out = pass(&mut app, &ctx, vec![], host);
        let center = out.shapes.iter().find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.job.text.starts_with("Value ") => Some(Rect::from_min_size(text.pos, text.galley.size()).center()),
            _ => None,
        }).expect("The selected curve key should expose its Value field");
        pass(&mut app, &ctx, vec![egui::Event::PointerMoved(center), press(center, true)], host);
        pass(&mut app, &ctx, vec![egui::Event::PointerMoved(center + EVec2::new(30., 0.))], host);
        let end = center + EVec2::new(70., 0.);
        pass(&mut app, &ctx, vec![egui::Event::PointerMoved(end)], host);
        pass(&mut app, &ctx, vec![press(end, false)], host);
        assert_ne!(app.project, before, "Dragging the Value field should edit its key");
        app.project.validate().unwrap();
        app.undo();
        assert_eq!(app.project, before);
        assert!(!app.history.can_undo(), "One numeric field drag should create one undo step in either curve host");
    }
}
