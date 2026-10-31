use crate::{features::ReferenceSequence, project};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn load_sequence(paths: &[PathBuf], fps: f32) -> Result<ReferenceSequence> {
    ensure!(
        !paths.is_empty()
            && paths.len() <= 240
            && fps.is_finite()
            && (1. / 60. ..=240.).contains(&fps),
        "Choose 1–240 image frames and valid FPS"
    );
    let mut paths = paths.to_vec();
    paths.sort_by(|a, b| natural(&a.to_string_lossy(), &b.to_string_lossy()));
    let mut pixels = 0u64;
    let mut frames = vec![];
    for path in paths {
        let frame = project::import_reference(&path)?;
        pixels += frame.width as u64 * frame.height as u64;
        ensure!(
            pixels <= 16 * 1024 * 1024,
            "Reference sequence exceeds 16 megapixels"
        );
        frames.push(frame);
    }
    Ok(ReferenceSequence {
        frames,
        fps,
        start_ms: 0,
        looped: true,
    })
}
fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let (si, sj) = (i, j);
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let (mut x, mut y) = (&a[si..i], &b[sj..j]);
            while x.len() > 1 && x[0] == b'0' {
                x = &x[1..];
            }
            while y.len() > 1 && y[0] == b'0' {
                y = &y[1..];
            }
            let order = x.len().cmp(&y.len()).then_with(|| x.cmp(y));
            if !order.is_eq() {
                return order;
            }
        } else {
            let order = a[i].to_ascii_lowercase().cmp(&b[j].to_ascii_lowercase());
            if !order.is_eq() {
                return order;
            }
            i += 1;
            j += 1;
        }
    }
    a.len().cmp(&b.len())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct VideoOptions {
    pub fps: f32,
    pub seconds: f32,
    pub start: f32,
    pub size: u32,
    pub ffmpeg: PathBuf,
}
impl Default for VideoOptions {
    fn default() -> Self {
        Self {
            fps: 12.,
            seconds: 5.,
            start: 0.,
            size: 256,
            ffmpeg: PathBuf::from("ffmpeg"),
        }
    }
}
pub fn video_frames(path: &Path, options: &VideoOptions) -> Result<ReferenceSequence> {
    ensure!(
        options.fps.is_finite()
            && (1. ..=60.).contains(&options.fps)
            && options.seconds.is_finite()
            && (0.1..=60.).contains(&options.seconds)
            && options.start.is_finite()
            && options.start >= 0.
            && (32..=256).contains(&options.size),
        "Invalid video extraction settings"
    );
    ensure!(path.is_file(), "Video file does not exist");
    let temp = std::env::temp_dir().join(format!("rebonebase-video-{}", crate::storage::unique()));
    std::fs::create_dir(&temp)?;
    let result = (|| -> Result<ReferenceSequence> {
        let log = std::fs::File::create(temp.join("ffmpeg.log"))?;
        let filter = format!(
            "fps={},scale={}:{}:force_original_aspect_ratio=decrease",
            options.fps, options.size, options.size
        );
        let mut command = Command::new(&options.ffmpeg);
        command
            .args([
                "-nostdin",
                "-hide_banner",
                "-loglevel",
                "error",
                "-protocol_whitelist",
                "file,pipe",
                "-ss",
            ])
            .arg(options.start.to_string())
            .arg("-i")
            .arg(path)
            .arg("-t")
            .arg(options.seconds.to_string())
            .args(["-vf", &filter, "-frames:v", "240", "-an", "-sn"])
            .arg(temp.join("frame-%05d.png"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::from(log));
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|e| {
            anyhow::anyhow!(
                "Could not start FFmpeg: {e}. Select its executable in Studio settings."
            )
        })?;
        let deadline = Instant::now() + Duration::from_secs(90);
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break status;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                bail!("Video decode exceeded 90 seconds");
            }
            std::thread::sleep(Duration::from_millis(30));
        };
        if !status.success() {
            let mut bytes = Vec::new();
            std::fs::File::open(temp.join("ffmpeg.log"))?
                .take(4096)
                .read_to_end(&mut bytes)?;
            bail!("Video decode failed: {}", String::from_utf8_lossy(&bytes));
        }
        let paths: Vec<_> = std::fs::read_dir(&temp)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "png"))
            .collect();
        load_sequence(&paths, options.fps)
    })();
    let _ = std::fs::remove_dir_all(&temp);
    result
}
