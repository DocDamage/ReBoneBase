use super::*;
use ::gltf as g;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use std::collections::HashSet;
struct Curve {
    node: usize,
    kind: u8,
    times: Vec<f32>,
    values: Vec<Vec4>,
    interpolation: g::animation::Interpolation,
}
impl Curve {
    fn sample(&self, time: f32) -> Vec4 {
        if time <= self.times[0] {
            return self.value(0);
        }
        let last = self.times.len() - 1;
        if time >= self.times[last] {
            return self.value(last);
        }
        let right = self.times.partition_point(|t| *t <= time);
        let left = right - 1;
        let duration = self.times[right] - self.times[left];
        let u = (time - self.times[left]) / duration;
        let a = self.value(left);
        let b = self.value(right);
        match self.interpolation {
            g::animation::Interpolation::Step => a,
            g::animation::Interpolation::CubicSpline => {
                let u2 = u * u;
                let u3 = u2 * u;
                let value = a * (2. * u3 - 3. * u2 + 1.)
                    + self.values[left * 3 + 2] * duration * (u3 - 2. * u2 + u)
                    + b * (-2. * u3 + 3. * u2)
                    + self.values[right * 3] * duration * (u3 - u2);
                if self.kind == 1 {
                    value.normalize()
                } else {
                    value
                }
            }
            _ => {
                if self.kind == 1 {
                    Vec4::from_array(
                        Quat::from_array(a.to_array())
                            .normalize()
                            .slerp(Quat::from_array(b.to_array()).normalize(), u)
                            .normalize()
                            .to_array(),
                    )
                } else {
                    a.lerp(b, u)
                }
            }
        }
    }
    fn value(&self, index: usize) -> Vec4 {
        self.values[index
            * if self.interpolation == g::animation::Interpolation::CubicSpline {
                3
            } else {
                1
            }
            + if self.interpolation == g::animation::Interpolation::CubicSpline {
                1
            } else {
                0
            }]
    }
}
fn uri(data: &str, base: Option<&Path>) -> Result<Vec<u8>> {
    if data.starts_with("data:") {
        let (header, body) = data
            .split_once(',')
            .ok_or_else(|| anyhow::anyhow!("Invalid data URI"))?;
        ensure!(
            header.ends_with(";base64") && body.len() <= 90 * 1024 * 1024,
            "3D data URI must be bounded base64"
        );
        let bytes = STANDARD.decode(body)?;
        ensure!(bytes.len() <= MAX_BYTES, "3D data URI exceeds64MiB");
        Ok(bytes)
    } else {
        let mut decoded = Vec::with_capacity(data.len());
        let mut bytes = data.bytes();
        while let Some(byte) = bytes.next() {
            if byte == b'%' {
                let high = bytes.next().and_then(|b| (b as char).to_digit(16));
                let low = bytes.next().and_then(|b| (b as char).to_digit(16));
                let (Some(high), Some(low)) = (high, low) else {
                    anyhow::bail!("Invalid percent-encoded glTF resource URI");
                };
                decoded.push((high * 16 + low) as u8);
            } else {
                decoded.push(byte);
            }
        }
        external(base, std::str::from_utf8(&decoded)?)
    }
}
pub(super) fn parse(data: &[u8], base: Option<&Path>) -> Result<Model3d> {
    ensure!(data.len() <= MAX_BYTES, "glTF exceeds64MiB");
    let doc = g::Gltf::from_slice(data)?;
    ensure!(
        doc.nodes().len() > 0
            && doc.nodes().len() <= 4096
            && doc.buffers().len() <= 256
            && doc.images().len() <= 64
            && doc.textures().len() <= 64
            && doc.animations().len() <= 32,
        "glTF exceeds structural limits"
    );
    let mut total = 0usize;
    let mut buffers = vec![];
    for b in doc.buffers() {
        let bytes = match b.source() {
            g::buffer::Source::Bin => doc
                .blob
                .clone()
                .ok_or_else(|| anyhow::anyhow!("Missing GLB binary data"))?,
            g::buffer::Source::Uri(s) => uri(s, base)?,
        };
        total = total.saturating_add(bytes.len());
        ensure!(
            total <= MAX_BYTES && bytes.len() >= b.length(),
            "Invalid glTF buffer length/budget"
        );
        buffers.push(bytes);
    }
    let mut nodes = doc
        .nodes()
        .map(|n| {
            let (t, r, s) = n.transform().decomposed();
            Node3d {
                name: n.name().unwrap_or("Node").chars().take(256).collect(),
                parent: None,
                transform: Transform3d {
                    translation: t,
                    rotation: r,
                    scale: s,
                },
            }
        })
        .collect::<Vec<_>>();
    for n in doc.nodes() {
        for child in n.children() {
            ensure!(
                nodes[child.index()].parent.is_none(),
                "glTF node has multiple parents"
            );
            nodes[child.index()].parent = Some(n.index());
        }
    }
    // Reject cyclic graphs before recursively finding scene membership.
    for i in 0..nodes.len() {
        let mut p = nodes[i].parent;
        let mut depth = 0;
        while let Some(id) = p {
            depth += 1;
            ensure!(depth <= 256 && id != i, "Cyclic/deep glTF hierarchy");
            p = nodes[id].parent;
        }
    }
    let mut active = HashSet::new();
    if let Some(scene) = doc.default_scene().or_else(|| doc.scenes().next()) {
        let mut stack = scene.nodes().map(|n| n.index()).collect::<Vec<_>>();
        while let Some(i) = stack.pop() {
            if active.insert(i) {
                stack.extend(doc.nodes().nth(i).unwrap().children().map(|n| n.index()));
            }
        }
    } else {
        active.extend(0..nodes.len());
    }
    let mut images = vec![];
    let mut pixels = 0u64;
    for image in doc.images() {
        let bytes = match image.source() {
            g::image::Source::Uri { uri: s, .. } => uri(s, base)?,
            g::image::Source::View { view, .. } => buffers[view.buffer().index()]
                .get(view.offset()..view.offset() + view.length())
                .ok_or_else(|| anyhow::anyhow!("Image buffer view is truncated"))?
                .to_vec(),
        };
        let image = image_texture(&bytes)?;
        pixels += u64::from(image.width) * u64::from(image.height);
        ensure!(pixels <= 16 * 1024 * 1024, "glTF images exceed16MP");
        images.push(image);
    }
    let wrap = |mode: g::texture::WrappingMode| match mode {
        g::texture::WrappingMode::ClampToEdge => Wrap::Clamp,
        g::texture::WrappingMode::MirroredRepeat => Wrap::Mirror,
        _ => Wrap::Repeat,
    };
    let mut textures = vec![];
    pixels = 0;
    for texture in doc.textures() {
        let original = &images[texture.source().index()];
        pixels += u64::from(original.width) * u64::from(original.height);
        ensure!(
            pixels <= 16 * 1024 * 1024,
            "Sampled glTF textures exceed16MP"
        );
        let mut t = original.clone();
        t.wrap_s = wrap(texture.sampler().wrap_s());
        t.wrap_t = wrap(texture.sampler().wrap_t());
        textures.push(t);
    }
    let mut primitives = vec![];
    let mut warnings = vec![];
    let mut vertices_total = 0;
    let mut triangle_total = 0;
    for node in doc.nodes().filter(|n| active.contains(&n.index())) {
        let Some(mesh) = node.mesh() else { continue };
        let skin = node
            .skin()
            .map(|s| {
                let joints = s.joints().map(|n| n.index()).collect::<Vec<_>>();
                ensure!(joints.len() <= 4096, "Too many glTF skin joints");
                let inverse_bind = s
                    .reader(|b| Some(buffers[b.index()].as_slice()))
                    .read_inverse_bind_matrices()
                    .map(|m| {
                        m.map(|v| Mat4::from_cols_array_2d(&v).to_cols_array())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|| vec![Mat4::IDENTITY.to_cols_array(); joints.len()]);
                Ok::<_, anyhow::Error>(Skin3d {
                    joints,
                    inverse_bind,
                })
            })
            .transpose()?;
        for primitive in mesh.primitives() {
            if !matches!(
                primitive.mode(),
                g::mesh::Mode::Triangles
                    | g::mesh::Mode::TriangleStrip
                    | g::mesh::Mode::TriangleFan
            ) {
                warnings.push("Non-triangle glTF primitive skipped".into());
                continue;
            }
            let reader = primitive.reader(|b| Some(buffers[b.index()].as_slice()));
            let positions = reader
                .read_positions()
                .ok_or_else(|| anyhow::anyhow!("Mesh has no POSITION"))?;
            vertices_total += positions.len();
            ensure!(
                vertices_total <= MAX_VERTICES && primitives.len() < 4096,
                "glTF exceeds100000 normalized vertices"
            );
            let positions = positions.collect::<Vec<_>>();
            let normals = reader
                .read_normals()
                .map(|v| v.collect())
                .unwrap_or_default();
            let material = primitive.material();
            let pbr = material.pbr_metallic_roughness();
            let texture = pbr.base_color_texture();
            let coord = texture.as_ref().map_or(0, |t| t.tex_coord());
            let uv = reader
                .read_tex_coords(coord)
                .map(|v| v.into_f32().collect())
                .unwrap_or_default();
            let colors = reader
                .read_colors(0)
                .map(|v| v.into_rgba_f32().collect())
                .unwrap_or_default();
            let material = Material3d {
                color: pbr.base_color_factor(),
                texture: texture.map(|t| t.texture().index()),
                alpha: match material.alpha_mode() {
                    g::material::AlphaMode::Opaque => AlphaMode::Opaque,
                    g::material::AlphaMode::Mask => AlphaMode::Mask,
                    g::material::AlphaMode::Blend => AlphaMode::Blend,
                },
                cutoff: material.alpha_cutoff().unwrap_or(0.5),
            };
            let indices = if let Some(indices) = reader.read_indices() {
                ensure!(
                    primitive
                        .indices()
                        .is_none_or(|a| a.count() <= MAX_TRIANGLES * 3),
                    "glTF index count exceeds budget"
                );
                indices.into_u32().collect::<Vec<_>>()
            } else {
                (0..positions.len() as u32).collect()
            };
            let mut triangles = vec![];
            match primitive.mode() {
                g::mesh::Mode::Triangles => {
                    ensure!(indices.len() % 3 == 0, "Invalid triangle index count");
                    triangles.extend(indices.as_chunks::<3>().0.iter().copied());
                }
                g::mesh::Mode::TriangleStrip => {
                    for i in 2..indices.len() {
                        triangles.push(if i % 2 == 0 {
                            [indices[i - 2], indices[i - 1], indices[i]]
                        } else {
                            [indices[i - 1], indices[i - 2], indices[i]]
                        });
                    }
                }
                _ => {
                    for i in 2..indices.len() {
                        triangles.push([indices[0], indices[i - 1], indices[i]]);
                    }
                }
            }
            triangle_total += triangles.len();
            ensure!(
                triangle_total <= MAX_TRIANGLES,
                "glTF exceeds100000 triangles"
            );
            let (joints, weights) = if skin.is_some() {
                if reader.read_joints(1).is_some() || reader.read_weights(1).is_some() {
                    warnings.push("Skinning limited to the first four vertex influences; additional glTF joint sets ignored".into());
                }
                let joints = reader
                    .read_joints(0)
                    .ok_or_else(|| anyhow::anyhow!("Skinned mesh is missing joint indices"))?
                    .into_u16()
                    .collect::<Vec<_>>();
                let mut weights = reader
                    .read_weights(0)
                    .ok_or_else(|| anyhow::anyhow!("Skinned mesh is missing weights"))?
                    .into_f32()
                    .collect::<Vec<_>>();
                for w in &mut weights {
                    let sum: f32 = w.iter().sum();
                    if sum > 1e-8 {
                        for v in w {
                            *v /= sum;
                        }
                    }
                }
                (joints, weights)
            } else {
                (vec![], vec![])
            };
            if primitive.morph_targets().len() > 0 {
                warnings
                    .push("Morph-target deformation is not supported; base mesh imported".into());
            }
            primitives.push(Primitive3d {
                node: node.index(),
                positions,
                normals,
                uv,
                colors,
                joints,
                weights,
                triangles,
                skin: skin.clone(),
                material,
            });
        }
    }
    let rest = nodes
        .iter()
        .map(|n| n.transform.clone())
        .collect::<Vec<_>>();
    let mut animations = vec![];
    let mut sample_total = 0;
    let mut key_total = 0;
    for animation in doc.animations() {
        let mut curves = vec![];
        let mut seen = HashSet::new();
        for channel in animation.channels() {
            let reader = channel.reader(|b| Some(buffers[b.index()].as_slice()));
            let times = reader
                .read_inputs()
                .ok_or_else(|| anyhow::anyhow!("Missing animation input"))?;
            key_total += times.len();
            ensure!(key_total <= 262144, "glTF animation keys exceed budget");
            let times = times.collect::<Vec<_>>();
            ensure!(
                !times.is_empty()
                    && times.iter().all(|t| t.is_finite() && *t >= 0.)
                    && times.windows(2).all(|p| p[1] > p[0]),
                "Invalid animation times"
            );
            use g::animation::util::ReadOutputs;
            let (kind, values) = match reader
                .read_outputs()
                .ok_or_else(|| anyhow::anyhow!("Missing animation values"))?
            {
                ReadOutputs::Translations(v) => (
                    0,
                    v.map(|v| Vec3::from_array(v).extend(0.))
                        .collect::<Vec<_>>(),
                ),
                ReadOutputs::Rotations(v) => (1, v.into_f32().map(Vec4::from_array).collect()),
                ReadOutputs::Scales(v) => (2, v.map(|v| Vec3::from_array(v).extend(0.)).collect()),
                ReadOutputs::MorphTargetWeights(_) => {
                    warnings.push("Morph-target animation skipped".into());
                    continue;
                }
            };
            let interpolation = channel.sampler().interpolation();
            let factor = if interpolation == g::animation::Interpolation::CubicSpline {
                3
            } else {
                1
            };
            ensure!(
                values.len() == times.len() * factor
                    && values.iter().all(|v| v.to_array().into_iter().all(finite))
                    && seen.insert((channel.target().node().index(), kind)),
                "Invalid/duplicate animation channels"
            );
            if kind == 1 {
                ensure!(
                    values
                        .iter()
                        .enumerate()
                        .all(|(i, v)| factor == 3 && i % 3 != 1 || v.length_squared() > 1e-12),
                    "Invalid quaternion key"
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
        let duration = curves
            .iter()
            .filter_map(|c| c.times.last().copied())
            .fold(0., f32::max);
        ensure!(duration <= 14400., "Animation exceeds4hours");
        let count = if duration > 0. {
            ((duration * 30.).ceil() as usize + 1).clamp(2, 240)
        } else {
            1
        };
        sample_total += count * nodes.len();
        ensure!(
            sample_total <= 262144,
            "glTF animation transform samples exceed budget"
        );
        if duration * 30. > 239. {
            warnings.push("Long animation resampled to240 poses".into());
        }
        let times = (0..count)
            .map(|i| {
                if count > 1 {
                    duration * i as f32 / (count - 1) as f32
                } else {
                    0.
                }
            })
            .collect::<Vec<_>>();
        let mut frames = vec![];
        for t in &times {
            let mut pose = rest.clone();
            for c in &curves {
                let v = c.sample(*t);
                match c.kind {
                    0 => pose[c.node].translation = v.truncate().to_array(),
                    1 => {
                        pose[c.node].rotation =
                            Quat::from_array(v.to_array()).normalize().to_array()
                    }
                    _ => pose[c.node].scale = v.truncate().to_array(),
                }
            }
            frames.push(pose);
        }
        animations.push(Animation3d {
            name: animation
                .name()
                .unwrap_or("Animation")
                .chars()
                .take(256)
                .collect(),
            duration,
            times,
            frames,
        });
    }
    warnings.sort();
    warnings.dedup();
    Ok(Model3d {
        nodes,
        primitives,
        textures,
        animations,
        warnings,
        origin: [0.; 3],
    })
}
