//! Project-aware, transactional clip editing. Temporal edits retain editable
//! source curves and live controllers behind independent, aligned clip windows.
use super::*;
use crate::editing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipEdit {
    Duplicate(usize),
    Delete(usize),
    Move { from: usize, to: usize },
    Trim { index: usize, start: u32, end: u32 },
    Split { index: usize, at: u32 },
    Resize { index: usize, length: u32 },
}

/// Reflect the evaluated skeleton while preserving the project's clip-owned
/// artwork/timelines. Global controllers are bypassed on the resulting FK clip.
pub fn mirror_project_clip(project: &mut Project, index: usize) -> std::result::Result<(), String> {
    project.validate().map_err(|e| e.to_string())?;
    let length = project.rig.clips.get(index).ok_or("No such clip")?.length;
    if (project.rig.bones.len() as u64 * 4 + project.rig.parts.len() as u64 * 2) * u64::from(length)
        > 100_000
    {
        return Err("Mirrored clip exceeds the 100,000-key baked-pose budget".into());
    }
    let mut source = project.clone();
    source.rig.active_clip = index;
    let mut evaluated = source.rig.clone();
    evaluated.active_clip = index;
    evaluated.clips[index].tracks.clear();
    evaluated.clips[index].source = None;
    for frame in 1..=length {
        crate::advanced_motion::bake_pose(
            &mut evaluated,
            &source.complete_pose_at(frame as f32),
            frame as f32,
        )?;
    }
    let reflected = crate::advanced_motion::mirror_clip(&mut evaluated, index)?;
    let mut candidate = project.clone();
    edit_clip(&mut candidate, ClipEdit::Duplicate(index))?;
    let target = candidate.rig.active_clip;
    candidate.rig.clips[target] = evaluated.clips[reflected].clone();
    candidate.rig.clips[target].restore_source_timing(source.rig.clips[index].source.clone())?;
    let axis = candidate.rig.mirror.x;
    for contact in candidate
        .extras
        .production
        .motion
        .contacts
        .iter_mut()
        .filter(|c| c.clip == Some(target))
    {
        contact.lower = candidate
            .rig
            .bone(contact.lower)
            .and_then(|b| b.twin)
            .unwrap_or(contact.lower);
        for key in &mut contact.keys {
            key.target.x = 2. * axis - key.target.x;
            key.bend = -key.bend;
        }
    }
    candidate.extras.production.baked_motion.insert(target);
    candidate.validate().map_err(|e| e.to_string())?;
    *project = candidate;
    Ok(())
}

/// Blend displayed playback endpoints. Props, faces, meshes and scene placement
/// start in the source endpoint's state; event windows are authored separately.
pub fn transition_project_clip(
    project: &mut Project,
    from: usize,
    to: usize,
    frames: u32,
) -> std::result::Result<(), String> {
    project.validate().map_err(|e| e.to_string())?;
    if !(2..=240).contains(&frames) {
        return Err("Transition needs 2–240 frames".into());
    }
    let a = project
        .rig
        .clips
        .get(from)
        .ok_or("Source clip is missing")?;
    let b = project
        .rig
        .clips
        .get(to)
        .ok_or("Destination clip is missing")?;
    let start = a.frame_sequence().last().copied().unwrap_or(1);
    let end = b.frame_sequence().first().copied().unwrap_or(1);
    let mut source = project.clone();
    source.rig.active_clip = from;
    let first = source.complete_pose_at(start as f32);
    source.rig.active_clip = to;
    let last = source.complete_pose_at(end as f32);
    let mut name = format!("{} → {}", a.name, b.name);
    while name.len() > 1024 {
        name.pop();
    }
    let mut candidate = project.clone();
    edit_clip(&mut candidate, ClipEdit::Duplicate(from))?;
    let target = candidate.rig.active_clip;
    edit_clip(
        &mut candidate,
        ClipEdit::Trim {
            index: target,
            start,
            end: start,
        },
    )?;
    edit_clip(
        &mut candidate,
        ClipEdit::Resize {
            index: target,
            length: frames,
        },
    )?;
    candidate.rig.clips[target].name = name;
    candidate.rig.clips[target].looped = false;
    candidate.rig.clips[target].playback = crate::animation::Playback::Forward;
    candidate.rig.clips[target].durations.clear();
    let mut timing = candidate.rig.clips[target].source.clone();
    if let Some(source) = &mut timing {
        source.end = frames;
    }
    hold_scene_at_first_frame(&mut candidate, target);
    candidate.rig.clips[target].tracks.clear();
    candidate.rig.clips[target].source = None;
    for frame in 1..=frames {
        let t = (frame - 1) as f32 / (frames - 1) as f32;
        let pose = crate::advanced_motion::transition_pose(
            &candidate.rig,
            &first,
            &last,
            t * t * (3. - 2. * t),
        );
        crate::advanced_motion::bake_pose(&mut candidate.rig, &pose, frame as f32)?;
    }
    candidate.rig.clips[target].restore_source_timing(timing)?;
    // A transition must not invent repeated source gameplay events or hit spans.
    candidate
        .extras
        .production
        .events
        .retain(|e| e.clip != target);
    candidate
        .extras
        .production
        .boxes
        .retain(|b| b.clip != target);
    candidate.extras.production.baked_motion.insert(target);
    candidate.validate().map_err(|e| e.to_string())?;
    *project = candidate;
    Ok(())
}

