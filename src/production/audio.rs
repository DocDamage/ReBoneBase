//! Bounded PCM16 WAV import, waveform analysis and native Windows preview playback.
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Instant};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AudioTrack {
    #[serde(with = "blob")]
    pub pcm: Arc<Vec<u8>>,
    pub sample_rate: u32,
    pub channels: u16,
    pub start_ms: u64,
    pub volume: f32,
    pub enabled: bool,
    pub name: String,
}
impl AudioTrack {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (8000..=192000).contains(&self.sample_rate)
                && (1..=2).contains(&self.channels)
                && !self.pcm.is_empty()
                && self.pcm.len() <= 32 * 1024 * 1024
                && self.pcm.len().is_multiple_of(self.channels as usize * 2)
                && self.volume.is_finite()
                && (0. ..=2.).contains(&self.volume)
                && self.name.len() <= 1024
                && self.start_ms <= 86_400_000,
            "Audio must be bounded mono/stereo PCM16 WAV with valid timing"
        );
        Ok(())
    }
    pub fn duration_ms(&self) -> u64 {
        self.pcm.len() as u64 * 1000 / (self.sample_rate as u64 * self.channels as u64 * 2)
    }
    pub fn samples(&self) -> Vec<i16> {
        self.pcm
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect()
    }
}
pub fn decode_wav(bytes: &[u8]) -> Result<AudioTrack> {
    ensure!(
        bytes.len() >= 12
            && bytes.len() <= 40 * 1024 * 1024
            && &bytes[..4] == b"RIFF"
            && &bytes[8..12] == b"WAVE",
        "Choose a PCM16 WAV file under 40 MB"
    );
    let mut offset = 12usize;
    let mut format = None;
    let mut pcm = None;
    while offset + 8 <= bytes.len() {
        let length = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into()?) as usize;
        let start = offset + 8;
        let end = start
            .checked_add(length)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| anyhow::anyhow!("Truncated WAV chunk"))?;
        match &bytes[offset..offset + 4] {
            b"fmt " => {
                ensure!(length >= 16, "Invalid WAV format chunk");
                let kind = u16::from_le_bytes(bytes[start..start + 2].try_into()?);
                let channels = u16::from_le_bytes(bytes[start + 2..start + 4].try_into()?);
                let rate = u32::from_le_bytes(bytes[start + 4..start + 8].try_into()?);
                let bits = u16::from_le_bytes(bytes[start + 14..start + 16].try_into()?);
                ensure!(
                    kind == 1 && bits == 16,
                    "Audio import supports uncompressed PCM16 WAV; convert other formats first"
                );
                format = Some((channels, rate));
            }
            b"data" => {
                ensure!(
                    length <= 32 * 1024 * 1024,
                    "Audio exceeds 32 MB decoded budget"
                );
                pcm = Some(Arc::new(bytes[start..end].to_vec()));
            }
            _ => {}
        }
        offset = end + length % 2;
    }
    let (channels, sample_rate) =
        format.ok_or_else(|| anyhow::anyhow!("WAV format chunk is missing"))?;
    let track = AudioTrack {
        pcm: pcm.ok_or_else(|| anyhow::anyhow!("WAV data chunk is missing"))?,
        sample_rate,
        channels,
        start_ms: 0,
        volume: 1.,
        enabled: true,
        name: "Audio".into(),
    };
    track.validate()?;
    Ok(track)
}
pub fn load_wav(path: &std::path::Path) -> Result<AudioTrack> {
    ensure!(
        std::fs::metadata(path)?.len() <= 40 * 1024 * 1024,
        "WAV exceeds 40 MB"
    );
    let mut track = decode_wav(&std::fs::read(path)?)?;
    track.name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .take(1024)
        .collect();
    Ok(track)
}
pub fn encode_wav(samples: &[i16], sample_rate: u32, channels: u16) -> Vec<u8> {
    let size = samples.len().saturating_mul(2);
    let mut out = Vec::with_capacity(44 + size);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36u32.saturating_add(size as u32)).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(
        &sample_rate
            .saturating_mul(channels as u32 * 2)
            .to_le_bytes(),
    );
    out.extend_from_slice(&(channels * 2).to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(size as u32).to_le_bytes());
    for value in samples {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}
