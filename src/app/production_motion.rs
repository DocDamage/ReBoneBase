use super::production_ui::clip_combo;
use super::*;
use rebonebase::advanced_motion::{self as motion, Chain, ContactKey, ContactTrack, Layer, Spring};
impl ReboneApp {
    pub(super) fn animation_templates_ui(&mut self, ui: &mut egui::Ui) {
        use rebonebase::animation_templates::{self, Preset};
        ui.group(|ui| {
            ui.strong("Animation templates");
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt("animation_template")
                    .selected_text(self.production.animation_preset.label())
                    .show_ui(ui, |ui| {
                        for p in Preset::ALL {
                            ui.selectable_value(
                                &mut self.production.animation_preset,
                                p,
                                p.label(),
                            );
                        }
                    });
                ui.add(
                    egui::Slider::new(&mut self.production.animation_strength, 0.1..=2.)
                        .text("Strength"),
                );
                if ui.button("Add animation template").clicked() {
                    match animation_templates::apply(
                        &mut self.project,
                        self.production.animation_preset,
                        self.production.animation_strength,
                    ) {
                        Ok(_) => {
                            self.mode = Mode::Pose;
                            self.frame = 1.;
                            self.studio.keys.clear();
                            self.production.transition_preview = false;
                            self.status = "Animation template added as an editable clip".into();
                        }
                        Err(e) => self.status = e.to_string(),
                    }
                }
            });
            ui.label(self.production.animation_preset.description());
        });
    }
    pub(super) fn production_motion(&mut self, ui: &mut egui::Ui) {
        ui.heading("Animation production");
        self.animation_templates_ui(ui);
        if self
            .project
            .extras
            .production
            .baked_motion
            .contains(&self.project.rig.active_clip)
        {
            ui.label(RichText::new("This clip has its evaluated motion baked to FK. Stored controllers are bypassed.").color(ACCENT));
            if ui.button("Enable live controllers on this clip").clicked() {
                self.project
                    .extras
                    .production
                    .baked_motion
                    .remove(&self.project.rig.active_clip);
            }
        }
        ui.label("Controllers keep root anchors fixed. Move Rig remains the whole-body drag tool.");
        egui::CollapsingHeader::new("Walk / run generator")
            .default_open(true)
            .show(ui, |ui| {
                let g = &mut self.production.gait;
                ui.text_edit_singleline(&mut g.name);
                ui.horizontal_wrapped(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut g.frames)
                            .range(8..=240)
                            .prefix("Frames "),
                    );
                    ui.add(
                        egui::DragValue::new(&mut g.cadence)
                            .range(0.05..=(240. / g.frames as f32))
                            .speed(0.05)
                            .prefix("Cycles/sec "),
                    );
                    ui.add(
                        egui::DragValue::new(&mut g.stride)
                            .range(0.1..=100.)
                            .speed(0.25)
                            .prefix("Stride "),
                    );
                    ui.add(
                        egui::DragValue::new(&mut g.bounce)
                            .range(0. ..=30.)
                            .prefix("Bounce "),
                    );
                    ui.add(
                        egui::DragValue::new(&mut g.arm_swing)
                            .range(0. ..=90.)
                            .prefix("Arms ° "),
                    );
                    ui.add(
                        egui::DragValue::new(&mut g.foot_lift)
                            .range(0. ..=40.)
                            .prefix("Foot lift "),
                    );
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut g.run, "Run gait");
                    ui.checkbox(&mut g.root_travel, "Author root travel");
                });
                if ui.button("Generate editable gait clip").clicked() {
                    match motion::generate_walk(&mut self.project.rig, &self.production.gait) {
                        Ok(_) => {
                            self.mode = Mode::Pose;
                            self.frame = 1.;
                            self.studio.keys.clear();
                            self.status = "Gait clip generated with editable joint keys".into();
                        }
                        Err(e) => self.status = e,
                    }
                }
            });
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.production.loop_frames)
                    .range(2..=120)
                    .prefix("Blend frames "),
            );
            ui.checkbox(&mut self.production.velocity, "Match seam velocity");
            if ui.button("Clean active loop").clicked() {
                let index = self.project.rig.active_clip;
                if let Err(e) = motion::smooth_loop(
                    &mut self.project.rig,
                    index,
                    self.production.loop_frames,
                    self.production.velocity,
                ) {
                    self.status = e;
                }
            }
            if ui.button("Mirror entire clip").clicked() {
                self.mirror_production_clip();
            }
        });
        egui::CollapsingHeader::new("Blend clips and transitions")
            .default_open(true)
            .show(ui, |ui| {
                clip_combo(
                    ui,
                    &self.project.rig,
                    "Target clip",
                    &mut self.production.transition_to,
                );
                ui.horizontal_wrapped(|ui| {
                    ui.add(
                        egui::DragValue::new(&mut self.production.transition_frames)
                            .range(2..=120)
                            .prefix("Transition frames "),
                    );
                    if ui.button("Create transition clip").clicked() {
                        let from = self.project.rig.active_clip;
                        match rebonebase::production::transition_project_clip(
                            &mut self.project,
                            from,
                            self.production.transition_to,
                            self.production.transition_frames,
                        ) {
                            Ok(_) => {
                                self.frame = 1.;
                                self.production.transition_preview = false;
                                self.mode = Mode::Pose;
                            }
                            Err(e) => self.status = e,
                        }
                    }
                    if ui
                        .checkbox(
                            &mut self.production.transition_preview,
                            "Preview blend on canvas",
                        )
                        .changed()
                    {
                        self.mode = Mode::Pose;
                        self.changed();
                    }
                    if ui
                        .add(
                            egui::Slider::new(&mut self.production.transition_weight, 0. ..=1.)
                                .text("Blend"),
                        )
                        .changed()
                    {
                        self.changed();
                    }
                });
            });
        let baked = self
            .project
            .extras
            .production
            .baked_motion
            .contains(&self.project.rig.active_clip);
        ui.add_enabled_ui(!baked,|ui|{
  egui::CollapsingHeader::new("Animation layers and bone masks").default_open(true).show(ui,|ui|{clip_combo(ui,&self.project.rig,"Layer source",&mut self.production.layer_clip);if ui.button("Add layer from clip").clicked(){self.project.extras.production.motion.layers.push(Layer{name:format!("Layer {}",self.project.extras.production.motion.layers.len()+1),clip:self.production.layer_clip,mask:self.selected_bones(),weight:0.5,..Default::default()});}let selected=self.selected_bones();let mut remove=None;for(i,l)in self.project.extras.production.motion.layers.iter_mut().enumerate(){ui.push_id(i,|ui|{ui.horizontal_wrapped(|ui|{ui.checkbox(&mut l.enabled,"");ui.text_edit_singleline(&mut l.name);ui.checkbox(&mut l.additive,"Additive");ui.add(egui::Slider::new(&mut l.weight,0. ..=1.).text("Weight"));ui.add(egui::DragValue::new(&mut l.offset).range(-240. ..=240.).prefix("Offset "));if ui.small_button("Remove layer").clicked(){remove=Some(i);}});clip_combo(ui,&self.project.rig,"Clip",&mut l.clip);ui.horizontal(|ui|{ui.label(format!("{} masked bones (empty = all)",l.mask.len()));if ui.button("Mask selected bones").clicked(){l.mask=selected.clone();}
if ui.button("Use all bones").clicked(){l.mask.clear();}});});}
if let Some(i)=remove{self.project.extras.production.motion.layers.remove(i);}});
        egui::CollapsingHeader::new("Contacts and seamless IK / FK")
            .default_open(true)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Hold selected limb here (IK)").clicked() {
                        self.key_contact(1.);
                    }
                    if ui.button("Release selected contact here").clicked() {
                        self.key_contact(0.);
                    }
                    if ui.button("Bake all clips and switch to FK").clicked() {
                        match self.bake_all_motion() {
                            Ok(()) => self.status = "All displayed controller motion baked to FK without a pose jump".into(),
                            Err(e) => self.status = e.to_string(),
                        }
                    }
                });
                ui.label("Select a lower limb or hand/foot. Contact target and strength keys interpolate across the active clip.");
                let mut remove = None;
                for (i, contact) in self.project.extras.production.motion.contacts.iter_mut().enumerate() {
                    ui.push_id(("contact", i), |ui| {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut contact.enabled, self.project.rig.bone(contact.lower).map_or("Limb", |b| b.name.as_str()));
                            ui.label(contact.clip.map_or("All clips".into(), |idx| self.project.rig.clips.get(idx).map_or("Missing".into(), |c| c.name.clone())));
                            if ui.small_button("Remove contact").clicked() {
                                remove = Some(i);
                            }
                        });
                        let visible_end = contact.clip.and_then(|idx| self.project.rig.clips.get(idx)).map(|clip| clip.length as f32);
                        let mut drop_key = None;
                        for (j, key) in contact.keys.iter_mut().enumerate() {
                            if visible_end.is_some_and(|end| !(1. ..=end).contains(&key.frame)) {
                                continue;
                            }
                            let before = *key;
                            ui.horizontal_wrapped(|ui| {
                                ui.label(format!("Frame {:.2}", key.frame));
                                ui.add(egui::DragValue::new(&mut key.target.x).prefix("X ").speed(0.1));
                                ui.add(egui::DragValue::new(&mut key.target.y).prefix("Y ").speed(0.1));
                                ui.add(egui::Slider::new(&mut key.weight, 0. ..=1.).text("Strength"));
                                if ui.small_button("Flip").clicked() {
                                    key.bend = -key.bend;
                                }
                                if ui.small_button("Delete contact key").clicked() {
                                    drop_key = Some(j);
                                }
                            });
                            if *key != before {
                                let index = contact.clip.unwrap_or(self.project.rig.active_clip);
                                let clip = &mut self.project.rig.clips[index];
                                let frame = if contact.clip.is_some() {
                                    key.frame
                                } else {
                                    key.frame - clip.source.as_ref().map_or(0, |source| source.origin) as f32
                                };
                                if (1. ..=clip.length as f32).contains(&frame)
                                    && let Err(error) = clip.release_hold(frame)
                                {
                                    *key = before;
                                    self.status = error;
                                }
                            }
                        }
                        if let Some(j) = drop_key {
                            contact.keys.remove(j);
                        }
                    });
                }
                if let Some(i) = remove {
                    self.project.extras.production.motion.contacts.remove(i);
                }
            });
  egui::CollapsingHeader::new("Long-chain IK").default_open(true).show(ui,|ui|{ui.horizontal(|ui|{ui.add(egui::DragValue::new(&mut self.production.chain_length).range(2..=64).prefix("Bones "));if ui.button("Create chain from selected tip").clicked(){self.add_long_chain();}});let mut remove=None;for(i,c)in self.project.extras.production.motion.chains.iter_mut().enumerate(){ui.push_id(("chain",i),|ui|{ui.horizontal_wrapped(|ui|{ui.checkbox(&mut c.enabled,&c.name);ui.add(egui::DragValue::new(&mut c.target.x).prefix("Target X "));ui.add(egui::DragValue::new(&mut c.target.y).prefix("Y "));ui.add(egui::Slider::new(&mut c.weight,0. ..=1.).text("Strength"));ui.add(egui::DragValue::new(&mut c.iterations).range(1..=64).prefix("Iterations "));if ui.small_button("Remove chain").clicked(){remove=Some(i);}});});}
if let Some(i)=remove{self.project.extras.production.motion.chains.remove(i);}ui.label("In IK mode, drag the final bone's tail to reposition a long-chain target.");});
  egui::CollapsingHeader::new("Secondary motion").default_open(true).show(ui,|ui|{if ui.button("Add springs to selected bones").clicked(){for id in self.selected_bones(){if self.project.rig.bone(id).is_some_and(|b|b.parent.is_some())&&!self.project.extras.production.motion.springs.iter().any(|s|s.bone==id){self.project.extras.production.motion.springs.push(Spring{bone:id,..Default::default()});}}}let mut remove=None;for(i,s)in self.project.extras.production.motion.springs.iter_mut().enumerate(){ui.push_id(("spring",i),|ui|ui.horizontal_wrapped(|ui|{ui.checkbox(&mut s.enabled,self.project.rig.bone(s.bone).map_or("Bone",|b|b.name.as_str()));ui.add(egui::DragValue::new(&mut s.stiffness).range(0.01..=1000.).prefix("Stiffness "));ui.add(egui::DragValue::new(&mut s.damping).range(0. ..=100.).prefix("Damping "));ui.add(egui::DragValue::new(&mut s.gravity).range(-1000. ..=1000.).prefix("Gravity "));ui.add(egui::DragValue::new(&mut s.wind).range(-1000. ..=1000.).prefix("Wind "));if ui.small_button("Remove spring").clicked(){remove=Some(i);}}));}
if let Some(i)=remove{self.project.extras.production.motion.springs.remove(i);}ui.label("Spring evaluation is deterministic when scrubbing and exporting. Bake controllers to retain ordinary editable keys.");});
 });
    }
    fn key_contact(&mut self, weight: f32) {
        let Some(mut id) = self.selected else {
            self.status = "Select a hand, foot or lower limb".into();
            return;
        };
        let bone = self.project.rig.bone(id).unwrap();
        if bone.name.to_lowercase().contains("hand") || bone.name.to_lowercase().contains("foot") {
            id = bone.parent.unwrap_or(id);
        }
        let Some(lower) = self.project.rig.bone(id) else {
            return;
        };
        let Some(upper) = lower.parent.and_then(|id| self.project.rig.bone(id)) else {
            self.status = "Contact needs a connected two-bone limb".into();
            return;
        };
        if !lower.connected || upper.parent.is_none() {
            self.status = "Contacts cannot drive a root".into();
            return;
        }
        let world = self
            .project
            .rig
            .world(Some(&self.project.pose_at(self.frame)));
        let lw = world.iter().find(|w| w.id == id).unwrap();
        let uw = world.iter().find(|w| w.id == upper.id).unwrap();
        let bend = if rebonebase::animation::wrap_degrees(lw.angle - uw.angle) < 0. {
            -1.
        } else {
            1.
        };
        let key = ContactKey {
            frame: self.frame,
            target: lw.tail,
            weight,
            bend,
        };
        if let Err(error) = self.project.rig.active_clip_mut().release_hold(self.frame) {
            self.status = error;
            return;
        }
        let clips = self.project.rig.active_clip;
        let contacts = &mut self.project.extras.production.motion.contacts;
        if let Some(c) = contacts
            .iter_mut()
            .find(|c| c.lower == id && c.clip == Some(clips))
        {
            c.enabled = true;
            c.keys.retain(|k| k.frame != key.frame);
            c.keys.push(key);
            c.keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        } else {
            contacts.push(ContactTrack {
                lower: id,
                clip: Some(clips),
                keys: vec![key],
                enabled: true,
            });
        }
        self.mode = Mode::Pose;
        self.tool = Tool::IK;
    }
    fn add_long_chain(&mut self) {
        let Some(mut id) = self.selected else {
            self.status = "Select a chain tip".into();
            return;
        };
        let world = self
            .project
            .rig
            .world(Some(&self.project.pose_at(self.frame)));
        let target = world.iter().find(|w| w.id == id).unwrap().tail;
        let mut bones = vec![];
        while bones.len() < self.production.chain_length as usize {
            let Some(b) = self.project.rig.bone(id) else {
                break;
            };
            if b.parent.is_none() {
                break;
            }
            bones.push(id);
            if !b.connected {
                break;
            }
            id = b.parent.unwrap();
        }
        bones.reverse();
        if bones.len() < 2 {
            self.status = "Select at least a two-bone connected non-root chain".into();
            return;
        }
        self.project.extras.production.motion.chains.push(Chain {
            name: format!(
                "Chain {}",
                self.project.extras.production.motion.chains.len() + 1
            ),
            bones,
            target,
            ..Default::default()
        });
        self.mode = Mode::Pose;
        self.tool = Tool::IK;
    }
    pub(super) fn bake_all_motion(&mut self) -> anyhow::Result<()> {
        self.project.validate()?;
        let work: u64 = self
            .project
            .rig
            .clips
            .iter()
            .map(|c| {
                c.length as u64
                    * c.source_frame(c.length as f32).ceil() as u64
                    * self.project.rig.bones.len().max(1) as u64
                    * self.project.extras.production.motion.springs.len().max(1) as u64
            })
            .sum();
        anyhow::ensure!(
            work <= 16_000_000,
            "Controller bake exceeds work budget; reduce clips or springs"
        );
        let mut rig = self.project.rig.clone();
        for index in 0..rig.clips.len() {
            let mut source = self.project.clone();
            source.rig.active_clip = index;
            rig.active_clip = index;
            let source_timing = rig.active_clip().source.clone();
            rig.active_clip_mut().source = None;
            rig.active_clip_mut().tracks.clear();
            for frame in 1..=rig.active_clip().length {
                motion::bake_pose(
                    &mut rig,
                    &source.complete_pose_at(frame as f32),
                    frame as f32,
                )
                .map_err(anyhow::Error::msg)?;
            }
            rig.active_clip_mut()
                .restore_source_timing(source_timing)
                .map_err(anyhow::Error::msg)?;
        }
        rig.active_clip = self.project.rig.active_clip;
        self.project.rig = rig;
        self.project.extras.production.motion = Default::default();
        self.project.extras.production.baked_motion.clear();
        for pin in &mut self.project.extras.pins {
            pin.enabled = false;
        }
        Ok(())
    }
    fn mirror_production_clip(&mut self) {
        let index = self.project.rig.active_clip;
        match rebonebase::production::mirror_project_clip(&mut self.project, index) {
            Ok(()) => {
                self.frame = 1.;
                self.mode = Mode::Pose;
                self.studio.keys.clear();
            }
            Err(e) => self.status = e,
        }
    }
}
