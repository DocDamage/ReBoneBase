use super::*;
use rebonebase::character3d::Model3d;
use rebonebase::motion::calibration::{
    Calibration3d, CalibrationReport, ReferencePose, RootScale, RootTravel, UpAxis,
};
use std::sync::{Arc, mpsc};
type Source = (PathBuf, Vec<String>, Motion);
pub(super) struct SourceJob {
    generation: u64,
    character: u64,
    receiver: mpsc::Receiver<Result<Source, String>>,
}
struct ApplyJob {
    generation: u64,
    base: Project,
    receiver: mpsc::Receiver<Result<Project, String>>,
}
pub(super) struct AnimationDialog {
    source: PathBuf,
    names: Vec<String>,
    motion: Motion,
    selected: Vec<bool>,
    index: usize,
    character: u64,
    model: Arc<Model3d>,
    calibration: Calibration3d,
    /// Target joint whose calibration offset is being edited.
    offset_joint: usize,
    saved: Option<usize>,
    also_2d: bool,
    projection: Projection,
    frame: f32,
    preview: Option<Project>,
    texture: Option<egui::TextureHandle>,
    pending: Option<ApplyJob>,
    error: Option<String>,
}
impl ReboneApp {
    pub(super) fn import_character_animation(&mut self) {
        let Some(character) = self.production.character else {
            return;
        };
        if self.production.animation_source.is_some() {
            return;
        }
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Animation",
                &["fbx", "bvh", "gltf", "glb", "anim", "uasset"],
            )
            .pick_file()
        else {
            return;
        };
        let (tx, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| {
                let names = Motion::animation_names(&path).map_err(|e| e.to_string())?;
                let motion = Motion::load_animation(&path, 0).map_err(|e| e.to_string())?;
                Ok((path, names, motion))
            })();
            let _ = tx.send(result);
        });
        self.production.animation_source = Some(SourceJob {
            generation: self.studio.generation,
            character,
            receiver,
        });
        self.status = "Reading animation source…".into();
    }
    fn commit_character_animation(
        &mut self,
        base: &Project,
        next: Project,
        generation: u64,
    ) -> Result<(), String> {
        if generation != self.studio.generation || &self.project != base {
            return Err("The project changed during import. Preview and import again to preserve your edits.".into());
        }
        next.validate().map_err(|e| e.to_string())?;
        self.history.record(&self.project);
        self.project = next;
        self.activate_clip(self.project.rig.active_clip);
        self.fit = true;
        self.status = "Imported animation clips · choose a timeline clip and press Play".into();
        Ok(())
    }
    pub(super) fn animation_import_dialog(&mut self, ctx: &egui::Context) {
        if let Some(job) = self.production.animation_source.take() {
            match job.receiver.try_recv() {
                Ok(Ok((source, names, motion))) if job.generation == self.studio.generation => {
                    if let Some(c) = self
                        .project
                        .extras
                        .characters3d
                        .iter()
                        .find(|c| c.id == job.character)
                    {
                        let calibration = Calibration3d::automatic(&c.model, &motion);
                        let mut selected = vec![false; names.len()];
                        selected[0] = true;
                        self.production.animation_dialog = Some(AnimationDialog {
                            source,
                            names,
                            motion,
                            selected,
                            index: 0,
                            character: c.id,
                            model: c.model.clone(),
                            calibration,
                            offset_joint: 0,
                            saved: None,
                            also_2d: false,
                            projection: Projection::Front,
                            frame: 1.,
                            preview: None,
                            texture: None,
                            pending: None,
                            error: None,
                        });
                    }
                }
                Ok(Ok(_)) => {
                    self.status = "Animation source discarded after switching projects".into()
                }
                Ok(Err(e)) => self.status = format!("Animation import: {e}"),
                Err(mpsc::TryRecvError::Empty) => {
                    self.production.animation_source = Some(job);
                    ctx.request_repaint_after(Duration::from_millis(50));
                }
                Err(_) => self.status = "Animation source reader stopped".into(),
            }
        }
        let Some(mut d) = self.production.animation_dialog.take() else {
            return;
        };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            return;
        }
        let Some(c) = self
            .project
            .extras
            .characters3d
            .iter()
            .find(|c| c.id == d.character)
        else {
            return;
        };
        if c.model != d.model {
            self.status = "The target model changed; reopen animation import".into();
            return;
        }
        if let Some(job) = d.pending.take() {
            match job.receiver.try_recv() {
                Ok(Ok(next)) => {
                    match self.commit_character_animation(&job.base, next, job.generation) {
                        Ok(()) => return,
                        Err(e) => d.error = Some(e),
                    }
                }
                Ok(Err(e)) => d.error = Some(e),
                Err(mpsc::TryRecvError::Empty) => {
                    d.pending = Some(job);
                    ctx.request_repaint_after(Duration::from_millis(50));
                }
                Err(_) => d.error = Some("Animation import worker stopped".into()),
            }
        }
        let mut open = true;
        egui::Window::new("Import animation onto 3D character").open(&mut open).default_size([680.,760.]).max_height((ctx.content_rect().height()-40.).max(240.)).show(ctx,|ui|{
            egui::ScrollArea::vertical().id_salt("external-animation").show(ui,|ui|{
                ui.heading(&d.motion.name);
                ui.label("Retarget motion to the existing model. Its mesh, textures and joint attachments stay in place.");
                let previous_options=(d.calibration.clone(),d.also_2d,d.projection);
                let (_, report) = d.calibration.resolve(&d.model, &d.motion);
                ui.add_enabled_ui(d.pending.is_none(),|ui|{
                    let old=d.index;
                    egui::ComboBox::from_label("Source animation").selected_text(&d.names[d.index]).show_ui(ui,|ui|{
                        for(i,name)in d.names.iter().enumerate(){ui.selectable_value(&mut d.index,i,name);}
                    });
                    if old!=d.index {
                        // The calibration names its joints, so it carries over unchanged.
                        match Motion::load_animation(&d.source,d.index) {
                            Ok(m)=>{d.motion=m;d.preview=None;d.texture=None;d.error=None;}
                            Err(e)=>{d.index=old;d.error=Some(e.to_string());}
                        }
                    }
                    self.calibration_controls(ui,&mut d);
                    ui.checkbox(&mut d.also_2d,"Also animate my 2D rig in the same clips (automatic joint mapping)");
                    if d.also_2d {
                        egui::ComboBox::from_label("2D projection").selected_text(format!("{:?}",d.projection)).show_ui(ui,|ui|{for p in [Projection::Front,Projection::Side,Projection::ThreeQuarter]{ui.selectable_value(&mut d.projection,p,format!("{p:?}"));}});
                        ui.label("For custom 2D joint mapping, use File → Import animation for 2D rig.");
                    }
                    report_ui(ui,&report);
                    egui::CollapsingHeader::new("Joint mapping").show(ui,|ui|{
                        let names=d.motion.joint_names();
                        egui::ScrollArea::vertical().id_salt("3d-joint-mapping").max_height(160.).show_rows(ui,22.,d.model.nodes.len(),|ui,range|{
                            for i in range {
                                let target=d.model.nodes[i].name.clone();
                                let mut chosen=d.calibration.mapping.get(&target).cloned();
                                let old=chosen.clone();
                                ui.horizontal(|ui|{ui.label(&target);egui::ComboBox::from_id_salt(("external-joint",i)).selected_text(chosen.as_deref().unwrap_or("Unmapped")).show_ui(ui,|ui|{ui.selectable_value(&mut chosen,None,"Unmapped");for name in &names{ui.selectable_value(&mut chosen,Some(name.clone()),name);}});});
                                if chosen!=old {d.calibration.set_source(&target,chosen.as_deref());}
                            }
                        });
                    });
                    if ui.button("Preview mapped animation").clicked() {
                        let mut preview=self.project.clone();
                        match rebonebase::animation_import::import_calibrated(&mut preview,d.character,std::slice::from_ref(&d.motion),&d.calibration,d.also_2d,d.projection){Ok(_)=>{d.preview=Some(preview);d.texture=None;d.frame=1.;d.error=None;},Err(e)=>d.error=Some(e.to_string())}
                    }
                    if let Some(preview)=&d.preview {
                        let changed=ui.add(egui::Slider::new(&mut d.frame,1. ..=preview.rig.active_clip().length as f32).text("Preview frame")).changed();
                        if changed || (d.texture.is_none() && d.error.is_none()) {
                            match preview.try_render_at(d.frame) {
                                Ok(img) => {
                                    d.error = None;
                                    d.texture = Some(ctx.load_texture("external-animation-preview",egui::ColorImage::from_rgba_unmultiplied([img.width()as usize,img.height()as usize],img.as_raw()),egui::TextureOptions::NEAREST));
                                }
                                Err(error) => {
                                    d.texture = None;
                                    d.error = Some(format!("Preview failed: {error:#}"));
                                }
                            }
                        }
                        if let Some(t)=&d.texture {ui.add(egui::Image::new(t).fit_to_exact_size(EVec2::new(220.,220.)));}
                    }
                    ui.separator();ui.label("Clips to import · each uses the same calibration");
                    ui.horizontal(|ui|{if ui.small_button("Select all").clicked(){d.selected.fill(true);}
if ui.small_button("Only previewed").clicked(){d.selected.fill(false);d.selected[d.index]=true;}});
                    egui::ScrollArea::vertical().id_salt("3d-animation-choices").max_height(95.).show(ui,|ui|{for(i,name)in d.names.iter().enumerate(){ui.checkbox(&mut d.selected[i],name);}});
                    if ui.add_enabled(d.selected.iter().any(|b|*b),egui::Button::new("Import selected animation clips")).clicked(){
                        let base=self.project.clone();let mut candidate=base.clone();let path=d.source.clone();let indices:Vec<_>=d.selected.iter().enumerate().filter_map(|(i,b)|b.then_some(i)).collect();let calibration=d.calibration.clone();let id=d.character;let both=d.also_2d;let projection=d.projection;
                        let(tx,receiver)=mpsc::channel();std::thread::spawn(move||{let result=(||{let motions:Result<Vec<_>,_>=indices.into_iter().map(|i|Motion::load_animation(&path,i).map_err(|e|e.to_string())).collect();rebonebase::animation_import::import_calibrated(&mut candidate,id,&motions?,&calibration,both,projection).map_err(|e|e.to_string())?;Ok(candidate)})();let _=tx.send(result);});
                        d.pending=Some(ApplyJob{generation:self.studio.generation,base,receiver});d.error=None;
                    }
                });
                if previous_options!=(d.calibration.clone(),d.also_2d,d.projection){d.preview=None;d.texture=None;}
                if d.pending.is_some(){ui.spinner();ui.label("Preparing all clips before changing the project…");}
                if let Some(e)=&d.error {ui.colored_label(Color32::YELLOW,e);}
                for warning in &d.motion.warnings {ui.label(warning);}
            });
        });
        if open {
            self.production.animation_dialog = Some(d);
        }
    }
}
fn report_ui(ui: &mut egui::Ui, report: &CalibrationReport) {
    let lines = report.lines();
    let problems = report.missing.len() + report.ambiguous.len() + report.stale.len();
    let summary = format!(
        "{} · {} unmapped · {} missing · {} ambiguous",
        lines[0],
        report.unmapped.len(),
        report.missing.len(),
        report.ambiguous.len()
    );
    if problems > 0 {
        ui.colored_label(Color32::YELLOW, summary);
    } else {
        ui.label(summary);
    }
    if lines.len() > 1 {
        egui::CollapsingHeader::new("Joint report").show(ui, |ui| {
            for line in &lines[1..] {
                ui.label(line);
            }
        });
    }
}
impl ReboneApp {
    fn calibration_controls(&mut self, ui: &mut egui::Ui, d: &mut AnimationDialog) {
        {
            {
                let old = d.saved;
                egui::ComboBox::from_label("Saved calibration")
                    .selected_text(
                        d.saved
                            .and_then(|i| self.studio.calibrations.get(i))
                            .map_or("Automatic name matching", |c| c.name.as_str()),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut d.saved, None, "Automatic name matching");
                        for (i, c) in self.studio.calibrations.iter().enumerate() {
                            ui.selectable_value(&mut d.saved, Some(i), &c.name);
                        }
                    });
                if old != d.saved {
                    d.calibration = match d.saved.and_then(|i| self.studio.calibrations.get(i)) {
                        Some(c) => c.clone(),
                        None => Calibration3d::automatic(&d.model, &d.motion),
                    };
                }
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut d.calibration.name)
                            .hint_text("Calibration name")
                            .desired_width(190.),
                    );
                    if ui
                        .add_enabled(
                            self.studio.calibrations.len() < 256,
                            egui::Button::new("Save calibration"),
                        )
                        .clicked()
                    {
                        match save_calibration(&d.calibration, &self.studio.store) {
                            Ok(()) => {
                                self.studio.calibrations.push(d.calibration.clone());
                                d.saved = Some(self.studio.calibrations.len() - 1);
                                self.status = "Calibration saved for future imports".into();
                            }
                            Err(e) => self.status = format!("Calibration was not saved: {e}"),
                        }
                    }
                });
            }
        }
        egui::CollapsingHeader::new("Calibration settings").show(ui, |ui| {
            {
                let c = &mut d.calibration;
                ui.horizontal_wrapped(|ui| {
                    let mut frame = matches!(c.reference, ReferencePose::Frame(_));
                    ui.label("Source reference pose");
                    ui.selectable_value(&mut frame, false, "Rest pose");
                    ui.selectable_value(&mut frame, true, "Clip frame");
                    let mut n = match c.reference {
                        ReferencePose::Frame(n) => n + 1,
                        ReferencePose::Rest => 1,
                    };
                    if frame {
                        ui.add(egui::DragValue::new(&mut n).range(1..=240).prefix("Frame "));
                    }
                    c.reference = if frame {
                        ReferencePose::Frame(n - 1)
                    } else {
                        ReferencePose::Rest
                    };
                })
                .response
                .on_hover_text(
                    "The source pose that matches the target model's rest pose, such as a leading T-pose frame.",
                );
                ui.horizontal_wrapped(|ui| {
                    ui.label("Source up axis");
                    ui.selectable_value(&mut c.up, UpAxis::Y, "Y up");
                    ui.selectable_value(&mut c.up, UpAxis::Z, "Z up");
                    ui.add(
                        egui::Slider::new(&mut c.facing, -180. ..=180.)
                            .step_by(1.)
                            .text("Facing °"),
                    );
                });
                ui.horizontal_wrapped(|ui| {
                    egui::ComboBox::from_label("Root travel")
                        .selected_text(format!("{:?}", c.root_travel))
                        .show_ui(ui, |ui| {
                            for v in [
                                RootTravel::Off,
                                RootTravel::Ground,
                                RootTravel::Vertical,
                                RootTravel::Full,
                            ] {
                                ui.selectable_value(&mut c.root_travel, v, format!("{v:?}"));
                            }
                        });
                    let mut fixed = matches!(c.root_scale, RootScale::Fixed(_));
                    ui.checkbox(&mut fixed, "Fixed root scale");
                    if fixed {
                        let mut v = match c.root_scale {
                            RootScale::Fixed(v) => v,
                            RootScale::Auto => 1.,
                        };
                        ui.add(
                            egui::DragValue::new(&mut v)
                                .range(0.001..=1000.)
                                .speed(0.01)
                                .suffix(" × source"),
                        );
                        c.root_scale = RootScale::Fixed(v);
                    } else {
                        c.root_scale = RootScale::Auto;
                    }
                });
                d.offset_joint = d.offset_joint.min(d.model.nodes.len().saturating_sub(1));
                if let Some(joint) = d.model.nodes.get(d.offset_joint).map(|n| n.name.clone()) {
                    ui.horizontal_wrapped(|ui| {
                        egui::ComboBox::from_id_salt("calibration-offset-joint")
                            .selected_text(&joint)
                            .show_ui(ui, |ui| {
                                for (i, n) in d.model.nodes.iter().enumerate() {
                                    ui.selectable_value(&mut d.offset_joint, i, &n.name);
                                }
                            });
                        let mut r = c.offsets.get(&joint).copied().unwrap_or([0.; 3]);
                        let old = r;
                        for (i, label) in ["Offset X° ", "Offset Y° ", "Offset Z° "]
                            .into_iter()
                            .enumerate()
                        {
                            ui.add(
                                egui::DragValue::new(&mut r[i])
                                    .range(-180. ..=180.)
                                    .prefix(label),
                            );
                        }
                        if r != old {
                            if r == [0.; 3] {
                                c.offsets.remove(&joint);
                            } else {
                                c.offsets.insert(joint.clone(), r);
                            }
                        }
                    });
                }
                ui.label("Calibration changes rotations and root travel only; the model's limb lengths and attachments stay as they are.");
            }
        });
    }
}
fn save_calibration(
    calibration: &Calibration3d,
    store: &rebonebase::storage::Store,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        !calibration.name.trim().is_empty(),
        "Give the calibration a name"
    );
    calibration.validate()?;
    let bytes = serde_json::to_vec_pretty(calibration)?;
    anyhow::ensure!(
        bytes.len() <= 1024 * 1024,
        "Calibration exceeds the metadata size limit"
    );
    let folder = store.folder("calibrations")?;
    rebonebase::project::atomic_write(
        &folder.join(format!(
            "calibration-{}.json",
            rebonebase::storage::unique()
        )),
        &bytes,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn app_with_character(ctx: &egui::Context) -> ReboneApp {
        let mut app = ReboneApp::with_context(ctx);
        let c = rebonebase::character3d::load(std::path::Path::new(
            "src/character3d/fixtures/textured_skin.glb",
        ))
        .unwrap();
        app.add_character3d(c).unwrap();
        app
    }
    fn frame(
        app: &mut ReboneApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, EVec2::new(1400., 1600.))),
                events,
                ..Default::default()
            },
            |ctx| app.animation_import_dialog(ctx),
        )
    }
    fn click(app: &mut ReboneApp, ctx: &egui::Context, label: &str) {
        let out = frame(app, ctx, vec![]);
        let pos = out
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Text(t) if t.galley.text() == label => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing {label}"));
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        pressed,
                        button: egui::PointerButton::Primary,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
    }
    fn source_job(app: &mut ReboneApp) {
        let path = PathBuf::from("src/character3d/fixtures/textured_skin.glb");
        let names = Motion::animation_names(&path).unwrap();
        let m = Motion::load_animation(&path, 0).unwrap();
        let (tx, receiver) = mpsc::channel();
        tx.send(Ok((path, names, m))).unwrap();
        app.production.animation_source = Some(SourceJob {
            generation: app.studio.generation,
            character: 1,
            receiver,
        });
    }
    #[test]
    fn animation_source_preview_batch_import_and_undo_work_in_ui() {
        let ctx = egui::Context::default();
        let mut app = app_with_character(&ctx);
        let before = app.project.clone();
        source_job(&mut app);
        frame(&mut app, &ctx, vec![]);
        assert!(app.production.animation_dialog.is_some());
        click(&mut app, &ctx, "Preview mapped animation");
        assert!(
            app.production
                .animation_dialog
                .as_ref()
                .unwrap()
                .preview
                .is_some(),
            "{:?}",
            app.production.animation_dialog.as_ref().unwrap().error
        );
        click(&mut app, &ctx, "Import selected animation clips");
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.production.animation_dialog.is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
            frame(&mut app, &ctx, vec![]);
        }
        assert!(app.production.animation_dialog.is_none());
        assert_eq!(app.project.rig.clips.len(), before.rig.clips.len() + 1);
        assert!(app.history.undo(&mut app.project));
        assert_eq!(app.project, before);
    }
    #[test]
    fn animation_import_rejects_changed_document_and_stale_source() {
        let ctx = egui::Context::default();
        let mut app = app_with_character(&ctx);
        let base = app.project.clone();
        app.project.rig.name = "newer edit".into();
        let edited = app.project.clone();
        assert!(
            app.commit_character_animation(&base, base.clone(), app.studio.generation)
                .is_err()
        );
        assert_eq!(app.project, edited);
        source_job(&mut app);
        app.studio.generation += 1;
        frame(&mut app, &ctx, vec![]);
        assert!(app.production.animation_dialog.is_none());
        app.project = base.clone();
        assert!(
            app.commit_character_animation(&base, base.clone(), app.studio.generation - 1)
                .is_err()
        );
    }
    #[test]
    fn saved_calibration_applies_to_the_dialog_and_reloads() {
        let ctx = egui::Context::default();
        let mut app = app_with_character(&ctx);
        source_job(&mut app);
        frame(&mut app, &ctx, vec![]);
        let d = app.production.animation_dialog.as_mut().unwrap();
        let automatic = d.calibration.clone();
        assert!(!automatic.mapping.is_empty());
        d.calibration.name = "Studio walk".into();
        d.calibration.facing = 45.;
        d.calibration.root_travel = RootTravel::Ground;
        click(&mut app, &ctx, "Save calibration");
        assert_eq!(app.studio.calibrations.len(), 1, "{}", app.status);
        let saved = app.studio.calibrations[0].clone();
        let stored: Vec<Calibration3d> = app.studio.store.read_records("calibrations").unwrap();
        assert_eq!(stored, std::slice::from_ref(&saved));
        // Switching back to automatic matching drops the edits; choosing the
        // saved calibration restores them.
        let d = app.production.animation_dialog.as_mut().unwrap();
        d.saved = None;
        d.calibration = automatic.clone();
        assert_ne!(d.calibration, saved);
        d.calibration = saved.clone();
        d.saved = Some(0);
        click(&mut app, &ctx, "Preview mapped animation");
        let d = app.production.animation_dialog.as_ref().unwrap();
        assert!(d.preview.is_some(), "{:?}", d.error);
        assert_eq!(d.calibration, saved);
    }
}
