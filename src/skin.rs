//! Pixel skin: tapered capsules, cosine bulges and smooth unions from skin.lua.
use crate::animation::Pose;
use crate::model::{Bone, Bulge, Rig, Vec2, WorldBone};
use image::{Rgba, RgbaImage};

pub fn bulge_at(list: &[Bulge], t: f32) -> f32 {
    bulge_precise(list, f64::from(t)) as f32
}
fn bulge_precise(list: &[Bulge], t: f64) -> f64 {
    if t <= 0. || t >= 1. {
        return 0.;
    }
    let mut previous = Bulge { t: 0., d: 0. };
    for next in list
        .iter()
        .cloned()
        .chain(std::iter::once(Bulge { t: 1., d: 0. }))
    {
        if t <= f64::from(next.t) {
            let u = (t - f64::from(previous.t)) / f64::from(next.t - previous.t).max(0.000001);
            return f64::from(previous.d)
                + f64::from(next.d - previous.d) * (1. - (std::f64::consts::PI * u).cos()) * 0.5;
        }
        previous = next;
    }
    0.
}
pub fn bone_distance(bone: &Bone, world: &WorldBone, point: Vec2) -> f32 {
    distance_precise(bone, world, point) as f32
}
fn distance_precise(bone: &Bone, world: &WorldBone, point: Vec2) -> f64 {
    let dx = f64::from(world.tail.x) - f64::from(world.head.x);
    let dy = f64::from(world.tail.y) - f64::from(world.head.y);
    let qx = f64::from(point.x) - f64::from(world.head.x);
    let qy = f64::from(point.y) - f64::from(world.head.y);
    let length = dx.hypot(dy);
    let head_radius = f64::from(bone.radius_head);
    let tail_radius = f64::from(bone.radius_tail);
    if length < 0.000001 {
        return qx.hypot(qy) - head_radius.max(tail_radius);
    }
    let along = (qx * dx + qy * dy) / length;
    let side = (-qx * dy + qy * dx) / length;
    let ax = side.abs();
    let b = (head_radius - tail_radius) / length;
    let distance = if b * b >= 1. {
        (along.hypot(side) - head_radius).min((along - length).hypot(side) - tail_radius)
    } else {
        let a = (1. - b * b).sqrt();
        let k = -b * ax + a * along;
        if k < 0. {
            ax.hypot(along) - head_radius
        } else if k > a * length {
            ax.hypot(along - length) - tail_radius
        } else {
            ax * a + along * b - head_radius
        }
    };
    distance
        - bulge_precise(
            if side < 0. {
                &bone.bulges_left
            } else {
                &bone.bulges_right
            },
            along / length,
        )
}
pub fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    smooth_precise(f64::from(a), f64::from(b), f64::from(k)) as f32
}
fn smooth_precise(a: f64, b: f64, k: f64) -> f64 {
    if k <= 0. {
        return a.min(b);
    }
    let h = (k - (a - b).abs()).max(0.) / k;
    a.min(b) - h * h * k * 0.25
}

