//! Animation-only native FBX loading: ufbx is never allowed to open resources.
use super::*;
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
fn load(bytes: &[u8]) -> Result<ufbx::SceneRoot> {
    ensure!(bytes.len() <= 32 * 1024 * 1024, "FBX motion exceeds32MiB");
    let deny = |_: &str, _: &ufbx::OpenFileInfo| -> Option<ufbx::Stream> { None };
    let scene = ufbx::load_memory(
        bytes,
        ufbx::LoadOpts {
            file_format: ufbx::FileFormat::Fbx,
            ignore_geometry: true,
            load_external_files: false,
            open_file_cb: ufbx::OpenFileCb::Ref(&deny),
            open_main_file_with_default: false,
            ignore_missing_external_files: true,
            node_depth_limit: 64,
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
        },
    )
    .map_err(|e| anyhow::anyhow!("FBX motion import failed: {e:?}"))?;
    ensure!(
        !scene.nodes.is_empty() && scene.nodes.len() <= 512 && scene.anim_stacks.len() <= 256,
        "FBX motion exceeds512nodes/256animations"
    );
    Ok(scene)
}
pub(super) fn names(bytes: &[u8]) -> Result<Vec<String>> {
    let s = load(bytes)?;
    Ok(if s.anim_stacks.is_empty() {
        vec!["Static pose".into()]
    } else {
        s.anim_stacks
            .iter()
            .enumerate()
            .map(|(i, a)| {
                if a.element.name.is_empty() {
                    format!("Animation {}", i + 1)
                } else {
                    a.element.name.to_string()
                }
            })
            .collect()
    })
}
pub(super) fn parse(bytes: &[u8], index: usize) -> Result<Motion> {
    let scene = load(bytes)?;
    ensure!(
        if scene.anim_stacks.is_empty() {
            index == 0
        } else {
            index < scene.anim_stacks.len()
        },
        "Animation index outside FBX animation range"
    );
    let nodes = scene
        .nodes
        .iter()
        .map(|n| Node {
            name: n.element.name.chars().take(1024).collect(),
            parent: n.parent.as_ref().map(|p| p.element.typed_id as usize),
            children: n
                .children
                .iter()
                .map(|c| c.element.typed_id as usize)
                .collect(),
        })
        .collect::<Vec<_>>();
    let rest = scene
        .nodes
        .iter()
        .map(|n| matrix(n.node_to_world))
        .collect::<Vec<_>>();
    let mut warnings = vec![];
    let mut frames = vec![];
    let mut duration = 0.;
    let mut name = "Static pose".to_owned();
    if let Some(stack) = scene.anim_stacks.get(index) {
        duration = (stack.time_end - stack.time_begin) as f32;
        ensure!(
            duration.is_finite() && (0. ..=14400.).contains(&duration),
            "FBX animation duration outside0..4hours"
        );
        name = if stack.element.name.is_empty() {
            format!("Animation {}", index + 1)
        } else {
            stack.element.name.to_string()
        };
        let count = if duration > 0. {
            ((duration * 30.).ceil() as usize + 1).clamp(2, 240)
        } else {
            1
        };
        if duration * 30. > 239. {
            warnings.push("FBX motion resampled to240 poses".into());
        }
        let deny = |_: &str, _: &ufbx::OpenFileInfo| -> Option<ufbx::Stream> { None };
        for i in 0..count {
            let time = stack.time_begin
                + if count > 1 {
                    f64::from(duration) * i as f64 / (count - 1) as f64
                } else {
                    0.
                };
            let evaluated = ufbx::evaluate_scene(
                &scene,
                &stack.anim,
                time,
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
            .map_err(|e| anyhow::anyhow!("FBX animation sampling failed: {e:?}"))?;
            ensure!(
                evaluated.nodes.len() == nodes.len(),
                "FBX hierarchy changed during sampling"
            );
            frames.push(
                evaluated
                    .nodes
                    .iter()
                    .map(|n| matrix(n.node_to_world))
                    .collect(),
            );
        }
    } else {
        frames.push(rest.clone());
    }
    if scene.anim_stacks.len() > 1 {
        warnings.push(format!(
            "Imported FBX animation {} of{}",
            index + 1,
            scene.anim_stacks.len()
        ));
    }
    let fps = if duration > 0. {
        (frames.len() - 1) as f32 / duration
    } else {
        30.
    };
    let motion = Motion {
        nodes,
        frames,
        rest,
        fps,
        name,
        warnings,
    };
    motion.validate_samples()?;
    Ok(motion)
}
