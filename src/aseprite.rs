//! Standalone layered RGBA writer for the official Aseprite file format.
//! Specification: https://github.com/aseprite/aseprite/blob/main/docs/ase-file-specs.md
use crate::{
    model::Rig,
    project::{self, Project},
    skin,
};
use anyhow::{Result, ensure};
use flate2::{Compression, write::ZlibEncoder};
use std::io::Write;

const MAX_LAYER_PIXELS: u64 = 16 * 1024 * 1024;
const MAX_WORK: u64 = 128 * 1024 * 1024;

/// Exports the active clip, retaining one editable layer per skin group.
/// Visibility is represented by empty cels and animated z by per-cel ordering.
pub fn encode(rig: &Rig) -> Result<Vec<u8>> {
    encode_indices(rig, &[rig.active_clip])
}
pub fn encode_all(rig: &Rig) -> Result<Vec<u8>> {
    encode_indices(rig, &(0..rig.clips.len()).collect::<Vec<_>>())
}
/// Export the displayed project. Pure skin projects retain editable group
/// layers; artwork projects retain the exact composited scene as an RGBA layer.
pub fn encode_project(project: &Project, all_clips: bool) -> Result<Vec<u8>> {
    project.validate()?;
    let mut prepared = project.clone();
    crate::canvas::grow_all(&mut prepared)?;
    let project = &prepared;
    project.validate()?;
    let indices: Vec<_> = if all_clips {
        (0..project.rig.clips.len()).collect()
    } else {
        vec![project.rig.active_clip]
    };
    let frames: u64 = indices
        .iter()
        .map(|i| u64::from(project.rig.clips[*i].length))
        .sum();
    project::project_export_budget(project, frames)?;
    if !project.extras.production.has_art()
        && project.extras.characters3d.is_empty()
        && project.extras.show_2d
    {
        let mut rig = project.rig.clone();
        for index in &indices {
            let mut p = project.clone();
            p.rig.active_clip = *index;
            let baked = p.baked_rig()?;
            rig.clips[*index] = baked.clips[*index].clone();
        }
        for root in rig.bones.iter_mut().filter(|b| b.parent.is_none()) {
            root.x -= project.extras.canvas_origin.x;
            root.y -= project.extras.canvas_origin.y;
        }
        return encode_indices(&rig, &indices);
    }
    let rig = &project.rig;
    ensure!(
        u64::from(rig.width) * u64::from(rig.height) * frames <= MAX_LAYER_PIXELS,
        "Aseprite scene exceeds 16 megapixel export budget"
    );
    let mut out = vec![0u8; 128];
    put_u16(&mut out, 4, 0xA5E0);
    put_u16(&mut out, 6, frames as u16);
    put_u16(&mut out, 8, rig.width as u16);
    put_u16(&mut out, 10, rig.height as u16);
    put_u16(&mut out, 12, 32);
    put_u32(&mut out, 14, 1);
    put_u16(&mut out, 18, rig.clips[indices[0]].duration_ms(1) as u16);
    out[34] = 1;
    out[35] = 1;
    let mut p = project.clone();
    let mut first = true;
    for index in &indices {
        p.rig.active_clip = *index;
        for frame in 1..=rig.clips[*index].length {
            let frame_start = out.len();
            out.resize(frame_start + 16, 0);
            let mut chunks = 0;
            if first {
                let start = begin_chunk(&mut out, 0x2004);
                word(&mut out, 3);
                word(&mut out, 0);
                word(&mut out, 0);
                word(&mut out, 0);
                word(&mut out, 0);
                word(&mut out, 0);
                out.push(255);
                out.extend_from_slice(&[0; 3]);
                let name = "Composited scene";
                word(&mut out, name.len() as u16);
                out.extend_from_slice(name.as_bytes());
                end_chunk(&mut out, start);
                chunks += 1;
                append_tags(&mut out, rig, &indices);
                chunks += 1;
                first = false;
            }
            let image = p.try_render_at(frame as f32)?;
            if image.pixels().any(|p| p[3] != 0) {
                let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
                encoder.write_all(image.as_raw())?;
                let compressed = encoder.finish()?;
                let start = begin_chunk(&mut out, 0x2005);
                word(&mut out, 0);
                word(&mut out, 0);
                word(&mut out, 0);
                out.push(255);
                word(&mut out, 2);
                word(&mut out, 0);
                out.extend_from_slice(&[0; 5]);
                word(&mut out, rig.width as u16);
                word(&mut out, rig.height as u16);
                out.extend_from_slice(&compressed);
                end_chunk(&mut out, start);
                chunks += 1;
            }
            let bytes = (out.len() - frame_start) as u32;
            put_u32(&mut out, frame_start, bytes);
            put_u16(&mut out, frame_start + 4, 0xF1FA);
            put_u16(&mut out, frame_start + 6, chunks as u16);
            put_u16(
                &mut out,
                frame_start + 8,
                rig.clips[*index].duration_ms(frame) as u16,
            );
            put_u32(&mut out, frame_start + 12, chunks);
        }
    }
    let bytes = out.len() as u32;
    put_u32(&mut out, 0, bytes);
    Ok(out)
}
fn append_tags(out: &mut Vec<u8>, rig: &Rig, indices: &[usize]) {
    let start = begin_chunk(out, 0x2018);
    word(out, indices.len() as u16);
    out.extend_from_slice(&[0; 8]);
    let mut from = 0u16;
    for index in indices {
        let clip = &rig.clips[*index];
        let to = from + clip.length as u16 - 1;
        word(out, from);
        word(out, to);
        out.push(match clip.playback {
            crate::animation::Playback::Forward => 0,
            crate::animation::Playback::Reverse => 1,
            crate::animation::Playback::PingPong => 2,
        });
        word(out, if clip.looped { 0 } else { 1 });
        out.extend_from_slice(&[0; 6]);
        out.extend_from_slice(&[119, 225, 187, 0]);
        word(out, clip.name.len() as u16);
        out.extend_from_slice(clip.name.as_bytes());
        from = to + 1;
    }
    end_chunk(out, start);
}
fn encode_indices(rig: &Rig, indices: &[usize]) -> Result<Vec<u8>> {
    rig.validate().map_err(anyhow::Error::msg)?;
    ensure!(
        !rig.parts.is_empty(),
        "Add a skin group before exporting layered Aseprite files"
    );
    let sequence: Vec<_> = indices
        .iter()
        .flat_map(|i| (1..=rig.clips[*i].length).map(move |f| (*i, f)))
        .collect();
    let frames = sequence.len() as u64;
    let pixels = u64::from(rig.width) * u64::from(rig.height);
    let layers = rig.parts.len() as u64;
    let bones = rig.bones.len().max(1) as u64;
    ensure!(
        pixels * bones * frames <= MAX_WORK,
        "Aseprite animation exceeds render budget; reduce canvas, bones, or frames"
    );
    ensure!(
        pixels * layers * frames <= MAX_LAYER_PIXELS,
        "Aseprite layers exceed 16 megapixel export budget; reduce canvas, groups, or frames"
    );
    // The FK solver accepts arbitrary input order. Its worst case is quadratic,
    // and each independent layer render solves the hierarchy once.
    ensure!(
        bones * bones * layers * frames <= MAX_WORK,
        "Aseprite skeleton and layer count exceed transform budget"
    );

    let duration = rig.clips[indices[0]].duration_ms(1) as u16;
    let mut order: Vec<usize> = (0..rig.parts.len()).collect();
    order.sort_by_key(|i| rig.parts[*i].z);
    let mut base_index = vec![0usize; rig.parts.len()];
    for (layer, index) in order.iter().enumerate() {
        base_index[*index] = layer;
    }
    let mut out = vec![0u8; 128];
    put_u16(&mut out, 4, 0xA5E0);
    put_u16(&mut out, 6, frames as u16);
    put_u16(&mut out, 8, rig.width as u16);
    put_u16(&mut out, 10, rig.height as u16);
    put_u16(&mut out, 12, 32);
    put_u32(&mut out, 14, 1); // layer opacity is valid
    put_u16(&mut out, 18, duration);
    out[34] = 1;
    out[35] = 1; // square pixels
    let mut single = rig.clone(); // Reuse geometry; never clone an entire document per cel.

    for (output_frame, (clip_index, frame)) in sequence.iter().copied().enumerate() {
        single.active_clip = clip_index;
        let frame_start = out.len();
        out.resize(frame_start + 16, 0);
        let mut chunks = 0u32;
        if output_frame == 0 {
            for index in &order {
                let part = &rig.parts[*index];
                let start = begin_chunk(&mut out, 0x2004);
                word(&mut out, 3); // visible and editable
                word(&mut out, 0);
                word(&mut out, 0); // normal image layer; no parent group
                word(&mut out, 0);
                word(&mut out, 0);
                word(&mut out, 0); // ignored size, normal blending
                out.push(255);
                out.extend_from_slice(&[0; 3]);
                word(&mut out, part.name.len() as u16);
                out.extend_from_slice(part.name.as_bytes());
                end_chunk(&mut out, start);
                chunks += 1;
            }
            append_tags(&mut out, rig, indices);
            chunks += 1;
        }
        let pose = rig.clips[clip_index].sample(frame as f32);
        let mut animated: Vec<usize> = (0..rig.parts.len()).collect();
        animated.sort_by(|a, b| {
            let z = |i: usize| {
                pose.get(&rig.parts[i].id)
                    .and_then(|p| p.z)
                    .unwrap_or(rig.parts[i].z as f32)
            };
            z(*a).total_cmp(&z(*b))
        });
        let mut ranks = vec![0usize; rig.parts.len()];
        for (rank, index) in animated.iter().enumerate() {
            ranks[*index] = rank;
        }
        for index in &order {
            let part = &rig.parts[*index];
            if pose
                .get(&part.id)
                .and_then(|p| p.visibility)
                .is_some_and(|v| v < 0.5)
                || part.color[3] == 0
            {
                continue;
            }
            if !rig.bones.iter().any(|b| b.part == part.id) {
                continue;
            }
            single.parts.clear();
            single.parts.push(part.clone());
            let image = skin::render(&single, Some(&pose));
            if !image.pixels().any(|p| p[3] != 0) {
                continue;
            }
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
            encoder.write_all(image.as_raw())?;
            let compressed = encoder.finish()?;
            let start = begin_chunk(&mut out, 0x2005);
            word(&mut out, base_index[*index] as u16);
            word(&mut out, 0);
            word(&mut out, 0); // full canvas cel, origin (0, 0)
            out.push(255);
            word(&mut out, 2); // opaque cel with zlib-compressed RGBA pixels
            let shift = ranks[*index] as i16 - base_index[*index] as i16;
            out.extend_from_slice(&shift.to_le_bytes());
            out.extend_from_slice(&[0; 5]);
            word(&mut out, rig.width as u16);
            word(&mut out, rig.height as u16);
            out.extend_from_slice(&compressed);
            end_chunk(&mut out, start);
            chunks += 1;
        }
        let frame_bytes = (out.len() - frame_start) as u32;
        put_u32(&mut out, frame_start, frame_bytes);
        put_u16(&mut out, frame_start + 4, 0xF1FA);
        put_u16(&mut out, frame_start + 6, chunks as u16);
        put_u16(
            &mut out,
            frame_start + 8,
            rig.clips[clip_index].duration_ms(frame) as u16,
        );
        put_u32(&mut out, frame_start + 12, chunks);
    }
    let file_bytes = out.len() as u32;
    put_u32(&mut out, 0, file_bytes);
    Ok(out)
}
fn word(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}
fn put_u16(out: &mut [u8], at: usize, value: u16) {
    out[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn put_u32(out: &mut [u8], at: usize, value: u32) {
    out[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn begin_chunk(out: &mut Vec<u8>, kind: u16) -> usize {
    let start = out.len();
    out.extend_from_slice(&[0; 4]);
    word(out, kind);
    start
}
fn end_chunk(out: &mut [u8], start: usize) {
    let size = (out.len() - start) as u32;
    put_u32(out, start, size);
}