/// A generated transition authors new motion from the selected endpoint. Keep
/// that endpoint's artwork editable without exposing its retained future keys.
fn hold_scene_at_first_frame(project: &mut Project, clip: usize) {
    let p = &mut project.extras.production;
    for prop in &mut p.attachments {
        let mut state = prop_state(prop, clip, 1.);
        state.frame = 1.;
        prop.clip_keys.insert(clip, vec![state]);
        if prop.clip == Some(clip) {
            prop.keys.clear();
        }
    }
    for mesh in &mut p.meshes {
        let key = MeshKey {
            frame: 1.,
            offsets: mesh_offsets(mesh, clip, 1.),
        };
        mesh.clip_keys.insert(clip, vec![key]);
        if mesh.clip == clip {
            mesh.keys.clear();
        }
    }
    for face in p.faces.iter_mut().filter(|face| face.clip == clip) {
        face.keys = face
            .keys
            .iter()
            .rev()
            .find(|key| key.frame <= 1.)
            .or_else(|| face.keys.first())
            .cloned()
            .into_iter()
            .map(|mut key| {
                key.frame = 1.;
                key
            })
            .collect();
    }
    for actor in &mut p.actors {
        let key = super::actor_placement(actor, clip, 1.);
        actor.keys.retain(|key| key.clip != clip);
        actor.keys.push(key);
    }
    // These tracks are inactive on generated FK motion, but retain an editable
    // endpoint controller if the user later enables live controls explicitly.
    for contact in p
        .motion
        .contacts
        .iter_mut()
        .filter(|c| c.clip == Some(clip))
    {
        if let Some(mut key) = crate::advanced_motion::contact_at(contact, 1.) {
            key.frame = 1.;
            contact.keys = vec![key];
        } else {
            contact.keys.clear();
        }
    }
}

