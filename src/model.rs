//! Native document and FK model, ported from Bonebase's MIT-licensed rig.lua.
use crate::animation::{Clip, Pose};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::ops::{Add, Mul, Sub};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}
impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn length(self) -> f32 {
        self.x.hypot(self.y)
    }
    pub fn rotate(self, degrees: f32) -> Self {
        let (s, c) = degrees.to_radians().sin_cos();
        Self::new(self.x * c - self.y * s, self.x * s + self.y * c)
    }
    pub fn angle(self) -> f32 {
        self.y.atan2(self.x).to_degrees()
    }
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}
impl Add for Vec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}
impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}
impl Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bulge {
    pub t: f32,
    pub d: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bone {
    pub id: u32,
    pub name: String,
    pub parent: Option<u32>,
    pub connected: bool,
    pub x: f32,
    pub y: f32,
    pub length: f32,
    pub angle: f32,
    pub radius_head: f32,
    pub radius_tail: f32,
    pub part: u32,
    pub twin: Option<u32>,
    pub bulges_left: Vec<Bulge>,
    pub bulges_right: Vec<Bulge>,
    pub locked: bool,
    pub length_locked: bool,
    pub rotation_min: Option<f32>,
    pub rotation_max: Option<f32>,
    pub bend: f32,
}
impl Default for Bone {
    fn default() -> Self {
        Self {
            id: 1,
            name: "Bone".into(),
            parent: None,
            connected: false,
            x: 0.,
            y: 0.,
            length: 8.,
            angle: 0.,
            radius_head: 4.,
            radius_tail: 4.,
            part: 0,
            twin: None,
            bulges_left: vec![],
            bulges_right: vec![],
            locked: false,
            length_locked: false,
            rotation_min: None,
            rotation_max: None,
            bend: 1.,
        }
    }
}
impl Bone {
    pub fn clamp_rotation(&self, value: f32) -> f32 {
        let min = self.rotation_min.unwrap_or(-100_000.);
        let max = self.rotation_max.unwrap_or(100_000.);
        if min <= max {
            value.clamp(min, max)
        } else {
            value
        }
    }
    pub fn clamp_scale(&self, value: f32) -> f32 {
        if self.length_locked {
            1.
        } else {
            value.clamp(0.1, 4.)
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Part {
    pub id: u32,
    pub name: String,
    pub color: [u8; 4],
    pub z: i32,
    pub blend: f32,
    pub outline_width: u32,
    pub outline_color: [u8; 4],
}
impl Default for Part {
    fn default() -> Self {
        Self {
            id: 1,
            name: "Body".into(),
            color: [175, 185, 205, 255],
            z: 0,
            blend: 2.,
            outline_width: 0,
            outline_color: [0, 0, 0, 255],
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mirror {
    pub on: bool,
    pub x: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rig {
    #[serde(default = "default_name")]
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub bones: Vec<Bone>,
    pub parts: Vec<Part>,
    #[serde(default = "default_clips")]
    pub clips: Vec<Clip>,
    #[serde(default)]
    pub active_clip: usize,
    #[serde(default)]
    pub mirror: Mirror,
}
fn default_name() -> String {
    "Untitled".into()
}
fn default_clips() -> Vec<Clip> {
    vec![Clip::default()]
}
impl Default for Rig {
    fn default() -> Self {
        Self::new("Untitled", 64, 64)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct WorldBone {
    pub id: u32,
    pub head: Vec2,
    pub tail: Vec2,
    pub angle: f32,
    pub radius_head: f32,
    pub radius_tail: f32,
    pub part: u32,
}

impl Rig {
    pub fn new(name: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            name: name.into(),
            width,
            height,
            bones: vec![],
            parts: vec![],
            clips: vec![Clip::default()],
            active_clip: 0,
            mirror: Mirror {
                on: false,
                x: width as f32 / 2.,
            },
        }
    }
    pub fn bone(&self, id: u32) -> Option<&Bone> {
        self.bones.iter().find(|b| b.id == id)
    }
    pub fn bone_mut(&mut self, id: u32) -> Option<&mut Bone> {
        self.bones.iter_mut().find(|b| b.id == id)
    }
    pub fn active_clip(&self) -> &Clip {
        &self.clips[self.active_clip.min(self.clips.len() - 1)]
    }
    pub fn active_clip_mut(&mut self) -> &mut Clip {
        let i = self.active_clip.min(self.clips.len() - 1);
        &mut self.clips[i]
    }
    fn next_id(&self) -> u32 {
        self.bones
            .iter()
            .map(|b| b.id)
            .chain(self.parts.iter().map(|p| p.id))
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }
    pub fn add_part(&mut self, name: impl Into<String>, color: [u8; 4]) -> u32 {
        let id = self.next_id();
        self.parts.push(Part {
            id,
            name: name.into(),
            color,
            z: self.parts.len() as i32,
            blend: 2.,
            ..Part::default()
        });
        id
    }
    /// Solves parent-relative rest transforms and optional animated offsets. Input order is preserved.
    pub fn world(&self, pose: Option<&Pose>) -> Vec<WorldBone> {
        let mut solved: BTreeMap<u32, WorldBone> = BTreeMap::new();
        for _ in 0..=self.bones.len() {
            let before = solved.len();
            for b in &self.bones {
                if solved.contains_key(&b.id) {
                    continue;
                }
                let p = match b.parent {
                    Some(id) => match solved.get(&id) {
                        Some(p) => Some(p),
                        None => continue,
                    },
                    None => None,
                };
                let d = pose.and_then(|p| p.get(&b.id)).copied().unwrap_or_default();
                let rotation = if pose.is_some() {
                    b.clamp_rotation(d.rotation)
                } else {
                    0.
                };
                let angle = b.angle + rotation + p.map_or(0., |p| p.angle);
                let offset = Vec2::new(b.x + d.x, b.y + d.y);
                let head = match p {
                    Some(p) if b.connected => p.tail,
                    Some(p) => p.head + offset.rotate(p.angle),
                    None => offset,
                };
                solved.insert(
                    b.id,
                    WorldBone {
                        id: b.id,
                        head,
                        tail: head + Vec2::new(b.length * b.clamp_scale(d.scale), 0.).rotate(angle),
                        angle,
                        radius_head: b.radius_head,
                        radius_tail: b.radius_tail,
                        part: b.part,
                    },
                );
            }
            if solved.len() == before {
                break;
            }
        }
        self.bones
            .iter()
            .filter_map(|b| solved.remove(&b.id))
            .collect()
    }
    pub fn add_bone(&mut self, head: Vec2, tail: Vec2, parent: Option<u32>) -> Result<u32, String> {
        if !head.is_finite()
            || !tail.is_finite()
            || (tail - head).length() < 1.
            || (tail - head).length() > 16384.
        {
            return Err("Bone endpoints must be finite and 1–16384 pixels apart".into());
        }
        if self.bones.len() >= 2048 {
            return Err("Too many bones".into());
        }
        let world = self.world(None);
        let p = match parent {
            Some(id) => Some(world.iter().find(|p| p.id == id).ok_or("No such parent")?),
            None => None,
        };
        let local = p.map_or(head, |p| (head - p.head).rotate(-p.angle));
        let connected = p.is_some_and(|p| (head - p.tail).length() < 0.001);
        let radius = parent
            .and_then(|id| self.bone(id))
            .map_or(4., |b| b.radius_tail);
        let part = parent.and_then(|id| self.bone(id)).map_or(0, |b| b.part);
        let id = self.next_id();
        self.bones.push(Bone {
            id,
            name: format!("Bone {}", self.bones.len() + 1),
            parent,
            connected,
            x: local.x,
            y: local.y,
            length: (tail - head).length(),
            angle: crate::animation::wrap_degrees(
                (tail - head).angle() - p.map_or(0., |p| p.angle),
            ),
            radius_head: radius,
            radius_tail: radius,
            part,
            ..Bone::default()
        });
        Ok(id)
    }
    pub fn delete_bone(&mut self, id: u32) -> usize {
        if self.bone(id).is_none() {
            return 0;
        }
        let mut gone = HashSet::from([id]);
        loop {
            let before = gone.len();
            for b in &self.bones {
                if b.parent.is_some_and(|p| gone.contains(&p)) {
                    gone.insert(b.id);
                }
            }
            if before == gone.len() {
                break;
            }
        }
        self.bones.retain(|b| !gone.contains(&b.id));
        for b in &mut self.bones {
            if b.twin.is_some_and(|t| gone.contains(&t)) {
                b.twin = None;
            }
        }
        for c in &mut self.clips {
            c.tracks.retain(|t| !gone.contains(&t.bone));
        }
        gone.len()
    }
    pub fn move_tail(&mut self, id: u32, target: Vec2) -> Result<(), String> {
        if self.bone(id).is_some_and(|b| b.locked) {
            return Err("Bone is locked".into());
        }
        if !target.is_finite() {
            return Err("Non-finite endpoint".into());
        }
        let world = self.world(None);
        let w = world.iter().find(|w| w.id == id).ok_or("No such bone")?;
        let delta = target - w.head;
        let length = delta.length();
        if !(1. ..=16384.).contains(&length) {
            return Err("Bone length outside limits".into());
        }
        let parent_angle = self
            .bone(id)
            .and_then(|b| b.parent)
            .and_then(|p| world.iter().find(|w| w.id == p))
            .map_or(0., |p| p.angle);
        let b = self.bone_mut(id).ok_or("No such bone")?;
        if !b.length_locked {
            b.length = length;
        }
        b.angle = crate::animation::wrap_degrees(delta.angle() - parent_angle);
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.width == 0 || self.height == 0 || self.width > 4096 || self.height > 4096 {
            return Err("Canvas dimensions must be 1–4096".into());
        }
        // A conservative bound prevents an imported rig from monopolizing the UI thread.
        if u64::from(self.width) * u64::from(self.height) * self.bones.len() as u64 > 64_000_000 {
            return Err("Canvas and bone count exceed the 64-million-sample render budget".into());
        }
        if self.bones.len() > 2048
            || self.parts.len() > 256
            || self.clips.is_empty()
            || self.clips.len() > 32
            || self.active_clip >= self.clips.len()
        {
            return Err("Document count or active clip outside limits".into());
        }
        if self.name.len() > 1024 || !bounded(self.mirror.x) {
            return Err("Invalid document metadata".into());
        }
        let mut ids = HashSet::new();
        for p in &self.parts {
            if p.id == 0
                || !ids.insert(p.id)
                || p.name.len() > 1024
                || !p.blend.is_finite()
                || !(0. ..=4096.).contains(&p.blend)
                || p.outline_width > 16
            {
                return Err("Invalid part".into());
            }
        }
        for b in &self.bones {
            if b.rotation_min
                .into_iter()
                .chain(b.rotation_max)
                .any(|v| !v.is_finite() || v.abs() > 100_000.)
                || b.rotation_min.unwrap_or(-100_000.) > b.rotation_max.unwrap_or(100_000.)
                || !matches!(b.bend, -1.0 | 1.0)
            {
                return Err("Invalid bone constraints".into());
            }
            if b.id == 0
                || !ids.insert(b.id)
                || b.name.len() > 1024
                || ![b.x, b.y, b.angle, b.length, b.radius_head, b.radius_tail]
                    .into_iter()
                    .all(bounded)
                || b.length < 1.
                || b.length > 16384.
                || b.radius_head < 0.
                || b.radius_tail < 0.
                || b.radius_head > 4096.
                || b.radius_tail > 4096.
            {
                return Err("Invalid bone geometry or duplicate id".into());
            }
            if b.part != 0 && !self.parts.iter().any(|p| p.id == b.part) {
                return Err("Bone references missing part".into());
            }
            if b.parent.is_some_and(|p| self.bone(p).is_none())
                || b.twin.is_some_and(|p| self.bone(p).is_none() || p == b.id)
            {
                return Err("Bone references missing parent or twin".into());
            }
            for list in [&b.bulges_left, &b.bulges_right] {
                if list.len() > 128 {
                    return Err("Too many bulges".into());
                }
                let mut previous = 0.;
                for q in list {
                    if !q.t.is_finite()
                        || q.t <= 0.
                        || q.t >= 1.
                        || q.t < previous
                        || !q.d.is_finite()
                        || q.d.abs() > 4096.
                    {
                        return Err("Invalid bulge".into());
                    }
                    previous = q.t;
                }
            }
        }
        if self.world(None).len() != self.bones.len() {
            return Err("Cyclic bone hierarchy".into());
        }
        for c in &self.clips {
            c.validate(&ids)?;
        }
        Ok(())
    }
    pub fn to_json(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }
    pub fn from_json(text: &str) -> Result<Self, String> {
        if text.len() > 16 * 1024 * 1024 {
            return Err("Document exceeds 16 MiB".into());
        }
        let rig: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        rig.validate()?;
        Ok(rig)
    }
}
fn bounded(x: f32) -> bool {
    x.is_finite() && x.abs() <= 100_000.
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(a: f32, b: f32) {
        assert!((a - b).abs() < 0.001, "{a} != {b}");
    }
    #[test]
    fn hierarchy_respects_local_parent_frames() {
        let mut r = Rig::new("Test", 64, 64);
        let a = r
            .add_bone(Vec2::new(10., 10.), Vec2::new(10., 20.), None)
            .unwrap();
        let b = r
            .add_bone(Vec2::new(10., 20.), Vec2::new(20., 20.), Some(a))
            .unwrap();
        let w = r.world(None);
        close(w[1].head.y, 20.);
        close(w[1].tail.x, 20.);
        r.bone_mut(a).unwrap().angle = 0.;
        let w = r.world(None);
        close(w[1].head.x, 20.);
        close(w[1].tail.y, 0.);
        assert_eq!(r.bone(b).unwrap().parent, Some(a));
    }
    #[test]
    fn serialization_clone_undo_and_cascade() {
        let mut r = Rig::new("Test", 32, 32);
        let a = r
            .add_bone(Vec2::new(2., 2.), Vec2::new(8., 2.), None)
            .unwrap();
        let b = r
            .add_bone(Vec2::new(8., 2.), Vec2::new(12., 2.), Some(a))
            .unwrap();
        r.clips[0]
            .set_key(b, crate::animation::Channel::Rotation, 1., 35.)
            .unwrap();
        let snapshot = r.clone();
        r.move_tail(a, Vec2::new(2., 12.)).unwrap();
        close(r.bone(a).unwrap().angle, 90.);
        r = snapshot;
        let restored = Rig::from_json(&r.to_json().unwrap()).unwrap();
        assert_eq!(restored, r);
        assert_eq!(r.delete_bone(a), 2);
        assert!(r.clips[0].tracks.is_empty());
    }
    #[test]
    fn validation_rejects_dangerous_or_broken_documents() {
        assert!(Rig::from_json("return os.execute('bad')").is_err());
        let mut r = Rig::new("Bad", 64, 64);
        let a = r
            .add_bone(Vec2::new(1., 1.), Vec2::new(4., 1.), None)
            .unwrap();
        r.bone_mut(a).unwrap().parent = Some(a);
        assert!(r.validate().is_err());
        r.bone_mut(a).unwrap().parent = None;
        r.bone_mut(a).unwrap().length = f32::NAN;
        assert!(r.validate().is_err());
        r.bone_mut(a).unwrap().length = 3.;
        r.width = 65535;
        assert!(r.validate().is_err());
        assert!(
            r.add_bone(Vec2::new(0., 0.), Vec2::new(1., 1.), Some(99))
                .is_err()
        );
    }

    #[test]
    fn loose_children_and_pose_offsets_use_parent_axes() {
        let mut r = Rig::new("Loose", 64, 64);
        let parent = r
            .add_bone(Vec2::new(10., 10.), Vec2::new(10., 20.), None)
            .unwrap();
        let child = r
            .add_bone(Vec2::new(12., 14.), Vec2::new(18., 14.), Some(parent))
            .unwrap();
        assert!(!r.bone(child).unwrap().connected);
        r.bones.reverse(); // File order need not be topological.
        assert!(r.validate().is_ok());
        let mut pose = Pose::new();
        pose.insert(
            parent,
            crate::animation::BonePose {
                rotation: -90.,
                x: 2.,
                scale: 2.,
                ..Default::default()
            },
        );
        let w = r.world(Some(&pose));
        let c = w.iter().find(|w| w.id == child).unwrap();
        close(c.head.x, 16.);
        close(c.head.y, 8.);
        close(c.tail.x, 16.);
        close(c.tail.y, 2.);
        let p = w.iter().find(|w| w.id == parent).unwrap();
        close((p.tail - p.head).length(), 20.);
    }

    #[test]
    fn rejects_bad_ids_bulges_tracks_and_duplicate_parts() {
        let mut r = Rig::new("Invalid", 32, 32);
        let part = r.add_part("Skin", [2, 3, 4, 255]);
        let bone = r
            .add_bone(Vec2::new(3., 3.), Vec2::new(12., 3.), None)
            .unwrap();
        r.bone_mut(bone).unwrap().part = part;
        assert!(r.validate().is_ok());
        let valid = r.clone();
        r.parts.push(r.parts[0].clone());
        assert!(r.validate().is_err());
        r = valid.clone();
        r.bone_mut(bone).unwrap().part = 999;
        assert!(r.validate().is_err());
        r = valid.clone();
        r.bone_mut(bone).unwrap().twin = Some(999);
        assert!(r.validate().is_err());
        r = valid.clone();
        r.bone_mut(bone).unwrap().parent = Some(999);
        assert!(r.validate().is_err());
        r = valid.clone();
        r.bone_mut(bone).unwrap().bulges_left =
            vec![Bulge { t: 0.8, d: 2. }, Bulge { t: 0.2, d: 3. }];
        assert!(r.validate().is_err());
        r = valid.clone();
        r.clips[0]
            .set_key(999, crate::animation::Channel::X, 1., 1.)
            .unwrap();
        assert!(r.validate().is_err());
        assert!(valid.to_json().is_ok());
    }

    #[test]
    fn invalid_edit_is_atomic_and_missing_delete_is_noop() {
        let mut r = Rig::new("Atomic", 32, 32);
        let id = r
            .add_bone(Vec2::new(1., 1.), Vec2::new(5., 1.), None)
            .unwrap();
        let original = r.clone();
        assert!(r.move_tail(id, Vec2::new(1., 1.)).is_err());
        assert!(r.move_tail(id, Vec2::new(f32::NAN, 0.)).is_err());
        assert!(r.move_tail(999, Vec2::new(3., 3.)).is_err());
        assert_eq!(r.delete_bone(999), 0);
        assert_eq!(original, r);
    }

    #[test]
    fn rejects_non_documents_and_excessive_raster_work() {
        assert!(Rig::from_json("{}").is_err());
        let mut rig = Rig::new("Too expensive", 4096, 4096);
        for i in 0..5 {
            rig.add_bone(Vec2::new(i as f32, 0.), Vec2::new(i as f32, 10.), None)
                .unwrap();
        }
        assert!(rig.validate().is_err());
    }
}
