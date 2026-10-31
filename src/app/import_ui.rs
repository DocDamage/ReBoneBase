use super::*;
use anyhow::{Result, ensure};
use rebonebase::{motion::RetargetProfile, storage::Store};
use std::sync::mpsc;

pub(super) struct ImportJob {
    base: Rig,
    generation: u64,
    receiver: mpsc::Receiver<Result<PreparedImport, String>>,
}
struct ImportRequest {
    base: Rig,
    source: Option<PathBuf>,
    motion: Motion,
    indices: Vec<usize>,
    projection: Projection,
    scale: f32,
    mapping: Vec<Option<usize>>,
}
struct PreparedImport {
    rig: Rig,
    count: usize,
    mapped: usize,
    total: usize,
    notes: Vec<String>,
}

fn run_import(request: ImportRequest) -> Result<PreparedImport, String> {
    if request.indices.is_empty() {
        return Err("Select at least one animation".into());
    }
    if request.base.clips.len() + request.indices.len() > 32 {
        return Err(
            "Selected animations exceed the 32-clip limit; remove clips or select fewer animations"
                .into(),
        );
    }
    let mut candidate = request.base;
    let mut notes = Vec::new();
    let mut mapped = 0;
    let total = candidate.bones.len();
    for index in &request.indices {
        let motion = match &request.source {
            Some(path) => Motion::load_animation(path, *index).map_err(|e| e.to_string())?,
            None => {
                if *index != 0 {
                    return Err("This motion contains a single animation".into());
                }
                request.motion.clone()
            }
        };
        let report = motion
            .retarget_mapped(
                &mut candidate,
                request.projection,
                request.scale,
                &request.mapping,
            )
            .map_err(|e| e.to_string())?;
        mapped = report.mapped;
        notes.extend(report.warnings);
    }
    candidate.validate()?;
    notes.sort();
    notes.dedup();
    Ok(PreparedImport {
        rig: candidate,
        count: request.indices.len(),
        mapped,
        total,
        notes,
    })
}

