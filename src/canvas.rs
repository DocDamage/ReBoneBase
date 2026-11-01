//! Growing canvas envelopes. World joints never move when the raster grows.
use crate::{
    animation::Pose,
    model::Vec2,
    production::{self, render},
    project::Project,
};
use anyhow::{Result, ensure};

const ENVELOPE_BUDGET: u64 = 128 * 1024 * 1024;

// Content bounds validate/deform geometry as well as solving the hierarchy.
// Charge unused vertices too: they are visited by the 3D bounds implementation.
fn content_work(p: &Project) -> u64 {
    let bones = p.rig.bones.len() as u64;
    let art = &p.extras.production;
    let mut work =
        1u64.saturating_add(bones.saturating_mul(bones.saturating_add(p.rig.parts.len() as u64)));
    work = work
        .saturating_add((art.attachments.len() as u64).saturating_mul(art.assets.len() as u64 + 4));
    work =
        work.saturating_add((art.faces.len() as u64).saturating_mul(art.assets.len() as u64 + 240));
    for m in art.meshes.iter().filter(|m| m.enabled) {
        work = work.saturating_add(
            (m.vertices.len() as u64).saturating_mul(bones.saturating_mul(8).saturating_add(4)),
        );
    }
    work = art
        .corrections
        .iter()
        .fold(work, |w, c| w.saturating_add(c.pixels.len() as u64));
    // Actor geometry is grown once in its own pass. Parent passes only inspect
    // its cached canvas rectangle and held placement keys.
    for a in art.actors.iter().filter(|a| a.visible) {
        work = work.saturating_add(16 + (a.keys.len() as u64).saturating_mul(16));
    }
    for c in p.extras.characters3d.iter().filter(|c| c.enabled) {
        let nodes = c.model.nodes.len() as u64;
        let samples = c.model.animations.iter().fold(0u64, |s, a| {
            s.saturating_add((a.frames.len() as u64).saturating_mul(nodes))
        });
        work = work
            .saturating_add((c.model.vertex_count() as u64).saturating_mul(8))
            .saturating_add((c.model.triangle_count() as u64).saturating_mul(3))
            .saturating_add(nodes.saturating_mul(nodes).saturating_mul(16))
            .saturating_add(samples.saturating_mul(16));
    }
    work
}
/// Conservative work for all automatic canvas envelopes, including visible
/// actors' own growth passes. Does not scan individual geometry or change data.
pub fn envelope_work(p: &Project) -> u64 {
    let frames = p
        .rig
        .clips
        .iter()
        .fold(1u64, |s, c| s.saturating_add(u64::from(c.length)));
    let own = if p.extras.auto_canvas {
        content_work(p).saturating_mul(frames)
    } else {
        0
    };
    p.extras
        .production
        .actors
        .iter()
        .filter(|a| a.visible)
        .fold(own, |s, a| s.saturating_add(envelope_work(&a.project)))
}

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub min: Vec2,
    pub max: Vec2,
}
impl Bounds {
    fn empty() -> Self {
        Self {
            min: Vec2::new(f32::INFINITY, f32::INFINITY),
            max: Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY),
        }
    }
    fn point(&mut self, p: Vec2) {
        self.min.x = self.min.x.min(p.x);
        self.min.y = self.min.y.min(p.y);
        self.max.x = self.max.x.max(p.x);
        self.max.y = self.max.y.max(p.y);
    }
    fn circle(&mut self, p: Vec2, r: f32) {
        self.point(p - Vec2::new(r, r));
        self.point(p + Vec2::new(r, r));
    }
    fn include(&mut self, b: Self) {
        if b.min.is_finite() && b.max.is_finite() {
            self.point(b.min);
            self.point(b.max);
        }
    }
}
pub fn content_bounds(p: &Project, frame: f32, pose: Option<&Pose>) -> Bounds {
    let mut out = Bounds::empty();
    let world = p.rig.world(pose);
    let art = &p.extras.production;
    for (b, w) in p.rig.bones.iter().zip(&world).filter(|_| p.extras.show_2d) {
        let part = p.rig.parts.iter().find(|v| v.id == b.part);
        let pad = b.radius_head.max(b.radius_tail)
            + b.bulges_left
                .iter()
                .chain(&b.bulges_right)
                .map(|q| q.d.max(0.))
                .fold(0., f32::max)
            + part.map_or(0., |p| p.blend + p.outline_width as f32)
            + 2.;
        out.circle(w.head, pad);
        out.circle(w.tail, pad);
    }
    let look = art.active_look.and_then(|i| art.looks.get(i));
    let mut sprite = |asset: u64, pos: Vec2, angle: f32, scale: f32, pivot: Vec2| {
        if let Some(a) = art.assets.iter().find(|a| a.id == asset) {
            let w = a.image.width as f32;
            let h = a.image.height as f32;
            for q in [
                Vec2::new(0., 0.),
                Vec2::new(w, 0.),
                Vec2::new(w, h),
                Vec2::new(0., h),
            ] {
                out.point(pos + ((q - Vec2::new(w * pivot.x, h * pivot.y)) * scale).rotate(angle));
            }
        }
    };
    for a in &art.attachments {
        if a.enabled && look.is_none_or(|l| l.attachments.contains(&a.id)) {
            let s = render::attachment_state(a, frame, p.rig.active_clip);
            if s.visible {
                let (pos, angle) = render::anchor(&world, s.bone, s.offset, s.angle);
                sprite(s.asset, pos, angle, a.scale, a.pivot);
            }
        }
    }
    for f in &art.faces {
        if f.enabled
            && f.clip == p.rig.active_clip
            && let Some(k) = f
                .keys
                .iter()
                .rev()
                .find(|k| k.frame <= frame)
                .or_else(|| f.keys.first())
        {
            let (pos, angle) = render::anchor(&world, f.bone, f.offset + k.offset, f.angle);
            sprite(k.asset, pos, angle, f.scale, Vec2::default());
        }
    }
    for m in &art.meshes {
        if m.enabled && look.is_none_or(|l| l.meshes.contains(&m.id)) {
            for v in production::mesh_vertices(&p.rig, pose, m, frame) {
                out.point(v);
            }
        }
    }
    if let Some(c) = art
        .corrections
        .iter()
        .find(|c| c.clip == p.rig.active_clip && c.frame == frame.floor().max(1.) as u32)
    {
        for (index, color) in &c.pixels {
            if color[3] > 0 {
                let pixel = p.extras.canvas_origin
                    + Vec2::new((index % p.rig.width) as f32, (index / p.rig.width) as f32);
                out.point(pixel);
                out.point(pixel + Vec2::new(1., 1.));
            }
        }
    }
    for a in &art.actors {
        let placement = production::actor_placement(a, p.rig.active_clip, frame);
        if !a.visible || !placement.visible {
            continue;
        }
        let min = a.project.extras.canvas_origin;
        let b = Bounds {
            min,
            max: min + Vec2::new(a.project.rig.width as f32, a.project.rig.height as f32),
        };
        out.include(Bounds {
            min: b.min * placement.scale + placement.offset,
            max: b.max * placement.scale + placement.offset,
        });
    }
    let time = production::timeline_ms(&p.rig, frame) as f64;
    for c in &p.extras.characters3d {
        let (c, time) = c.for_clip(
            p.rig.active_clip,
            p.rig.active_clip().evaluation_frame(frame),
            time,
        );
        if c.enabled
            && let Some((min, max)) = c.bounds(time)
        {
            out.include(Bounds { min, max });
        }
    }
    out
}
/// Preserve corrections' world locations when changing their raster stride/origin.
pub fn set_bounds(p: &mut Project, origin: Vec2, width: u32, height: u32) -> Result<bool> {
    ensure!(
        origin.is_finite()
            && origin.x.fract() == 0.
            && origin.y.fract() == 0.
            && (1..=4096).contains(&width)
            && (1..=4096).contains(&height),
        "Character exceeds the maximum 4096 × 4096 canvas"
    );
    if p.rig.width == width && p.rig.height == height && p.extras.canvas_origin == origin {
        return Ok(false);
    }
    let mut candidate = p.clone();
    let old = p.extras.canvas_origin;
    let pivot = p.extras.production.pivot;
    let anchor = old + Vec2::new(p.rig.width as f32 * pivot.x, p.rig.height as f32 * pivot.y);
    candidate.extras.production.pivot = Vec2::new(
        ((anchor.x - origin.x) / width as f32).clamp(0., 1.),
        ((anchor.y - origin.y) / height as f32).clamp(0., 1.),
    );
    for c in &mut candidate.extras.production.corrections {
        c.pixels = c
            .pixels
            .iter()
            .filter_map(|(i, color)| {
                let x = (*i % p.rig.width) as f32 + old.x - origin.x;
                let y = (*i / p.rig.width) as f32 + old.y - origin.y;
                (x >= 0. && y >= 0. && x < width as f32 && y < height as f32)
                    .then_some((y as u32 * width + x as u32, *color))
            })
            .collect();
    }
    candidate.rig.width = width;
    candidate.rig.height = height;
    candidate.extras.canvas_origin = origin;
    candidate.validate()?;
    *p = candidate;
    Ok(true)
}
fn apply(p: &mut Project, content: Bounds) -> Result<bool> {
    if !content.min.is_finite() || !content.max.is_finite() {
        return Ok(false);
    }
    let old = p.extras.canvas_origin;
    let min = Vec2::new(
        old.x.min((content.min.x - 2.).floor()),
        old.y.min((content.min.y - 2.).floor()),
    );
    let max = Vec2::new(
        (old.x + p.rig.width as f32).max((content.max.x + 2.).ceil()),
        (old.y + p.rig.height as f32).max((content.max.y + 2.).ceil()),
    );
    ensure!(
        max.x - min.x <= 4096. && max.y - min.y <= 4096.,
        "Character exceeds the maximum 4096 × 4096 canvas"
    );
    set_bounds(p, min, (max.x - min.x) as u32, (max.y - min.y) as u32)
}
pub fn grow(p: &mut Project, frame: f32, pose: Option<&Pose>) -> Result<bool> {
    p.validate()?;
    if !p.extras.auto_canvas {
        return Ok(false);
    }
    let b = content_bounds(p, frame, pose);
    apply(p, b)
}
/// A stable envelope across all clips prevents sprite sheets cropping later frames.
pub fn grow_all(p: &mut Project) -> Result<bool> {
    p.validate()?;
    ensure!(
        envelope_work(p) <= ENVELOPE_BUDGET,
        "Canvas envelope exceeds the work budget; reduce frames, character detail or visible actors, or disable automatic canvas growth"
    );
    let mut prepared = p.clone();
    let mut changed = false;
    for actor in &mut prepared.extras.production.actors {
        if actor.visible {
            changed |= grow_all(&mut actor.project)?;
        }
    }
    if !prepared.extras.auto_canvas {
        prepared.validate()?;
        if changed {
            *p = prepared;
        }
        return Ok(changed);
    }
    let mut probe = prepared.clone();
    let mut b = content_bounds(&prepared, 1., None);
    for i in 0..probe.rig.clips.len() {
        probe.rig.active_clip = i;
        for f in 1..=probe.rig.active_clip().length {
            b.include(content_bounds(
                &probe,
                f as f32,
                Some(&probe.pose_at(f as f32)),
            ));
        }
    }
    changed |= apply(&mut prepared, b)?;
    prepared.validate()?;
    if changed {
        *p = prepared;
    }
    Ok(changed)
}