pub fn prop_keys(prop: &Attachment, clip: usize) -> &[AttachmentKey] {
    prop.clip_keys
        .get(&clip)
        .map(Vec::as_slice)
        .unwrap_or_else(|| {
            if prop.clip.is_none_or(|i| i == clip) {
                &prop.keys
            } else {
                &[]
            }
        })
}
pub fn mesh_keys(mesh: &Mesh, clip: usize) -> &[MeshKey] {
    mesh.clip_keys
        .get(&clip)
        .map(Vec::as_slice)
        .unwrap_or_else(|| if mesh.clip == clip { &mesh.keys } else { &[] })
}
pub(crate) fn prop_state(prop: &Attachment, clip: usize, frame: f32) -> AttachmentKey {
    prop_keys(prop, clip)
        .iter()
        .rev()
        .find(|k| k.frame <= frame)
        .cloned()
        .unwrap_or(AttachmentKey {
            frame,
            bone: prop.bone,
            offset: prop.offset,
            angle: prop.angle,
            asset: prop.asset,
            visible: prop.enabled,
        })
}
fn offsets(keys: &[MeshKey], count: usize, frame: f32) -> Vec<Vec2> {
    let previous = keys.iter().rev().find(|k| k.frame <= frame);
    let next = keys.iter().find(|k| k.frame > frame);
    let alpha = previous.zip(next).map_or(0., |(a, b)| {
        ((frame - a.frame) / (b.frame - a.frame)).clamp(0., 1.)
    });
    (0..count)
        .map(|i| match (previous, next) {
            (Some(a), Some(b)) => {
                a.offsets.get(i).copied().unwrap_or_default() * (1. - alpha)
                    + b.offsets.get(i).copied().unwrap_or_default() * alpha
            }
            (Some(a), None) => a.offsets.get(i).copied().unwrap_or_default(),
            (None, Some(b)) => b.offsets.get(i).copied().unwrap_or_default(),
            _ => Vec2::default(),
        })
        .collect()
}
pub fn mesh_offsets(mesh: &Mesh, clip: usize, frame: f32) -> Vec<Vec2> {
    offsets(mesh_keys(mesh, clip), mesh.vertices.len(), frame)
}
/// Initialize a clip override while preserving the current held prop state.
pub fn prop_keys_mut(prop: &mut Attachment, clip: usize, length: u32) -> &mut Vec<AttachmentKey> {
    if !prop.clip_keys.contains_key(&clip) {
        let mut keys: Vec<_> = prop_keys(prop, clip)
            .iter()
            .filter(|k| k.frame > 1. && k.frame <= length.min(240) as f32)
            .cloned()
            .collect();
        if length > 0 {
            let mut boundary = prop_state(prop, clip, 1.);
            boundary.frame = 1.;
            keys.insert(0, boundary);
        }
        prop.clip_keys.insert(clip, keys);
    }
    prop.clip_keys.get_mut(&clip).unwrap()
}
/// Initialize an independent clip deformation timeline, retaining its boundary.
pub fn mesh_keys_mut(mesh: &mut Mesh, clip: usize, length: u32) -> &mut Vec<MeshKey> {
    if !mesh.clip_keys.contains_key(&clip) {
        let length = length.min(240);
        let source = mesh_keys(mesh, clip);
        let mut keys: Vec<_> = source
            .iter()
            .filter(|k| k.frame > 1. && k.frame <= length as f32)
            .cloned()
            .collect();
        if length > 0 {
            keys.insert(
                0,
                MeshKey {
                    frame: 1.,
                    offsets: mesh_offsets(mesh, clip, 1.),
                },
            );
            if source.iter().any(|k| k.frame > length as f32)
                && keys.last().is_some_and(|k| k.frame < length as f32)
            {
                keys.push(MeshKey {
                    frame: length as f32,
                    offsets: mesh_offsets(mesh, clip, length as f32),
                });
            }
        }
        mesh.clip_keys.insert(clip, keys);
    }
    mesh.clip_keys.get_mut(&clip).unwrap()
}
fn mapping_insert(count: usize, index: usize) -> Vec<Option<usize>> {
    (0..count)
        .map(|i| Some(if i > index { i + 1 } else { i }))
        .collect()
}
pub(crate) fn remap_metadata(p: &mut Production, map: &[Option<usize>]) {
    let mapped = |i: usize| map.get(i).copied().flatten();
    for a in &mut p.attachments {
        if let Some(old) = a.clip {
            if let Some(new) = mapped(old) {
                a.clip = Some(new);
            } else {
                a.clip = None;
                a.keys.clear();
            }
        }
        a.clip_keys = std::mem::take(&mut a.clip_keys)
            .into_iter()
            .filter_map(|(i, keys)| mapped(i).map(|i| (i, keys)))
            .collect();
    }
    for m in &mut p.meshes {
        if let Some(new) = mapped(m.clip) {
            m.clip = new;
        } else {
            m.clip = 0;
            m.keys.clear();
        }
        m.clip_keys = std::mem::take(&mut m.clip_keys)
            .into_iter()
            .filter_map(|(i, keys)| mapped(i).map(|i| (i, keys)))
            .collect();
    }
    for a in &mut p.actors {
        a.keys.retain_mut(|k| {
            if let Some(i) = mapped(k.clip) {
                k.clip = i;
                true
            } else {
                false
            }
        });
        a.clip_time_ms = std::mem::take(&mut a.clip_time_ms)
            .into_iter()
            .filter_map(|(i, ms)| mapped(i).map(|i| (i, ms)))
            .collect();
    }
    p.faces.retain_mut(|v| {
        if let Some(i) = mapped(v.clip) {
            v.clip = i;
            true
        } else {
            false
        }
    });
    p.boxes.retain_mut(|v| {
        if let Some(i) = mapped(v.clip) {
            v.clip = i;
            true
        } else {
            false
        }
    });
    p.events.retain_mut(|v| {
        if let Some(i) = mapped(v.clip) {
            v.clip = i;
            true
        } else {
            false
        }
    });
    p.corrections.retain_mut(|v| {
        if let Some(i) = mapped(v.clip) {
            v.clip = i;
            true
        } else {
            false
        }
    });
    p.motion.remap_clips(map);
    p.baked_motion = std::mem::take(&mut p.baked_motion)
        .into_iter()
        .filter_map(mapped)
        .collect();
}

