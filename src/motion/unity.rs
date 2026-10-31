//! Safe Unity text AnimationClip reader. This recognizes only numeric transform
//! curves, never YAML object construction, aliases, scripts, or asset references.
//! .anim contains no bind skeleton: first-sample local TRS supplies its rest pose.
use super::*;
pub(super) const GUIDANCE: &str = "Unity animation must be a text YAML .anim with transform curves. Binary, compressed and humanoid muscle-only clips need exporting as a baked skeleton FBX (or GLB/glTF) from Unity.";
#[derive(Clone)]
struct Key {
    time: f32,
    value: Vec4,
    in_slope: Option<Vec4>,
    out_slope: Option<Vec4>,
}
struct Track {
    kind: u8,
    path: String,
    keys: Vec<Key>,
}
fn scalar(s: &str, slope: bool) -> Result<f32> {
    let v = match s.trim() {
        ".inf" | "Infinity" | "inf" => f32::INFINITY,
        "-.inf" | "-Infinity" | "-inf" => f32::NEG_INFINITY,
        v => v.parse::<f32>()?,
    };
    ensure!(
        (slope && !v.is_nan()) || (v.is_finite() && v.abs() <= 1e6),
        "Invalid Unity curve numeric value"
    );
    Ok(v)
}
fn vector(s: &str, kind: u8, slope: bool) -> Result<Vec4> {
    let s = s.trim();
    ensure!(
        s.starts_with('{') && s.ends_with('}'),
        "Unity curve values must be inline numeric vectors; {GUIDANCE}"
    );
    let mut v = Vec4::ZERO;
    let mut seen = HashSet::new();
    for item in s[1..s.len() - 1].split(',') {
        let (k, value) = item
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("Invalid Unity numeric vector"))?;
        let i = match k.trim() {
            "x" => 0,
            "y" => 1,
            "z" => 2,
            "w" if kind == 1 => 3,
            _ => bail!("Invalid Unity vector component"),
        };
        ensure!(seen.insert(i), "Duplicate Unity vector component");
        v[i] = scalar(value, slope)?;
    }
    ensure!(
        seen.len() == if kind == 1 { 4 } else { 3 },
        "Incomplete Unity numeric vector"
    );
    if kind == 1 && !slope {
        ensure!(
            v.length_squared().is_finite() && v.length_squared() > 1e-12,
            "Invalid Unity rotation quaternion"
        );
    }
    Ok(v)
}
fn value(track: &Track, t: f32) -> Vec4 {
    let keys = &track.keys;
    if t <= keys[0].time {
        return keys[0].value;
    }
    let r = keys.partition_point(|k| k.time <= t);
    if r == keys.len() {
        return keys[r - 1].value;
    }
    let a = &keys[r - 1];
    let b = &keys[r];
    let d = b.time - a.time;
    let u = (t - a.time) / d;
    match (a.out_slope, b.in_slope) {
        (Some(x), Some(y)) => {
            if !x.is_finite() || !y.is_finite() {
                return a.value;
            }
            let u2 = u * u;
            let u3 = u2 * u;
            a.value * (2. * u3 - 3. * u2 + 1.)
                + x * d * (u3 - 2. * u2 + u)
                + b.value * (-2. * u3 + 3. * u2)
                + y * d * (u3 - u2)
        }
        _ => a.value.lerp(b.value, u),
    }
}
fn string(s: &str) -> Result<String> {
    let s = s.trim();
    ensure!(
        s.len() <= 4096 && !s.contains('\0'),
        "Unity name/path exceeds bounds"
    );
    if s.starts_with('"') {
        Ok(serde_json::from_str(s)?)
    } else if s.starts_with('\'') {
        ensure!(
            s.ends_with('\'') && s.len() >= 2,
            "Invalid quoted Unity string"
        );
        Ok(s[1..s.len() - 1].replace("''", "'"))
    } else {
        ensure!(
            !s.starts_with(['*', '&', '!', '{', '[']),
            "Unsupported Unity YAML alias or structured string"
        );
        Ok(s.to_owned())
    }
}
fn push(tracks: &mut Vec<Track>, track: &mut Option<Track>) -> Result<()> {
    if let Some(t) = track.take() {
        ensure!(
            !t.keys.is_empty()
                && !t.path.is_empty()
                && t.keys.windows(2).all(|k| k[1].time > k[0].time),
            "Missing Unity curve path/keys or unsorted times"
        );
        ensure!(
            tracks.len() < 1536 && tracks.iter().all(|v| v.kind != t.kind || v.path != t.path),
            "Duplicate or excessive Unity transform curves"
        );
        tracks.push(t);
    }
    Ok(())
}
pub(super) fn parse(text: &str) -> Result<Motion> {
    ensure!(
        text.len() <= 32 * 1024 * 1024
            && !text.contains('\0')
            && text.lines().any(|s| s.trim() == "AnimationClip:"),
        "{GUIDANCE}"
    );
    let mut tracks = vec![];
    let mut track: Option<Track> = None;
    let mut kind = None;
    let mut name = "Unity animation".to_owned();
    let mut key_count = 0usize;
    let mut lines = 0;
    let mut warnings=vec!["Unity .anim has no bind skeleton: rest joint positions/orientations inferred from the first sample; use a skeleton FBX for exact bind-pose retargeting".into()];
    for line in text.lines() {
        lines += 1;
        ensure!(
            lines <= 250000 && line.len() <= 16384,
            "Unity text exceeds line budget"
        );
        let s = line.trim();
        if s.is_empty() || s.starts_with(['%', '#']) || s.starts_with("---") {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if s.starts_with("m_Name:") {
            name = string(s.split_once(':').unwrap().1)?;
            ensure!(name.len() <= 1024, "Unity clip name too long");
        }
        if let Some(v) = s.strip_prefix("m_Compressed:") {
            ensure!(v.trim() == "0", "{GUIDANCE}");
        }
        if let Some(v) = s.strip_prefix("m_CompressedRotationCurves:") {
            ensure!(v.trim() == "[]", "{GUIDANCE}");
        }
        if indent == 2 && s.starts_with("m_") && s.contains(':') {
            push(&mut tracks, &mut track)?;
            kind = match s.split_once(':').unwrap().0 {
                "m_PositionCurves" => Some(0),
                "m_RotationCurves" => Some(1),
                "m_ScaleCurves" => Some(2),
                "m_EulerCurves" => Some(3),
                _ => None,
            };
            continue;
        }
        let Some(k) = kind else {
            continue;
        };
        if s == "- curve:" {
            push(&mut tracks, &mut track)?;
            track = Some(Track {
                kind: k,
                path: String::new(),
                keys: vec![],
            });
            continue;
        }
        let Some(track) = &mut track else {
            continue;
        };
        if let Some(p) = s.strip_prefix("path:") {
            let p = string(p)?;
            track.path = if p.is_empty() { "/".into() } else { p };
            continue;
        }
        let s = s.strip_prefix("- ").unwrap_or(s);
        if let Some(t) = s.strip_prefix("time:") {
            key_count += 1;
            ensure!(key_count <= 100000, "Unity curves exceed100000keys");
            let time = scalar(t, false)?;
            ensure!(
                (0. ..=14400.).contains(&time),
                "Unity key time exceeds4hours"
            );
            track.keys.push(Key {
                time,
                value: Vec4::splat(f32::NAN),
                in_slope: None,
                out_slope: None,
            });
        }
        if let Some(v) = s.strip_prefix("value:") {
            let key = track
                .keys
                .last_mut()
                .ok_or_else(|| anyhow::anyhow!("Unity value before key time"))?;
            key.value = vector(v, k, false)?;
        }
        if let Some(v) = s.strip_prefix("inSlope:") {
            let key = track
                .keys
                .last_mut()
                .ok_or_else(|| anyhow::anyhow!("Unity tangent before key time"))?;
            key.in_slope = Some(vector(v, k, true)?);
        }
        if let Some(v) = s.strip_prefix("outSlope:") {
            let key = track
                .keys
                .last_mut()
                .ok_or_else(|| anyhow::anyhow!("Unity tangent before key time"))?;
            key.out_slope = Some(vector(v, k, true)?);
        }
        if let Some(v) = s.strip_prefix("weightedMode:") {
            ensure!(
                v.trim() == "0",
                "Weighted Unity tangents require a baked FBX export"
            );
        }
    }
    push(&mut tracks, &mut track)?;
    ensure!(!tracks.is_empty(), "{GUIDANCE}");
    ensure!(
        tracks
            .iter()
            .all(|t| t.keys.iter().all(|k| k.value.is_finite())),
        "Missing/nonfinite Unity curve values"
    );
    let mut paths = BTreeMap::new();
    let mut nodes: Vec<Node> = vec![];
    if tracks.iter().any(|t| t.path == "/") {
        nodes.push(Node {
            name: "Root".into(),
            parent: None,
            children: vec![],
        });
        paths.insert("/".to_owned(), 0);
    }
    for t in &tracks {
        if t.path == "/" {
            continue;
        }
        let mut path = String::new();
        let mut parent = paths.get("/").copied();
        let parts = t.path.split('/').collect::<Vec<_>>();
        ensure!(parts.len() <= 64, "Unity hierarchy exceeds64levels");
        for part in parts {
            ensure!(
                !part.is_empty() && part != ".." && part != ".",
                "Invalid Unity hierarchy path"
            );
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(part);
            let id = if let Some(i) = paths.get(&path) {
                *i
            } else {
                ensure!(nodes.len() < 512, "Unity skeleton exceeds512nodes");
                let i = nodes.len();
                nodes.push(Node {
                    name: part.into(),
                    parent,
                    children: vec![],
                });
                if let Some(p) = parent {
                    nodes[p].children.push(i);
                }
                paths.insert(path.clone(), i);
                i
            };
            parent = Some(id);
        }
    }
    let duration = tracks
        .iter()
        .map(|t| t.keys.last().unwrap().time)
        .fold(0., f32::max);
    let count = if duration > 0. {
        ((duration * 30.).ceil() as usize + 1).clamp(2, 240)
    } else {
        1
    };
    ensure!(
        count * nodes.len() <= 262144,
        "Unity sampled transform budget exceeded"
    );
    if duration * 30. > 239. {
        warnings.push("Unity animation resampled to240poses".into());
    }
    let mut frames = vec![];
    for f in 0..count {
        let time = if count > 1 {
            duration * f as f32 / (count - 1) as f32
        } else {
            0.
        };
        let mut trs = vec![(Vec3::ZERO, Quat::IDENTITY, Vec3::ONE); nodes.len()];
        for t in &tracks {
            let i = paths[&t.path];
            let v = value(t, time);
            ensure!(v.is_finite(), "Unity interpolated curve overflow");
            match t.kind {
                0 => trs[i].0 = Vec3::new(v.x, v.y, -v.z),
                1 => {
                    let q = Quat::from_xyzw(-v.x, -v.y, v.z, v.w);
                    ensure!(
                        q.length_squared().is_finite() && q.length_squared() > 1e-12,
                        "Unity rotation interpolates through zero quaternion"
                    );
                    trs[i].1 = q.normalize();
                }
                2 => trs[i].2 = v.truncate(),
                _ => {
                    let q = Quat::from_euler(
                        glam::EulerRot::YXZ,
                        v.y.to_radians(),
                        v.x.to_radians(),
                        v.z.to_radians(),
                    );
                    trs[i].1 = Quat::from_xyzw(-q.x, -q.y, q.z, q.w);
                }
            }
        }
        let local = trs
            .into_iter()
            .map(|(t, r, s)| Mat4::from_scale_rotation_translation(s, r, t))
            .collect::<Vec<_>>();
        frames.push(world_matrices(&nodes, &local)?);
    }
    let rest = frames[0].clone();
    let fps = if duration > 0. {
        (count - 1) as f32 / duration
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
