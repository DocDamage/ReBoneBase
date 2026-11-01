//! Dope-sheet support for the selected 3D character's keyed joint corrections.
use super::*;
use rebonebase::character3d::{Character, JointKeyRef};

pub(in crate::app) struct JointDrag {
    pub base: Project,
    pub character: u64,
    pub selected: Vec<JointKeyRef>,
    pub start: f32,
    pub recorded: bool,
}
impl ReboneApp {
    pub(in crate::app) fn joint_character(&self) -> Option<&Character> {
        let id = self.production.character?;
        self.project.extras.characters3d.iter().find(|c| c.id == id)
    }
    fn joint_character_mut(&mut self) -> Option<&mut Character> {
        let id = self.production.character?;
        self.project
            .extras
            .characters3d
            .iter_mut()
            .find(|c| c.id == id)
    }
    /// Keyed joints of the selected 3D character, plus its selected joint.
    pub(in crate::app) fn joint_rows(&self) -> Vec<(usize, String)> {
        let Some(c) = self.joint_character() else {
            return vec![];
        };
        let clip = self.project.rig.active_clip;
        let query = self.timeline.search.trim().to_lowercase();
        c.model
            .nodes
            .iter()
            .enumerate()
            .filter(|(j, n)| {
                let keyed = !c.joint_keys(clip, *j).is_empty();
                let selected = *j == self.production.character_joint;
                n.name.to_lowercase().contains(&query)
                    && match self.timeline.filter {
                        TrackFilter::All => keyed || selected,
                        TrackFilter::Selected => selected,
                        TrackFilter::Animated => keyed,
                    }
            })
            .map(|(j, n)| (j, format!("3D · {}", n.name)))
            .collect()
    }
    pub(in crate::app) fn joint_key_frames(&self, joint: usize) -> Vec<f32> {
        self.joint_character().map_or_else(Vec::new, |c| {
            c.joint_keys(self.project.rig.active_clip, joint)
                .iter()
                .map(|k| k.frame)
                .collect()
        })
    }
    pub(in crate::app) fn joint_key_selected(&self, joint: usize, frame: f32) -> bool {
        self.studio
            .joint_keys
            .iter()
            .any(|k| k.joint == joint && (k.frame - frame).abs() < 0.001)
    }
    /// Click or drag-start on a joint row: update the selection and maybe
    /// begin moving the selected keys.
    pub(in crate::app) fn press_joint_keys(
        &mut self,
        hits: Vec<JointKeyRef>,
        shift: bool,
        drag: bool,
        frame: f32,
    ) {
        if !drag
            || !hits
                .iter()
                .any(|h| self.joint_key_selected(h.joint, h.frame))
        {
            if !shift {
                self.studio.keys.clear();
                self.studio.joint_keys.clear();
            }
            for hit in hits.iter().copied() {
                if shift && self.joint_key_selected(hit.joint, hit.frame) {
                    self.studio.joint_keys.retain(|k| *k != hit);
                } else if !self.joint_key_selected(hit.joint, hit.frame) {
                    self.studio.joint_keys.push(hit);
                }
            }
        }
        if drag
            && !hits.is_empty()
            && let Some(character) = self.production.character
        {
            self.studio.joint_drag = Some(JointDrag {
                base: self.project.clone(),
                character,
                selected: self.studio.joint_keys.clone(),
                start: frame,
                recorded: false,
            });
        }
    }
    pub(in crate::app) fn drag_joint_keys(&mut self, frame: f32) {
        let Some(mut drag) = self.studio.joint_drag.take() else {
            return;
        };
        let offset = (frame - drag.start).round();
        let clip = drag.base.rig.active_clip;
        let length = drag.base.rig.active_clip().length;
        let mut candidate = drag.base.clone();
        let moved = candidate
            .extras
            .characters3d
            .iter_mut()
            .find(|c| c.id == drag.character)
            .map_or(Err("The 3D character was removed".to_string()), |c| {
                c.move_joint_keys(clip, &drag.selected, offset, length)
            });
        match moved {
            Ok(()) => {
                if offset != 0. && !drag.recorded {
                    self.history.record(&drag.base);
                    drag.recorded = true;
                }
                self.project = candidate;
                self.studio.joint_keys = drag
                    .selected
                    .iter()
                    .map(|k| JointKeyRef {
                        frame: k.frame + offset,
                        ..*k
                    })
                    .collect();
                self.changed();
            }
            Err(e) => self.status = e,
        }
        self.studio.joint_drag = Some(drag);
    }
    pub(in crate::app) fn select_all_joint_keys(&mut self) {
        let clip = self.project.rig.active_clip;
        let length = self.project.rig.active_clip().length as f32;
        self.studio.joint_keys = self.joint_character().map_or_else(Vec::new, |c| {
            c.tracks
                .get(&clip)
                .into_iter()
                .flatten()
                .flat_map(|(j, keys)| {
                    keys.iter()
                        .filter(|k| (1. ..=length).contains(&k.frame))
                        .map(|k| JointKeyRef {
                            joint: *j,
                            frame: k.frame,
                        })
                })
                .collect()
        });
    }
    pub(in crate::app) fn copy_joint_keys(&mut self) -> usize {
        let clip = self.project.rig.active_clip;
        self.studio.joint_clipboard = self
            .joint_character()
            .filter(|_| !self.studio.joint_keys.is_empty())
            .map(|c| c.copy_joint_keys(clip, &self.studio.joint_keys));
        self.studio
            .joint_clipboard
            .as_ref()
            .map_or(0, |c| c.keys.len())
    }
    pub(in crate::app) fn paste_joint_keys(&mut self) -> Result<bool, String> {
        let Some(clipboard) = self.studio.joint_clipboard.clone() else {
            return Ok(false);
        };
        let (clip, frame) = (self.project.rig.active_clip, self.frame.round());
        let c = self
            .joint_character_mut()
            .ok_or("Choose a 3D character to paste joint keys")?;
        let pasted = c.paste_joint_keys(clip, &clipboard, frame)?;
        self.studio.joint_keys = pasted;
        Ok(true)
    }
    pub(in crate::app) fn delete_joint_keys(&mut self) {
        let clip = self.project.rig.active_clip;
        let keys = std::mem::take(&mut self.studio.joint_keys);
        if let Some(c) = self.joint_character_mut() {
            c.delete_joint_keys(clip, &keys);
        }
        self.studio.joint_drag = None;
    }
}
