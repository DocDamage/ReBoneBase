//! Bounded skeleton motion import and rest-relative 2D / 3D retargeting.
mod fbx;
mod retarget3d;
mod unity;
use crate::{
    animation::{Channel, Clip, Ease},
    model::Rig,
};
use anyhow::{Result, bail, ensure};
use base64::Engine as _;
use glam::{Mat4, Quat, Vec3, Vec4};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Projection {
    #[default]
    Front,
    Side,
    ThreeQuarter,
}
impl Projection {
    fn project(self, p: Vec3) -> crate::model::Vec2 {
        crate::model::Vec2::new(
            match self {
                Self::Front => p.x,
                Self::Side => p.z,
                Self::ThreeQuarter => (p.x + p.z) * std::f32::consts::FRAC_1_SQRT_2,
            },
            -p.y,
        )
    }
}
#[derive(Clone)]
struct Node {
    name: String,
    parent: Option<usize>,
    children: Vec<usize>,
}
#[derive(Clone)]
pub struct Motion {
    nodes: Vec<Node>,
    frames: Vec<Vec<Mat4>>,
    rest: Vec<Mat4>,
    fps: f32,
    pub name: String,
    pub warnings: Vec<String>,
}
pub struct RetargetReport {
    pub mapped: usize,
    pub total: usize,
    pub warnings: Vec<String>,
}
/// Name-based mappings survive glTF node reordering between source revisions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetargetProfile {
    pub name: String,
    pub projection: Projection,
    pub scale: f32,
    pub mapping: BTreeMap<String, String>,
}
impl RetargetProfile {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.name.len() <= 1024
                && self.scale.is_finite()
                && (0.001..=1000.).contains(&self.scale)
                && self.mapping.len() <= 2048,
            "Invalid retarget profile metadata"
        );
        ensure!(
            self.mapping
                .iter()
                .all(|(target, source)| !target.is_empty()
                    && !source.is_empty()
                    && target.len() <= 1024
                    && source.len() <= 1024),
            "Invalid retarget profile names"
        );
        Ok(())
    }
    pub fn from_mapping(
        rig: &Rig,
        motion: &Motion,
        name: impl Into<String>,
        projection: Projection,
        scale: f32,
        mapping: &[Option<usize>],
    ) -> Result<Self> {
        rig.validate().map_err(anyhow::Error::msg)?;
        ensure!(
            mapping.len() == rig.bones.len()
                && mapping
                    .iter()
                    .flatten()
                    .all(|index| *index < motion.nodes.len()),
            "Invalid source joint mapping"
        );
        let mut names = BTreeMap::new();
        let mut targets = HashSet::new();
        for (bone, index) in rig.bones.iter().zip(mapping) {
            if let Some(index) = index {
                ensure!(
                    rig.bones
                        .iter()
                        .filter(|other| other.name == bone.name)
                        .count()
                        == 1,
                    "Duplicate target bone names cannot be saved in a name-based profile"
                );
                ensure!(
                    targets.insert(&bone.name),
                    "Duplicate target bone names cannot be saved in a name-based profile"
                );
                let source = &motion.nodes[*index].name;
                ensure!(
                    motion.nodes.iter().filter(|n| n.name == *source).count() == 1,
                    "Duplicate source joint names cannot be saved in a name-based profile"
                );
                names.insert(bone.name.clone(), source.clone());
            }
        }
        ensure!(
            !names.is_empty(),
            "Map at least one bone before saving a retarget profile"
        );
        let profile = Self {
            name: name.into(),
            projection,
            scale,
            mapping: names,
        };
        profile.validate()?;
        Ok(profile)
    }
    pub fn resolve(&self, rig: &Rig, motion: &Motion) -> Vec<Option<usize>> {
        rig.bones
            .iter()
            .map(|bone| {
                if rig
                    .bones
                    .iter()
                    .filter(|other| other.name == bone.name)
                    .count()
                    != 1
                {
                    return None;
                }
                let name = self.mapping.get(&bone.name)?;
                let mut found = motion
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, n)| n.name == *name);
                let first = found.next()?.0;
                if found.next().is_none() {
                    Some(first)
                } else {
                    None
                }
            })
            .collect()
    }
}
impl Motion {
    pub fn animation_names(path: &Path) -> Result<Vec<String>> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if extension == "uasset" {
            bail!(
                "Unreal .uasset animation binaries cannot be imported directly. In Unreal Engine, export the Animation Sequence as FBX, then import that FBX."
            );
        }
        ensure!(
            std::fs::metadata(path)?.len() <= 32 * 1024 * 1024,
            "Motion file exceeds 32 MiB"
        );
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("bvh"))
        {
            return Ok(vec![
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            ]);
        }
        let bytes = std::fs::read(path)?;
        if extension == "fbx" {
            return fbx::names(&bytes);
        }
        if extension == "anim" {
            return Ok(vec![
                Self::from_unity_anim(
                    std::str::from_utf8(&bytes).map_err(|_| anyhow::anyhow!(unity::GUIDANCE))?,
                )?
                .name,
            ]);
        }
        let gltf = gltf::Gltf::from_slice(&bytes)?;
        ensure!(
            gltf.animations().len() <= 256,
            "glTF exceeds 256 animations"
        );
        let mut names: Vec<_> = gltf
            .animations()
            .enumerate()
            .map(|(i, a)| {
                a.name()
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Animation {}", i + 1))
            })
            .collect();
        if names.is_empty() {
            names.push("Static pose".into());
        }
        Ok(names)
    }
    pub fn load_animation(path: &Path, index: usize) -> Result<Self> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if extension == "uasset" {
            bail!(
                "Unreal .uasset animation binaries cannot be imported directly. In Unreal Engine, export the Animation Sequence as FBX, then import that FBX."
            );
        }
        ensure!(
            std::fs::metadata(path)?.len() <= 32 * 1024 * 1024,
            "Motion file exceeds 32 MiB"
        );
        let bytes = std::fs::read(path)?;
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("bvh"))
        {
            ensure!(index == 0, "BVH contains a single animation");
            let mut motion = Self::from_bvh(std::str::from_utf8(&bytes)?)?;
            motion.name = stem.into_owned();
            Ok(motion)
        } else if extension == "fbx" {
            Self::from_fbx(&bytes, index)
        } else if extension == "anim" {
            ensure!(index == 0, "Unity .anim contains a single AnimationClip");
            Self::from_unity_anim(
                std::str::from_utf8(&bytes).map_err(|_| anyhow::anyhow!(unity::GUIDANCE))?,
            )
        } else {
            let mut motion = Self::from_gltf_animation(&bytes, path.parent(), index)?;
            motion.name = format!("{stem} · {}", motion.name);
            Ok(motion)
        }
    }
    pub fn preview_rig(
        &self,
        rig: &Rig,
        projection: Projection,
        scale: f32,
        mapping: &[Option<usize>],
        frame_index: usize,
    ) -> Result<image::RgbaImage> {
        ensure!(
            frame_index < self.frames.len(),
            "Preview frame is outside the motion range"
        );
        let mut candidate = rig.clone();
        self.retarget_mapped(&mut candidate, projection, scale, mapping)?;
        let pose = candidate.active_clip().sample((frame_index + 1) as f32);
        Ok(crate::skin::render(&candidate, Some(&pose)))
    }
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
    pub fn duration(&self) -> f32 {
        (self.frames.len().saturating_sub(1)) as f32 / self.fps
    }
    pub fn fps(&self) -> f32 {
        self.fps
    }
    pub fn from_fbx(bytes: &[u8], index: usize) -> Result<Self> {
        fbx::parse(bytes, index)
    }
    pub fn from_unity_anim(text: &str) -> Result<Self> {
        unity::parse(text)
    }
    pub fn preview(
        &self,
        projection: Projection,
        frame: usize,
    ) -> Vec<(crate::model::Vec2, crate::model::Vec2)> {
        let f = &self.frames[frame.min(self.frames.len() - 1)];
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                n.parent.map(|p| {
                    (
                        projection.project(f[p].w_axis.truncate()),
                        projection.project(f[i].w_axis.truncate()),
                    )
                })
            })
            .collect()
    }
    pub fn load(path: &Path) -> Result<Self> {
        Self::load_animation(path, 0)
    }
    pub fn from_bvh(text: &str) -> Result<Self> {
        ensure!(text.len() <= 32 * 1024 * 1024, "BVH exceeds 32 MiB");
        let spaced = text.replace('{', " { ").replace('}', " } ");
        let tokens: Vec<_> = spaced.split_whitespace().collect();
        let mut p = Bvh {
            tokens,
            at: 0,
            nodes: vec![],
            offsets: vec![],
            channels: vec![],
        };
        p.expect("HIERARCHY")?;
        p.expect("ROOT")?;
        let name = p.next()?.to_string();
        p.joint(name, None, 0, false)?;
        p.expect("MOTION")?;
        p.expect("Frames:")?;
        let count = p.integer()?;
        p.expect("Frame")?;
        p.expect("Time:")?;
        let dt = p.number()?;
        ensure!(
            count > 0 && count <= 100_000 && (1. / 240. ..=60.).contains(&dt),
            "Invalid BVH frame count or frame time"
        );
        let channel_count: usize = p.channels.iter().map(Vec::len).sum();
        ensure!(
            channel_count > 0 && channel_count * count <= 8_000_000,
            "Invalid BVH sample budget"
        );
        let n = count.min(240);
        let mut frames = Vec::with_capacity(n);
        let mut next_index = 0;
        for frame in 0..count {
            let selected = frame
                == if n == 1 {
                    0
                } else {
                    next_index * (count - 1) / (n - 1)
                };
            if !selected {
                for _ in 0..channel_count {
                    p.number()?;
                }
                continue;
            }
            let mut local = Vec::with_capacity(p.nodes.len());
            for (i, channels) in p.channels.clone().iter().enumerate() {
                let mut matrix = Mat4::from_translation(p.offsets[i]);
                for channel in channels {
                    let v = p.number()?;
                    let m = match channel.as_str() {
                        "Xposition" => Mat4::from_translation(Vec3::X * v),
                        "Yposition" => Mat4::from_translation(Vec3::Y * v),
                        "Zposition" => Mat4::from_translation(Vec3::Z * v),
                        "Xrotation" => Mat4::from_rotation_x(v.to_radians()),
                        "Yrotation" => Mat4::from_rotation_y(v.to_radians()),
                        "Zrotation" => Mat4::from_rotation_z(v.to_radians()),
                        _ => bail!("Unknown BVH channel"),
                    };
                    matrix *= m;
                }
                local.push(matrix);
            }
            if selected && next_index < n {
                frames.push(world_matrices(&p.nodes, &local)?);
                next_index += 1;
            }
        }
        ensure!(p.at == p.tokens.len(), "Trailing BVH sample data");
        let fps = if count <= 1 {
            1. / dt
        } else {
            (n - 1) as f32 / (dt * (count - 1) as f32)
        };
        ensure!(
            (1. / 60. ..=240.).contains(&fps),
            "Motion duration exceeds supported range"
        );
        let warnings = if count > 240 {
            vec![format!("Resampled {count} source frames to 240 frames")]
        } else {
            vec![]
        };
        Ok(Self {
            rest: world_matrices(
                &p.nodes,
                &p.offsets
                    .iter()
                    .map(|v| Mat4::from_translation(*v))
                    .collect::<Vec<_>>(),
            )?,
            nodes: p.nodes,
            frames,
            fps,
            name: "BVH motion".into(),
            warnings,
        })
    }
    pub fn from_gltf(bytes: &[u8], base: Option<&Path>) -> Result<Self> {
        Self::from_gltf_animation(bytes, base, 0)
    }
    pub fn from_gltf_animation(bytes: &[u8], base: Option<&Path>, index: usize) -> Result<Self> {
        ensure!(bytes.len() <= 32 * 1024 * 1024, "glTF exceeds 32 MiB");
        let gltf = gltf::Gltf::from_slice(bytes)?;
        ensure!(gltf.buffers().len() <= 256, "glTF exceeds256motion buffers");
        let animation_count = gltf.animations().len();
        ensure!(animation_count <= 256, "glTF exceeds 256 animations");
        ensure!(
            if animation_count == 0 {
                index == 0
            } else {
                index < animation_count
            },
            "Animation index is outside the glTF animation range"
        );
        let animation_name = gltf
            .animations()
            .nth(index)
            .map(|a| {
                a.name()
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Animation {}", index + 1))
            })
            .unwrap_or_else(|| "Static pose".into());
        let mut buffers = Vec::new();
        let mut size = 0;
        for b in gltf.buffers() {
            let data = match b.source() {
                gltf::buffer::Source::Bin => gltf
                    .blob
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("Missing GLB binary buffer"))?,
                gltf::buffer::Source::Uri(uri) => {
                    if uri.starts_with("data:") {
                        let (header, payload) = uri
                            .split_once(',')
                            .ok_or_else(|| anyhow::anyhow!("Invalid data URI"))?;
                        ensure!(
                            header.ends_with(";base64"),
                            "Only base64 buffer data URIs are supported"
                        );
                        base64::engine::general_purpose::STANDARD.decode(payload)?
                    } else {
                        let mut decoded = Vec::with_capacity(uri.len());
                        let mut iter = uri.bytes();
                        while let Some(b) = iter.next() {
                            if b == b'%' {
                                let h = iter.next().and_then(|b| (b as char).to_digit(16));
                                let l = iter.next().and_then(|b| (b as char).to_digit(16));
                                let (Some(h), Some(l)) = (h, l) else {
                                    bail!("Invalid percent-encoded motion buffer URI");
                                };
                                decoded.push((h * 16 + l) as u8);
                            } else {
                                decoded.push(b);
                            }
                        }
                        let uri = std::str::from_utf8(&decoded)?;
                        let relative = Path::new(uri);
                        ensure!(
                            !relative.is_absolute()
                                && !uri.contains(':')
                                && !uri.contains('\0')
                                && !uri.contains('\\')
                                && relative
                                    .components()
                                    .all(|c| matches!(c, std::path::Component::Normal(_))),
                            "External buffer must be a relative file inside the motion directory"
                        );
                        let root = base
                            .ok_or_else(|| {
                                anyhow::anyhow!("External buffer needs a motion directory")
                            })?
                            .canonicalize()?;
                        let path = root.join(relative).canonicalize()?;
                        ensure!(
                            path.starts_with(&root),
                            "External buffer escapes motion directory"
                        );
                        ensure!(
                            std::fs::metadata(&path)?.len() <= 32 * 1024 * 1024,
                            "External buffer exceeds 32 MiB"
                        );
                        std::fs::read(path)?
                    }
                }
            };
            size += data.len();
            ensure!(
                size <= 64 * 1024 * 1024 && data.len() >= b.length(),
                "Invalid glTF buffer length or budget"
            );
            buffers.push(data);
        }
        let count = gltf.nodes().len();
        ensure!(
            count > 0 && count <= 512,
            "glTF skeleton must contain 1–512 nodes"
        );
        let mut nodes: Vec<_> = gltf
            .nodes()
            .map(|n| Node {
                name: n.name().unwrap_or("Joint").into(),
                parent: None,
                children: n.children().map(|n| n.index()).collect(),
            })
            .collect();
        for n in gltf.nodes() {
            for child in n.children() {
                ensure!(
                    nodes[child.index()].parent.is_none(),
                    "glTF node has multiple parents"
                );
                nodes[child.index()].parent = Some(n.index());
            }
        }
        let base_trs: Vec<_> = gltf
            .nodes()
            .map(|n| {
                let (t, r, s) = n.transform().decomposed();
                (Vec3::from(t), Quat::from_array(r), Vec3::from(s))
            })
            .collect();
        ensure!(
            base_trs.iter().all(|(t, r, s)| t.is_finite()
                && r.is_finite()
                && r.length_squared().is_finite()
                && r.length_squared() > 1e-12
                && s.is_finite()),
            "Invalid node transforms"
        );
        let mut curves = Vec::new();
        let mut warnings = Vec::new();
        let mut sample_budget = 0usize;
        if animation_count > 1 {
            warnings.push(if index==0{"Imported the first glTF animation; additional animations were not selected".into()}else{format!("Imported animation {} of {animation_count}; additional animations were not selected",index+1)});
        }
        if let Some(anim) = gltf.animations().nth(index) {
            for channel in anim.channels() {
                ensure!(curves.len() < 1024, "Animation exceeds 1024 channels");
                let input_count = channel.sampler().input().count();
                let output_count = channel.sampler().output().count();
                ensure!(
                    input_count <= 100_000 && output_count <= 300_000,
                    "Animation accessor exceeds sample budget"
                );
                sample_budget = sample_budget
                    .checked_add(input_count + output_count)
                    .ok_or_else(|| anyhow::anyhow!("Animation sample budget overflow"))?;
                ensure!(
                    sample_budget <= 2_000_000,
                    "Animation exceeds aggregate sample budget"
                );
                let reader = channel.reader(|b| buffers.get(b.index()).map(Vec::as_slice));
                let times: Vec<_> = reader
                    .read_inputs()
                    .ok_or_else(|| anyhow::anyhow!("Missing animation input"))?
                    .collect();
                ensure!(
                    !times.is_empty()
                        && times.len() <= 100_000
                        && times.iter().all(|t| t.is_finite() && *t >= 0.)
                        && times.windows(2).all(|p| p[1] > p[0]),
                    "Invalid animation sample times"
                );
                use gltf::animation::util::ReadOutputs;
                let (kind, values): (u8, Vec<Vec4>) = match reader
                    .read_outputs()
                    .ok_or_else(|| anyhow::anyhow!("Missing animation output"))?
                {
                    ReadOutputs::Translations(v) => {
                        (0, v.map(|v| Vec3::from(v).extend(0.)).collect())
                    }
                    ReadOutputs::Rotations(v) => (1, v.into_f32().map(Vec4::from_array).collect()),
                    ReadOutputs::Scales(v) => (2, v.map(|v| Vec3::from(v).extend(0.)).collect()),
                    ReadOutputs::MorphTargetWeights(_) => {
                        warnings.push(
                            "Morph target animation is not skeletal motion and was skipped".into(),
                        );
                        continue;
                    }
                };
                let interpolation = channel.sampler().interpolation();
                let factor = if interpolation == gltf::animation::Interpolation::CubicSpline {
                    3
                } else {
                    1
                };
                ensure!(
                    values.len() == times.len() * factor && values.iter().all(|v| v.is_finite()),
                    "Invalid animation output count or values"
                );
                if kind == 1 {
                    ensure!(
                        values
                            .iter()
                            .enumerate()
                            .all(|(i, v)| factor == 3 && i % 3 != 1
                                || v.length_squared().is_finite() && v.length_squared() > 1e-12),
                        "Zero rotation quaternion"
                    );
                }
                curves.push(Curve {
                    node: channel.target().node().index(),
                    kind,
                    times,
                    values,
                    interpolation,
                });
            }
        }
        let duration = curves
            .iter()
            .filter_map(|c| c.times.last().copied())
            .fold(0., f32::max);
        ensure!(duration <= 14_400., "Animation is too long");
        let n = if duration > 0. {
            ((duration * 30.).round() as usize + 1).clamp(2, 240)
        } else {
            1
        };
        let fps = if duration > 0. {
            (n - 1) as f32 / duration
        } else {
            30.
        };
        let mut frames = Vec::with_capacity(n);
        for i in 0..n {
            let t = if n > 1 {
                i as f32 * duration / (n - 1) as f32
            } else {
                0.
            };
            let mut trs = base_trs.clone();
            for c in &curves {
                let v = c.sample(t);
                match c.kind {
                    0 => trs[c.node].0 = v.truncate(),
                    1 => trs[c.node].1 = Quat::from_xyzw(v.x, v.y, v.z, v.w).normalize(),
                    _ => trs[c.node].2 = v.truncate(),
                }
            }
            let local: Vec<_> = trs
                .into_iter()
                .map(|(t, r, s)| Mat4::from_scale_rotation_translation(s, r, t))
                .collect();
            frames.push(world_matrices(&nodes, &local)?);
        }
        Ok(Self {
            rest: world_matrices(
                &nodes,
                &base_trs
                    .iter()
                    .map(|(t, r, s)| Mat4::from_scale_rotation_translation(*s, *r, *t))
                    .collect::<Vec<_>>(),
            )?,
            nodes,
            frames,
            fps,
            name: animation_name,
            warnings,
        })
    }
    pub fn suggested_scale(&self, rig: &Rig) -> f32 {
        let frame = &self.frames[0];
        let lo = frame
            .iter()
            .map(|m| m.w_axis.y)
            .fold(f32::INFINITY, f32::min);
        let hi = frame
            .iter()
            .map(|m| m.w_axis.y)
            .fold(f32::NEG_INFINITY, f32::max);
        (rig.height as f32 * 0.8 / (hi - lo).max(0.01)).clamp(0.001, 1000.)
    }
    pub fn retarget(
        &self,
        rig: &mut Rig,
        projection: Projection,
        scale: f32,
    ) -> Result<RetargetReport> {
        let mapping = self.mapping(rig);
        self.retarget_mapped(rig, projection, scale, &mapping)
    }
    pub fn joint_names(&self) -> Vec<String> {
        self.nodes.iter().map(|n| n.name.clone()).collect()
    }
    pub fn mapping(&self, rig: &Rig) -> Vec<Option<usize>> {
        rig.bones
            .iter()
            .map(|b| {
                let key = canonical(&b.name);
                self.nodes.iter().position(|n| canonical(&n.name) == key)
            })
            .collect()
    }
    pub fn retarget_mapped(
        &self,
        rig: &mut Rig,
        projection: Projection,
        scale: f32,
        mapping: &[Option<usize>],
    ) -> Result<RetargetReport> {
        ensure!(
            scale.is_finite() && (0.001..=1000.).contains(&scale),
            "Motion scale is outside limits"
        );
        rig.validate().map_err(anyhow::Error::msg)?;
        ensure!(
            mapping.len() == rig.bones.len()
                && mapping.iter().flatten().all(|n| *n < self.nodes.len()),
            "Invalid source joint mapping"
        );
        let mapped = mapping.iter().filter(|n| n.is_some()).count();
        ensure!(
            mapped > 0,
            "No matching joints; use standard humanoid bone names or rename your bones to match the source skeleton"
        );
        let mut clip = Clip {
            name: self.name.clone(),
            fps: self.fps.clamp(1. / 60., 240.),
            length: self.frames.len() as u32,
            looped: true,
            tracks: vec![],
            ..Default::default()
        };
        for (f, frame) in self.frames.iter().enumerate() {
            let mut angles: std::collections::HashMap<u32, f32> = std::collections::HashMap::new();
            for _ in 0..rig.bones.len() {
                for (i, b) in rig.bones.iter().enumerate() {
                    if angles.contains_key(&b.id)
                        || b.parent.is_some_and(|p| !angles.contains_key(&p))
                    {
                        continue;
                    }
                    let parent = b.parent.and_then(|p| angles.get(&p)).copied().unwrap_or(0.);
                    let mut angle = parent + b.angle;
                    if let Some(node) = mapping[i] {
                        let n = &self.nodes[node];
                        let mut children = n.children.clone();
                        if canonical(&b.name) == "hips" {
                            children.sort_by_key(|i| {
                                !matches!(
                                    canonical(&self.nodes[*i].name).as_str(),
                                    "chest" | "spine"
                                )
                            });
                        }
                        let head = frame[node].transform_point3(Vec3::ZERO);
                        let tail = children.first().map_or_else(
                            || frame[node].transform_point3(Vec3::Y),
                            |c| frame[*c].transform_point3(Vec3::ZERO),
                        );
                        let mut delta = projection.project(tail - head);
                        // Unity rotation-only clips do not carry bind offsets,
                        // so a child position may coincide with this joint.
                        if delta.length() <= 0.00001 {
                            delta = projection.project(frame[node].transform_vector3(Vec3::Y));
                        }
                        if delta.length() > 0.00001 {
                            angle = delta.angle();
                        }
                        clip.set_key(
                            b.id,
                            Channel::Rotation,
                            (f + 1) as f32,
                            crate::animation::wrap_degrees(angle - parent - b.angle),
                        )
                        .map_err(anyhow::Error::msg)?;
                        if b.parent.is_none() {
                            let first = self.frames[0][node].transform_point3(Vec3::ZERO);
                            let offset = projection.project(head - first);
                            clip.set_key(b.id, Channel::X, (f + 1) as f32, offset.x * scale)
                                .map_err(anyhow::Error::msg)?;
                            clip.set_key(b.id, Channel::Y, (f + 1) as f32, offset.y * scale)
                                .map_err(anyhow::Error::msg)?;
                        }
                    }
                    angles.insert(b.id, angle);
                }
                if angles.len() == rig.bones.len() {
                    break;
                }
            }
        }
        for t in &mut clip.tracks {
            for k in &mut t.keys {
                k.easing = Ease::Linear;
            }
        }
        let mut candidate = rig.clone();
        ensure!(candidate.clips.len() < 32, "Project already has 32 clips");
        candidate.clips.push(clip);
        candidate.active_clip = candidate.clips.len() - 1;
        candidate.validate().map_err(anyhow::Error::msg)?;
        *rig = candidate;
        let mut warnings = self.warnings.clone();
        if mapped < rig.bones.len() {
            warnings.push(format!(
                "{} bones had no matching source joint",
                rig.bones.len() - mapped
            ));
        }
        Ok(RetargetReport {
            mapped,
            total: rig.bones.len(),
            warnings,
        })
    }
}
fn canonical(name: &str) -> String {
    let key = name
        .rsplit(':')
        .next()
        .unwrap_or(name)
        .to_lowercase()
        .replace("mixamorig", "");
    let compact: String = key.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let side = if compact.contains("left") || key.ends_with("_l") || key.starts_with("l_") {
        "l"
    } else if compact.contains("right") || key.ends_with("_r") || key.starts_with("r_") {
        "r"
    } else {
        ""
    };
    let stem = compact.replace("left", "").replace("right", "");
    let root = if !side.is_empty() && (key.ends_with("_l") || key.ends_with("_r")) {
        &stem[..stem.len() - 1]
    } else if key.starts_with("l_") || key.starts_with("r_") {
        &stem[1..]
    } else {
        &stem
    };
    let role = match root {
        "pelvis" | "hip" | "hips" => "hips",
        "spine03" | "spine04" | "spine05" | "upperchest" => "upperchest",
        "spine1" | "spine2" | "spine02" | "chest" => "chest",
        "spine" | "spine01" | "abdomen" => "spine",
        "shoulder" | "clavicle" | "collar" => "clavicle",
        "upperarm" | "arm" => "upperarm",
        "lowerarm" | "forearm" => "forearm",
        "upleg" | "upperleg" | "thigh" => "thigh",
        "leg" | "lowerleg" | "calf" | "shin" => "shin",
        "toebase" | "toes" | "foot" => "foot",
        other => other,
    };
    format!("{role}{side}")
}
fn world_matrices(nodes: &[Node], local: &[Mat4]) -> Result<Vec<Mat4>> {
    let mut out = vec![None; nodes.len()];
    for _ in 0..nodes.len() {
        let mut progress = false;
        for (i, n) in nodes.iter().enumerate() {
            if out[i].is_some() {
                continue;
            }
            if let Some(parent) = n.parent {
                if let Some(p) = out[parent] {
                    out[i] = Some(p * local[i]);
                    progress = true;
                }
            } else {
                out[i] = Some(local[i]);
                progress = true;
            }
        }
        if out.iter().all(Option::is_some) {
            break;
        }
        ensure!(progress, "Cyclic skeleton");
    }
    let matrices: Vec<Mat4> = out
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| anyhow::anyhow!("Cyclic skeleton"))?;
    ensure!(
        matrices.iter().all(|m| m.is_finite()),
        "Non-finite skeleton"
    );
    Ok(matrices)
}
struct Curve {
    node: usize,
    kind: u8,
    times: Vec<f32>,
    values: Vec<Vec4>,
    interpolation: gltf::animation::Interpolation,
}
impl Curve {
    fn sample(&self, t: f32) -> Vec4 {
        let cubic = self.interpolation == gltf::animation::Interpolation::CubicSpline;
        let index = |i: usize| if cubic { i * 3 + 1 } else { i };
        if t <= self.times[0] {
            return self.values[index(0)];
        }
        let last = self.times.len() - 1;
        if t >= self.times[last] {
            return self.values[index(last)];
        }
        let b = self.times.partition_point(|v| *v <= t).min(last);
        let a = b - 1;
        let dt = self.times[b] - self.times[a];
        let u = (t - self.times[a]) / dt;
        let av = self.values[index(a)];
        let bv = self.values[index(b)];
        if self.interpolation == gltf::animation::Interpolation::Step {
            av
        } else if cubic {
            let u2 = u * u;
            let u3 = u2 * u;
            av * (2. * u3 - 3. * u2 + 1.)
                + self.values[a * 3 + 2] * dt * (u3 - 2. * u2 + u)
                + bv * (-2. * u3 + 3. * u2)
                + self.values[b * 3] * dt * (u3 - u2)
        } else if self.kind == 1 {
            Vec4::from_array(
                Quat::from_array(av.to_array())
                    .normalize()
                    .slerp(Quat::from_array(bv.to_array()).normalize(), u)
                    .to_array(),
            )
        } else {
            av.lerp(bv, u)
        }
    }
}
struct Bvh<'a> {
    tokens: Vec<&'a str>,
    at: usize,
    nodes: Vec<Node>,
    offsets: Vec<Vec3>,
    channels: Vec<Vec<String>>,
}

