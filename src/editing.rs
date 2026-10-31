//! Transactional native rig, timeline and clip editing operations.
use crate::{
    animation::{Channel, Clip, Ease, Keyframe},
    model::{Rig, Vec2},
};
use std::collections::HashSet;

fn commit(rig: &mut Rig, candidate: Rig) -> Result<(), String> {
    candidate.validate()?;
    *rig = candidate;
    Ok(())
}
fn selected_roots(rig: &Rig, ids: &[u32]) -> Result<Vec<u32>, String> {
    let mut selected = HashSet::new();
    for id in ids {
        let b = rig.bone(*id).ok_or("Selected bone is missing")?;
        if !b.locked {
            selected.insert(*id);
        }
    }
    let mut roots = Vec::new();
    for b in &rig.bones {
        if !selected.contains(&b.id) {
            continue;
        }
        let mut parent = b.parent;
        let mut covered = false;
        while let Some(id) = parent {
            if selected.contains(&id) {
                covered = true;
                break;
            }
            parent = rig.bone(id).and_then(|b| b.parent);
        }
        if !covered {
            roots.push(b.id);
        }
    }
    if roots.is_empty() {
        return Err("Select at least one unlocked bone".into());
    }
    Ok(roots)
}
fn parameters(delta: Vec2, rotation: f32, pivot: Vec2) -> Result<(), String> {
    if !delta.is_finite() || !rotation.is_finite() || !pivot.is_finite() {
        Err("Transform must be finite".into())
    } else {
        Ok(())
    }
}

/// Reparents the rest skeleton without changing its visible rest-space transforms.
/// A connection is possible only when the head already meets the new parent's tail.
pub fn reparent(rig: &mut Rig, id: u32, parent: Option<u32>, connect: bool) -> Result<(), String> {
    rig.validate()?;
    let bone = rig.bone(id).ok_or("No such bone")?;
    if bone.locked {
        return Err("Bone is locked".into());
    }
    if connect && parent.is_none() {
        return Err("A connected bone needs a parent".into());
    }
    let mut ancestor = parent;
    while let Some(p) = ancestor {
        if p == id {
            return Err("Reparenting would create a cycle".into());
        }
        ancestor = rig.bone(p).ok_or("No such parent")?.parent;
    }
    let world = rig.world(None);
    let original = world.iter().find(|w| w.id == id).ok_or("No such bone")?;
    let p = parent.and_then(|id| world.iter().find(|w| w.id == id));
    if connect && p.is_some_and(|p| (p.tail - original.head).length() > 0.001) {
        return Err("Head must meet the parent tail before connecting".into());
    }
    let local = p.map_or(original.head, |p| (original.head - p.head).rotate(-p.angle));
    let mut candidate = rig.clone();
    let b = candidate.bone_mut(id).unwrap();
    b.parent = parent;
    b.connected = connect;
    b.x = local.x;
    b.y = local.y;
    b.angle = crate::animation::wrap_degrees(original.angle - p.map_or(0., |p| p.angle));
    commit(rig, candidate)
}

/// Rigidly transforms top-level selected subtrees; connected heads detach only if moved.
pub fn transform_rest(
    rig: &mut Rig,
    ids: &[u32],
    translation: Vec2,
    rotation: f32,
    pivot: Vec2,
) -> Result<(), String> {
    rig.validate()?;
    parameters(translation, rotation, pivot)?;
    let selected = selected_roots(rig, ids)?;
    let world = rig.world(None);
    let mut candidate = rig.clone();
    for id in selected {
        let w = world.iter().find(|w| w.id == id).unwrap();
        let b = rig.bone(id).unwrap();
        let p = b.parent.and_then(|id| world.iter().find(|w| w.id == id));
        let head = pivot + (w.head - pivot).rotate(rotation) + translation;
        let local = p.map_or(head, |p| (head - p.head).rotate(-p.angle));
        let out = candidate.bone_mut(id).unwrap();
        if (head - w.head).length() > 0.001 {
            out.connected = false;
        }
        out.x = local.x;
        out.y = local.y;
        out.angle = crate::animation::wrap_degrees(w.angle + rotation - p.map_or(0., |p| p.angle));
    }
    commit(rig, candidate)
}

