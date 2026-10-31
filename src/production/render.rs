use super::*;
use crate::{animation::Pose, model::WorldBone};
use anyhow::Context;
use image::{Rgba, RgbaImage};

/// Compatibility renderer for callers that cannot display a Result. A failed
/// render is deliberately conspicuous; exports must use try_render_project.
pub(crate) fn render_project(
    project: &Project,
    frame: f32,
    pose: Option<&Pose>,
    scene: bool,
) -> RgbaImage {
    try_render_project(project, frame, pose, scene).unwrap_or_else(|_| {
        let mut image = RgbaImage::new(
            project.rig.width.clamp(1, 4096),
            project.rig.height.clamp(1, 4096),
        );
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            *pixel = Rgba(if (x / 8 + y / 8) % 2 == 0 {
                [230, 0, 110, 255]
            } else {
                [40, 0, 20, 255]
            });
        }
        image
    })
}

pub(crate) fn try_render_project(
    project: &Project,
    frame: f32,
    pose: Option<&Pose>,
    scene: bool,
) -> Result<RgbaImage> {
    let requested_frame = frame;
    let frame = project.rig.active_clip().evaluation_frame(frame);
    let p = &project.extras.production;
    let mut rig = project.rig.clone();
    let look = p.active_look.and_then(|i| p.looks.get(i));
    if let Some(look) = look {
        for part in &mut rig.parts {
            if let Some(c) = look.colors.get(&part.id) {
                part.color = *c;
            }
        }
    }
    let origin = project.extras.canvas_origin;
    let world = rig.world(pose);
    let rest = rig.world(None);
    let mut out = RgbaImage::new(rig.width.clamp(1, 4096), rig.height.clamp(1, 4096));
    enum Layer<'a> {
        Character(&'a crate::character3d::Character),
        Part(&'a crate::model::Part),
        Prop(&'a Attachment),
        Mesh(&'a Mesh),
        Face(&'a FaceTrack),
        Actor(&'a SceneActor),
    }
    let mesh_visible = |m: &Mesh| m.enabled && look.is_none_or(|l| l.meshes.contains(&m.id));
    let mut layers: Vec<(f32, Layer<'_>)> = rig
        .parts
        .iter()
        .filter(|part| {
            project.extras.show_2d
                && !p
                    .meshes
                    .iter()
                    .any(|m| mesh_visible(m) && m.replace_part == Some(part.id))
        })
        .map(|part| {
            (
                pose.and_then(|v| v.get(&part.id))
                    .and_then(|v| v.z)
                    .unwrap_or(part.z as f32),
                Layer::Part(part),
            )
        })
        .collect();
    layers.extend(
        p.attachments
            .iter()
            .filter(|a| a.enabled && look.is_none_or(|l| l.attachments.contains(&a.id)))
            .map(|a| (a.z as f32, Layer::Prop(a))),
    );
    layers.extend(
        p.meshes
            .iter()
            .filter(|m| mesh_visible(m))
            .map(|m| (m.z as f32, Layer::Mesh(m))),
    );
    layers.extend(
        p.faces
            .iter()
            .filter(|f| f.enabled && f.clip == rig.active_clip)
            .map(|f| (f.z as f32, Layer::Face(f))),
    );
    if scene {
        layers.extend(
            p.actors
                .iter()
                .filter(|a| a.visible)
                .map(|a| (a.z as f32, Layer::Actor(a))),
        );
    }
    layers.extend(
        project
            .extras
            .characters3d
            .iter()
            .filter(|c| c.enabled)
            .map(|c| (c.z as f32, Layer::Character(c))),
    );
    layers.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (_, layer) in layers {
        match layer {
            Layer::Character(c) => {
                let (c, time) = c.for_clip(rig.active_clip, timeline_ms(&rig, frame) as f64);
                let image = c
                    .render(out.width(), out.height(), origin, time)
                    .with_context(|| {
                        format!(
                            "Could not render 3D character {:?} (id {}) in clip {:?} (#{}) at frame {}",
                            c.name,
                            c.id,
                            rig.active_clip().name,
                            rig.active_clip + 1,
                            requested_frame
                        )
                    })?;
                composite_image(&mut out, &image, Vec2::default(), 1.);
            }
            Layer::Part(part) => {
                let mut single = rig.clone();
                single.parts = vec![part.clone()];
                let img = crate::skin::render_region(&single, pose, origin);
                composite_image(&mut out, &img, Vec2::default(), 1.);
            }
            Layer::Prop(a) => {
                let state = attachment_state(a, frame, rig.active_clip);
                if state.visible {
                    let asset =
                        p.assets
                            .iter()
                            .find(|v| v.id == state.asset)
                            .with_context(|| {
                                format!(
                                    "Missing asset {} for prop {:?} in clip {:?} at frame {}",
                                    state.asset,
                                    a.name,
                                    rig.active_clip().name,
                                    requested_frame
                                )
                            })?;
                    let (position, angle) = anchor(&world, state.bone, state.offset, state.angle);
                    draw_sprite(
                        &mut out,
                        &asset.image,
                        position - origin,
                        angle,
                        a.scale,
                        a.pivot,
                    );
                }
            }
            Layer::Mesh(m) => {
                let asset = p.assets.iter().find(|a| a.id == m.asset).with_context(|| {
                    format!(
                        "Missing asset {} for mesh {} in clip {:?} at frame {}",
                        m.asset,
                        m.id,
                        rig.active_clip().name,
                        requested_frame
                    )
                })?;
                let points = deform(m, &rest, &world, frame, rig.active_clip)
                    .into_iter()
                    .map(|p| p - origin)
                    .collect::<Vec<_>>();
                draw_mesh(&mut out, &asset.image, m, &points);
            }
            Layer::Face(f) => {
                if let Some(k) = f
                    .keys
                    .iter()
                    .rev()
                    .find(|k| k.frame <= frame)
                    .or_else(|| f.keys.first())
                {
                    let asset = p.assets.iter().find(|a| a.id == k.asset).with_context(|| {
                        format!(
                            "Missing asset {} for face {:?} in clip {:?} at frame {}",
                            k.asset,
                            f.name,
                            rig.active_clip().name,
                            requested_frame
                        )
                    })?;
                    let (position, angle) = anchor(&world, f.bone, f.offset + k.offset, f.angle);
                    draw_sprite(
                        &mut out,
                        &asset.image,
                        position - origin,
                        angle,
                        f.scale,
                        Vec2::default(),
                    );
                }
            }
            Layer::Actor(a) => {
                let placement = actor_placement(a, rig.active_clip, frame);
                if !placement.visible {
                    continue;
                }
                let base_ms = timeline_ms(&rig, frame) as f64
                    + a.clip_time_ms.get(&rig.active_clip).copied().unwrap_or(0.);
                let actor_clip = a.project.rig.active_clip();
                let time = (base_ms * a.speed as f64
                    + a.frame_offset as f64 * 1000. / actor_clip.fps as f64)
                    .max(0.);
                let (af, _) = actor_clip.frame_at_ms(time);
                let ap = a.project.pose_at(af as f32);
                let img = try_render_project(&a.project, af as f32, Some(&ap), false)
                    .with_context(|| {
                        format!(
                            "Could not render scene actor {:?} (id {}) in clip {:?} (#{}) at frame {}",
                            a.name,
                            a.id,
                            rig.active_clip().name,
                            rig.active_clip + 1,
                            requested_frame
                        )
                    })?;
                composite_image(
                    &mut out,
                    &img,
                    placement.offset + a.project.extras.canvas_origin * placement.scale - origin,
                    placement.scale,
                );
            }
        }
    }
    if let Some(c) = p
        .corrections
        .iter()
        .find(|c| c.clip == rig.active_clip && c.frame == frame.floor().max(1.) as u32)
    {
        for (index, color) in &c.pixels {
            if *index < out.width() * out.height() {
                out.put_pixel(index % out.width(), index / out.width(), Rgba(*color));
            }
        }
    }
    Ok(out)
}
pub fn timeline_ms(rig: &Rig, frame: f32) -> u64 {
    let frame = rig.active_clip().evaluation_frame(frame);
    let whole = frame.floor().max(1.) as u32;
    let elapsed: u64 = (1..whole.min(rig.active_clip().length + 1))
        .map(|f| rig.active_clip().duration_ms(f) as u64)
        .sum();
    elapsed
        + ((frame.fract().max(0.))
            * rig
                .active_clip()
                .duration_ms(whole.min(rig.active_clip().length)) as f32) as u64
}
pub fn actor_placement(actor: &SceneActor, clip: usize, frame: f32) -> ActorKey {
    let mut keys: Vec<_> = actor.keys.iter().filter(|k| k.clip == clip).collect();
    keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
    let previous = keys
        .iter()
        .rev()
        .find(|k| k.frame <= frame)
        .copied()
        .or_else(|| keys.first().copied());
    let next = keys.iter().find(|k| k.frame > frame).copied();
    let Some(a) = previous else {
        return ActorKey {
            clip,
            frame,
            offset: actor.offset,
            scale: actor.scale,
            visible: actor.visible,
        };
    };
    let mut result = a.clone();
    if let Some(b) = next {
        let t = ((frame - a.frame) / (b.frame - a.frame)).clamp(0., 1.);
        result.offset = a.offset * (1. - t) + b.offset * t;
        result.scale = a.scale * (1. - t) + b.scale * t;
    }
    result
}
fn over(dst: &mut Rgba<u8>, src: [u8; 4]) {
    if src[3] == 0 {
        return;
    }
    let a = src[3] as f32 / 255.;
    let b = dst[3] as f32 / 255.;
    let alpha = a + b * (1. - a);
    for i in 0..3 {
        dst[i] = ((src[i] as f32 * a + dst[i] as f32 * b * (1. - a)) / alpha).round() as u8;
    }
    dst[3] = (alpha * 255.).round() as u8;
}
fn sample(image: &Reference, x: u32, y: u32) -> [u8; 4] {
    if x >= image.width || y >= image.height {
        return [0; 4];
    }
    let i = (y as usize * image.width as usize + x as usize) * 4;
    image
        .pixels
        .get(i..i + 4)
        .and_then(|v| v.try_into().ok())
        .unwrap_or([0; 4])
}
pub(crate) fn anchor(
    world: &[WorldBone],
    bone: Option<u32>,
    offset: Vec2,
    angle: f32,
) -> (Vec2, f32) {
    bone.and_then(|id| world.iter().find(|w| w.id == id))
        .map_or((offset, angle), |w| {
            (w.head + offset.rotate(w.angle), w.angle + angle)
        })
}
pub(crate) fn attachment_state(a: &Attachment, frame: f32, clip: usize) -> AttachmentKey {
    super::clip_edit::prop_state(a, clip, frame)
}
fn bounds(points: &[Vec2], out: &RgbaImage) -> (u32, u32, u32, u32) {
    let minx = points
        .iter()
        .map(|v| v.x)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .clamp(0., out.width() as f32) as u32;
    let miny = points
        .iter()
        .map(|v| v.y)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .clamp(0., out.height() as f32) as u32;
    let maxx = points
        .iter()
        .map(|v| v.x)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .clamp(0., out.width() as f32) as u32;
    let maxy = points
        .iter()
        .map(|v| v.y)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .clamp(0., out.height() as f32) as u32;
    (minx, miny, maxx, maxy)
}
fn draw_sprite(
    out: &mut RgbaImage,
    image: &Reference,
    position: Vec2,
    angle: f32,
    scale: f32,
    pivot: Vec2,
) {
    if scale <= 0. || !scale.is_finite() {
        return;
    }
    let size = Vec2::new(image.width as f32, image.height as f32);
    let pivot = Vec2::new(pivot.x * size.x, pivot.y * size.y);
    let corners = [
        Vec2::default(),
        Vec2::new(size.x, 0.),
        size,
        Vec2::new(0., size.y),
    ]
    .map(|v| position + ((v - pivot) * scale).rotate(angle));
    let (l, t, r, b) = bounds(&corners, out);
    for y in t..b {
        for x in l..r {
            let local = (Vec2::new(x as f32 + 0.5, y as f32 + 0.5) - position).rotate(-angle)
                * (1. / scale)
                + pivot;
            if local.x >= 0. && local.y >= 0. && local.x < size.x && local.y < size.y {
                over(
                    out.get_pixel_mut(x, y),
                    sample(image, local.x as u32, local.y as u32),
                );
            }
        }
    }
}
fn composite_image(out: &mut RgbaImage, image: &RgbaImage, offset: Vec2, scale: f32) {
    if scale <= 0. {
        return;
    }
    let l = offset.x.floor().clamp(0., out.width() as f32) as u32;
    let t = offset.y.floor().clamp(0., out.height() as f32) as u32;
    let r = (offset.x + image.width() as f32 * scale)
        .ceil()
        .clamp(0., out.width() as f32) as u32;
    let b = (offset.y + image.height() as f32 * scale)
        .ceil()
        .clamp(0., out.height() as f32) as u32;
    for y in t..b {
        for x in l..r {
            let sx = ((x as f32 + 0.5 - offset.x) / scale) as u32;
            let sy = ((y as f32 + 0.5 - offset.y) / scale) as u32;
            if sx < image.width() && sy < image.height() {
                over(out.get_pixel_mut(x, y), image.get_pixel(sx, sy).0);
            }
        }
    }
}
pub fn mesh_vertices(rig: &Rig, pose: Option<&Pose>, mesh: &Mesh, frame: f32) -> Vec<Vec2> {
    deform(
        mesh,
        &rig.world(None),
        &rig.world(pose),
        frame,
        rig.active_clip,
    )
}
fn deform(
    mesh: &Mesh,
    rest: &[WorldBone],
    world: &[WorldBone],
    frame: f32,
    clip: usize,
) -> Vec<Vec2> {
    let keys = if frame > 0. {
        mesh_keys(mesh, clip)
    } else {
        &[]
    };
    let previous = keys.iter().rev().find(|k| k.frame <= frame);
    let next = keys.iter().find(|k| k.frame > frame);
    let alpha = previous.zip(next).map_or(0., |(a, b)| {
        ((frame - a.frame) / (b.frame - a.frame)).clamp(0., 1.)
    });
    mesh.vertices
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let mut p = Vec2::default();
            for w in &v.weights {
                let original = mesh.bind.get(&w.bone).copied().or_else(|| {
                    rest.iter().find(|b| b.id == w.bone).map(|r| BindTransform {
                        head: r.head,
                        angle: r.angle,
                        length: (r.tail - r.head).length(),
                    })
                });
                if let (Some(r), Some(t)) = (original, world.iter().find(|b| b.id == w.bone)) {
                    let local = (v.rest - r.head).rotate(-r.angle);
                    let scale = (t.tail - t.head).length() / r.length.max(0.0001);
                    p = p + (t.head + (local * scale).rotate(t.angle)) * w.weight;
                }
            }
            let offset = match (previous, next) {
                (Some(a), Some(b)) => {
                    a.offsets.get(i).copied().unwrap_or_default() * (1. - alpha)
                        + b.offsets.get(i).copied().unwrap_or_default() * alpha
                }
                (Some(a), None) => a.offsets.get(i).copied().unwrap_or_default(),
                (None, Some(b)) => b.offsets.get(i).copied().unwrap_or_default(),
                _ => Vec2::default(),
            };
            p + offset
        })
        .collect()
}
fn draw_mesh(out: &mut RgbaImage, image: &Reference, mesh: &Mesh, points: &[Vec2]) {
    let (left, top, right, bottom) = bounds(points, out);
    let width = (right - left) as usize;
    // One mesh is one translucent surface. A shared edge (or a folded overlap)
    // belongs to the first covering triangle, avoiding repeated alpha blending.
    let mut covered = vec![false; width * (bottom - top) as usize];
    for tri in &mesh.triangles {
        let Some(a) = points.get(tri[0]).copied() else {
            continue;
        };
        let Some(b) = points.get(tri[1]).copied() else {
            continue;
        };
        let Some(c) = points.get(tri[2]).copied() else {
            continue;
        };
        let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
        let denom = cross(b - a, c - a);
        if denom.abs() < 0.00001 {
            continue;
        }
        let (l, t, r, bottom) = bounds(&[a, b, c], out);
        for y in t..bottom {
            for x in l..r {
                let index = (y - top) as usize * width + (x - left) as usize;
                if covered[index] {
                    continue;
                }
                let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let wb = cross(p - a, c - a) / denom;
                let wc = cross(b - a, p - a) / denom;
                let wa = 1. - wb - wc;
                if wa >= -0.00001 && wb >= -0.00001 && wc >= -0.00001 {
                    covered[index] = true;
                    let uv = mesh.vertices[tri[0]].uv * wa
                        + mesh.vertices[tri[1]].uv * wb
                        + mesh.vertices[tri[2]].uv * wc;
                    over(
                        out.get_pixel_mut(x, y),
                        sample(
                            image,
                            (uv.x * image.width as f32)
                                .floor()
                                .clamp(0., image.width.saturating_sub(1) as f32)
                                as u32,
                            (uv.y * image.height as f32)
                                .floor()
                                .clamp(0., image.height.saturating_sub(1) as f32)
                                as u32,
                        ),
                    );
                }
            }
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub fn grid_mesh(
    rig: &Rig,
    id: u64,
    asset: u64,
    origin: Vec2,
    size: Vec2,
    columns: u32,
    rows: u32,
    bones: &[u32],
) -> Result<Mesh> {
    rig.validate().map_err(anyhow::Error::msg)?;
    ensure!(
        (1..=12).contains(&columns)
            && (1..=12).contains(&rows)
            && !bones.is_empty()
            && bones.len() <= 2048
            && bones.iter().all(|id| rig.bone(*id).is_some())
            && point(origin)
            && point(size)
            && point(origin + size)
            && size.x > 0.
            && size.y > 0.,
        "Choose valid image bounds, a 1–12 grid and target bones"
    );
    let world = rig.world(None);
    let mut mesh = Mesh {
        id,
        name: "Deformable artwork".into(),
        asset,
        clip: rig.active_clip,
        enabled: true,
        ..Default::default()
    };
    let unique: HashSet<_> = bones.iter().copied().collect();
    for y in 0..=rows {
        for x in 0..=columns {
            let uv = Vec2::new(x as f32 / columns as f32, y as f32 / rows as f32);
            let rest = origin + Vec2::new(uv.x * size.x, uv.y * size.y);
            let mut nearest: Vec<_> = unique
                .iter()
                .filter_map(|id| {
                    world.iter().find(|w| w.id == *id).map(|w| {
                        let d = w.tail - w.head;
                        let denom = d.x * d.x + d.y * d.y;
                        let t = if denom > 0. {
                            ((rest.x - w.head.x) * d.x + (rest.y - w.head.y) * d.y) / denom
                        } else {
                            0.
                        };
                        (
                            *id,
                            (rest - (w.head + d * t.clamp(0., 1.))).length().max(0.1),
                        )
                    })
                })
                .collect();
            nearest.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
            nearest.truncate(2);
            let total: f32 = nearest.iter().map(|v| 1. / v.1).sum();
            mesh.vertices.push(Vertex {
                rest,
                uv,
                weights: nearest
                    .into_iter()
                    .map(|(bone, d)| Weight {
                        bone,
                        weight: 1. / d / total,
                    })
                    .collect(),
            });
        }
    }
    for id in mesh
        .vertices
        .iter()
        .flat_map(|v| v.weights.iter().map(|w| w.bone))
    {
        let w = world.iter().find(|w| w.id == id).unwrap();
        mesh.bind.insert(
            id,
            BindTransform {
                head: w.head,
                angle: w.angle,
                length: (w.tail - w.head).length(),
            },
        );
    }
    for y in 0..rows {
        for x in 0..columns {
            let a = (y * (columns + 1) + x) as usize;
            let b = a + 1;
            let c = a + (columns + 1) as usize;
            mesh.triangles.extend([[a, b, c], [b, c + 1, c]]);
        }
    }
    Ok(mesh)
}
pub fn switch_attachment_parent(
    project: &mut Project,
    id: u64,
    bone: Option<u32>,
    frame: f32,
) -> Result<()> {
    let clip = project.rig.active_clip;
    let active = project.rig.clips.get(clip).context("Invalid active clip")?;
    ensure!(
        bone.is_none_or(|id| project.rig.bone(id).is_some())
            && frame.is_finite()
            && (1. ..=active.length as f32).contains(&frame),
        "Invalid parent or frame"
    );
    let displayed = active.evaluation_frame(frame);
    let length = active.length;
    let index = project
        .extras
        .production
        .attachments
        .iter()
        .position(|a| a.id == id)
        .context("Attachment not found")?;
    let state = attachment_state(
        &project.extras.production.attachments[index],
        displayed,
        clip,
    );
    let pose = project.pose_at(displayed);
    let world = project.rig.world(Some(&pose));
    let (pos, angle) = anchor(&world, state.bone, state.offset, state.angle);

    let mut candidate = project.clone();
    candidate.rig.clips[clip]
        .release_hold(frame)
        .map_err(anyhow::Error::msg)?;
    // Releasing a held tail also resumes its parent controller clock. Express
    // the displayed world transform relative to that newly evaluated parent.
    let world = candidate.rig.world(Some(&candidate.pose_at(frame)));
    let (offset, relative) = bone
        .and_then(|id| world.iter().find(|w| w.id == id))
        .map_or((pos, angle), |w| {
            ((pos - w.head).rotate(-w.angle), angle - w.angle)
        });
    let keys = prop_keys_mut(
        &mut candidate.extras.production.attachments[index],
        clip,
        length,
    );
    keys.retain(|k| k.frame != frame);
    keys.push(AttachmentKey {
        frame,
        bone,
        offset,
        angle: relative,
        asset: state.asset,
        visible: state.visible,
    });
    keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
    candidate.validate()?;
    *project = candidate;
    Ok(())
}
pub fn paint_pixel(
    project: &mut Project,
    clip: usize,
    frame: u32,
    x: u32,
    y: u32,
    color: Option<[u8; 4]>,
) -> Result<()> {
    paint_pixels(project, clip, frame, &[(x, y)], color)
}

/// Apply a complete brush stroke with one clone, clock release and validation.
/// Invalid coordinates or a failed release leave the whole project unchanged.
pub fn paint_pixels(
    project: &mut Project,
    clip: usize,
    frame: u32,
    pixels: &[(u32, u32)],
    color: Option<[u8; 4]>,
) -> Result<()> {
    ensure!(
        clip < project.rig.clips.len()
            && frame >= 1
            && frame <= project.rig.clips[clip].length
            && pixels
                .iter()
                .all(|&(x, y)| x < project.rig.width && y < project.rig.height),
        "Pixel correction is outside the canvas or clip"
    );
    if pixels.is_empty() {
        return Ok(());
    }
    if color.is_none()
        && !project
            .extras
            .production
            .corrections
            .iter()
            .any(|c| c.clip == clip && c.frame == frame)
    {
        return Ok(());
    }
    let mut candidate = project.clone();
    if color.is_some() {
        candidate.rig.clips[clip]
            .release_hold(frame as f32)
            .map_err(anyhow::Error::msg)?;
    }
    let all = &mut candidate.extras.production.corrections;
    let i = if let Some(i) = all.iter().position(|c| c.clip == clip && c.frame == frame) {
        i
    } else {
        if color.is_none() {
            return Ok(());
        }
        all.push(Correction {
            clip,
            frame,
            pixels: BTreeMap::new(),
        });
        all.len() - 1
    };
    for &(x, y) in pixels {
        let index = y * project.rig.width + x;
        if let Some(color) = color {
            all[i].pixels.insert(index, color);
        } else {
            all[i].pixels.remove(&index);
        }
    }
    if all[i].pixels.is_empty() {
        all.remove(i);
    }
    candidate.validate()?;
    *project = candidate;
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CollisionShape {
    pub id: u64,
    pub name: String,
    pub kind: BoxKind,
    pub center: Vec2,
    pub size: Vec2,
    pub angle: f32,
    pub corners: [Vec2; 4],
}
pub fn collision_shapes(project: &Project, frame: f32) -> Vec<CollisionShape> {
    let frame = project.rig.active_clip().evaluation_frame(frame);
    let world = project.rig.world(Some(&project.pose_at(frame)));
    project
        .extras
        .production
        .boxes
        .iter()
        .filter(|b| {
            b.enabled
                && b.clip == project.rig.active_clip
                && frame >= b.start as f32
                && frame <= b.end as f32
        })
        .filter_map(|b| {
            let a = b.keys.iter().rev().find(|k| k.frame <= frame);
            let n = b.keys.iter().find(|k| k.frame > frame);
            if a.is_some_and(|k| !k.enabled) {
                return None;
            }
            let (mut offset, mut size, mut angle) = (b.offset, b.size, b.angle);
            if let Some(a) = a {
                offset = a.offset;
                size = a.size;
                angle = a.angle;
                if let Some(n) = n {
                    let t = ((frame - a.frame) / (n.frame - a.frame)).clamp(0., 1.);
                    offset = offset * (1. - t) + n.offset * t;
                    size = size * (1. - t) + n.size * t;
                    angle += crate::animation::wrap_degrees(n.angle - angle) * t;
                }
            }
            let (center, angle) = anchor(&world, b.bone, offset, angle);
            let corners = [
                Vec2::new(-size.x * 0.5, -size.y * 0.5),
                Vec2::new(size.x * 0.5, -size.y * 0.5),
                Vec2::new(size.x * 0.5, size.y * 0.5),
                Vec2::new(-size.x * 0.5, size.y * 0.5),
            ]
            .map(|p| center + p.rotate(angle));
            Some(CollisionShape {
                id: b.id,
                name: b.name.clone(),
                kind: b.kind,
                center,
                size,
                angle,
                corners,
            })
        })
        .collect()
}
/// Exported regions share the raster's top-left origin, including a grown canvas.
pub fn collision_pixels(project: &Project, frame: f32) -> Vec<CollisionShape> {
    let origin = project.extras.canvas_origin;
    collision_shapes(project, frame)
        .into_iter()
        .map(|mut b| {
            b.center = b.center - origin;
            for p in &mut b.corners {
                *p = *p - origin;
            }
            b
        })
        .collect()
}
