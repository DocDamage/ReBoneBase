//! Embedded, bounded native 3D character data and CPU pixel rendering.
//! Imported files are normalized into owned Rust arrays; no native importer
//! pointers or external files are retained in project/history snapshots.
pub mod cleanup;
mod gltf;
mod native;
pub mod tracks;
use crate::model::Vec2;
use anyhow::{Result, bail, ensure};
use glam::{EulerRot, Mat4, Quat, Vec3, Vec4};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Component, Path},
    sync::Arc,
};
pub use tracks::{JointClipboard, JointKey, JointKeyRef, JointTracks};
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_VERTICES: usize = 100_000;
const MAX_TRIANGLES: usize = 100_000;
const MAX_WORK: u64 = 128 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform3d {
    pub translation: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: [f32; 3],
}
impl Default for Transform3d {
    fn default() -> Self {
        Self {
            translation: [0.; 3],
            rotation: [0., 0., 0., 1.],
            scale: [1.; 3],
        }
    }
}
impl Transform3d {
    fn matrix(&self) -> Mat4 {
        Mat4::from_scale_rotation_translation(
            Vec3::from_array(self.scale),
            Quat::from_array(self.rotation).normalize(),
            Vec3::from_array(self.translation),
        )
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node3d {
    pub name: String,
    pub parent: Option<usize>,
    pub transform: Transform3d,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Animation3d {
    pub name: String,
    pub duration: f32,
    pub times: Vec<f32>,
    pub frames: Vec<Vec<Transform3d>>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum AlphaMode {
    #[default]
    Opaque,
    Mask,
    Blend,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Wrap {
    #[default]
    Repeat,
    Clamp,
    Mirror,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Texture3d {
    pub width: u32,
    pub height: u32,
    #[serde(with = "bytes")]
    pub pixels: Vec<u8>,
    pub wrap_s: Wrap,
    pub wrap_t: Wrap,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Material3d {
    pub color: [f32; 4],
    pub texture: Option<usize>,
    pub alpha: AlphaMode,
    pub cutoff: f32,
}
impl Default for Material3d {
    fn default() -> Self {
        Self {
            color: [1.; 4],
            texture: None,
            alpha: AlphaMode::Opaque,
            cutoff: 0.5,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Skin3d {
    pub joints: Vec<usize>,
    pub inverse_bind: Vec<[f32; 16]>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Primitive3d {
    pub node: usize,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uv: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub joints: Vec<[u16; 4]>,
    pub weights: Vec<[f32; 4]>,
    pub triangles: Vec<[u32; 3]>,
    pub skin: Option<Skin3d>,
    pub material: Material3d,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Model3d {
    pub nodes: Vec<Node3d>,
    pub primitives: Vec<Primitive3d>,
    pub textures: Vec<Texture3d>,
    pub animations: Vec<Animation3d>,
    pub warnings: Vec<String>,
    pub origin: [f32; 3],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Character {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    pub position: Vec2,
    pub scale: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub z: i32,
    pub animation: Option<usize>,
    pub speed: f32,
    pub looped: bool,
    #[serde(default)]
    pub clips: BTreeMap<usize, Playback3d>,
    /// Whole-clip joint offsets in XYZ degrees.
    pub joints: BTreeMap<usize, [f32; 3]>,
    /// Keyed joint corrections per timeline clip.
    pub tracks: BTreeMap<usize, JointTracks>,
    #[serde(with = "shared")]
    pub model: Arc<Model3d>,
    /// Timeline clip and frame whose keyed corrections apply; set by `for_clip`.
    #[serde(skip)]
    pub sample: Option<(usize, f32)>,
}
pub type Character3d = Character;
/// Playback belongs to a project timeline; offsets preserve trimmed/split motion.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Playback3d {
    pub animation: Option<usize>,
    pub speed: f32,
    pub looped: bool,
    pub offset_ms: f64,
}
impl Default for Playback3d {
    fn default() -> Self {
        Self {
            animation: None,
            speed: 1.,
            looped: true,
            offset_ms: 0.,
        }
    }
}
impl Default for Character {
    fn default() -> Self {
        Self {
            id: 1,
            name: "Character".into(),
            enabled: true,
            position: Vec2::new(32., 56.),
            scale: 24.,
            yaw: 0.,
            pitch: 0.,
            z: 10,
            animation: None,
            speed: 1.,
            looped: true,
            clips: BTreeMap::new(),
            joints: BTreeMap::new(),
            tracks: BTreeMap::new(),
            model: Arc::new(Model3d::default()),
            sample: None,
        }
    }
}
impl Default for Model3d {
    fn default() -> Self {
        Self {
            nodes: vec![Node3d {
                name: "Cube".into(),
                parent: None,
                transform: Transform3d::default(),
            }],
            primitives: vec![Primitive3d {
                node: 0,
                positions: vec![
                    [-0.5, 0., -0.5],
                    [0.5, 0., -0.5],
                    [-0.5, 1., -0.5],
                    [0.5, 1., -0.5],
                    [-0.5, 0., 0.5],
                    [0.5, 0., 0.5],
                    [-0.5, 1., 0.5],
                    [0.5, 1., 0.5],
                ],
                normals: vec![],
                uv: vec![],
                colors: vec![],
                joints: vec![],
                weights: vec![],
                triangles: vec![
                    [0, 2, 1],
                    [1, 2, 3],
                    [4, 5, 6],
                    [5, 7, 6],
                    [0, 4, 2],
                    [4, 6, 2],
                    [1, 3, 5],
                    [5, 3, 7],
                    [0, 1, 4],
                    [1, 5, 4],
                    [2, 6, 3],
                    [3, 6, 7],
                ],
                skin: None,
                material: Material3d {
                    color: [0.7, 0.8, 0.9, 1.],
                    ..Default::default()
                },
            }],
            textures: vec![],
            animations: vec![],
            warnings: vec![],
            origin: [0.; 3],
        }
    }
}
fn finite(v: f32) -> bool {
    v.is_finite() && v.abs() <= 1_000_000.
}
fn valid_transform(t: &Transform3d) -> bool {
    let norm = Quat::from_array(t.rotation).length_squared();
    t.translation.iter().copied().all(finite)
        && t.rotation.iter().copied().all(finite)
        && norm.is_finite()
        && norm > 1e-12
        && t.scale.iter().all(|v| v.is_finite() && v.abs() <= 1000.)
}
impl Model3d {
    pub fn triangle_count(&self) -> usize {
        self.primitives.iter().map(|p| p.triangles.len()).sum()
    }
    pub fn vertex_count(&self) -> usize {
        self.primitives.iter().map(|p| p.positions.len()).sum()
    }
    pub fn byte_cost(&self) -> usize {
        self.textures.iter().map(|t| t.pixels.len()).sum::<usize>()
            + self
                .primitives
                .iter()
                .map(|p| {
                    p.positions.len() * 12
                        + p.normals.len() * 12
                        + p.uv.len() * 8
                        + p.colors.len() * 16
                        + p.joints.len() * 8
                        + p.weights.len() * 16
                        + p.triangles.len() * 12
                        + p.skin
                            .as_ref()
                            .map_or(0, |s| s.inverse_bind.len() * 64 + s.joints.len() * 8)
                })
                .sum::<usize>()
            + self
                .animations
                .iter()
                .map(|a| a.frames.len() * self.nodes.len() * 40 + a.times.len() * 4)
                .sum::<usize>()
            + self.nodes.iter().map(|n| n.name.len() + 64).sum::<usize>()
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.nodes.is_empty()
                && self.nodes.len() <= 4096
                && !self.primitives.is_empty()
                && self.primitives.len() <= 4096
                && self.vertex_count() <= MAX_VERTICES
                && self.triangle_count() > 0
                && self.triangle_count() <= MAX_TRIANGLES
                && self.textures.len() <= 64
                && self.animations.len() <= 32
                && self.byte_cost() <= MAX_BYTES,
            "3D model exceeds node, geometry or64MiB memory limits"
        );
        ensure!(
            self.origin.iter().copied().all(finite)
                && self.warnings.len() <= 256
                && self.warnings.iter().all(|s| s.len() <= 4096),
            "Invalid model origin/warnings"
        );
        for (i, n) in self.nodes.iter().enumerate() {
            ensure!(
                n.name.len() <= 1024
                    && n.parent.is_none_or(|p| p < self.nodes.len() && p != i)
                    && valid_transform(&n.transform),
                "Invalid node transform or parent"
            );
            let mut p = n.parent;
            let mut depth = 0;
            while let Some(id) = p {
                depth += 1;
                ensure!(depth <= 256 && id != i, "Cyclic or overly deep3D hierarchy");
                p = self
                    .nodes
                    .get(id)
                    .ok_or_else(|| anyhow::anyhow!("Missing parent"))?
                    .parent;
            }
        }
        let mut texels = 0u64;
        for t in &self.textures {
            texels += t.width as u64 * t.height as u64;
            ensure!(
                t.width > 0
                    && t.height > 0
                    && t.width <= 4096
                    && t.height <= 4096
                    && t.pixels.len() as u64 == t.width as u64 * t.height as u64 * 4,
                "Invalid3D texture"
            );
        }
        ensure!(texels <= 16 * 1024 * 1024, "3D textures exceed16megapixels");
        for p in &self.primitives {
            let n = p.positions.len();
            ensure!(
                p.node < self.nodes.len()
                    && n > 0
                    && p.positions.iter().flatten().copied().all(finite)
                    && [p.normals.len(), p.uv.len(), p.colors.len()]
                        .iter()
                        .all(|l| *l == 0 || *l == n)
                    && p.normals.iter().flatten().copied().all(finite)
                    && p.uv.iter().flatten().copied().all(finite)
                    && p.colors
                        .iter()
                        .flatten()
                        .all(|v| v.is_finite() && (0. ..=1.).contains(v))
                    && p.triangles.iter().flatten().all(|i| (*i as usize) < n),
                "Invalid mesh vertex/index data"
            );
            ensure!(
                p.material
                    .color
                    .iter()
                    .all(|v| v.is_finite() && (0. ..=1.).contains(v))
                    && p.material.cutoff.is_finite()
                    && (0. ..=1.).contains(&p.material.cutoff)
                    && p.material.texture.is_none_or(|i| i < self.textures.len()),
                "Invalid material"
            );
            if let Some(s) = &p.skin {
                ensure!(
                    !s.joints.is_empty()
                        && s.joints.len() <= 4096
                        && s.joints.len() == s.inverse_bind.len()
                        && s.joints.iter().all(|i| *i < self.nodes.len())
                        && s.inverse_bind.iter().flatten().copied().all(finite)
                        && p.joints.len() == n
                        && p.weights.len() == n,
                    "Invalid skin bind data"
                );
                for (j, w) in p.joints.iter().zip(&p.weights) {
                    let sum: f32 = w.iter().sum();
                    ensure!(
                        w.iter().all(|v| v.is_finite() && *v >= 0.)
                            && (sum < 1e-8 || (sum - 1.).abs() < 0.001)
                            && j.iter()
                                .zip(w)
                                .all(|(j, w)| *w <= 0. || (*j as usize) < s.joints.len()),
                        "Invalid skin weights"
                    );
                }
            } else {
                ensure!(
                    p.joints.is_empty() && p.weights.is_empty(),
                    "Unbound skin attributes"
                );
            }
        }
        let mut samples = 0usize;
        for a in &self.animations {
            ensure!(
                a.name.len() <= 1024
                    && a.duration.is_finite()
                    && (0. ..=14400.).contains(&a.duration)
                    && !a.times.is_empty()
                    && a.times.len() <= 240
                    && a.times.len() == a.frames.len(),
                "Invalid animation limits"
            );
            let mut prev = -1.;
            for (t, frame) in a.times.iter().zip(&a.frames) {
                ensure!(
                    t.is_finite()
                        && *t >= 0.
                        && *t > prev
                        && *t <= a.duration + 0.001
                        && frame.len() == self.nodes.len()
                        && frame.iter().all(valid_transform),
                    "Invalid animation frame"
                );
                prev = *t;
            }
            samples += a.frames.len() * self.nodes.len();
        }
        ensure!(
            samples <= 262144,
            "Animation exceeds262144 transform samples"
        );
        // Interpolating individually safe scale keys can overflow a deep
        // hierarchy between its sampled endpoints. Bound the product of each
        // node's maximum absolute scale across all authored poses instead.
        let mut scale_max = self
            .nodes
            .iter()
            .map(|n| {
                n.transform
                    .scale
                    .iter()
                    .map(|v| f64::from(v.abs()))
                    .fold(0., f64::max)
            })
            .collect::<Vec<_>>();
        for a in &self.animations {
            for frame in &a.frames {
                for (i, t) in frame.iter().enumerate() {
                    for s in t.scale {
                        scale_max[i] = scale_max[i].max(f64::from(s.abs()));
                    }
                }
            }
        }
        for i in 0..self.nodes.len() {
            let mut product = scale_max[i];
            let mut parent = self.nodes[i].parent;
            while let Some(p) = parent {
                product *= scale_max[p];
                ensure!(
                    product.is_finite() && product <= 1e12,
                    "3D hierarchy scale envelope exceeds safe composition limits"
                );
                parent = self.nodes[p].parent;
            }
        }
        for a in &self.animations {
            for frame in &a.frames {
                self.world(frame)?;
            }
        }
        let rest = self
            .nodes
            .iter()
            .map(|n| n.transform.clone())
            .collect::<Vec<_>>();
        ensure!(
            self.world(&rest)?.iter().all(|m| m.is_finite()),
            "Non-finite world transforms"
        );
        Ok(())
    }
    fn world(&self, local: &[Transform3d]) -> Result<Vec<Mat4>> {
        fn visit(
            i: usize,
            model: &Model3d,
            local: &[Transform3d],
            out: &mut [Mat4],
            state: &mut [u8],
        ) -> Result<()> {
            if state[i] == 2 {
                return Ok(());
            }
            ensure!(state[i] != 1, "Cyclic hierarchy");
            state[i] = 1;
            let parent = if let Some(p) = model.nodes[i].parent {
                ensure!(p < out.len(), "Missing parent");
                visit(p, model, local, out, state)?;
                out[p]
            } else {
                Mat4::IDENTITY
            };
            out[i] = parent * local[i].matrix();
            ensure!(out[i].is_finite(), "World transform overflow");
            state[i] = 2;
            Ok(())
        }
        let mut out = vec![Mat4::IDENTITY; self.nodes.len()];
        let mut state = vec![0; out.len()];
        for i in 0..out.len() {
            visit(i, self, local, &mut out, &mut state)?;
        }
        Ok(out)
    }
    fn recenter(&mut self) -> Result<()> {
        let local = self
            .nodes
            .iter()
            .map(|n| n.transform.clone())
            .collect::<Vec<_>>();
        let world = self.world(&local)?;
        let mut lo = Vec3::splat(f32::INFINITY);
        let mut hi = Vec3::splat(f32::NEG_INFINITY);
        for p in &self.primitives {
            for v in vertices(p, &world) {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        ensure!(
            lo.is_finite() && hi.is_finite(),
            "Model contains no finite geometry"
        );
        self.origin = [(lo.x + hi.x) * 0.5, lo.y, (lo.z + hi.z) * 0.5];
        Ok(())
    }
}
impl Character {
    pub fn embedded_bytes(&self) -> usize {
        self.model.byte_cost()
    }
    pub fn triangle_count(&self) -> usize {
        self.model.triangle_count()
    }
    pub fn render_work(&self, width: u32, height: u32) -> u64 {
        if !self.enabled {
            return 0;
        }
        u64::from(width)
            .saturating_mul(u64::from(height))
            .saturating_mul(self.model.triangle_count() as u64 + 1)
            .saturating_add(self.model.vertex_count() as u64 * 4)
    }
    pub fn validate(&self) -> Result<()> {
        self.model.validate()?;
        ensure!(
            self.clips.len() <= 32
                && self.clips.values().all(|b| b
                    .animation
                    .is_none_or(|i| i < self.model.animations.len())
                    && b.speed.is_finite()
                    && (0. ..=8.).contains(&b.speed)
                    && b.offset_ms.is_finite()
                    && (0. ..=1.0e12).contains(&b.offset_ms)),
            "Invalid 3D animation clip binding"
        );
        ensure!(
            self.id > 0
                && self.name.len() <= 1024
                && self.position.is_finite()
                && self.position.x.abs() <= 100000.
                && self.position.y.abs() <= 100000.
                && self.scale.is_finite()
                && (0.001..=10000.).contains(&self.scale)
                && finite(self.yaw)
                && finite(self.pitch)
                && self.speed.is_finite()
                && (0. ..=8.).contains(&self.speed)
                && self
                    .animation
                    .is_none_or(|i| i < self.model.animations.len())
                && self.joints.len() <= 4096
                && self
                    .joints
                    .iter()
                    .all(|(i, r)| *i < self.model.nodes.len() && r.iter().copied().all(finite)),
            "Invalid3D character pose/camera"
        );
        self.validate_tracks()
    }
    pub fn validate_render(&self, width: u32, height: u32) -> Result<()> {
        self.validate()?;
        ensure!(
            width > 0
                && height > 0
                && width <= 4096
                && height <= 4096
                && u64::from(width) * u64::from(height) <= 16 * 1024 * 1024
                && self.render_work(width, height) <= MAX_WORK,
            "3D canvas/model exceeds128-million-sample render budget; reduce canvas or mesh detail"
        );
        Ok(())
    }
    pub fn playback(&self, clip: usize) -> Playback3d {
        self.clips.get(&clip).cloned().unwrap_or(Playback3d {
            animation: self.animation,
            speed: self.speed,
            looped: self.looped,
            offset_ms: 0.,
        })
    }
    /// The character as seen in timeline `clip` at its evaluation `frame`, and
    /// the motion time for the clip's project time `time_ms`.
    pub fn for_clip(&self, clip: usize, frame: f32, time_ms: f64) -> (Self, f64) {
        let b = self.playback(clip);
        let mut c = self.clone();
        c.animation = b.animation;
        c.speed = 1.;
        c.looped = b.looped;
        c.sample = Some((clip, frame));
        (c, time_ms * f64::from(b.speed) + b.offset_ms)
    }
    fn matrices(&self, time_ms: f64) -> Result<Vec<Mat4>> {
        self.model.world(&self.local_pose(time_ms, true)?)
    }
    /// Model-space joint matrices for a local pose from [`Self::local_pose`].
    pub fn world_pose(&self, local: &[Transform3d]) -> Result<Vec<Mat4>> {
        ensure!(local.len() == self.model.nodes.len(), "Pose size mismatch");
        self.model.world(local)
    }
    /// Local joint transforms at a motion time: imported motion and whole-clip
    /// offsets, plus the keyed corrections of `sample` when `keyed` is set.
    pub fn local_pose(&self, time_ms: f64, keyed: bool) -> Result<Vec<Transform3d>> {
        ensure!(time_ms.is_finite(), "Invalid3D animation time");
        let mut local = self
            .model
            .nodes
            .iter()
            .map(|n| n.transform.clone())
            .collect::<Vec<_>>();
        if let Some(a) = self.animation.and_then(|i| self.model.animations.get(i)) {
            let seconds = (time_ms.max(0.) / 1000.) * f64::from(self.speed);
            let t = if self.looped && a.duration > 0. {
                seconds.rem_euclid(f64::from(a.duration)) as f32
            } else {
                seconds.min(f64::from(a.duration)) as f32
            };
            let end = a.times.partition_point(|v| *v <= t).min(a.times.len() - 1);
            let start = if a.times[end] > t {
                end.saturating_sub(1)
            } else {
                end
            };
            let u = if start == end {
                0.
            } else {
                ((t - a.times[start]) / (a.times[end] - a.times[start])).clamp(0., 1.)
            };
            for (i, target) in local.iter_mut().enumerate() {
                *target = interpolate(&a.frames[start][i], &a.frames[end][i], u);
            }
        }
        let keyed = self
            .sample
            .filter(|_| keyed)
            .map_or_else(Vec::new, |(clip, frame)| self.corrections(clip, frame));
        let offsets = self.joints.iter().map(|(i, e)| (*i, tracks::quat(*e)));
        // Imported motion, then the whole-clip offset, then the keyed correction.
        for (i, q) in offsets.chain(keyed) {
            let p = local
                .get_mut(i)
                .ok_or_else(|| anyhow::anyhow!("Unknown joint"))?;
            p.rotation = (Quat::from_array(p.rotation).normalize() * q)
                .normalize()
                .to_array();
        }
        Ok(local)
    }
    fn camera(&self, v: Vec3) -> Vec3 {
        let q = Quat::from_rotation_x(self.pitch.to_radians())
            * Quat::from_rotation_y(self.yaw.to_radians());
        q * (v - Vec3::from_array(self.model.origin))
    }
    fn screen(&self, v: Vec3) -> Vec3 {
        let v = self.camera(v);
        Vec3::new(
            self.position.x + v.x * self.scale,
            self.position.y - v.y * self.scale,
            v.z,
        )
    }
    pub fn joint_positions(&self, time_ms: f64) -> Result<Vec<Vec2>> {
        self.validate()?;
        Ok(self
            .matrices(time_ms)?
            .iter()
            .map(|m| {
                let v = self.screen(m.transform_point3(Vec3::ZERO));
                Vec2::new(v.x, v.y)
            })
            .collect())
    }
    pub fn bounds(&self, time_ms: f64) -> Option<(Vec2, Vec2)> {
        if !self.enabled || self.validate().is_err() {
            return None;
        }
        let world = self.matrices(time_ms).ok()?;
        let mut lo = Vec2::new(f32::INFINITY, f32::INFINITY);
        let mut hi = Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for p in &self.model.primitives {
            for v in vertices(p, &world) {
                let v = self.screen(v);
                if !v.is_finite() {
                    return None;
                }
                lo.x = lo.x.min(v.x);
                lo.y = lo.y.min(v.y);
                hi.x = hi.x.max(v.x);
                hi.y = hi.y.max(v.y);
            }
        }
        lo.is_finite().then_some((lo, hi))
    }
    pub fn render(&self, width: u32, height: u32, origin: Vec2, time_ms: f64) -> Result<RgbaImage> {
        self.validate_render(width, height)?;
        ensure!(origin.is_finite(), "Invalid canvas origin");
        let mut out = RgbaImage::new(width, height);
        if !self.enabled {
            return Ok(out);
        }
        let world = self.matrices(time_ms)?;
        let mut opaque = vec![];
        let mut blend = vec![];
        for (p_index, p) in self.model.primitives.iter().enumerate() {
            let positions = vertices(p, &world)
                .into_iter()
                .map(|v| {
                    let mut v = self.screen(v);
                    v.x -= origin.x;
                    v.y -= origin.y;
                    v
                })
                .collect::<Vec<_>>();
            ensure!(
                positions.iter().all(|v| v.is_finite()),
                "Projection overflow"
            );
            for t in &p.triangles {
                let tri = RenderTriangle {
                    positions: [
                        positions[t[0] as usize],
                        positions[t[1] as usize],
                        positions[t[2] as usize],
                    ],
                    indices: *t,
                    primitive: p_index,
                };
                if p.material.alpha == AlphaMode::Blend {
                    blend.push(tri);
                } else {
                    opaque.push(tri);
                }
            }
        }
        blend.sort_by(|a, b| a.depth().total_cmp(&b.depth()));
        let mut depth = vec![f32::NEG_INFINITY; width as usize * height as usize];
        for tri in opaque.iter().chain(&blend) {
            raster(&mut out, &mut depth, tri, &self.model);
        }
        Ok(out)
    }
}
fn interpolate(a: &Transform3d, b: &Transform3d, u: f32) -> Transform3d {
    Transform3d {
        translation: Vec3::from_array(a.translation)
            .lerp(Vec3::from_array(b.translation), u)
            .to_array(),
        scale: Vec3::from_array(a.scale)
            .lerp(Vec3::from_array(b.scale), u)
            .to_array(),
        rotation: Quat::from_array(a.rotation)
            .normalize()
            .slerp(Quat::from_array(b.rotation).normalize(), u)
            .normalize()
            .to_array(),
    }
}
fn vertices(p: &Primitive3d, world: &[Mat4]) -> Vec<Vec3> {
    p.positions
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let point = Vec3::from_array(*v);
            if let Some(s) = &p.skin {
                let w = p.weights[i];
                if w.iter().sum::<f32>() > 1e-8 {
                    let mut out = Vec3::ZERO;
                    for (j, weight) in p.joints[i].iter().zip(w) {
                        if weight > 0. {
                            out += (world[s.joints[*j as usize]]
                                * Mat4::from_cols_array(&s.inverse_bind[*j as usize]))
                            .transform_point3(point)
                                * weight;
                        }
                    }
                    return out;
                }
            }
            world[p.node].transform_point3(point)
        })
        .collect()
}
struct RenderTriangle {
    positions: [Vec3; 3],
    indices: [u32; 3],
    primitive: usize,
}
impl RenderTriangle {
    fn depth(&self) -> f32 {
        self.positions.iter().map(|p| p.z).sum::<f32>() / 3.
    }
}
fn raster(out: &mut RgbaImage, depth: &mut [f32], tri: &RenderTriangle, model: &Model3d) {
    let mut pos = tri.positions;
    let mut indices = tri.indices;
    let edge = |a: Vec3, b: Vec3, x: f64, y: f64| {
        (f64::from(b.x) - f64::from(a.x)) * (y - f64::from(a.y))
            - (f64::from(b.y) - f64::from(a.y)) * (x - f64::from(a.x))
    };
    let mut area = edge(pos[0], pos[1], f64::from(pos[2].x), f64::from(pos[2].y));
    if area.abs() < 1e-10 {
        return;
    }
    if area < 0. {
        pos.swap(1, 2);
        indices.swap(1, 2);
        area = -area;
    }
    let left = pos
        .iter()
        .map(|p| p.x)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .clamp(0., out.width() as f32) as u32;
    let right = pos
        .iter()
        .map(|p| p.x)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .clamp(0., out.width() as f32) as u32;
    let top = pos
        .iter()
        .map(|p| p.y)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .clamp(0., out.height() as f32) as u32;
    let bottom = pos
        .iter()
        .map(|p| p.y)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .clamp(0., out.height() as f32) as u32;
    let p = &model.primitives[tri.primitive];
    let top_left = |a: Vec3, b: Vec3| b.y < a.y || (b.y == a.y && b.x > a.x);
    for y in top..bottom {
        for x in left..right {
            let e = [
                edge(pos[1], pos[2], f64::from(x) + 0.5, f64::from(y) + 0.5),
                edge(pos[2], pos[0], f64::from(x) + 0.5, f64::from(y) + 0.5),
                edge(pos[0], pos[1], f64::from(x) + 0.5, f64::from(y) + 0.5),
            ];
            if !(0..3).all(|i| {
                e[i] > 1e-8 || (e[i].abs() <= 1e-8 && top_left(pos[(i + 1) % 3], pos[(i + 2) % 3]))
            }) {
                continue;
            }
            let w = e.map(|v| (v / area) as f32);
            let z = pos.iter().zip(w).map(|(v, w)| v.z * w).sum::<f32>();
            let offset = y as usize * out.width() as usize + x as usize;
            if z < depth[offset] {
                continue;
            }
            let mut color = Vec4::from_array(p.material.color);
            if !p.colors.is_empty() {
                let mut c = Vec4::ZERO;
                for (i, w) in indices.iter().zip(w) {
                    c += Vec4::from_array(p.colors[*i as usize]) * w;
                }
                color *= c;
            }
            if let Some(t) = p.material.texture {
                let mut uv = glam::Vec2::ZERO;
                for (i, w) in indices.iter().zip(w) {
                    uv +=
                        glam::Vec2::from_array(p.uv.get(*i as usize).copied().unwrap_or([0., 0.]))
                            * w;
                }
                color *= texture(&model.textures[t], uv);
            }
            match p.material.alpha {
                AlphaMode::Opaque => color.w = 1.,
                AlphaMode::Mask => {
                    if color.w < p.material.cutoff {
                        continue;
                    }
                    color.w = 1.;
                }
                AlphaMode::Blend => {
                    if color.w <= 0. {
                        continue;
                    }
                }
            }
            let rgba = color
                .clamp(Vec4::ZERO, Vec4::ONE)
                .to_array()
                .map(|v| (v * 255.).round() as u8);
            over(out.get_pixel_mut(x, y), rgba);
            if p.material.alpha != AlphaMode::Blend {
                depth[offset] = z;
            }
        }
    }
}
fn texture(t: &Texture3d, uv: glam::Vec2) -> Vec4 {
    let wrap = |v: f32, mode: Wrap| match mode {
        Wrap::Repeat => v.rem_euclid(1.),
        Wrap::Clamp => v.clamp(0., 1.),
        Wrap::Mirror => {
            let v = v.rem_euclid(2.);
            if v > 1. { 2. - v } else { v }
        }
    };
    let x = (wrap(uv.x, t.wrap_s) * t.width as f32)
        .floor()
        .min((t.width - 1) as f32) as usize;
    let y = (wrap(uv.y, t.wrap_t) * t.height as f32)
        .floor()
        .min((t.height - 1) as f32) as usize;
    let i = (y * t.width as usize + x) * 4;
    Vec4::new(
        t.pixels[i] as f32,
        t.pixels[i + 1] as f32,
        t.pixels[i + 2] as f32,
        t.pixels[i + 3] as f32,
    ) / 255.
}
fn over(dst: &mut Rgba<u8>, src: [u8; 4]) {
    let a = src[3] as f32 / 255.;
    let b = dst[3] as f32 / 255.;
    let alpha = a + b * (1. - a);
    if alpha <= 0. {
        return;
    }
    for i in 0..3 {
        dst[i] = ((src[i] as f32 * a + dst[i] as f32 * b * (1. - a)) / alpha).round() as u8;
    }
    dst[3] = (alpha * 255.).round() as u8;
}
pub fn load(path: &Path) -> Result<Character> {
    ensure!(
        std::fs::metadata(path)?.len() <= MAX_BYTES as u64,
        "3D file exceeds64MiB"
    );
    let data = std::fs::read(path)?;
    let mut c = match path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "glb" | "gltf" => from_gltf(&data, path.parent())?,
        "obj" => from_obj(&data, path.parent())?,
        "fbx" => from_fbx(&data, path.parent())?,
        _ => bail!("Choose GLB,glTF,FBX or OBJ"),
    };
    c.name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .take(256)
        .collect();
    Ok(c)
}
pub fn from_gltf(data: &[u8], base: Option<&Path>) -> Result<Character> {
    finish(gltf::parse(data, base)?)
}
pub fn from_obj(data: &[u8], base: Option<&Path>) -> Result<Character> {
    finish(native::parse(data, base, true)?)
}
pub fn from_fbx(data: &[u8], base: Option<&Path>) -> Result<Character> {
    finish(native::parse(data, base, false)?)
}
fn finish(mut model: Model3d) -> Result<Character> {
    model.validate()?;
    model.recenter()?;
    model.validate()?;
    let animated = !model.animations.is_empty();
    let mut c = Character {
        animation: animated.then_some(0),
        model: Arc::new(model),
        ..Default::default()
    };
    if let Some((a, b)) = c.bounds(0.) {
        let size = (b.x - a.x).max(b.y - a.y) / c.scale;
        if size > 0.00001 {
            c.scale = (48. / size).clamp(0.001, 10000.);
        }
    }
    c.validate()?;
    Ok(c)
}
fn external(base: Option<&Path>, uri: &str) -> Result<Vec<u8>> {
    let uri = uri.replace('\\', "/");
    ensure!(
        !uri.contains(':') && !uri.starts_with('/') && !uri.contains('\0'),
        "External3D assets must be relative local paths"
    );
    let p = Path::new(&uri);
    ensure!(
        p.components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir)),
        "External asset escapes selected directory"
    );
    let root = base
        .ok_or_else(|| anyhow::anyhow!("External asset needs its model directory"))?
        .canonicalize()?;
    let path = root.join(p).canonicalize()?;
    ensure!(
        path.starts_with(&root) && std::fs::metadata(&path)?.len() <= MAX_BYTES as u64,
        "External3D asset escapes directory or exceeds64MiB"
    );
    Ok(std::fs::read(path)?)
}
fn image_texture(data: &[u8]) -> Result<Texture3d> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format()?;
    let (w, h) = reader.into_dimensions()?;
    ensure!(
        w > 0 && h > 0 && w <= 4096 && h <= 4096 && u64::from(w) * u64::from(h) <= 16 * 1024 * 1024,
        "Texture exceeds4096/16MP limits"
    );
    reader = image::ImageReader::new(std::io::Cursor::new(data)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_BYTES as u64);
    reader.limits(limits);
    let pixels = reader.decode()?.into_rgba8().into_raw();
    Ok(Texture3d {
        width: w,
        height: h,
        pixels,
        wrap_s: Wrap::Repeat,
        wrap_t: Wrap::Repeat,
    })
}
mod shared {
    use super::*;
    pub fn serialize<S: serde::Serializer>(
        m: &Arc<Model3d>,
        s: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        m.as_ref().serialize(s)
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> std::result::Result<Arc<Model3d>, D::Error> {
        Model3d::deserialize(d).map(Arc::new)
    }
}
mod bytes {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    pub fn serialize<S: serde::Serializer>(v: &[u8], s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(v))
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> std::result::Result<Vec<u8>, D::Error> {
        use serde::de::Error;
        let text = String::deserialize(d)?;
        if text.len() > 90 * 1024 * 1024 {
            return Err(D::Error::custom("3D texture base64 exceeds budget"));
        }
        STANDARD.decode(text).map_err(D::Error::custom)
    }
}