pub fn edit_clip(project: &mut Project, operation: ClipEdit) -> std::result::Result<(), String> {
    project.validate().map_err(|e| e.to_string())?;
    let source = project.clone();
    let mut candidate = source.clone();
    let count = source.rig.clips.len();
    match operation {
        ClipEdit::Duplicate(index) => {
            let new = editing::duplicate_clip(&mut candidate.rig, index)?;
            candidate
                .extras
                .production
                .remap_clips(&mapping_insert(count, index));
            copy_range(
                &source,
                &mut candidate,
                index,
                new,
                1,
                source.rig.clips[index].length,
                true,
            )?;
            if source.extras.production.baked_motion.contains(&index) {
                candidate.extras.production.baked_motion.insert(new);
            }
        }
        ClipEdit::Delete(index) => {
            editing::delete_clip(&mut candidate.rig, index)?;
            let map = (0..count)
                .map(|i| {
                    if i == index {
                        None
                    } else {
                        Some(if i > index { i - 1 } else { i })
                    }
                })
                .collect::<Vec<_>>();
            candidate.extras.production.remap_clips(&map);
        }
        ClipEdit::Move { from, to } => {
            editing::move_clip(&mut candidate.rig, from, to)?;
            let mut order = (0..count).collect::<Vec<_>>();
            let value = order.remove(from);
            order.insert(to, value);
            let map = (0..count)
                .map(|i| order.iter().position(|old| *old == i))
                .collect::<Vec<_>>();
            candidate.extras.production.remap_clips(&map);
        }
        ClipEdit::Trim { index, start, end } => {
            editing::trim_clip(&mut candidate.rig, index, start, end)?;
            copy_range(&source, &mut candidate, index, index, start, end, false)?;
        }
        ClipEdit::Split { index, at } => {
            let new = editing::split_clip(&mut candidate.rig, index, at)?;
            candidate
                .extras
                .production
                .remap_clips(&mapping_insert(count, index));
            copy_range(&source, &mut candidate, index, index, 1, at - 1, false)?;
            copy_range(
                &source,
                &mut candidate,
                index,
                new,
                at,
                source.rig.clips[index].length,
                true,
            )?;
            if source.extras.production.baked_motion.contains(&index) {
                candidate.extras.production.baked_motion.insert(new);
            }
        }
        ClipEdit::Resize { index, length } => {
            if !(1..=240).contains(&length) {
                return Err("Clip length must be 1–240".into());
            }
            let old = source.rig.clips.get(index).ok_or("No such clip")?.length;
            if length == old {
                return Ok(());
            }
            candidate.rig.clips[index] = source.rig.clips[index].resized(length)?;
            copy_range(&source, &mut candidate, index, index, 1, length, false)?;
        }
    }
    edit_character_clips(&source, &mut candidate, operation);
    candidate.validate().map_err(|e| e.to_string())?;
    *project = candidate;
    Ok(())
}
fn edit_character_clips(source: &Project, target: &mut Project, operation: ClipEdit) {
    let count = source.rig.clips.len();
    let mut ranges: Vec<(usize, usize, u32)> = (0..count).map(|i| (i, i, 1)).collect();
    match operation {
        ClipEdit::Duplicate(index) => {
            for (new, _, _) in &mut ranges {
                if *new > index {
                    *new += 1;
                }
            }
            ranges.push((index + 1, index, 1));
        }
        ClipEdit::Delete(index) => {
            ranges.retain(|(_, old, _)| *old != index);
            for (new, _, _) in &mut ranges {
                if *new > index {
                    *new -= 1;
                }
            }
        }
        ClipEdit::Move { from, to } => {
            let mut order: Vec<_> = (0..count).collect();
            let old = order.remove(from);
            order.insert(to, old);
            ranges = order
                .into_iter()
                .enumerate()
                .map(|(new, old)| (new, old, 1))
                .collect();
        }
        ClipEdit::Trim { index, start, .. } => ranges[index].2 = start,
        ClipEdit::Split { index, at } => {
            for (new, _, _) in &mut ranges {
                if *new > index {
                    *new += 1;
                }
            }
            ranges.push((index + 1, index, at));
        }
        ClipEdit::Resize { .. } => {}
    }
    for c in &mut target.extras.characters3d {
        if let Some(old) = source.extras.characters3d.iter().find(|v| v.id == c.id) {
            c.clips = ranges
                .iter()
                .map(|&(new, index, start)| {
                    let mut b = old.playback(index);
                    let frame = source.rig.clips[index].evaluation_frame(start as f32);
                    let whole = frame.floor() as u32;
                    b.offset_ms += (1..whole)
                        .map(|f| u64::from(source.rig.clips[index].duration_ms(f)))
                        .sum::<u64>() as f64
                        * f64::from(b.speed);
                    (new, b)
                })
                .collect();
        }
    }
}
fn fresh_id(p: &Production) -> std::result::Result<u64, String> {
    let id = p.next_id();
    if id == u64::MAX {
        Err("Production ID limit reached".into())
    } else {
        Ok(id)
    }
}
/// Copy retained sparse timelines into an independent clip window. Artwork is
/// shared; animation keys remain editable at their shifted local coordinates,
/// including hidden keys needed for interpolation and source-history replay.
fn copy_range(
    source: &Project,
    target: &mut Project,
    from: usize,
    to: usize,
    start: u32,
    end: u32,
    fresh: bool,
) -> std::result::Result<(), String> {
    let old = &source.extras.production;
    let clip = &source.rig.clips[from];
    let left = clip.evaluation_frame(start as f32);
    let right = clip.evaluation_frame(end as f32);
    let shift = left - 1.;
    let (first, last) = clip.source_bounds();
    let out = &mut target.extras.production;
    for original in &old.attachments {
        if let Some(prop) = out.attachments.iter_mut().find(|p| p.id == original.id) {
            let keys = prop_keys(original, from)
                .iter()
                .filter(|k| k.frame >= first && k.frame <= last)
                .cloned()
                .map(|mut k| {
                    k.frame -= shift;
                    k
                })
                .collect();
            prop.clip_keys.insert(to, keys);
            if prop.clip == Some(to) {
                prop.keys.clear();
            }
        }
    }
    for original in &old.meshes {
        if let Some(mesh) = out.meshes.iter_mut().find(|m| m.id == original.id) {
            let keys = mesh_keys(original, from)
                .iter()
                .cloned()
                .map(|mut k| {
                    k.frame -= shift;
                    k
                })
                .collect();
            mesh.clip_keys.insert(to, keys);
            if mesh.clip == to {
                mesh.keys.clear();
            }
        }
    }
    out.faces.retain(|f| f.clip != to);
    for original in old.faces.iter().filter(|f| f.clip == from) {
        let mut face = original.clone();
        face.clip = to;
        if fresh {
            face.id = fresh_id(out)?;
        }
        for key in &mut face.keys {
            key.frame -= shift;
        }
        out.faces.push(face);
    }
    out.boxes.retain(|b| b.clip != to);
    for original in old.boxes.iter().filter(|b| b.clip == from) {
        let left = original.start.max(left as u32);
        let right = original.end.min(right as u32);
        if left > right {
            continue;
        }
        let mut track = original.clone();
        track.clip = to;
        if fresh {
            track.id = fresh_id(out)?;
        }
        track.start = left - shift as u32;
        track.end = right - shift as u32;
        for key in &mut track.keys {
            key.frame -= shift;
        }
        out.boxes.push(track);
    }
    // Events are authored in the visible sequence: extending a hold must not
    // invent another occurrence of the source endpoint's gameplay event.
    out.events.retain(|e| e.clip != to);
    for original in old
        .events
        .iter()
        .filter(|e| e.clip == from && e.frame >= start as f32 && e.frame <= end as f32)
    {
        let mut e = original.clone();
        e.clip = to;
        e.frame -= start as f32 - 1.;
        out.events.push(e);
    }
    out.corrections.retain(|c| c.clip != to);
    for original in old
        .corrections
        .iter()
        .filter(|c| c.clip == from && c.frame as f32 >= left && c.frame as f32 <= right)
    {
        let mut c = original.clone();
        c.clip = to;
        c.frame -= shift as u32;
        out.corrections.push(c);
    }
    out.motion.contacts.retain(|c| c.clip != Some(to));
    for original in old.motion.contacts.iter().filter(|c| c.clip == Some(from)) {
        let mut c = original.clone();
        c.clip = Some(to);
        for key in &mut c.keys {
            key.frame -= shift;
        }
        out.motion.contacts.push(c);
    }
    let mut rig = source.rig.clone();
    rig.active_clip = from;
    let prefix = super::timeline_ms(&rig, left) as f64;
    for original in &old.actors {
        if let Some(actor) = out.actors.iter_mut().find(|a| a.id == original.id) {
            actor.keys.retain(|k| k.clip != to);
            actor.keys.extend(
                original
                    .keys
                    .iter()
                    .filter(|k| k.clip == from)
                    .cloned()
                    .map(|mut k| {
                        k.clip = to;
                        k.frame -= shift;
                        k
                    }),
            );
            actor.clip_time_ms.insert(
                to,
                original.clip_time_ms.get(&from).copied().unwrap_or(0.) + prefix,
            );
        }
    }
    Ok(())
}
