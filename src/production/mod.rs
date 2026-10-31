//! Native production assets and scene data; old projects default to an empty scene.
pub mod audio;
pub mod clip_edit;
pub mod engine_export;
pub(crate) mod render;
use crate::{
    model::{Rig, Vec2},
    project::{Project, Reference},
};
use anyhow::{Result, ensure};
pub use clip_edit::{
    ClipEdit, edit_clip, mesh_keys, mesh_keys_mut, mesh_offsets, mirror_project_clip, prop_keys,
    prop_keys_mut, transition_project_clip,
};
pub use render::{
    CollisionShape, actor_placement, collision_pixels, collision_shapes, grid_mesh, mesh_vertices,
    paint_pixel, paint_pixels, switch_attachment_parent, timeline_ms,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpriteAsset {
    pub id: u64,
    pub name: String,
    pub image: Reference,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Attachment {
    pub id: u64,
    pub name: String,
    pub asset: u64,
    pub bone: Option<u32>,
    pub offset: Vec2,
    pub angle: f32,
    pub scale: f32,
    pub pivot: Vec2,
    pub z: i32,
    pub enabled: bool,
    pub keys: Vec<AttachmentKey>,
    pub clip: Option<usize>,
    pub clip_keys: BTreeMap<usize, Vec<AttachmentKey>>,
}
impl Default for Attachment {
    fn default() -> Self {
        Self {
            id: 0,
            name: "Prop".into(),
            asset: 0,
            bone: None,
            offset: Vec2::default(),
            angle: 0.,
            scale: 1.,
            pivot: Vec2::default(),
            z: 10,
            enabled: true,
            keys: vec![],
            clip: None,
            clip_keys: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AttachmentKey {
    pub frame: f32,
    pub bone: Option<u32>,
    pub offset: Vec2,
    pub angle: f32,
    pub asset: u64,
    pub visible: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Weight {
    pub bone: u32,
    pub weight: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    pub rest: Vec2,
    pub uv: Vec2,
    pub weights: Vec<Weight>,
}
/// Original world transform used to attach artwork to a bone. Unlike the
/// current rest skeleton, this snapshot stays fixed when rest joints move.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BindTransform {
    pub head: Vec2,
    pub angle: f32,
    pub length: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeshKey {
    pub frame: f32,
    pub offsets: Vec<Vec2>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mesh {
    pub id: u64,
    pub name: String,
    pub asset: u64,
    pub clip: usize,
    pub vertices: Vec<Vertex>,
    pub triangles: Vec<[usize; 3]>,
    pub keys: Vec<MeshKey>,
    pub z: i32,
    pub enabled: bool,
    pub replace_part: Option<u32>,
    pub bind: BTreeMap<u32, BindTransform>,
    pub clip_keys: BTreeMap<usize, Vec<MeshKey>>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Look {
    pub name: String,
    pub colors: BTreeMap<u32, [u8; 4]>,
    pub attachments: Vec<u64>,
    pub meshes: Vec<u64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FaceTrack {
    pub id: u64,
    pub name: String,
    pub bone: Option<u32>,
    pub clip: usize,
    pub offset: Vec2,
    pub angle: f32,
    pub scale: f32,
    pub z: i32,
    pub enabled: bool,
    pub keys: Vec<FaceKey>,
}
impl Default for FaceTrack {
    fn default() -> Self {
        Self {
            id: 0,
            name: "Eyes / mouth".into(),
            bone: None,
            clip: 0,
            offset: Vec2::default(),
            angle: 0.,
            scale: 1.,
            z: 20,
            enabled: true,
            keys: vec![],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FaceKey {
    pub frame: f32,
    pub asset: u64,
    pub offset: Vec2,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneActor {
    pub id: u64,
    pub name: String,
    pub project: Box<Project>,
    pub offset: Vec2,
    pub scale: f32,
    pub frame_offset: f32,
    pub speed: f32,
    pub visible: bool,
    pub z: i32,
    pub keys: Vec<ActorKey>,
    pub clip_time_ms: BTreeMap<usize, f64>,
}
impl Default for SceneActor {
    fn default() -> Self {
        Self {
            id: 0,
            name: "Actor".into(),
            project: Box::new(Project::new(Rig::default())),
            offset: Vec2::default(),
            scale: 1.,
            frame_offset: 0.,
            speed: 1.,
            visible: true,
            z: 0,
            keys: vec![],
            clip_time_ms: BTreeMap::new(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActorKey {
    pub clip: usize,
    pub frame: f32,
    pub offset: Vec2,
    pub scale: f32,
    pub visible: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Correction {
    pub clip: usize,
    pub frame: u32,
    pub pixels: BTreeMap<u32, [u8; 4]>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum BoxKind {
    #[default]
    Hurt,
    Hit,
    Interaction,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BoxTrack {
    pub id: u64,
    pub name: String,
    pub kind: BoxKind,
    pub clip: usize,
    pub bone: Option<u32>,
    pub offset: Vec2,
    pub size: Vec2,
    pub angle: f32,
    pub start: u32,
    pub end: u32,
    pub enabled: bool,
    pub keys: Vec<BoxKey>,
}
impl Default for BoxTrack {
    fn default() -> Self {
        Self {
            id: 0,
            name: "Hit region".into(),
            kind: BoxKind::Hurt,
            clip: 0,
            bone: None,
            offset: Vec2::default(),
            size: Vec2::new(8., 8.),
            angle: 0.,
            start: 1,
            end: 12,
            enabled: true,
            keys: vec![],
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoxKey {
    pub frame: f32,
    pub offset: Vec2,
    pub size: Vec2,
    pub angle: f32,
    pub enabled: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EventMarker {
    pub clip: usize,
    pub frame: f32,
    pub name: String,
    pub payload: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Production {
    pub motion: crate::advanced_motion::MotionTools,
    pub assets: Vec<SpriteAsset>,
    pub attachments: Vec<Attachment>,
    pub meshes: Vec<Mesh>,
    pub looks: Vec<Look>,
    pub active_look: Option<usize>,
    pub faces: Vec<FaceTrack>,
    pub actors: Vec<SceneActor>,
    pub corrections: Vec<Correction>,
    pub boxes: Vec<BoxTrack>,
    pub events: Vec<EventMarker>,
    pub audio: Option<audio::AudioTrack>,
    pub pivot: Vec2,
    pub baked_motion: BTreeSet<usize>,
}
impl Default for Production {
    fn default() -> Self {
        Self {
            motion: Default::default(),
            assets: vec![],
            attachments: vec![],
            meshes: vec![],
            looks: vec![],
            active_look: None,
            faces: vec![],
            actors: vec![],
            corrections: vec![],
            boxes: vec![],
            events: vec![],
            audio: None,
            pivot: Vec2::new(0.5, 1.),
            baked_motion: BTreeSet::new(),
        }
    }
}
fn finite(v: f32) -> bool {
    v.is_finite() && v.abs() <= 100_000.
}
fn point(v: Vec2) -> bool {
    finite(v.x) && finite(v.y)
}
fn named(s: &str) -> bool {
    s.len() <= 1024
}
impl Production {
    pub fn render_work(&self, rig: &Rig) -> u64 {
        let base = rig.width as u64 * rig.height as u64;
        let units = rig
            .bones
            .len()
            .max(1)
            .saturating_add(self.attachments.len())
            .saturating_add(self.faces.len())
            .saturating_add(self.meshes.iter().map(|m| m.triangles.len()).sum::<usize>());
        self.actors
            .iter()
            .fold(base.saturating_mul(units as u64), |sum, a| {
                sum.saturating_add(a.project.extras.production.render_work(&a.project.rig))
            })
    }
    pub fn next_id(&self) -> u64 {
        self.assets
            .iter()
            .map(|v| v.id)
            .chain(self.attachments.iter().map(|v| v.id))
            .chain(self.meshes.iter().map(|v| v.id))
            .chain(self.faces.iter().map(|v| v.id))
            .chain(self.actors.iter().map(|v| v.id))
            .chain(self.boxes.iter().map(|v| v.id))
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }
    pub fn has_art(&self) -> bool {
        !self.attachments.is_empty()
            || !self.meshes.is_empty()
            || !self.faces.is_empty()
            || !self.actors.is_empty()
            || !self.corrections.is_empty()
            || self.active_look.is_some()
    }
    pub fn validate(&self, rig: &Rig) -> Result<()> {
        self.motion.validate(rig).map_err(anyhow::Error::msg)?;
        ensure!(
            self.assets.len() <= 64
                && self.attachments.len() <= 128
                && self.meshes.len() <= 32
                && self.faces.len() <= 32
                && self.looks.len() <= 64
                && self.actors.len() <= 8
                && self.boxes.len() <= 128
                && self.events.len() <= 4096
                && self.corrections.len() <= 7680,
            "Production data exceeds scene limits"
        );
        let bone = |id: Option<u32>| id.is_none_or(|id| rig.bone(id).is_some());
        let asset = |id: u64| self.assets.iter().any(|a| a.id == id);
        let clip = |i: usize| i < rig.clips.len();
        let key = |f: f32, i: usize| {
            clip(i) && f.is_finite() && f >= 1. && f <= rig.clips[i].length as f32
        };
        let source_key = |f: f32, i: usize| {
            clip(i) && f.is_finite() && {
                let (first, last) = rig.clips[i].source_bounds();
                f >= first && f <= last
            }
        };
        ensure!(
            self.baked_motion.iter().all(|i| clip(*i)),
            "Invalid baked-motion clip"
        );
        let mut prop_key_count = 0usize;
        for a in &self.attachments {
            ensure!(a.clip_keys.len() <= 32, "Too many prop clip timelines");
            for (i, keys) in &a.clip_keys {
                ensure!(clip(*i) && keys.len() <= 240, "Invalid prop clip timeline");
                let mut last = rig.clips[*i].source_bounds().0 - 1.;
                for k in keys {
                    ensure!(
                        source_key(k.frame, *i)
                            && k.frame > last
                            && bone(k.bone)
                            && asset(k.asset)
                            && point(k.offset)
                            && finite(k.angle),
                        "Invalid prop clip key"
                    );
                    last = k.frame;
                }
                prop_key_count += keys.len();
            }
        }
        ensure!(prop_key_count <= 100_000, "Too many prop animation keys");
        for a in &self.actors {
            ensure!(
                a.clip_time_ms.len() <= 32
                    && a.clip_time_ms
                        .iter()
                        .all(|(i, ms)| clip(*i) && ms.is_finite() && (0. ..=1e12).contains(ms)),
                "Invalid actor clip time origin"
            );
        }
        let mut ids = HashSet::new();
        for id in self
            .assets
            .iter()
            .map(|v| v.id)
            .chain(self.attachments.iter().map(|v| v.id))
            .chain(self.meshes.iter().map(|v| v.id))
            .chain(self.faces.iter().map(|v| v.id))
            .chain(self.actors.iter().map(|v| v.id))
            .chain(self.boxes.iter().map(|v| v.id))
        {
            ensure!(
                id > 0 && ids.insert(id),
                "Asset and controller IDs must be unique and positive"
            );
        }
        let mut pixels = 0u64;
        for a in &self.assets {
            let r = &a.image;
            pixels += r.width as u64 * r.height as u64;
            ensure!(
                named(&a.name)
                    && r.width > 0
                    && r.height > 0
                    && r.width <= 4096
                    && r.height <= 4096
                    && r.pixels.len() as u64 == r.width as u64 * r.height as u64 * 4,
                "Invalid sprite asset"
            );
        }
        ensure!(pixels <= 16 * 1024 * 1024, "Artwork exceeds 16 megapixels");
        for a in &self.attachments {
            ensure!(
                named(&a.name)
                    && asset(a.asset)
                    && bone(a.bone)
                    && point(a.offset)
                    && point(a.pivot)
                    && finite(a.angle)
                    && a.scale.is_finite()
                    && (0.01..=100.).contains(&a.scale)
                    && a.keys.len() <= 240
                    && a.clip.is_none_or(clip),
                "Invalid attachment"
            );
            let (first, end) = a.clip.map_or((1., 240.), |i| rig.clips[i].source_bounds());
            let mut last = first - 1.;
            for k in &a.keys {
                ensure!(
                    k.frame.is_finite()
                        && k.frame >= first
                        && k.frame <= end
                        && k.frame > last
                        && bone(k.bone)
                        && asset(k.asset)
                        && point(k.offset)
                        && finite(k.angle),
                    "Invalid prop animation key"
                );
                last = k.frame;
            }
        }
        let mut mesh_work = 0u64;
        let mut mesh_samples = 0usize;
        for m in &self.meshes {
            ensure!(
                named(&m.name)
                    && asset(m.asset)
                    && clip(m.clip)
                    && m.vertices.len() >= 3
                    && m.vertices.len() <= 256
                    && m.triangles.len() <= 512
                    && m.keys.len() <= 240
                    && m.replace_part
                        .is_none_or(|id| rig.parts.iter().any(|p| p.id == id)),
                "Invalid mesh"
            );
            ensure!(
                m.bind.len() <= 2048
                    && m.bind.iter().all(|(id, b)| bone(Some(*id))
                        && point(b.head)
                        && finite(b.angle)
                        && b.length.is_finite()
                        && (0.0001..=65536.).contains(&b.length)),
                "Invalid mesh bind transform"
            );
            mesh_work += m.triangles.len() as u64 * rig.width as u64 * rig.height as u64;
            for v in &m.vertices {
                ensure!(
                    point(v.rest)
                        && point(v.uv)
                        && (0. ..=1.).contains(&v.uv.x)
                        && (0. ..=1.).contains(&v.uv.y)
                        && !v.weights.is_empty()
                        && v.weights.len() <= 4,
                    "Invalid mesh vertex"
                );
                let sum: f32 = v.weights.iter().map(|w| w.weight).sum();
                let mut weighted = HashSet::new();
                ensure!(
                    (sum - 1.).abs() < 0.001
                        && v.weights.iter().all(|w| bone(Some(w.bone))
                            && w.weight.is_finite()
                            && w.weight >= 0.
                            && weighted.insert(w.bone)
                            && (m.bind.is_empty() || m.bind.contains_key(&w.bone))),
                    "Mesh weights must sum to one, be unique, and have bind transforms"
                );
            }
            for t in &m.triangles {
                ensure!(
                    t.iter().all(|i| *i < m.vertices.len())
                        && t[0] != t[1]
                        && t[1] != t[2]
                        && t[0] != t[2],
                    "Invalid mesh triangle"
                );
            }
            let mut last = rig.clips[m.clip].source_bounds().0 - 1.;
            for k in &m.keys {
                ensure!(
                    source_key(k.frame, m.clip)
                        && k.frame > last
                        && k.offsets.len() == m.vertices.len()
                        && k.offsets.iter().all(|v| point(*v)),
                    "Invalid mesh deformation key"
                );
                last = k.frame;
            }
            mesh_samples += m.keys.len() * m.vertices.len();
            ensure!(m.clip_keys.len() <= 32, "Too many mesh clip timelines");
            for (i, keys) in &m.clip_keys {
                ensure!(clip(*i) && keys.len() <= 240, "Invalid mesh clip timeline");
                let mut last = rig.clips[*i].source_bounds().0 - 1.;
                for k in keys {
                    ensure!(
                        source_key(k.frame, *i)
                            && k.frame > last
                            && k.offsets.len() == m.vertices.len()
                            && k.offsets.iter().all(|v| point(*v)),
                        "Invalid mesh clip key"
                    );
                    last = k.frame;
                }
                mesh_samples += keys.len() * m.vertices.len();
            }
        }
        ensure!(
            mesh_work <= 128 * 1024 * 1024,
            "Mesh raster work exceeds budget"
        );
        ensure!(
            mesh_samples <= 4 * 1024 * 1024,
            "Mesh deformation timelines exceed sample budget"
        );
        ensure!(
            self.active_look.is_none_or(|i| i < self.looks.len()),
            "Missing active outfit"
        );
        for l in &self.looks {
            ensure!(
                named(&l.name)
                    && l.colors
                        .keys()
                        .all(|id| rig.parts.iter().any(|p| p.id == *id))
                    && l.attachments
                        .iter()
                        .all(|id| self.attachments.iter().any(|a| a.id == *id))
                    && l.meshes
                        .iter()
                        .all(|id| self.meshes.iter().any(|a| a.id == *id)),
                "Invalid outfit references"
            );
        }
        for f in &self.faces {
            ensure!(
                named(&f.name)
                    && bone(f.bone)
                    && clip(f.clip)
                    && point(f.offset)
                    && finite(f.angle)
                    && f.scale.is_finite()
                    && (0.01..=100.).contains(&f.scale)
                    && f.keys.len() <= 240,
                "Invalid face track"
            );
            let mut last = rig.clips[f.clip].source_bounds().0 - 1.;
            for k in &f.keys {
                ensure!(
                    source_key(k.frame, f.clip)
                        && k.frame > last
                        && asset(k.asset)
                        && point(k.offset),
                    "Invalid facial animation key"
                );
                last = k.frame;
            }
        }
        let mut actor_work = 0u64;
        for a in &self.actors {
            ensure!(
                a.project.extras.production.actors.is_empty(),
                "Nested scenes are not supported; import actors individually"
            );
            ensure!(
                named(&a.name)
                    && point(a.offset)
                    && finite(a.frame_offset)
                    && a.speed.is_finite()
                    && (0. ..=8.).contains(&a.speed)
                    && a.scale.is_finite()
                    && (0.05..=16.).contains(&a.scale),
                "Invalid scene actor transform"
            );
            a.project.validate()?;
            ensure!(a.keys.len() <= 7680, "Too many actor placement keys");
            let mut used = HashSet::new();
            for k in &a.keys {
                ensure!(
                    source_key(k.frame, k.clip)
                        && point(k.offset)
                        && k.scale.is_finite()
                        && (0.05..=16.).contains(&k.scale)
                        && used.insert((k.clip, k.frame.to_bits())),
                    "Invalid actor placement key"
                );
            }
            actor_work += a.project.extras.production.render_work(&a.project.rig);
        }
        ensure!(
            actor_work <= 64 * 1024 * 1024,
            "Scene actors exceed render budget"
        );
        ensure!(
            self.render_work(rig) <= 128 * 1024 * 1024,
            "Scene artwork exceeds per-frame render budget; reduce canvas, props or actors"
        );
        let mut correction_pixels = 0usize;
        let mut correction_ids = HashSet::new();
        for c in &self.corrections {
            ensure!(
                key(c.frame as f32, c.clip)
                    && correction_ids.insert((c.clip, c.frame))
                    && c.pixels.keys().all(|i| *i < rig.width * rig.height),
                "Invalid pixel correction frame"
            );
            correction_pixels += c.pixels.len();
        }
        ensure!(
            correction_pixels <= 4 * 1024 * 1024,
            "Pixel corrections exceed four megapixels"
        );
        for b in &self.boxes {
            ensure!(
                named(&b.name)
                    && clip(b.clip)
                    && bone(b.bone)
                    && point(b.offset)
                    && point(b.size)
                    && b.size.x > 0.
                    && b.size.y > 0.
                    && finite(b.angle)
                    && b.start >= 1
                    && b.end >= b.start
                    && b.end <= rig.clips[b.clip].length
                    && b.keys.len() <= 240,
                "Invalid collision region"
            );
            let mut last = rig.clips[b.clip].source_bounds().0 - 1.;
            for k in &b.keys {
                ensure!(
                    source_key(k.frame, b.clip)
                        && k.frame > last
                        && point(k.offset)
                        && point(k.size)
                        && k.size.x > 0.
                        && k.size.y > 0.
                        && finite(k.angle),
                    "Invalid collision animation key"
                );
                last = k.frame;
            }
        }
        for e in &self.events {
            ensure!(
                key(e.frame, e.clip)
                    && !e.name.trim().is_empty()
                    && named(&e.name)
                    && e.payload.len() <= 4096,
                "Invalid event marker"
            );
        }
        if let Some(a) = &self.audio {
            a.validate()?;
        }
        ensure!(
            point(self.pivot)
                && (0. ..=1.).contains(&self.pivot.x)
                && (0. ..=1.).contains(&self.pivot.y),
            "Export pivot must be normalized"
        );
        Ok(())
    }
    /// Keep clip-owned data attached after delete/reorder operations; missing clips are discarded.
    pub fn remap_clips(&mut self, map: &[Option<usize>]) {
        clip_edit::remap_metadata(self, map);
    }
    pub fn prune_bones(&mut self, rig: &Rig) {
        let valid = |id: u32| rig.bone(id).is_some();
        self.attachments.retain(|a| {
            a.bone.is_none_or(valid)
                && a.keys
                    .iter()
                    .chain(a.clip_keys.values().flatten())
                    .all(|k| k.bone.is_none_or(valid))
        });
        self.faces.retain(|a| a.bone.is_none_or(valid));
        self.boxes.retain(|a| a.bone.is_none_or(valid));
        self.meshes.retain(|m| {
            m.vertices
                .iter()
                .all(|v| v.weights.iter().all(|w| valid(w.bone)))
        });
        for m in &mut self.meshes {
            m.bind.retain(|id, _| valid(*id));
        }
        self.motion.chains.retain(|c| {
            c.bones.len() >= 2
                && c.bones.iter().enumerate().all(|(i, id)| {
                    rig.bone(*id).is_some_and(|b| {
                        b.parent.is_some()
                            && (i == 0 || (b.connected && b.parent == Some(c.bones[i - 1])))
                    })
                })
        });
        self.motion.contacts.retain(|c| {
            rig.bone(c.lower)
                .filter(|b| b.connected)
                .and_then(|b| b.parent)
                .and_then(|id| rig.bone(id))
                .is_some_and(|upper| upper.parent.is_some())
        });
        self.motion
            .springs
            .retain(|c| rig.bone(c.bone).is_some_and(|b| b.parent.is_some()));
        self.motion.layers.retain_mut(|l| {
            let masked = !l.mask.is_empty();
            l.mask
                .retain(|id| valid(*id) || rig.parts.iter().any(|p| p.id == *id));
            !masked || !l.mask.is_empty()
        });
        for l in &mut self.looks {
            l.colors
                .retain(|id, _| rig.parts.iter().any(|p| p.id == *id));
            l.attachments
                .retain(|id| self.attachments.iter().any(|a| a.id == *id));
            l.meshes
                .retain(|id| self.meshes.iter().any(|a| a.id == *id));
        }
    }
}