pub fn waveform(track: &AudioTrack, bins: usize) -> Vec<(f32, f32)> {
    let bins = bins.clamp(1, 4096);
    let frames = track.pcm.len() / (track.channels as usize * 2);
    (0..bins)
        .map(|i| {
            let start = i * frames / bins;
            let end = ((i + 1) * frames / bins).max(start + 1).min(frames);
            let mut min = 0f32;
            let mut max = 0f32;
            for f in start..end {
                for ch in 0..track.channels as usize {
                    let p = (f * track.channels as usize + ch) * 2;
                    if p + 1 < track.pcm.len() {
                        let v =
                            i16::from_le_bytes([track.pcm[p], track.pcm[p + 1]]) as f32 / 32768.;
                        min = min.min(v);
                        max = max.max(v);
                    }
                }
            }
            (min, max)
        })
        .collect()
}
pub fn playback_wav(track: &AudioTrack, timeline_ms: u64, max_ms: u64) -> Vec<u8> {
    if !track.enabled || timeline_ms < track.start_ms {
        return vec![];
    }
    let relative = timeline_ms - track.start_ms;
    let first =
        (relative as u128 * track.sample_rate as u128 / 1000).min(usize::MAX as u128) as usize;
    let count =
        (max_ms as u128 * track.sample_rate as u128 / 1000).min(usize::MAX as u128) as usize;
    let width = track.channels as usize * 2;
    let start = first.saturating_mul(width);
    if start >= track.pcm.len() {
        return vec![];
    }
    let end = first
        .saturating_add(count)
        .saturating_mul(width)
        .min(track.pcm.len());
    let samples: Vec<_> = track.pcm[start..end]
        .chunks_exact(2)
        .map(|v| {
            (i16::from_le_bytes([v[0], v[1]]) as f32 * track.volume)
                .round()
                .clamp(-32768., 32767.) as i16
        })
        .collect();
    encode_wav(&samples, track.sample_rate, track.channels)
}
/// Source audio time advances continuously within each held frame. Reverse
/// traversal reverses PCM frames while retaining the order of stereo channels.
pub fn playback_cursor(clip: &crate::animation::Clip, elapsed: f64) -> (u64, bool) {
    let frames = clip.frame_sequence();
    if frames.is_empty() {
        return (0, false);
    }
    let total = clip.total_duration_ms().max(1) as f64;
    let mut t = if elapsed.is_finite() {
        elapsed.max(0.)
    } else {
        0.
    };
    if clip.looped {
        t = t.rem_euclid(total);
    } else if t >= total {
        return match clip.playback {
            crate::animation::Playback::Forward => (
                (1..=clip.length).map(|f| clip.duration_ms(f) as u64).sum(),
                false,
            ),
            _ => (0, true),
        };
    }
    for (index, frame) in frames.iter().enumerate() {
        let duration = clip.duration_ms(*frame) as f64;
        if t < duration {
            let reverse = clip.playback == crate::animation::Playback::Reverse
                || (clip.playback == crate::animation::Playback::PingPong
                    && index >= clip.length as usize);
            let origin: u64 = (1..*frame).map(|f| clip.duration_ms(f) as u64).sum();
            return (
                origin
                    + if reverse {
                        (duration - t).max(0.) as u64
                    } else {
                        t as u64
                    },
                reverse,
            );
        }
        t -= duration;
    }
    (0, false)
}
pub fn playback_wav_direction(
    track: &AudioTrack,
    time: u64,
    max_ms: u64,
    reverse: bool,
) -> Vec<u8> {
    if !reverse {
        return playback_wav(track, time, max_ms);
    }
    if !track.enabled || time < track.start_ms {
        return vec![];
    }
    let relative = time - track.start_ms;
    let first =
        (relative as u128 * track.sample_rate as u128 / 1000).min(usize::MAX as u128) as usize;
    let count =
        (max_ms as u128 * track.sample_rate as u128 / 1000).min(usize::MAX as u128) as usize;
    let width = track.channels as usize * 2;
    let frames = track.pcm.len() / width;
    if first > frames || first == 0 {
        return vec![];
    }
    let start = first.saturating_sub(count);
    let mut samples = Vec::with_capacity((first - start) * track.channels as usize);
    for frame in track.pcm[start * width..first * width]
        .chunks_exact(width)
        .rev()
    {
        for value in frame.chunks_exact(2) {
            samples.push(
                (i16::from_le_bytes([value[0], value[1]]) as f32 * track.volume)
                    .round()
                    .clamp(-32768., 32767.) as i16,
            );
        }
    }
    encode_wav(&samples, track.sample_rate, track.channels)
}
#[derive(Default)]
pub struct Player {
    buffer: Vec<u8>,
    anchor: u64,
    started: Option<Instant>,
    identity: Option<usize>,
    reverse: bool,
}
impl Player {
    pub fn stop(&mut self) {
        if self.started.is_some() {
            native_stop();
        }
        self.started = None;
        self.identity = None;
        self.buffer.clear();
    }
    pub fn play(&mut self, track: &AudioTrack, time: u64, max_ms: u64) -> Result<()> {
        self.play_direction(track, time, max_ms, false)
    }
    pub fn play_direction(
        &mut self,
        track: &AudioTrack,
        time: u64,
        max_ms: u64,
        reverse: bool,
    ) -> Result<()> {
        track.validate()?;
        self.stop();
        self.buffer = playback_wav_direction(track, time, max_ms, reverse);
        if self.buffer.is_empty() {
            return Ok(());
        }
        if !native_play(&self.buffer) {
            self.buffer.clear();
            bail!("Audio playback is unavailable on this device");
        }
        self.anchor = time;
        self.reverse = reverse;
        self.identity = Some(Arc::as_ptr(&track.pcm) as usize);
        self.started = Some(Instant::now());
        Ok(())
    }
    pub fn sync(&mut self, track: Option<&AudioTrack>, time: u64, playing: bool) -> Result<()> {
        self.sync_direction(track, time, playing, false)
    }
    pub fn sync_direction(
        &mut self,
        track: Option<&AudioTrack>,
        time: u64,
        playing: bool,
        reverse: bool,
    ) -> Result<()> {
        let Some(track) = track.filter(|a| a.enabled && playing && time >= a.start_ms) else {
            self.stop();
            return Ok(());
        };
        let identity = Arc::as_ptr(&track.pcm) as usize;
        let expected = self.started.map(|s| {
            if reverse {
                self.anchor.saturating_sub(s.elapsed().as_millis() as u64)
            } else {
                self.anchor + s.elapsed().as_millis() as u64
            }
        });
        if self.identity != Some(identity)
            || self.reverse != reverse
            || expected.is_none_or(|t| t.abs_diff(time) > 120)
        {
            self.play_direction(track, time, track.duration_ms(), reverse)?;
        }
        Ok(())
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}
#[cfg(windows)]
#[link(name = "winmm")]
unsafe extern "system" {
    fn PlaySoundW(psz_sound: *const u16, hmod: *mut std::ffi::c_void, flags: u32) -> i32;
}
#[cfg(windows)]
fn native_play(bytes: &[u8]) -> bool {
    // SND_MEMORY requires this allocation to live until a synchronous stop.
    unsafe {
        PlaySoundW(
            bytes.as_ptr().cast(),
            std::ptr::null_mut(),
            0x0004 | 0x0001 | 0x0002,
        ) != 0
    }
}
#[cfg(windows)]
fn native_stop() {
    unsafe {
        PlaySoundW(std::ptr::null(), std::ptr::null_mut(), 0);
    }
}
#[cfg(not(windows))]
fn native_play(_: &[u8]) -> bool {
    false
}
#[cfg(not(windows))]
fn native_stop() {}
mod blob {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use serde::{Deserializer, Serializer, de::Error};
    pub fn serialize<S: Serializer>(data: &Arc<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(data.as_slice()))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Arc<Vec<u8>>, D::Error> {
        let encoded = String::deserialize(d)?;
        if encoded.len() > 45 * 1024 * 1024 {
            return Err(D::Error::custom("Audio base64 exceeds budget"));
        }
        STANDARD
            .decode(encoded)
            .map(Arc::new)
            .map_err(D::Error::custom)
    }
}