/// Renders one canvas-sized transparent RGBA frame. Parts are composited by z.
pub fn render(rig: &Rig, pose: Option<&Pose>) -> RgbaImage {
    render_region(rig, pose, Vec2::default())
}
pub fn render_region(rig: &Rig, pose: Option<&Pose>, origin: Vec2) -> RgbaImage {
    let mut image = RgbaImage::new(rig.width.clamp(1, 4096), rig.height.clamp(1, 4096));
    let world = rig.world(pose);
    let mut parts: Vec<_> = rig.parts.iter().collect();
    let z = |id: u32, base: i32| {
        pose.and_then(|p| p.get(&id))
            .and_then(|p| p.z)
            .unwrap_or(base as f32)
    };
    parts.sort_by(|a, b| z(a.id, a.z).total_cmp(&z(b.id, b.z)));
    for part in parts {
        if pose
            .and_then(|p| p.get(&part.id))
            .and_then(|p| p.visibility)
            .is_some_and(|v| v < 0.5)
            || part.color[3] == 0
        {
            continue;
        }
        let bones: Vec<_> = rig
            .bones
            .iter()
            .zip(world.iter())
            .filter(|(b, _)| b.part == part.id)
            .collect();
        if bones.is_empty() {
            continue;
        }
        let mut min = Vec2::new(f32::INFINITY, f32::INFINITY);
        let mut max = Vec2::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        for (b, w) in &bones {
            let padding = b.radius_head.max(b.radius_tail)
                + b.bulges_left
                    .iter()
                    .chain(&b.bulges_right)
                    .map(|q| q.d.max(0.))
                    .fold(0., f32::max)
                + part.blend
                + 1.;
            min.x = min.x.min(w.head.x.min(w.tail.x) - padding);
            min.y = min.y.min(w.head.y.min(w.tail.y) - padding);
            max.x = max.x.max(w.head.x.max(w.tail.x) + padding);
            max.y = max.y.max(w.head.y.max(w.tail.y) + padding);
        }
        let left = (min.x - origin.x).floor().max(0.) as u32;
        let top = (min.y - origin.y).floor().max(0.) as u32;
        let right = ((max.x - origin.x).ceil().max(0.) as u32).min(image.width());
        let bottom = ((max.y - origin.y).ceil().max(0.) as u32).min(image.height());
        let mut layer =
            (part.outline_width > 0).then(|| RgbaImage::new(image.width(), image.height()));
        for y in top..bottom {
            for x in left..right {
                let point = Vec2::new(x as f32 + 0.5 + origin.x, y as f32 + 0.5 + origin.y);
                let mut distance = f64::INFINITY;
                for (bone, w) in &bones {
                    let d = distance_precise(bone, w, point);
                    distance = if distance.is_infinite() {
                        d
                    } else {
                        smooth_precise(distance, d, f64::from(part.blend))
                    };
                }
                if distance <= 0. {
                    let dst = match &mut layer {
                        Some(layer) => layer.get_pixel_mut(x, y),
                        None => image.get_pixel_mut(x, y),
                    };
                    *dst = Rgba(composite(dst.0, part.color));
                }
            }
        }
        if let Some(layer) = layer {
            let outlined = outline(&layer, part.outline_width, part.outline_color);
            for (dst, src) in image.pixels_mut().zip(outlined.pixels()) {
                if src[3] > 0 {
                    *dst = Rgba(composite(dst.0, src.0));
                }
            }
        }
    }
    image
}
/// Expands a crisp outside outline using an O(pixels) 8-connected distance field.
/// Existing fill pixels remain unchanged; the output retains the canvas dimensions.
pub fn outline(image: &RgbaImage, width: u32, color: [u8; 4]) -> RgbaImage {
    if width == 0 || color[3] == 0 {
        return image.clone();
    }
    let width = width.min(16) as u8;
    let w = image.width() as usize;
    let h = image.height() as usize;
    let mut distances: Vec<u8> = image
        .pixels()
        .map(|p| if p[3] > 0 { 0 } else { u8::MAX })
        .collect();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let mut d = distances[i];
            if x > 0 {
                d = d.min(distances[i - 1].saturating_add(1));
            }
            if y > 0 {
                d = d.min(distances[i - w].saturating_add(1));
                if x > 0 {
                    d = d.min(distances[i - w - 1].saturating_add(1));
                }
                if x + 1 < w {
                    d = d.min(distances[i - w + 1].saturating_add(1));
                }
            }
            distances[i] = d;
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            let mut d = distances[i];
            if x + 1 < w {
                d = d.min(distances[i + 1].saturating_add(1));
            }
            if y + 1 < h {
                d = d.min(distances[i + w].saturating_add(1));
                if x > 0 {
                    d = d.min(distances[i + w - 1].saturating_add(1));
                }
                if x + 1 < w {
                    d = d.min(distances[i + w + 1].saturating_add(1));
                }
            }
            distances[i] = d;
        }
    }
    let mut result = image.clone();
    for (pixel, distance) in result.pixels_mut().zip(distances) {
        if pixel[3] == 0 && distance <= width {
            *pixel = Rgba(color);
        }
    }
    result
}
fn composite(dst: [u8; 4], src: [u8; 4]) -> [u8; 4] {
    if src[3] == 255 || dst[3] == 0 {
        return src;
    }
    let sa = src[3] as f32 / 255.;
    let da = dst[3] as f32 / 255.;
    let a = sa + da * (1. - sa);
    let mut out = [0; 4];
    for i in 0..3 {
        out[i] = ((src[i] as f32 * sa + dst[i] as f32 * da * (1. - sa)) / a).round() as u8;
    }
    out[3] = (a * 255.).round() as u8;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Rig, Vec2};
    #[test]
    fn renders_transparent_background_and_part_z_order() {
        let mut r = Rig::new("Pixels", 24, 24);
        let a = r.add_part("red", [255, 0, 0, 255]);
        let b = r.add_part("blue", [0, 0, 255, 255]);
        let x = r
            .add_bone(Vec2::new(5., 12.), Vec2::new(19., 12.), None)
            .unwrap();
        let y = r
            .add_bone(Vec2::new(12., 5.), Vec2::new(12., 19.), None)
            .unwrap();
        r.bone_mut(x).unwrap().part = a;
        r.bone_mut(y).unwrap().part = b;
        let image = render(&r, None);
        assert_eq!(image.dimensions(), (24, 24));
        assert_eq!(image.get_pixel(0, 0).0, [0, 0, 0, 0]);
        assert_eq!(image.get_pixel(12, 12).0, [0, 0, 255, 255]);
        r.parts[0].z = 5;
        assert_eq!(render(&r, None).get_pixel(12, 12).0, [255, 0, 0, 255]);
    }
    #[test]
    fn bulges_and_taper_distance_follow_original_field() {
        let mut r = Rig::new("Shape", 32, 32);
        let id = r
            .add_bone(Vec2::new(5., 16.), Vec2::new(25., 16.), None)
            .unwrap();
        let world = r.world(None);
        let bone = r.bone_mut(id).unwrap();
        bone.radius_head = 2.;
        bone.radius_tail = 2.;
        assert!((bone_distance(bone, &world[0], Vec2::new(15., 20.)) - 2.).abs() < 0.001);
        bone.bulges_right
            .push(crate::model::Bulge { t: 0.5, d: 3. });
        assert!(bone_distance(bone, &world[0], Vec2::new(15., 20.)) < 0.);
        assert_eq!(bulge_at(&bone.bulges_right, 0.), 0.);
    }

    #[test]
    fn animated_visibility_and_alpha_are_composited() {
        let mut r = Rig::new("Alpha", 16, 16);
        let a = r.add_part("Red", [255, 0, 0, 255]);
        let b = r.add_part("Blue", [0, 0, 255, 128]);
        let id = r
            .add_bone(Vec2::new(4., 8.), Vec2::new(12., 8.), None)
            .unwrap();
        r.bone_mut(id).unwrap().part = a;
        let id = r
            .add_bone(Vec2::new(4., 8.), Vec2::new(12., 8.), None)
            .unwrap();
        r.bone_mut(id).unwrap().part = b;
        let color = render(&r, None).get_pixel(8, 8).0;
        assert_eq!(color, [127, 0, 128, 255]);
        let mut pose = Pose::new();
        pose.insert(
            b,
            crate::animation::BonePose {
                visibility: Some(0.),
                ..Default::default()
            },
        );
        assert_eq!(render(&r, Some(&pose)).get_pixel(8, 8).0, [255, 0, 0, 255]);
        pose.insert(
            b,
            crate::animation::BonePose {
                z: Some(-1.),
                ..Default::default()
            },
        );
        assert_eq!(render(&r, Some(&pose)).get_pixel(8, 8).0, [255, 0, 0, 255]);
        assert_eq!(smooth_min(2., 2., 4.), 1.);
        assert_eq!(smooth_min(1., 3., 0.), 1.);
    }

    #[test]
    fn tapered_capsule_end_discs_and_degenerate_length_are_finite() {
        let mut r = Rig::new("Distance", 32, 32);
        let id = r
            .add_bone(Vec2::new(5., 16.), Vec2::new(25., 16.), None)
            .unwrap();
        let mut w = r.world(None).remove(0);
        let bone = r.bone_mut(id).unwrap();
        bone.radius_head = 5.;
        bone.radius_tail = 2.;
        assert!((bone_distance(bone, &w, Vec2::new(0., 16.))).abs() < 0.001);
        assert!((bone_distance(bone, &w, Vec2::new(27., 16.))).abs() < 0.001);
        bone.radius_head = 30.;
        assert!(bone_distance(bone, &w, Vec2::new(5., 16.)) < 0.);
        w.tail = w.head;
        assert_eq!(bone_distance(bone, &w, w.head), -30.);
    }
}
