//! Native FBX and OBJ normalization through ufbx. Its filesystem callback is
//! denied: materials/textures are supplied through the bounded local resolver.
use super::*;
use std::collections::HashMap;
fn matrix(m: ufbx::Matrix) -> Mat4 {
    Mat4::from_cols_array(&[
        m.m00 as f32,
        m.m10 as f32,
        m.m20 as f32,
        0.,
        m.m01 as f32,
        m.m11 as f32,
        m.m21 as f32,
        0.,
        m.m02 as f32,
        m.m12 as f32,
        m.m22 as f32,
        0.,
        m.m03 as f32,
        m.m13 as f32,
        m.m23 as f32,
        1.,
    ])
}
fn transform(n: &ufbx::Node) -> Transform3d {
    let m = matrix(n.node_to_parent);
    if m.determinant().abs() < 1e-12 {
        let t = n.local_transform;
        return Transform3d {
            translation: [
                t.translation.x as f32,
                t.translation.y as f32,
                t.translation.z as f32,
            ],
            rotation: [
                t.rotation.x as f32,
                t.rotation.y as f32,
                t.rotation.z as f32,
                t.rotation.w as f32,
            ],
            scale: [t.scale.x as f32, t.scale.y as f32, t.scale.z as f32],
        };
    }
    let (s, r, t) = m.to_scale_rotation_translation();
    Transform3d {
        translation: t.to_array(),
        rotation: r.to_array(),
        scale: s.to_array(),
    }
}
fn texture_image(t: &ufbx::Texture, base: Option<&Path>) -> Result<Texture3d> {
    let bytes = if !t.content.is_empty() {
        t.content.to_vec()
    } else if let Some(video) = &t.video {
        if !video.content.is_empty() {
            video.content.to_vec()
        } else {
            let path = if !t.relative_filename.is_empty() {
                t.relative_filename.as_ref()
            } else {
                t.filename.as_ref()
            };
            external(base, path)?
        }
    } else {
        let path = if !t.relative_filename.is_empty() {
            t.relative_filename.as_ref()
        } else {
            t.filename.as_ref()
        };
        external(base, path)?
    };
    image_texture(&bytes)
}
pub(super) fn parse(data: &[u8], base: Option<&Path>, obj: bool) -> Result<Model3d> {
    ensure!(data.len() <= MAX_BYTES, "FBX/OBJ exceeds64MiB");
    let mut mtl = vec![];
    if obj {
        let text = std::str::from_utf8(data)?;
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("mtllib ") {
                let bytes = external(base, rest.trim().trim_matches('"'))?;
                ensure!(
                    mtl.len() + bytes.len() <= 16 * 1024 * 1024,
                    "OBJ material libraries exceed16MiB"
                );
                mtl.extend(bytes);
                mtl.push(b'\n');
            }
        }
    }
    let deny = |_: &str, _: &ufbx::OpenFileInfo| -> Option<ufbx::Stream> { None };
    let opts = ufbx::LoadOpts {
        file_format: if obj {
            ufbx::FileFormat::Obj
        } else {
            ufbx::FileFormat::Fbx
        },
        load_external_files: false,
        ignore_missing_external_files: true,
        open_file_cb: ufbx::OpenFileCb::Ref(&deny),
        open_main_file_with_default: false,
        obj_search_mtl_by_filename: false,
        obj_mtl_data: mtl.as_slice().into(),
        node_depth_limit: 256,
        generate_missing_normals: true,
        clean_skin_weights: true,
        geometry_transform_handling: ufbx::GeometryTransformHandling::HelperNodes,
        inherit_mode_handling: ufbx::InheritModeHandling::HelperNodes,
        target_unit_meters: 1.,
        target_axes: ufbx::CoordinateAxes {
            right: ufbx::CoordinateAxis::PositiveX,
            up: ufbx::CoordinateAxis::PositiveY,
            front: ufbx::CoordinateAxis::PositiveZ,
        },
        temp_allocator: ufbx::AllocatorOpts {
            memory_limit: 128 * 1024 * 1024,
            ..Default::default()
        },
        result_allocator: ufbx::AllocatorOpts {
            memory_limit: 128 * 1024 * 1024,
            ..Default::default()
        },
        ..Default::default()
    };
    let scene = ufbx::load_memory(data, opts)
        .map_err(|e| anyhow::anyhow!("Native model import failed: {e:?}"))?;
    ensure!(
        !scene.nodes.is_empty() && scene.nodes.len() <= 4096 && scene.anim_stacks.len() <= 32,
        "FBX/OBJ exceeds node/animation budget"
    );
    let nodes = scene
        .nodes
        .iter()
        .map(|n| Node3d {
            name: n.element.name.to_string(),
            parent: n.parent.as_ref().map(|p| p.element.typed_id as usize),
            transform: transform(n),
        })
        .collect::<Vec<_>>();
    let mut textures = vec![];
    let mut texture_lookup = HashMap::new();
    let mut texels = 0u64;
    let mut primitives = vec![];
    let mut warnings = vec![];
    let mut vertex_total = 0;
    let mut triangle_total = 0;
    for node in &scene.nodes {
        let Some(mesh) = &node.mesh else { continue };
        ensure!(
            mesh.num_triangles <= MAX_TRIANGLES
                && mesh.num_indices <= MAX_TRIANGLES * 3
                && mesh.max_face_triangles <= MAX_TRIANGLES,
            "FBX/OBJ mesh exceeds triangulation budget"
        );
        let skin = mesh.skin_deformers.first();
        if skin.is_some_and(|s| s.max_weights_per_vertex > 4) {
            warnings.push("Skinning limited to the four largest vertex influences".into());
        }
        let binding = skin.map(|s| Skin3d {
            joints: s
                .clusters
                .iter()
                .map(|c| {
                    c.bone_node
                        .as_ref()
                        .map_or(usize::MAX, |n| n.element.typed_id as usize)
                })
                .collect(),
            inverse_bind: s
                .clusters
                .iter()
                .map(|c| matrix(c.geometry_to_bone).to_cols_array())
                .collect(),
        });
        if mesh.skin_deformers.len() > 1 {
            warnings.push("Multiple FBX skin deformers: first skin used".into());
        }
        if let Some(s) = skin
            && !matches!(
                s.skinning_method,
                ufbx::SkinningMethod::Linear | ufbx::SkinningMethod::Rigid
            )
        {
            warnings
                .push("Dual-quaternion skinning approximated with linear blend skinning".into());
        }
        let mut groups =
            BTreeMap::<usize, (Primitive3d, HashMap<(u32, [u32; 2], [u32; 3]), u32>)>::new();
        for (face_index, face) in mesh.faces.iter().copied().enumerate() {
            if face.num_indices < 3 {
                continue;
            }
            let material_index = mesh.face_material.get(face_index).copied().unwrap_or(0) as usize;
            if !groups.contains_key(&material_index) {
                ensure!(
                    groups.len() + primitives.len() < 4096,
                    "Too many native material primitives"
                );
                let material = node
                    .materials
                    .get(material_index)
                    .or_else(|| mesh.materials.get(material_index));
                let mut normalized = Material3d::default();
                if let Some(material) = material {
                    let map = if material.pbr.base_color.has_value {
                        &material.pbr.base_color
                    } else {
                        &material.fbx.diffuse_color
                    };
                    if map.has_value {
                        let v = map.value_vec4;
                        normalized.color =
                            [v.x as f32, v.y as f32, v.z as f32, 1.].map(|v| v.clamp(0., 1.));
                    }
                    if material.pbr.opacity.has_value {
                        normalized.color[3] =
                            (material.pbr.opacity.value_vec4.x as f32).clamp(0., 1.);
                    } else if material.fbx.transparency_factor.has_value {
                        normalized.color[3] = (1.
                            - material.fbx.transparency_factor.value_vec4.x as f32)
                            .clamp(0., 1.);
                    }
                    let texture =
                        map.texture
                            .as_ref()
                            .or(material.fbx.diffuse_color.texture.as_ref());
                    if let Some(t) = texture {
                        let id = t.element.typed_id;
                        if let Some(index) = texture_lookup.get(&id) {
                            normalized.texture = Some(*index);
                        } else {
                            ensure!(textures.len() < 64, "Too many model textures");
                            let image = texture_image(t, base)?;
                            texels += u64::from(image.width) * u64::from(image.height);
                            ensure!(texels <= 16 * 1024 * 1024, "FBX/OBJ textures exceed16MP");
                            normalized.texture = Some(textures.len());
                            texture_lookup.insert(id, textures.len());
                            textures.push(image);
                        }
                    }
                }
                if normalized.color[3] < 1.
                    || normalized
                        .texture
                        .is_some_and(|i| textures[i].pixels.chunks_exact(4).any(|p| p[3] < 255))
                {
                    normalized.alpha = AlphaMode::Blend;
                }
                groups.insert(
                    material_index,
                    (
                        Primitive3d {
                            node: node.element.typed_id as usize,
                            positions: vec![],
                            normals: vec![],
                            uv: vec![],
                            colors: vec![],
                            joints: vec![],
                            weights: vec![],
                            triangles: vec![],
                            skin: binding.clone(),
                            material: normalized,
                        },
                        HashMap::new(),
                    ),
                );
            }
            let (p, lookup) = groups.get_mut(&material_index).unwrap();
            let mut triangles = vec![];
            ufbx::triangulate_face_vec(&mut triangles, mesh, face);
            triangle_total += triangles.len() / 3;
            ensure!(
                triangle_total <= MAX_TRIANGLES,
                "Native model exceeds100000 triangles"
            );
            for triangle in triangles.chunks_exact(3) {
                let mut indices = [0; 3];
                for (i, corner) in triangle.iter().copied().enumerate() {
                    let corner = corner as usize;
                    let control = mesh.vertex_indices[corner];
                    let v = mesh.vertex_position[corner];
                    let normal = if mesh.vertex_normal.exists {
                        let n = mesh.vertex_normal[corner];
                        [n.x as f32, n.y as f32, n.z as f32]
                    } else {
                        [0., 0., 1.]
                    };
                    let uv = if mesh.vertex_uv.exists {
                        let uv = mesh.vertex_uv[corner];
                        [uv.x as f32, 1. - uv.y as f32]
                    } else {
                        [0., 0.]
                    };
                    let key = (control, uv.map(f32::to_bits), normal.map(f32::to_bits));
                    if let Some(index) = lookup.get(&key) {
                        indices[i] = *index;
                        continue;
                    }
                    vertex_total += 1;
                    ensure!(
                        vertex_total <= MAX_VERTICES,
                        "Native model exceeds100000 vertices"
                    );
                    let index = p.positions.len() as u32;
                    p.positions.push([v.x as f32, v.y as f32, v.z as f32]);
                    p.normals.push(normal);
                    p.uv.push(uv);
                    if let Some(s) = skin {
                        let vertex = s
                            .vertices
                            .get(control as usize)
                            .ok_or_else(|| anyhow::anyhow!("Missing skin vertex"))?;
                        let begin = vertex.weight_begin as usize;
                        let end = begin
                            .checked_add(vertex.num_weights as usize)
                            .filter(|e| *e <= s.weights.len())
                            .ok_or_else(|| anyhow::anyhow!("Invalid skin weight range"))?;
                        let mut weights = s.weights.as_ref()[begin..end]
                            .iter()
                            .map(|w| (w.cluster_index, w.weight as f32))
                            .collect::<Vec<_>>();
                        weights.sort_by(|a, b| b.1.total_cmp(&a.1));
                        weights.truncate(4);
                        let sum: f32 = weights.iter().map(|w| w.1).sum();
                        let mut joints = [0; 4];
                        let mut values = [0.; 4];
                        for (i, (j, w)) in weights.into_iter().enumerate() {
                            ensure!(j <= u16::MAX as u32, "Skin joint index exceeds budget");
                            joints[i] = j as u16;
                            values[i] = if sum > 1e-8 { w / sum } else { 0. };
                        }
                        p.joints.push(joints);
                        p.weights.push(values);
                    }
                    lookup.insert(key, index);
                    indices[i] = index;
                }
                p.triangles.push(indices);
            }
        }
        primitives.extend(groups.into_values().map(|(p, _)| p));
    }
    let mut animations = vec![];
    let mut sample_total = 0;
    for stack in &scene.anim_stacks {
        let duration = (stack.time_end - stack.time_begin).max(0.) as f32;
        ensure!(
            duration.is_finite() && duration <= 14400.,
            "FBX animation exceeds4hours"
        );
        let count = if duration > 0. {
            ((duration * 30.).ceil() as usize + 1).clamp(2, 240)
        } else {
            1
        };
        sample_total += count * nodes.len();
        ensure!(
            sample_total <= 262144,
            "FBX animation transform samples exceed budget"
        );
        if duration * 30. > 239. {
            warnings.push("Long FBX animation resampled to240 poses".into());
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
            let evaluated = ufbx::evaluate_scene(
                &scene,
                &stack.anim,
                stack.time_begin + f64::from(*t),
                ufbx::EvaluateOpts {
                    load_external_files: false,
                    open_file_cb: ufbx::OpenFileCb::Ref(&deny),
                    temp_allocator: ufbx::AllocatorOpts {
                        memory_limit: 64 * 1024 * 1024,
                        ..Default::default()
                    },
                    result_allocator: ufbx::AllocatorOpts {
                        memory_limit: 128 * 1024 * 1024,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .map_err(|e| anyhow::anyhow!("FBX animation evaluation failed: {e:?}"))?;
            ensure!(
                evaluated.nodes.len() == nodes.len(),
                "FBX animation hierarchy changed"
            );
            frames.push(evaluated.nodes.iter().map(|n| transform(n)).collect());
        }
        animations.push(Animation3d {
            name: stack.element.name.to_string(),
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
