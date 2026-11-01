//! Keyed 3D joint corrections. Each timeline clip owns sparse rotation keys per
//! joint, applied after imported motion and the whole-clip joint offsets. Keys
//! store editable XYZ degrees but interpolate as shortest-path quaternions, so
//! 179° → -179° turns 2°, not 358°. Only rotation is keyed, so a corrected limb
//! stays attached to its parent.
use super::*;
use crate::animation::Ease;

pub const MAX_KEYS: usize = 100_000;
const FRAME_LIMIT: f32 = 1_000_000.;
const SAME_FRAME: f32 = 0.001;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JointKey {
    pub frame: f32,
    /// XYZ Euler degrees, composed after the animated local rotation.
    pub rotation: [f32; 3],
    /// Easing toward the next key.
    #[serde(default)]
    pub easing: Ease,
}
/// Joint node index → keys sorted by frame.
pub type JointTracks = BTreeMap<usize, Vec<JointKey>>;
/// A key in one clip, identified by joint and frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointKeyRef {
    pub joint: usize,
    pub frame: f32,
}
/// Copied keys relative to the earliest copied frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JointClipboard {
    pub keys: Vec<(usize, JointKey)>,
}

pub fn quat(euler: [f32; 3]) -> Quat {
    Quat::from_euler(
        EulerRot::XYZ,
        euler[0].to_radians(),
        euler[1].to_radians(),
        euler[2].to_radians(),
    )
}
pub fn euler(q: Quat) -> [f32; 3] {
    let (x, y, z) = q.normalize().to_euler(EulerRot::XYZ);
    let clean = |v: f32| {
        let d = v.to_degrees();
        if d.abs() < 1e-4 { 0. } else { d }
    };
    [clean(x), clean(y), clean(z)]
}
/// Correction at a fractional frame; identity when a joint has no keys. Values
/// hold before the first and after the last key.
pub fn sample(keys: &[JointKey], frame: f32) -> Quat {
    let (Some(first), Some(last)) = (keys.first(), keys.last()) else {
        return Quat::IDENTITY;
    };
    if frame <= first.frame {
        return quat(first.rotation);
    }
    if frame >= last.frame {
        return quat(last.rotation);
    }
    let end = keys.partition_point(|k| k.frame <= frame);
    let (a, b) = (&keys[end - 1], &keys[end]);
    let u = a
        .easing
        .sample(((frame - a.frame) / (b.frame - a.frame)).clamp(0., 1.));
    // glam's slerp takes the shorter arc, which handles angle wraparound.
    quat(a.rotation).slerp(quat(b.rotation), u).normalize()
}
fn validate_keys(keys: &[JointKey]) -> bool {
    keys.windows(2)
        .all(|w| w[1].frame - w[0].frame >= SAME_FRAME)
        && keys.iter().all(|k| {
            k.frame.is_finite()
                && k.frame.abs() <= FRAME_LIMIT
                && k.rotation
                    .iter()
                    .all(|v| v.is_finite() && v.abs() <= 100_000.)
        })
}
impl Character {
    pub(super) fn validate_tracks(&self) -> Result<()> {
        let count: usize = self
            .tracks
            .values()
            .flat_map(|t| t.values())
            .map(Vec::len)
            .sum();
        ensure!(
            self.tracks.len() <= 32
                && count <= MAX_KEYS
                && self.tracks.values().all(|t| t
                    .iter()
                    .all(|(j, keys)| *j < self.model.nodes.len() && validate_keys(keys))),
            "Invalid 3D joint keys"
        );
        Ok(())
    }
    /// Keyed correction for each joint of `clip` at `frame`.
    pub(super) fn corrections(&self, clip: usize, frame: f32) -> Vec<(usize, Quat)> {
        self.tracks.get(&clip).map_or_else(Vec::new, |t| {
            t.iter()
                .filter(|(_, keys)| !keys.is_empty())
                .map(|(j, keys)| (*j, sample(keys, frame)))
                .collect()
        })
    }
    pub fn joint_keys(&self, clip: usize, joint: usize) -> &[JointKey] {
        self.tracks
            .get(&clip)
            .and_then(|t| t.get(&joint))
            .map_or(&[], Vec::as_slice)
    }
    /// Current correction in degrees, for display and as a new key's value.
    pub fn sample_joint(&self, clip: usize, joint: usize, frame: f32) -> [f32; 3] {
        euler(sample(self.joint_keys(clip, joint), frame))
    }
    pub fn key_at(&self, clip: usize, joint: usize, frame: f32) -> Option<&JointKey> {
        self.joint_keys(clip, joint)
            .iter()
            .find(|k| (k.frame - frame).abs() < SAME_FRAME)
    }
    /// Insert or replace a key. A replaced key keeps its easing.
    pub fn set_joint_key(
        &mut self,
        clip: usize,
        joint: usize,
        frame: f32,
        rotation: [f32; 3],
    ) -> std::result::Result<(), String> {
        if joint >= self.model.nodes.len() {
            return Err("Unknown 3D joint".into());
        }
        if !frame.is_finite()
            || frame.abs() > FRAME_LIMIT
            || !rotation.iter().all(|v| v.is_finite())
        {
            return Err("Invalid 3D joint key".into());
        }
        let keys = self
            .tracks
            .entry(clip)
            .or_default()
            .entry(joint)
            .or_default();
        if let Some(k) = keys
            .iter_mut()
            .find(|k| (k.frame - frame).abs() < SAME_FRAME)
        {
            k.rotation = rotation;
        } else {
            let at = keys.partition_point(|k| k.frame < frame);
            keys.insert(
                at,
                JointKey {
                    frame,
                    rotation,
                    easing: Ease::Linear,
                },
            );
        }
        Ok(())
    }
    pub fn set_joint_easing(&mut self, clip: usize, keys: &[JointKeyRef], easing: Ease) {
        for r in keys {
            if let Some(k) = self
                .tracks
                .get_mut(&clip)
                .and_then(|t| t.get_mut(&r.joint))
                .and_then(|v| {
                    v.iter_mut()
                        .find(|k| (k.frame - r.frame).abs() < SAME_FRAME)
                })
            {
                k.easing = easing;
            }
        }
    }
    pub fn delete_joint_keys(&mut self, clip: usize, keys: &[JointKeyRef]) {
        if let Some(tracks) = self.tracks.get_mut(&clip) {
            for r in keys {
                if let Some(v) = tracks.get_mut(&r.joint) {
                    v.retain(|k| (k.frame - r.frame).abs() >= SAME_FRAME);
                }
            }
            tracks.retain(|_, v| !v.is_empty());
        }
        self.tracks.retain(|_, t| !t.is_empty());
    }
    pub fn clear_joint_track(&mut self, clip: usize, joint: usize) {
        if let Some(tracks) = self.tracks.get_mut(&clip) {
            tracks.remove(&joint);
        }
        self.tracks.retain(|_, t| !t.is_empty());
    }
    /// Shift selected keys by whole frames. Fails without changes if a moved
    /// key would land on an unselected key or leave `1..=length`.
    pub fn move_joint_keys(
        &mut self,
        clip: usize,
        keys: &[JointKeyRef],
        offset: f32,
        length: u32,
    ) -> std::result::Result<(), String> {
        if offset == 0. || keys.is_empty() {
            return Ok(());
        }
        let mut next = self.tracks.get(&clip).cloned().unwrap_or_default();
        for (joint, track) in &mut next {
            let selected = |k: &JointKey| {
                keys.iter()
                    .any(|r| r.joint == *joint && (k.frame - r.frame).abs() < SAME_FRAME)
            };
            let (mut moved, kept): (Vec<_>, Vec<_>) = track.drain(..).partition(|k| selected(k));
            for k in &mut moved {
                k.frame += offset;
                if !(1. ..=length.max(1) as f32).contains(&k.frame) {
                    return Err("Keys cannot move outside the clip".into());
                }
                if kept.iter().any(|o| (o.frame - k.frame).abs() < SAME_FRAME) {
                    return Err("A moved key would replace another key".into());
                }
            }
            *track = kept.into_iter().chain(moved).collect();
            track.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        }
        self.tracks.insert(clip, next);
        Ok(())
    }
    pub fn copy_joint_keys(&self, clip: usize, keys: &[JointKeyRef]) -> JointClipboard {
        let mut out: Vec<_> = keys
            .iter()
            .filter_map(|r| {
                self.joint_keys(clip, r.joint)
                    .iter()
                    .find(|k| (k.frame - r.frame).abs() < SAME_FRAME)
                    .map(|k| (r.joint, k.clone()))
            })
            .collect();
        let first = out
            .iter()
            .map(|(_, k)| k.frame)
            .fold(f32::INFINITY, f32::min);
        for (_, k) in &mut out {
            k.frame -= first;
        }
        JointClipboard { keys: out }
    }
    /// Paste with the earliest copied key at `frame`, replacing keys it lands on.
    pub fn paste_joint_keys(
        &mut self,
        clip: usize,
        clipboard: &JointClipboard,
        frame: f32,
    ) -> std::result::Result<Vec<JointKeyRef>, String> {
        let mut next = self.clone();
        let mut pasted = vec![];
        for (joint, key) in &clipboard.keys {
            let at = frame + key.frame;
            next.set_joint_key(clip, *joint, at, key.rotation)?;
            next.set_joint_easing(
                clip,
                &[JointKeyRef {
                    joint: *joint,
                    frame: at,
                }],
                key.easing,
            );
            pasted.push(JointKeyRef {
                joint: *joint,
                frame: at,
            });
        }
        *self = next;
        Ok(pasted)
    }
}
/// Rebuild per-clip keys after a clip edit. Each `(new, old, start)` entry
/// takes clip `old`'s keys, shifted so its frame `shift + 1` becomes frame 1.
/// Hidden keys outside the new window are kept for later trims and splits.
pub(crate) fn remap_clips(
    tracks: &BTreeMap<usize, JointTracks>,
    ranges: impl IntoIterator<Item = (usize, usize, f32)>,
) -> BTreeMap<usize, JointTracks> {
    ranges
        .into_iter()
        .filter_map(|(new, old, shift)| {
            let mut t = tracks.get(&old)?.clone();
            for k in t.values_mut().flatten() {
                k.frame -= shift;
            }
            Some((new, t))
        })
        .collect()
}