impl<'a> Bvh<'a> {
    fn next(&mut self) -> Result<&'a str> {
        let v = self
            .tokens
            .get(self.at)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Truncated BVH"))?;
        self.at += 1;
        Ok(v)
    }
    fn expect(&mut self, v: &str) -> Result<()> {
        let got = self.next()?;
        ensure!(got == v, "Expected {v}, got {got}");
        Ok(())
    }
    fn number(&mut self) -> Result<f32> {
        let v: f32 = self.next()?.parse()?;
        ensure!(v.is_finite() && v.abs() <= 1e9, "Invalid BVH numeric value");
        Ok(v)
    }
    fn integer(&mut self) -> Result<usize> {
        Ok(self.next()?.parse()?)
    }
    fn joint(
        &mut self,
        name: String,
        parent: Option<usize>,
        depth: usize,
        end: bool,
    ) -> Result<()> {
        ensure!(
            depth < 64 && self.nodes.len() < 512,
            "BVH hierarchy limit exceeded"
        );
        let id = self.nodes.len();
        self.nodes.push(Node {
            name,
            parent,
            children: vec![],
        });
        self.offsets.push(Vec3::ZERO);
        self.channels.push(vec![]);
        if let Some(p) = parent {
            self.nodes[p].children.push(id);
        }
        self.expect("{")?;
        self.expect("OFFSET")?;
        self.offsets[id] = Vec3::new(self.number()?, self.number()?, self.number()?);
        if !end {
            self.expect("CHANNELS")?;
            let n = self.integer()?;
            ensure!(n <= 9, "Too many BVH channels");
            for _ in 0..n {
                let c = self.next()?.to_string();
                ensure!(
                    matches!(
                        c.as_str(),
                        "Xposition"
                            | "Yposition"
                            | "Zposition"
                            | "Xrotation"
                            | "Yrotation"
                            | "Zrotation"
                    ),
                    "Unknown BVH channel"
                );
                self.channels[id].push(c);
            }
        }
        loop {
            match self.next()? {
                "}" => break,
                "JOINT" if !end => {
                    let name = self.next()?.to_string();
                    self.joint(name, Some(id), depth + 1, false)?;
                }
                "End" if !end => {
                    self.expect("Site")?;
                    self.joint(format!("End_{id}"), Some(id), depth + 1, true)?;
                }
                other => bail!("Unexpected BVH hierarchy token {other}"),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn curve_interpolation_respects_linear_step_and_cubic_gltf_rules() {
        let mut curve = Curve {
            node: 0,
            kind: 0,
            times: vec![0., 1.],
            values: vec![Vec4::ZERO, Vec4::splat(10.)],
            interpolation: gltf::animation::Interpolation::Linear,
        };
        assert_eq!(curve.sample(-1.), Vec4::ZERO);
        assert_eq!(curve.sample(2.), Vec4::splat(10.));
        assert_eq!(curve.sample(0.5), Vec4::splat(5.));
        curve.interpolation = gltf::animation::Interpolation::Step;
        assert_eq!(curve.sample(0.5), Vec4::ZERO);
        curve.interpolation = gltf::animation::Interpolation::CubicSpline;
        curve.values = vec![
            Vec4::ZERO,
            Vec4::ZERO,
            Vec4::ZERO,
            Vec4::ZERO,
            Vec4::splat(10.),
            Vec4::ZERO,
        ];
        assert_eq!(curve.sample(0.5), Vec4::splat(5.));
        curve.kind = 1;
        curve.interpolation = gltf::animation::Interpolation::Linear;
        curve.values = vec![
            Vec4::from_array(Quat::IDENTITY.to_array()),
            Vec4::from_array(Quat::from_rotation_z(1.).to_array()),
        ];
        let q = Quat::from_array(curve.sample(0.5).to_array());
        assert!(q.angle_between(Quat::from_rotation_z(0.5)) < 0.001);
    }
    #[test]
    fn canonical_joint_aliases_cover_both_side_conventions() {
        for (a, b) in [
            ("l_arm", "LeftArm"),
            ("r_forearm", "forearm_R"),
            ("mixamorig:LeftUpLeg", "thigh_L"),
            ("RightLeg", "shin_R"),
            ("Pelvis", "hips"),
            ("Spine2", "chest"),
        ] {
            assert_eq!(canonical(a), canonical(b), "{a} vs {b}");
        }
    }
    #[test]
    fn bvh_downsampling_preserves_duration_and_projection() {
        let mut text="HIERARCHY ROOT hips { OFFSET 0 0 0 CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation End Site { OFFSET 0 10 0 } } MOTION Frames: 500 Frame Time: 0.02 ".to_string();
        for i in 0..500 {
            text.push_str(&format!("{i} 0 0 0 0 0 "));
        }
        let m = Motion::from_bvh(&text).unwrap();
        assert_eq!(m.frame_count(), 240);
        assert_eq!(m.warnings.len(), 1);
        assert!((239. / m.fps - 9.98).abs() < 0.001);
        assert!(!m.preview(Projection::Side, 9999).is_empty());
        assert!(!m.preview(Projection::ThreeQuarter, 0).is_empty());
        let mut r = Rig::new("x", 32, 32);
        let id = r
            .add_bone(
                crate::model::Vec2::new(16., 20.),
                crate::model::Vec2::new(16., 10.),
                None,
            )
            .unwrap();
        r.bone_mut(id).unwrap().name = "hips".into();
        assert!(m.retarget(&mut r, Projection::Side, f32::NAN).is_err());
        assert!(
            m.retarget_mapped(&mut r, Projection::Front, 1., &[Some(1000)])
                .is_err()
        );
        assert!(m.suggested_scale(&r).is_finite());
    }
    #[test]
    fn gltf_static_hierarchies_and_local_buffers_are_bounded() {
        let path = std::env::temp_dir().join(format!("rebone-motion-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("motion.bin"), [0u8; 4]).unwrap();
        let data = serde_json::json!({"asset":{"version":"2.0"},"buffers":[{"byteLength":4,"uri":"motion.bin"}],"nodes":[{"name":"hips","children":[1]},{"name":"chest","translation":[0.,10.,0.]}]});
        let motion = Motion::from_gltf(&serde_json::to_vec(&data).unwrap(), Some(&path)).unwrap();
        assert_eq!(motion.frame_count(), 1);
        assert_eq!(motion.preview(Projection::Front, 0).len(), 1);
        let mut invalid = data.clone();
        invalid["nodes"][1]["children"] = serde_json::json!([0]);
        assert!(Motion::from_gltf(&serde_json::to_vec(&invalid).unwrap(), Some(&path)).is_err());
        std::fs::remove_file(path.join("motion.bin")).unwrap();
        std::fs::remove_dir(path).unwrap();
    }
}