/// Connected subtrees rotate at their joints. Loose subtrees can also translate.
pub fn transform_pose(
    rig: &mut Rig,
    ids: &[u32],
    translation: Vec2,
    rotation: f32,
    pivot: Vec2,
    frame: f32,
) -> Result<(), String> {
    rig.validate()?;
    parameters(translation, rotation, pivot)?;
    let selected = selected_roots(rig, ids)?;
    let pose = rig.active_clip().sample(frame);
    let world = rig.world(Some(&pose));
    let mut candidate = rig.clone();
    for id in selected {
        let b = rig.bone(id).unwrap();
        let w = world.iter().find(|w| w.id == id).unwrap();
        let p = b.parent.and_then(|id| world.iter().find(|w| w.id == id));
        if b.connected && translation.length() > 0.00001 {
            return Err(
                "Connected joints cannot translate; move their parent or detach first".into(),
            );
        }
        let head = if b.connected {
            w.head
        } else {
            pivot + (w.head - pivot).rotate(rotation) + translation
        };
        let local = p.map_or(head, |p| (head - p.head).rotate(-p.angle));
        if !b.connected && (head - w.head).length() > 0.00001 {
            candidate
                .active_clip_mut()
                .set_key(id, Channel::X, frame, local.x - b.x)?;
            candidate
                .active_clip_mut()
                .set_key(id, Channel::Y, frame, local.y - b.y)?;
        }
        let value = b.clamp_rotation(w.angle + rotation - p.map_or(0., |p| p.angle) - b.angle);
        candidate
            .active_clip_mut()
            .set_key(id, Channel::Rotation, frame, value)?;
    }
    commit(rig, candidate)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeySelection {
    pub bone: u32,
    pub channel: Channel,
    pub frame: f32,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyClipboard {
    pub keys: Vec<(u32, Channel, Keyframe)>,
    pub start: f32,
}
fn key_selected(selected: &[KeySelection], bone: u32, channel: Channel, frame: f32) -> bool {
    selected
        .iter()
        .any(|s| s.bone == bone && s.channel == channel && s.frame == frame)
}
pub fn copy_keys(clip: &Clip, selected: &[KeySelection]) -> KeyClipboard {
    let keys: Vec<_> = clip
        .tracks
        .iter()
        .flat_map(|t| {
            t.keys
                .iter()
                .filter(|k| key_selected(selected, t.bone, t.channel, k.frame))
                .map(|k| (t.bone, t.channel, k.clone()))
        })
        .collect();
    let start = keys
        .iter()
        .map(|(_, _, k)| k.frame)
        .reduce(f32::min)
        .unwrap_or(1.);
    KeyClipboard { keys, start }
}
fn validate_clip(clip: &Clip) -> Result<(), String> {
    let owners = clip.tracks.iter().map(|t| t.bone).collect();
    clip.validate(&owners)
}
fn insert_key(clip: &mut Clip, bone: u32, channel: Channel, key: Keyframe) -> Result<(), String> {
    clip.set_key(bone, channel, key.frame, key.value)?;
    let t = clip
        .tracks
        .iter_mut()
        .find(|t| t.bone == bone && t.channel == channel)
        .unwrap();
    let frame = key.frame;
    *t.keys.iter_mut().find(|k| k.frame == frame).unwrap() = key;
    Ok(())
}
pub fn paste_keys(clip: &mut Clip, clipboard: &KeyClipboard, at: f32) -> Result<(), String> {
    if !at.is_finite() || !clipboard.start.is_finite() {
        return Err("Invalid clipboard time".into());
    }
    let mut candidate = clip.clone();
    for (bone, ch, key) in &clipboard.keys {
        let mut key = key.clone();
        key.frame = key.frame - clipboard.start + at;
        insert_key(&mut candidate, *bone, *ch, key)?;
    }
    validate_clip(&candidate)?;
    *clip = candidate;
    Ok(())
}
pub fn delete_keys(clip: &mut Clip, selected: &[KeySelection]) -> usize {
    let mut count = 0;
    for t in &mut clip.tracks {
        let before = t.keys.len();
        t.keys
            .retain(|k| !key_selected(selected, t.bone, t.channel, k.frame));
        count += before - t.keys.len();
    }
    clip.tracks.retain(|t| !t.keys.is_empty());
    count
}
fn map_keys(
    clip: &mut Clip,
    selected: &[KeySelection],
    map: impl Fn(f32) -> f32,
    tangent_scale: f32,
) -> Result<(), String> {
    let copied = copy_keys(clip, selected);
    let mut candidate = clip.clone();
    delete_keys(&mut candidate, selected);
    for (bone, ch, mut key) in copied.keys {
        key.frame = map(key.frame);
        key.tangent_in /= tangent_scale;
        key.tangent_out /= tangent_scale;
        insert_key(&mut candidate, bone, ch, key)?;
    }
    validate_clip(&candidate)?;
    *clip = candidate;
    Ok(())
}
pub fn retime_keys(
    clip: &mut Clip,
    selected: &[KeySelection],
    offset: f32,
    scale: f32,
    pivot: f32,
) -> Result<(), String> {
    if !offset.is_finite() || !scale.is_finite() || scale <= 0. || !pivot.is_finite() {
        return Err("Invalid retiming transform".into());
    }
    map_keys(
        clip,
        selected,
        |f| (f - pivot) * scale + pivot + offset,
        scale,
    )
}
pub fn snap_keys(clip: &mut Clip, selected: &[KeySelection], step: f32) -> Result<(), String> {
    if !step.is_finite() || step <= 0. {
        return Err("Snap interval must be positive".into());
    }
    map_keys(
        clip,
        selected,
        |f| ((f - 1.) / step).round() * step + 1.,
        1.,
    )
}

fn unique_clip_name(rig: &Rig, base: &str) -> String {
    let mut suffix = 1;
    loop {
        let tail = if suffix == 1 {
            " copy".to_string()
        } else {
            format!(" copy {suffix}")
        };
        let mut name = base.to_owned();
        while name.len() + tail.len() > 1024 {
            name.pop();
        }
        name.push_str(&tail);
        if !rig.clips.iter().any(|c| c.name == name) {
            return name;
        }
        suffix += 1;
    }
}
pub fn duplicate_clip(rig: &mut Rig, index: usize) -> Result<usize, String> {
    let mut candidate = rig.clone();
    let mut clip = candidate.clips.get(index).ok_or("No such clip")?.clone();
    if candidate.clips.len() >= 32 {
        return Err("Clip limit reached".into());
    }
    clip.name = unique_clip_name(rig, &clip.name);
    candidate.clips.insert(index + 1, clip);
    candidate.active_clip = index + 1;
    commit(rig, candidate)?;
    Ok(index + 1)
}
pub fn delete_clip(rig: &mut Rig, index: usize) -> Result<(), String> {
    if rig.clips.len() <= 1 {
        return Err("Keep at least one clip".into());
    }
    if index >= rig.clips.len() {
        return Err("No such clip".into());
    }
    let mut candidate = rig.clone();
    candidate.clips.remove(index);
    candidate.active_clip = if candidate.active_clip > index {
        candidate.active_clip - 1
    } else if candidate.active_clip == index {
        index.min(candidate.clips.len() - 1)
    } else {
        candidate.active_clip
    };
    commit(rig, candidate)
}
pub fn move_clip(rig: &mut Rig, from: usize, to: usize) -> Result<(), String> {
    if from >= rig.clips.len() || to >= rig.clips.len() {
        return Err("No such clip".into());
    }
    let mut candidate = rig.clone();
    let clip = candidate.clips.remove(from);
    candidate.clips.insert(to, clip);
    candidate.active_clip = if rig.active_clip == from {
        to
    } else if from < to && rig.active_clip > from && rig.active_clip <= to {
        rig.active_clip - 1
    } else if to < from && rig.active_clip >= to && rig.active_clip < from {
        rig.active_clip + 1
    } else {
        rig.active_clip
    };
    commit(rig, candidate)
}
fn trimmed(clip: &Clip, start: u32, end: u32) -> Result<Clip, String> {
    if start == 0 || end < start || end > clip.length {
        return Err("Invalid trim range".into());
    }
    let mut result = clip.clone();
    result.length = end - start + 1;
    for (target, source) in result.tracks.iter_mut().zip(&clip.tracks) {
        target.keys = (start..=end)
            .map(|f| Keyframe {
                frame: (f - start + 1) as f32,
                value: clip.value(source, f as f32),
                easing: Ease::Linear,
                tangent_in: 0.,
                tangent_out: 0.,
            })
            .collect();
    }
    result.durations = clip
        .durations
        .iter()
        .filter(|(f, _)| **f >= start && **f <= end)
        .map(|(f, ms)| (*f - start + 1, *ms))
        .collect();
    Ok(result)
}
/// Frame-exact trim: channel values are baked at each retained frame.
pub fn trim_clip(rig: &mut Rig, index: usize, start: u32, end: u32) -> Result<(), String> {
    let mut candidate = rig.clone();
    let clip = candidate.clips.get(index).ok_or("No such clip")?;
    candidate.clips[index] = trimmed(clip, start, end)?;
    commit(rig, candidate)
}
pub fn split_clip(rig: &mut Rig, index: usize, at: u32) -> Result<usize, String> {
    let source = rig.clips.get(index).ok_or("No such clip")?;
    if at < 2 || at > source.length || rig.clips.len() >= 32 {
        return Err("Split must leave two nonempty clips within the clip limit".into());
    }
    let mut candidate = rig.clone();
    let mut right = trimmed(source, at, source.length)?;
    right.name = unique_clip_name(rig, &source.name);
    candidate.clips[index] = trimmed(source, 1, at - 1)?;
    candidate.clips.insert(index + 1, right);
    candidate.active_clip = index + 1;
    commit(rig, candidate)?;
    Ok(index + 1)
}
pub fn root_path(rig: &Rig, clip_index: usize, root_id: u32) -> Result<Vec<Vec2>, String> {
    rig.validate()?;
    if rig.bone(root_id).is_none_or(|b| b.parent.is_some()) {
        return Err("Select a rig root".into());
    }
    let clip = rig.clips.get(clip_index).ok_or("No such clip")?;
    let bone = rig.bone(root_id).unwrap();
    Ok((1..=clip.length)
        .map(|f| {
            let pose = clip.sample(f as f32);
            let p = pose.get(&root_id).copied().unwrap_or_default();
            Vec2::new(bone.x + p.x, bone.y + p.y)
        })
        .collect())
}
pub fn make_in_place(rig: &mut Rig, clip_index: usize) -> Result<(), String> {
    let mut candidate = rig.clone();
    let roots: Vec<_> = rig
        .bones
        .iter()
        .filter(|b| b.parent.is_none())
        .map(|b| b.id)
        .collect();
    let pose = rig.clips.get(clip_index).ok_or("No such clip")?.sample(1.);
    let clip = &mut candidate.clips[clip_index];
    clip.tracks
        .retain(|t| !roots.contains(&t.bone) || !matches!(t.channel, Channel::X | Channel::Y));
    for id in roots {
        let p = pose.get(&id).copied().unwrap_or_default();
        clip.set_key(id, Channel::X, 1., p.x)?;
        clip.set_key(id, Channel::Y, 1., p.y)?;
    }
    commit(rig, candidate)
}
