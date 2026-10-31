use super::*;
use rebonebase::character3d::Model3d;
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
    mapping: Vec<Option<usize>>,
    root_motion: bool,
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
                        let mapping = motion.mapping_3d(&c.model);
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
                            mapping,
                            root_motion: false,
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
        egui::Window::new("Import animation onto 3D character").open(&mut open).default_size([680.,650.]).max_height((ctx.content_rect().height()-40.).max(240.)).show(ctx,|ui|{
            egui::ScrollArea::vertical().id_salt("external-animation").show(ui,|ui|{
                ui.heading(&d.motion.name);
                ui.label("Retarget motion to the existing model. Its mesh, textures and joint attachments stay in place.");
                let previous_options=(d.mapping.clone(),d.root_motion,d.also_2d,d.projection);
                ui.add_enabled_ui(d.pending.is_none(),|ui|{
                    let old=d.index;
                    egui::ComboBox::from_label("Source animation").selected_text(&d.names[d.index]).show_ui(ui,|ui|{
                        for(i,name)in d.names.iter().enumerate(){ui.selectable_value(&mut d.index,i,name);}
                    });
                    if old!=d.index {
                        match Motion::load_animation(&d.source,d.index) {
                            Ok(m)=>{let old_names=d.motion.joint_names();let names=m.joint_names();d.mapping=d.mapping.iter().map(|n|n.and_then(|i|old_names.get(i)).and_then(|name|names.iter().position(|s|s==name))).collect();d.motion=m;d.preview=None;d.texture=None;d.error=None;}
                            Err(e)=>{d.index=old;d.error=Some(e.to_string());}
                        }
                    }
                    ui.checkbox(&mut d.root_motion,"Include source root travel for 3D");
                    ui.checkbox(&mut d.also_2d,"Also animate my 2D rig in the same clips (automatic joint mapping)");
                    if d.also_2d {
                        egui::ComboBox::from_label("2D projection").selected_text(format!("{:?}",d.projection)).show_ui(ui,|ui|{for p in [Projection::Front,Projection::Side,Projection::ThreeQuarter]{ui.selectable_value(&mut d.projection,p,format!("{p:?}"));}});
                        ui.label("For custom 2D joint mapping, use File → Import animation for 2D rig.");
                    }
                    ui.label(format!("{} / {} target joints matched",d.mapping.iter().flatten().count(),d.mapping.len()));
                    egui::CollapsingHeader::new("Joint mapping").show(ui,|ui|{
                        let names=d.motion.joint_names();
                        egui::ScrollArea::vertical().id_salt("3d-joint-mapping").max_height(160.).show_rows(ui,22.,d.model.nodes.len(),|ui,range|{
                            for i in range {ui.horizontal(|ui|{ui.label(&d.model.nodes[i].name);egui::ComboBox::from_id_salt(("external-joint",i)).selected_text(d.mapping[i].and_then(|n|names.get(n)).map_or("Unmapped",String::as_str)).show_ui(ui,|ui|{ui.selectable_value(&mut d.mapping[i],None,"Unmapped");for(n,name)in names.iter().enumerate(){ui.selectable_value(&mut d.mapping[i],Some(n),name);}});});}
                        });
                    });
                    if ui.button("Preview mapped animation").clicked() {
                        let mut preview=self.project.clone();
                        match rebonebase::animation_import::import_character_motions(&mut preview,d.character,std::slice::from_ref(&d.motion),&d.mapping,d.root_motion,d.also_2d,d.projection){Ok(_)=>{d.preview=Some(preview);d.texture=None;d.frame=1.;d.error=None;},Err(e)=>d.error=Some(e.to_string())}
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
                    ui.separator();ui.label("Clips to import");
                    ui.horizontal(|ui|{if ui.small_button("Select all").clicked(){d.selected.fill(true);}
if ui.small_button("Only previewed").clicked(){d.selected.fill(false);d.selected[d.index]=true;}});
                    egui::ScrollArea::vertical().id_salt("3d-animation-choices").max_height(95.).show(ui,|ui|{for(i,name)in d.names.iter().enumerate(){ui.checkbox(&mut d.selected[i],name);}});
                    if ui.add_enabled(d.selected.iter().any(|b|*b),egui::Button::new("Import selected animation clips")).clicked(){
                        let base=self.project.clone();let mut candidate=base.clone();let path=d.source.clone();let indices:Vec<_>=d.selected.iter().enumerate().filter_map(|(i,b)|b.then_some(i)).collect();let mapping=d.mapping.clone();let id=d.character;let root=d.root_motion;let both=d.also_2d;let projection=d.projection;
                        let(tx,receiver)=mpsc::channel();std::thread::spawn(move||{let result=(||{let motions:Result<Vec<_>,_>=indices.into_iter().map(|i|Motion::load_animation(&path,i).map_err(|e|e.to_string())).collect();rebonebase::animation_import::import_character_motions(&mut candidate,id,&motions?,&mapping,root,both,projection).map_err(|e|e.to_string())?;Ok(candidate)})();let _=tx.send(result);});
                        d.pending=Some(ApplyJob{generation:self.studio.generation,base,receiver});d.error=None;
                    }
                });
                if previous_options!=(d.mapping.clone(),d.root_motion,d.also_2d,d.projection){d.preview=None;d.texture=None;}
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
}