impl MotionSetup {
    pub(super) fn new(motion: Motion, rig: &Rig) -> Self {
        let scale = motion.suggested_scale(rig);
        let mapping = motion.mapping(rig);
        let name = motion.name.clone();
        Self {
            motion,
            projection: Projection::Front,
            scale,
            mapping,
            preview_frame: 0,
            source: None,
            animation_names: vec![name],
            animation_index: 0,
            selected_animations: vec![true],
            target_ids: rig.bones.iter().map(|b| b.id).collect(),
            profile_name: "My retarget profile".into(),
            profile_index: None,
            preview_base: None,
            preview_options: None,
            preview_target: None,
            preview_texture: None,
            preview_texture_frame: None,
            preview_extras: None,
            preview_error: None,
            pending: None,
        }
    }
    pub(super) fn from_path(path: PathBuf, rig: &Rig) -> Result<Self> {
        let names = Motion::animation_names(&path)?;
        let motion = Motion::load_animation(&path, 0)?;
        let mut setup = Self::new(motion, rig);
        setup.selected_animations = vec![false; names.len()];
        setup.selected_animations[0] = true;
        setup.animation_names = names;
        setup.source = Some(path);
        Ok(setup)
    }
    fn reconcile_target(&mut self, rig: &Rig) {
        let ids: Vec<_> = rig.bones.iter().map(|b| b.id).collect();
        if ids == self.target_ids {
            return;
        }
        let automatic = self.motion.mapping(rig);
        self.mapping = ids
            .iter()
            .enumerate()
            .map(|(i, id)| {
                self.target_ids
                    .iter()
                    .position(|old| old == id)
                    .and_then(|old| self.mapping.get(old).copied())
                    .unwrap_or(automatic[i])
            })
            .collect();
        self.target_ids = ids;
    }
    fn select_animation(&mut self, index: usize) -> Result<()> {
        let path = self
            .source
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("This source has one animation"))?;
        let motion = Motion::load_animation(path, index)?;
        let old_names = self.motion.joint_names();
        let new_names = motion.joint_names();
        self.mapping = self
            .mapping
            .iter()
            .map(|old| {
                old.and_then(|i| old_names.get(i))
                    .and_then(|name| new_names.iter().position(|candidate| candidate == name))
            })
            .collect();
        self.motion = motion;
        self.animation_index = index;
        self.preview_frame = 0;
        self.preview_options = None;
        Ok(())
    }
    fn apply_profile(&mut self, rig: &Rig, profile: &RetargetProfile) -> Result<()> {
        profile.validate()?;
        let mapping = profile.resolve(rig, &self.motion);
        ensure!(
            mapping.iter().any(Option::is_some),
            "No saved joint names matched this rig and source"
        );
        self.mapping = mapping;
        self.projection = profile.projection;
        self.scale = profile.scale;
        self.profile_name = profile.name.clone();
        Ok(())
    }
    fn save_profile(&self, rig: &Rig, store: &Store) -> Result<RetargetProfile> {
        let profile = RetargetProfile::from_mapping(
            rig,
            &self.motion,
            self.profile_name.trim(),
            self.projection,
            self.scale,
            &self.mapping,
        )?;
        ensure!(!profile.name.is_empty(), "Give the profile a name");
        let bytes = serde_json::to_vec_pretty(&profile)?;
        ensure!(
            bytes.len() <= 1024 * 1024,
            "Retarget profile exceeds the metadata size limit"
        );
        let folder = store.folder("profiles")?;
        project::atomic_write(
            &folder.join(format!("profile-{}.json", rebonebase::storage::unique())),
            &bytes,
        )?;
        Ok(profile)
    }
    fn prepare_target(&mut self, rig: &Rig) -> Result<bool> {
        self.reconcile_target(rig);
        let options = (
            self.projection,
            self.scale,
            self.mapping.clone(),
            self.animation_index,
        );
        let changed = self.preview_base.as_ref() != Some(rig)
            || self.preview_options.as_ref() != Some(&options);
        if changed {
            self.preview_base = Some(rig.clone());
            self.preview_options = Some(options);
            self.preview_target = None;
            self.preview_texture = None;
            self.preview_texture_frame = None;
            self.preview_error = None;
            let mut target = rig.clone();
            match self.motion.retarget_mapped(
                &mut target,
                self.projection,
                self.scale,
                &self.mapping,
            ) {
                Ok(_) => self.preview_target = Some(target),
                Err(e) => self.preview_error = Some(e.to_string()),
            }
        }
        if let Some(error) = &self.preview_error {
            return Err(anyhow::anyhow!(error.clone()));
        }
        Ok(changed)
    }
    fn target_image(&mut self, project: &Project) -> Result<image::RgbaImage> {
        self.prepare_target(&project.rig)?;
        let rig = self
            .preview_target
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Choose source joints to preview this mannequin"))?;
        let raw = rig.active_clip().sample((self.preview_frame + 1) as f32);
        let pose = rebonebase::features::solve_pins(rig, &project.extras, &raw);
        Ok(skin::render(rig, Some(&pose)))
    }
    fn preview_texture(
        &mut self,
        project: &Project,
        ctx: &egui::Context,
    ) -> Result<egui::TextureId> {
        self.prepare_target(&project.rig)?;
        if self.preview_extras.as_ref() != Some(&project.extras) {
            self.preview_texture_frame = None;
            self.preview_extras = Some(project.extras.clone());
        }
        if self.preview_texture.is_none() || self.preview_texture_frame != Some(self.preview_frame)
        {
            let image = self.target_image(project)?;
            self.preview_texture = Some(ctx.load_texture(
                "retarget-mannequin",
                egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                ),
                egui::TextureOptions::NEAREST,
            ));
            self.preview_texture_frame = Some(self.preview_frame);
        }
        Ok(self.preview_texture.as_ref().unwrap().id())
    }
    fn import_request(&self, rig: &Rig) -> Result<ImportRequest, String> {
        let indices: Vec<_> = self
            .selected_animations
            .iter()
            .enumerate()
            .filter_map(|(i, selected)| selected.then_some(i))
            .collect();
        if indices.is_empty() {
            return Err("Select at least one animation".into());
        }
        if self.selected_animations.len() != self.animation_names.len() {
            return Err("Animation selection changed; open the motion source again".into());
        }
        Ok(ImportRequest {
            base: rig.clone(),
            source: self.source.clone(),
            motion: self.motion.clone(),
            indices,
            projection: self.projection,
            scale: self.scale,
            mapping: self.mapping.clone(),
        })
    }
}

