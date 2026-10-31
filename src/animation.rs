//! Keyframe channels and easing derived from Bonebase's anim.lua.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub type Pose = BTreeMap<u32, BonePose>;
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BonePose {
    pub rotation: f32,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub z: Option<f32>,
    pub visibility: Option<f32>,
}
impl Default for BonePose {
    fn default() -> Self {
        Self {
            rotation: 0.,
            x: 0.,
            y: 0.,
            scale: 1.,
            z: None,
            visibility: None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Channel {
    #[serde(alias = "rot")]
    Rotation,
    #[serde(alias = "x")]
    X,
    #[serde(alias = "y")]
    Y,
    #[serde(alias = "s")]
    Scale,
    #[serde(alias = "z")]
    Z,
    #[serde(alias = "vis")]
    Visibility,
}
impl Channel {
    fn neutral(self) -> f32 {
        if matches!(self, Self::Scale | Self::Visibility) {
            1.
        } else {
            0.
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Ease {
    Linear,
    #[default]
    Ease,
    #[serde(alias = "Ease in")]
    EaseIn,
    #[serde(alias = "Ease out")]
    EaseOut,
    Back,
    Bounce,
    Step,
    Cubic,
}
impl Ease {
    pub fn sample(self, u: f32) -> f32 {
        let u = u.clamp(0., 1.);
        match self {
            Self::Linear | Self::Cubic => u,
            Self::Step => 0.,
            Self::Ease => (1. - (std::f32::consts::PI * u).cos()) * 0.5,
            Self::EaseIn => u * u * u,
            Self::EaseOut => 1. - (1. - u).powi(3),
            Self::Back => {
                let v = u - 1.;
                1. + 2.70158 * v * v * v + 1.70158 * v * v
            }
            Self::Bounce => {
                let n = 7.5625;
                let d = 2.75;
                if u < 1. / d {
                    n * u * u
                } else if u < 2. / d {
                    n * (u - 1.5 / d).powi(2) + 0.75
                } else if u < 2.5 / d {
                    n * (u - 2.25 / d).powi(2) + 0.9375
                } else {
                    n * (u - 2.625 / d).powi(2) + 0.984375
                }
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub frame: f32,
    pub value: f32,
    #[serde(default)]
    pub easing: Ease,
    #[serde(default)]
    pub tangent_in: f32,
    #[serde(default)]
    pub tangent_out: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum Playback {
    #[default]
    Forward,
    Reverse,
    PingPong,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub bone: u32,
    pub channel: Channel,
    #[serde(default)]
    pub keys: Vec<Keyframe>,
}
/// Retained source timing for a non-destructive clip window. Track keys stay in
/// local coordinates, including keys outside the visible window. `origin` maps
/// them back to the original source clock; its prefix also drives live springs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClipSource {
    pub origin: u32,
    pub length: u32,
    pub end: u32,
    pub looped: bool,
    pub durations: BTreeMap<u32, u32>,
    /// Global audio/reference origin. This clock advances through motion holds.
    #[serde(default)]
    pub time_offset_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clip {
    pub name: String,
    pub fps: f32,
    pub length: u32,
    #[serde(rename = "loop", alias = "looped")]
    pub looped: bool,
    pub tracks: Vec<Track>,
    pub playback: Playback,
    pub durations: BTreeMap<u32, u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ClipSource>,
}
impl Default for Clip {
    fn default() -> Self {
        Self {
            name: "Animation".into(),
            fps: 10.,
            length: 12,
            looped: true,
            tracks: vec![],
            playback: Playback::Forward,
            durations: BTreeMap::new(),
            source: None,
        }
    }
}
pub fn wrap_degrees(a: f32) -> f32 {
    let a = a.rem_euclid(360.);
    if a > 180. { a - 360. } else { a }
}
impl Clip {
    /// Bounds for retained editable source keys, in this clip's local frame
    /// coordinates. Gameplay events and clip windows still use `1..=length`.
    pub fn source_bounds(&self) -> (f32, f32) {
        self.source.as_ref().map_or((1., self.length as f32), |s| {
            (1. - s.origin as f32, (s.length - s.origin) as f32)
        })
    }
    /// Local frame used by motion and scene evaluation. A length extension
    /// holds the previous endpoint until an author adds a key beyond it.
    pub fn evaluation_frame(&self, frame: f32) -> f32 {
        let end = self.source.as_ref().map_or(self.length, |s| s.end);
        if frame.is_finite() {
            frame.clamp(1., self.length.max(1) as f32).min(end as f32)
        } else {
            1.
        }
    }
    /// Original controller clock, including the prefix hidden by a trim.
    pub fn source_frame(&self, frame: f32) -> f32 {
        self.evaluation_frame(frame) + self.source.as_ref().map_or(0, |s| s.origin) as f32
    }
    /// Duration on the original source clock. Visible duration edits remain
    /// live, while hidden prefix/suffix timing is retained through later edits.
    pub fn source_duration_ms(&self, frame: u32) -> u32 {
        let Some(source) = &self.source else {
            return self.duration_ms(frame);
        };
        if frame > source.origin && frame <= source.origin + source.end {
            self.duration_ms(frame - source.origin)
        } else {
            source
                .durations
                .get(&frame)
                .copied()
                .unwrap_or_else(|| self.default_duration_ms())
        }
    }
    /// Elapsed source time for global audio/reference alignment after a trim.
    pub fn source_time_ms(&self, frame: f32) -> u64 {
        self.source.as_ref().map_or(0, |s| s.time_offset_ms) + self.local_time_ms(frame)
    }
    fn local_time_ms(&self, frame: f32) -> u64 {
        let frame = if frame.is_finite() {
            frame.clamp(1., self.length as f32 + 1.)
        } else {
            1.
        };
        let whole = frame.floor() as u32;
        (1..whole)
            .map(|f| u64::from(self.duration_ms(f)))
            .sum::<u64>()
            + (frame.fract() * self.duration_ms(whole.min(self.length)) as f32) as u64
    }
    fn default_duration_ms(&self) -> u32 {
        (1000. / self.fps).round().clamp(1., 60000.) as u32
    }
    fn source_snapshot(&self) -> ClipSource {
        let mut source = self.source.clone().unwrap_or_else(|| ClipSource {
            origin: 0,
            length: self.length,
            end: self.length,
            looped: self.looped,
            durations: self.durations.clone(),
            time_offset_ms: 0,
        });
        // Commit live duration edits into the retained clock before moving it.
        for frame in 1..=source.end {
            let at = source.origin + frame;
            if let Some(ms) = self.durations.get(&frame) {
                source.durations.insert(at, *ms);
            } else {
                source.durations.remove(&at);
            }
        }
        source
    }
    /// Preserve the complete source curves and history behind a local window.
    pub fn windowed(&self, start: u32, end: u32) -> Result<Self, String> {
        if start == 0 || end < start || end > self.length {
            return Err("Invalid trim range".into());
        }
        if start == 1 && end == self.length {
            return Ok(self.clone());
        }
        let mut result = self.clone();
        let mut source = self.source_snapshot();
        source.time_offset_ms += self.local_time_ms(start as f32);
        // A window wholly inside an extended hold starts at that held source
        // frame, without inventing additional spring/controller history.
        let shift = (start - 1).min(source.end - 1);
        source.origin += shift;
        source.end = source.end.min(end) - shift;
        result.length = end - start + 1;
        for track in &mut result.tracks {
            for key in &mut track.keys {
                key.frame -= shift as f32;
            }
        }
        result.durations = self
            .durations
            .iter()
            .filter(|(f, _)| **f >= start && **f <= end)
            .map(|(f, ms)| (*f - start + 1, *ms))
            .collect();
        result.source = Some(source);
        Ok(result)
    }
    pub fn resized(&self, length: u32) -> Result<Self, String> {
        if !(1..=240).contains(&length) {
            return Err("Clip length must be 1–240".into());
        }
        if length <= self.length {
            return self.windowed(1, length);
        }
        let mut result = self.clone();
        result.source = Some(self.source_snapshot());
        result.length = length;
        Ok(result)
    }
    /// Restore scene/audio timing after explicitly baking local FK keys. The
    /// original curve loop no longer applies to the newly baked local curves.
    pub fn restore_source_timing(&mut self, source: Option<ClipSource>) -> Result<(), String> {
        self.source = match source {
            Some(mut source) => {
                source.length = source.length.max(source.origin + self.length);
                if source.length > 4096 {
                    return Err("Retained animation source exceeds 4096 frames".into());
                }
                source.looped = false;
                Some(source)
            }
            None => None,
        };
        Ok(())
    }
    /// Enable authoring at a visible local frame, including a previously held
    /// extension. Controller and scene-key editors use the same path as FK keys.
    pub fn release_hold(&mut self, frame: f32) -> Result<(), String> {
        if !frame.is_finite() || frame < 1. || frame > self.length as f32 {
            return Err("Keyframe is outside the visible clip".into());
        }
        if let Some(source) = &mut self.source {
            let end = frame.ceil() as u32;
            if source.origin.saturating_add(end) > 4096 {
                return Err(
                    "Retained animation source exceeds 4096 frames; bake before extending".into(),
                );
            }
            source.end = source.end.max(end);
            source.length = source.length.max(source.origin + end);
        }
        Ok(())
    }
    pub fn duration_ms(&self, frame: u32) -> u32 {
        self.durations
            .get(&frame)
            .copied()
            .unwrap_or_else(|| self.default_duration_ms())
    }
    pub fn frame_sequence(&self) -> Vec<u32> {
        match self.playback {
            Playback::Forward => (1..=self.length).collect(),
            Playback::Reverse => (1..=self.length).rev().collect(),
            Playback::PingPong => (1..=self.length).chain((2..self.length).rev()).collect(),
        }
    }
    pub fn total_duration_ms(&self) -> u64 {
        self.frame_sequence()
            .into_iter()
            .map(|f| u64::from(self.duration_ms(f)))
            .sum()
    }
    pub fn frame_at_ms(&self, elapsed: f64) -> (u32, bool) {
        let sequence = self.frame_sequence();
        let Some(first) = sequence.first().copied() else {
            return (1, true);
        };
        let total = self.total_duration_ms() as f64;
        let mut time = if elapsed.is_finite() {
            elapsed.max(0.)
        } else {
            0.
        };
        if !self.looped && time >= total {
            return (
                if self.playback == Playback::PingPong {
                    1
                } else {
                    *sequence.last().unwrap()
                },
                true,
            );
        }
        if total > 0. {
            time = time.rem_euclid(total);
        }
        for frame in sequence {
            let duration = f64::from(self.duration_ms(frame));
            if time < duration {
                return (frame, false);
            }
            time -= duration;
        }
        (first, false)
    }
    pub fn set_key(
        &mut self,
        bone: u32,
        channel: Channel,
        frame: f32,
        value: f32,
    ) -> Result<(), String> {
        if !frame.is_finite()
            || frame < 1.
            || frame > self.length as f32
            || !value.is_finite()
            || value.abs() > 100_000.
            || (channel == Channel::Scale && !(0.1..=4.).contains(&value))
        {
            return Err("Invalid keyframe time or value".into());
        }
        self.release_hold(frame)?;
        let index = match self
            .tracks
            .iter()
            .position(|t| t.bone == bone && t.channel == channel)
        {
            Some(i) => i,
            None => {
                self.tracks.push(Track {
                    bone,
                    channel,
                    keys: vec![],
                });
                self.tracks.len() - 1
            }
        };
        let keys = &mut self.tracks[index].keys;
        if let Some(key) = keys.iter_mut().find(|k| k.frame == frame) {
            key.value = value;
        } else {
            keys.push(Keyframe {
                frame,
                value,
                easing: Ease::default(),
                tangent_in: 0.,
                tangent_out: 0.,
            });
            keys.sort_by(|a, b| a.frame.total_cmp(&b.frame));
        }
        Ok(())
    }
    pub fn remove_key(&mut self, bone: u32, channel: Channel, frame: f32) -> bool {
        let mut removed = false;
        for t in &mut self.tracks {
            if t.bone == bone && t.channel == channel {
                let before = t.keys.len();
                t.keys.retain(|k| k.frame != frame);
                removed |= before != t.keys.len();
            }
        }
        self.tracks.retain(|t| !t.keys.is_empty());
        removed
    }
    pub fn value(&self, track: &Track, frame: f32) -> f32 {
        if let Some(source) = &self.source {
            return self.value_in_domain(
                track,
                self.evaluation_frame(frame),
                1. - source.origin as f32,
                source.length,
                source.looped,
            );
        }
        self.value_in_domain(track, frame, 1., self.length, self.looped)
    }
    /// Sample the retained, editable curve on its original clock. Layers and
    /// spring replay use this instead of wrapping inside the visible window.
    pub fn source_value(&self, track: &Track, frame: f32) -> f32 {
        if let Some(source) = &self.source {
            self.value_in_domain(
                track,
                frame - source.origin as f32,
                1. - source.origin as f32,
                source.length,
                source.looped,
            )
        } else {
            self.value_in_domain(track, frame, 1., self.length, self.looped)
        }
    }
    fn value_in_domain(
        &self,
        track: &Track,
        frame: f32,
        first_frame: f32,
        length: u32,
        looped: bool,
    ) -> f32 {
        let keys = &track.keys;
        let Some(first) = keys.first() else {
            return track.channel.neutral();
        };
        let last = &keys[keys.len() - 1];
        if keys.len() == 1 {
            return first.value;
        }
        let frame = if !frame.is_finite() {
            first_frame
        } else if looped && length > 0 {
            (frame - first_frame).rem_euclid(length as f32) + first_frame
        } else {
            frame
        };
        let at_seam = frame < first.frame || frame > last.frame;
        let (a, b, f, end_frame) = if at_seam {
            if !looped {
                return if frame < first.frame {
                    first.value
                } else {
                    last.value
                };
            }
            (
                last,
                first,
                if frame < first.frame {
                    frame + length as f32
                } else {
                    frame
                },
                first.frame + length as f32,
            )
        } else {
            if let Some(k) = keys.iter().find(|k| k.frame == frame) {
                return k.value;
            }
            let pair = keys
                .windows(2)
                .find(|p| frame > p[0].frame && frame < p[1].frame)
                .unwrap_or(&keys[0..2]);
            (&pair[0], &pair[1], frame, pair[1].frame)
        };
        let ease = if matches!(track.channel, Channel::Z | Channel::Visibility) {
            Ease::Step
        } else {
            a.easing
        };
        let delta = if track.channel == Channel::Rotation && at_seam {
            wrap_degrees(b.value - a.value)
        } else {
            b.value - a.value
        };
        let span = (end_frame - a.frame).max(0.0001);
        let u = ((f - a.frame) / span).clamp(0., 1.);
        let value = if ease == Ease::Cubic {
            let u2 = u * u;
            let u3 = u2 * u;
            (2. * u3 - 3. * u2 + 1.) * a.value
                + (u3 - 2. * u2 + u) * span * a.tangent_out
                + (-2. * u3 + 3. * u2) * (a.value + delta)
                + (u3 - u2) * span * b.tangent_in
        } else {
            a.value + delta * ease.sample(u)
        };
        if track.channel == Channel::Scale {
            value.clamp(0.1, 4.)
        } else {
            value
        }
    }
    pub fn sample(&self, frame: f32) -> Pose {
        self.sample_values(|track| self.value(track, frame))
    }
    pub fn source_sample(&self, frame: f32) -> Pose {
        self.sample_values(|track| self.source_value(track, frame))
    }
    fn sample_values(&self, value: impl Fn(&Track) -> f32) -> Pose {
        let mut pose = Pose::new();
        for t in &self.tracks {
            let value = value(t);
            let p = pose.entry(t.bone).or_default();
            match t.channel {
                Channel::Rotation => p.rotation = value,
                Channel::X => p.x = value,
                Channel::Y => p.y = value,
                Channel::Scale => p.scale = value,
                Channel::Z => p.z = Some(value),
                Channel::Visibility => p.visibility = Some(value),
            }
        }
        pose
    }
    pub fn validate(&self, owners: &HashSet<u32>) -> Result<(), String> {
        if let Some(source) = &self.source
            && (source.length == 0
                || source.length > 4096
                || source.origin >= source.length
                || source.end == 0
                || source.end > self.length
                || source
                    .origin
                    .checked_add(source.end)
                    .is_none_or(|end| end > source.length)
                || source.time_offset_ms > 1_000_000_000_000
                || source
                    .durations
                    .iter()
                    .any(|(f, ms)| *f == 0 || *f > source.length || !(1..=60000).contains(ms)))
        {
            return Err("Invalid retained clip source".into());
        }
        if self
            .durations
            .iter()
            .any(|(frame, ms)| *frame == 0 || *frame > self.length || !(1..=60000).contains(ms))
        {
            return Err("Invalid per-frame duration".into());
        }
        if self.name.len() > 1024
            || !self.fps.is_finite()
            || !(1. / 60. ..=240.).contains(&self.fps)
            || self.length == 0
            || self.length > 240
            || self.tracks.len() > 16384
        {
            return Err("Invalid animation limits".into());
        }
        let mut seen = HashSet::new();
        let mut total = 0;
        for t in &self.tracks {
            if !owners.contains(&t.bone) || !seen.insert((t.bone, t.channel)) || t.keys.len() > 240
            {
                return Err("Invalid or duplicate animation track".into());
            }
            let first = 1. - self.source.as_ref().map_or(0, |s| s.origin) as f32;
            let last = self
                .source
                .as_ref()
                .map_or(self.length, |s| s.length - s.origin) as f32;
            let mut previous = first - 1.;
            for k in &t.keys {
                if !k.tangent_in.is_finite()
                    || !k.tangent_out.is_finite()
                    || k.tangent_in.abs() > 100_000.
                    || k.tangent_out.abs() > 100_000.
                {
                    return Err("Invalid keyframe tangent".into());
                }
                if !k.frame.is_finite()
                    || k.frame <= previous
                    || k.frame < first
                    || k.frame > last
                    || !k.value.is_finite()
                    || k.value.abs() > 100_000.
                    || (t.channel == Channel::Scale && !(0.1..=4.).contains(&k.value))
                {
                    return Err("Invalid keyframe".into());
                }
                previous = k.frame;
                total += 1;
            }
        }
        if total > 100_000 {
            return Err("Too many animation keys".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn samples_source_rotation_and_updates_existing_key() {
        let mut c = Clip {
            looped: false,
            ..Default::default()
        };
        c.set_key(1, Channel::Rotation, 1., 170.).unwrap();
        c.set_key(1, Channel::Rotation, 11., -170.).unwrap();
        // Original anim.lua keeps explicit turns between keys; only the loop seam wraps.
        assert!(c.sample(6.)[&1].rotation.abs() < 0.001);
        c.set_key(1, Channel::Rotation, 11., -160.).unwrap();
        assert_eq!(c.tracks[0].keys.len(), 2);
        assert_eq!(c.sample(12.)[&1].rotation, -160.);
    }
    #[test]
    fn loop_seam_and_step_and_scale() {
        let mut c = Clip::default();
        c.set_key(1, Channel::X, 3., 0.).unwrap();
        c.set_key(1, Channel::X, 9., 12.).unwrap();
        assert!((c.sample(12.)[&1].x - 6.).abs() < 0.001);
        c.set_key(2, Channel::Visibility, 1., 1.).unwrap();
        c.set_key(2, Channel::Visibility, 7., 0.).unwrap();
        assert_eq!(c.sample(6.)[&2].visibility, Some(1.));
        assert_eq!(c.sample(7.)[&2].visibility, Some(0.));
        assert!(c.set_key(1, Channel::Scale, 1., -1.).is_err());
        assert!(c.set_key(1, Channel::X, 0., 0.).is_err());
    }

    #[test]
    fn all_eases_have_expected_endpoints_and_step_holds() {
        for ease in [
            Ease::Linear,
            Ease::Ease,
            Ease::EaseIn,
            Ease::EaseOut,
            Ease::Back,
            Ease::Bounce,
        ] {
            assert!(ease.sample(0.).abs() < 0.00001);
            assert!((ease.sample(1.) - 1.).abs() < 0.00001);
            assert!(ease.sample(0.5).is_finite());
        }
        assert_eq!(Ease::Step.sample(0.99), 0.);
        assert!(Ease::EaseIn.sample(0.5) < Ease::EaseOut.sample(0.5));
    }

    #[test]
    fn channels_defaults_clamping_and_remove_key() {
        let mut c = Clip {
            looped: false,
            ..Default::default()
        };
        for channel in [
            Channel::X,
            Channel::Y,
            Channel::Rotation,
            Channel::Scale,
            Channel::Z,
            Channel::Visibility,
        ] {
            assert_eq!(
                c.value(
                    &Track {
                        bone: 1,
                        channel,
                        keys: vec![]
                    },
                    1.
                ),
                channel.neutral()
            );
        }
        c.set_key(1, Channel::Y, 1., 2.).unwrap();
        c.set_key(1, Channel::Y, 12., 8.).unwrap();
        c.tracks[0].keys[0].easing = Ease::Linear;
        assert_eq!(c.sample(6.5)[&1].y, 5.);
        assert_eq!(c.sample(-12.)[&1].y, 2.);
        assert_eq!(c.sample(100.)[&1].y, 8.);
        assert!(c.remove_key(1, Channel::Y, 12.));
        assert!(!c.remove_key(1, Channel::X, 12.));
        assert_eq!(c.sample(6.)[&1].y, 2.);
        assert!(c.remove_key(1, Channel::Y, 1.));
        assert!(c.tracks.is_empty());
        assert!(c.set_key(1, Channel::X, f32::NAN, 0.).is_err());
        assert!(c.set_key(1, Channel::X, 1., f32::INFINITY).is_err());
    }

    #[test]
    fn supports_original_sixty_second_frames() {
        let clip = Clip {
            fps: 1. / 60.,
            ..Default::default()
        };
        assert!(clip.validate(&HashSet::new()).is_ok());
    }
}