fn source_preview(ui: &mut egui::Ui, setup: &MotionSetup, size: EVec2) {
    ui.label("Source skeleton");
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4., Color32::from_rgb(18, 23, 29));
    let segments = setup.motion.preview(setup.projection, setup.preview_frame);
    if segments.is_empty() {
        return;
    }
    let points: Vec<_> = segments.iter().flat_map(|(a, b)| [*a, *b]).collect();
    let min = Vec2::new(
        points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min),
        points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min),
    );
    let max = Vec2::new(
        points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max),
        points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max),
    );
    let scale = ((rect.width() - 24.) / (max.x - min.x).max(0.01))
        .min((rect.height() - 24.) / (max.y - min.y).max(0.01));
    let map = |p: Vec2| {
        rect.center() + EVec2::new(p.x - (min.x + max.x) * 0.5, p.y - (min.y + max.y) * 0.5) * scale
    };
    for (a, b) in segments {
        painter.line_segment([map(a), map(b)], Stroke::new(1.5, ACCENT));
    }
}

impl ReboneApp {
    fn apply_motion_import(&mut self, base: &Rig, prepared: PreparedImport) -> Result<(), String> {
        if &self.project.rig != base {
            return Err("The rig changed while motion was importing; preview and import again to keep your edits".into());
        }
        let mut candidate = self.project.clone();
        candidate.rig = prepared.rig;
        candidate.validate().map_err(|e| e.to_string())?;
        self.history.record(&self.project);
        self.project = candidate;
        self.mode = Mode::Pose;
        self.frame = 1.;
        self.playing = false;
        self.studio.playback_ms = 0.;
        self.changed();
        self.status = format!(
            "Imported {} animation(s) · {} / {} joints mapped",
            prepared.count, prepared.mapped, prepared.total
        );
        if !prepared.notes.is_empty() {
            self.import_notes = Some(prepared.notes);
        }
        Ok(())
    }
    pub(super) fn import_dialogs(&mut self, ctx: &egui::Context) {
        if let Some(notes) = self.import_notes.take() {
            let mut open = true;
            let mut dismiss = false;
            egui::Window::new("Import notes")
                .open(&mut open)
                .default_width(500.)
                .show(ctx, |ui| {
                    ui.label("The import finished. These source features need attention:");
                    for note in &notes {
                        ui.label(format!("• {note}"));
                    }
                    if ui.button("Continue").clicked() {
                        dismiss = true;
                    }
                });
            if open && !dismiss {
                self.import_notes = Some(notes);
            }
        }
        let Some(mut setup) = self.motion_setup.take() else {
            return;
        };
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.status = "Motion import canceled".into();
            return;
        }
        setup.reconcile_target(&self.project.rig);
        let mut applied = false;
        if let Some(job) = setup.pending.take() {
            match job.receiver.try_recv() {
                Ok(Ok(_)) if job.generation!=self.studio.generation=>self.status="Motion import finished for the previous project; select the animations again to import here".into(),
                Ok(Ok(prepared)) => match self.apply_motion_import(&job.base, prepared) {
                    Ok(()) => applied = true,
                    Err(e) => self.status = e,
                },
                Ok(Err(e)) => self.status = format!("Motion import failed: {e}"),
                Err(mpsc::TryRecvError::Empty) => {
                    setup.pending = Some(job);
                    ctx.request_repaint_after(Duration::from_millis(30));
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.status = "Motion import ended before it could finish".into()
                }
            }
        }
        if applied {
            return;
        }
        let mut open = true;
        egui::Window::new("Import animation for 2D rig").open(&mut open).default_size([760.,680.]).max_height((ctx.content_rect().height()-40.).max(240.)).show(ctx,|ui|{
            egui::ScrollArea::vertical().id_salt("motion-import-scroll").show(ui,|ui|{
                ui.heading(&setup.motion.name);ui.label(format!("{} source joints · {} sampled frames",setup.motion.joint_names().len(),setup.motion.frame_count()));
                ui.add_enabled_ui(setup.pending.is_none(),|ui|{
                    let previous=setup.animation_index;
                    egui::ComboBox::from_label("Preview animation").selected_text(&setup.animation_names[setup.animation_index]).show_ui(ui,|ui|{for(index,name)in setup.animation_names.iter().enumerate(){ui.selectable_value(&mut setup.animation_index,index,name);}});
                    if previous!=setup.animation_index{let selected=setup.animation_index;setup.animation_index=previous;if let Err(e)=setup.select_animation(selected){self.status=format!("Could not preview animation: {e}");}}
                    ui.horizontal(|ui|{egui::ComboBox::from_label("Projection").selected_text(format!("{:?}",setup.projection)).show_ui(ui,|ui|{for p in [Projection::Front,Projection::Side,Projection::ThreeQuarter]{ui.selectable_value(&mut setup.projection,p,format!("{p:?}"));}});ui.add(egui::DragValue::new(&mut setup.scale).range(0.001..=1000.).speed(0.01).prefix("Root scale "));});
                    let old_profile=setup.profile_index;
                    egui::ComboBox::from_label("Retarget profile").selected_text(setup.profile_index.and_then(|i|self.studio.profiles.get(i)).map_or("Choose saved profile",|p|p.name.as_str())).show_ui(ui,|ui|{for(i,p)in self.studio.profiles.iter().enumerate(){if p.validate().is_ok(){ui.selectable_value(&mut setup.profile_index,Some(i),&p.name);}}});
                    if old_profile!=setup.profile_index&&let Some(profile)=setup.profile_index.and_then(|i|self.studio.profiles.get(i)){match setup.apply_profile(&self.project.rig,profile){Ok(())=>self.status="Retarget profile applied".into(),Err(e)=>self.status=e.to_string()}}
                    ui.horizontal(|ui|{ui.add(egui::TextEdit::singleline(&mut setup.profile_name).hint_text("Profile name").desired_width(190.));if ui.add_enabled(self.studio.profiles.len()<256,egui::Button::new("Save new profile")).clicked(){match setup.save_profile(&self.project.rig,&self.studio.store){Ok(profile)=>{self.studio.profiles.push(profile);setup.profile_index=Some(self.studio.profiles.len()-1);self.status="Retarget profile saved for future imports".into();},Err(e)=>self.status=format!("Profile was not saved: {e}")}}});
                });
                let size=EVec2::new(((ui.available_width()-12.)*0.5).max(100.),180.);
                ui.horizontal_top(|ui|{ui.vertical(|ui|source_preview(ui,&setup,size));ui.vertical(|ui|{ui.label("Your mannequin");let(rect,_)=ui.allocate_exact_size(size,egui::Sense::hover());let painter=ui.painter_at(rect);painter.rect_filled(rect,4.,Color32::from_rgb(27,34,42));match setup.preview_texture(&self.project,ctx){Ok(texture)=>{let ratio=(size.x/self.project.rig.width as f32).min(size.y/self.project.rig.height as f32)*0.9;let image_rect=Rect::from_center_size(rect.center(),EVec2::new(self.project.rig.width as f32,self.project.rig.height as f32)*ratio);painter.image(texture,image_rect,Rect::from_min_max(Pos2::ZERO,Pos2::new(1.,1.)),Color32::WHITE);},Err(e)=>{painter.text(rect.center(),egui::Align2::CENTER_CENTER,"Map joints below to preview",egui::FontId::proportional(13.),DIM);ui.label(RichText::new(e.to_string()).small().color(DIM));}}});});
                ui.add(egui::Slider::new(&mut setup.preview_frame,0..=setup.motion.frame_count()-1).text("Preview frame"));
                egui::CollapsingHeader::new(format!("Joint mapping · {} / {} matched",setup.mapping.iter().flatten().count(),setup.mapping.len())).default_open(true).show(ui,|ui|{
                    let names=setup.motion.joint_names();ui.add_enabled_ui(setup.pending.is_none(),|ui|egui::ScrollArea::vertical().id_salt("motion-joints").max_height(150.).show(ui,|ui|{for(i,b)in self.project.rig.bones.iter().enumerate(){ui.horizontal(|ui|{ui.add_sized([170.,20.],egui::Label::new(&b.name));egui::ComboBox::from_id_salt(("source-joint",i)).selected_text(setup.mapping.get(i).copied().flatten().and_then(|n|names.get(n)).map_or("Unmapped",String::as_str)).show_ui(ui,|ui|{ui.selectable_value(&mut setup.mapping[i],None,"Unmapped");for(n,name)in names.iter().enumerate(){ui.selectable_value(&mut setup.mapping[i],Some(n),name);}});});}}));
                });
                ui.separator();ui.label("Animations to import");ui.add_enabled_ui(setup.pending.is_none(),|ui|{
                    ui.horizontal(|ui|{
                        if ui.small_button("Select all").clicked(){setup.selected_animations.fill(true);}
                        if ui.small_button("Only previewed").clicked(){setup.selected_animations.fill(false);setup.selected_animations[setup.animation_index]=true;}
                    });
                    egui::ScrollArea::vertical().id_salt("animation-choices").max_height(100.).show(ui,|ui|{for(i,name)in setup.animation_names.iter().enumerate(){ui.checkbox(&mut setup.selected_animations[i],name);}});
                    let selected=setup.selected_animations.iter().filter(|v|**v).count();ui.label(RichText::new(format!("{selected} selected · {} clip slots available",32usize.saturating_sub(self.project.rig.clips.len()))).small().color(DIM));
                    if ui.add_enabled(selected>0,egui::Button::new(RichText::new("Import selected animations").color(ACCENT))).clicked(){match setup.import_request(&self.project.rig){Ok(request)=>{let base=request.base.clone();let(sender,receiver)=mpsc::channel();std::thread::spawn(move||{let _=sender.send(run_import(request));});setup.pending=Some(ImportJob{base,generation:self.studio.generation,receiver});self.status="Importing selected animations…".into();ctx.request_repaint_after(Duration::from_millis(30));},Err(e)=>self.status=e}}
                });
                if setup.pending.is_some(){ui.spinner();ui.label("Preparing clips; your project stays unchanged until every animation is ready.");}
                for note in &setup.motion.warnings{ui.label(RichText::new(note).small().color(DIM));}
            });
        });
        if open {
            self.motion_setup = Some(setup);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = "HIERARCHY ROOT hips { OFFSET 0 0 0 CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation JOINT chest { OFFSET 0 10 0 CHANNELS 3 Zrotation Xrotation Yrotation End Site { OFFSET 0 10 0 } } } MOTION Frames: 2 Frame Time: 0.1 0 0 0 0 0 0 0 0 0 2 0 0 45 0 0 0 0 0";
    fn setup(project: &Project) -> MotionSetup {
        MotionSetup::new(Motion::from_bvh(SOURCE).unwrap(), &project.rig)
    }
    #[test]
    fn synthetic_import_setup_previews_the_actual_rig_and_applies_a_profile() {
        let p = Project::new(project::templates()[0].clone());
        let mut setup = setup(&p);
        let before = p.clone();
        setup.preview_frame = 1;
        let image = setup.target_image(&p).unwrap();
        let expected = setup
            .motion
            .preview_rig(&p.rig, setup.projection, setup.scale, &setup.mapping, 1)
            .unwrap();
        assert_eq!(image, expected);
        assert_eq!(p, before);
        let profile = rebonebase::motion::RetargetProfile::from_mapping(
            &p.rig,
            &setup.motion,
            "Reusable",
            Projection::Side,
            2.,
            &setup.mapping,
        )
        .unwrap();
        setup.apply_profile(&p.rig, &profile).unwrap();
        assert_eq!(setup.projection, Projection::Side);
        assert_eq!(setup.scale, 2.);
    }
    #[test]
    fn prepared_motion_import_is_atomic_and_an_imported_clip_is_undoable() {
        let ctx = egui::Context::default();
        let mut app = ReboneApp::with_context(&ctx);
        let before = app.project.clone();
        let setup = setup(&app.project);
        let prepared = run_import(setup.import_request(&app.project.rig).unwrap()).unwrap();
        app.apply_motion_import(&before.rig, prepared).unwrap();
        assert_eq!(app.project.rig.clips.len(), before.rig.clips.len() + 1);
        app.undo();
        assert_eq!(app.project, before);
        let mut setup = setup;
        setup
            .animation_names
            .push("Unavailable second animation".into());
        setup.selected_animations.push(true);
        assert!(run_import(setup.import_request(&app.project.rig).unwrap()).is_err());
        assert_eq!(app.project, before);
    }
    #[test]
    fn completed_background_import_cannot_overwrite_newer_rig_edits() {
        let ctx = egui::Context::default();
        let mut app = ReboneApp::with_context(&ctx);
        let setup = setup(&app.project);
        let base = app.project.rig.clone();
        let prepared = run_import(setup.import_request(&base).unwrap()).unwrap();
        app.project.rig.name = "Newer user edit".into();
        let edited = app.project.clone();
        assert!(app.apply_motion_import(&base, prepared).is_err());
        assert_eq!(app.project, edited);
    }

    #[test]
    fn background_import_is_discarded_after_switching_to_an_identical_rig_project() {
        let ctx = egui::Context::default();
        let mut app = ReboneApp::with_context(&ctx);
        let mut setup = setup(&app.project);
        let base = app.project.rig.clone();
        let prepared = run_import(setup.import_request(&base).unwrap()).unwrap();
        let (sender, receiver) = mpsc::channel();
        sender.send(Ok(prepared)).unwrap();
        setup.pending = Some(ImportJob {
            base,
            generation: app.studio.generation,
            receiver,
        });
        app.motion_setup = Some(setup);
        app.studio.generation += 1;
        let before = app.project.clone();
        let _ = ctx.run(egui::RawInput::default(), |ctx| app.import_dialogs(ctx));
        assert_eq!(app.project, before);
    }
}
