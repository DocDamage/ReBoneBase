use rebonebase::{
    references::{self, VideoOptions},
    storage,
};
use std::{path::PathBuf, process::Command};

#[test]
#[ignore = "Runs the installed external FFmpeg decoder; invoke explicitly on hosts with FFmpeg"]
fn real_ffmpeg_extracts_a_generated_animated_reference() {
    let ffmpeg = std::env::var_os("REBONEBASE_FFMPEG")
        .map(PathBuf::from)
        .or_else(|| {
            let p = PathBuf::from("C:/Program Files/Krita (x64)/bin/ffmpeg.exe");
            p.is_file().then_some(p)
        })
        .unwrap_or_else(|| PathBuf::from("ffmpeg"));
    assert!(
        Command::new(&ffmpeg)
            .arg("-version")
            .output()
            .expect("Install FFmpeg or set REBONEBASE_FFMPEG")
            .status
            .success()
    );
    let dir = std::env::temp_dir().join(format!(
        "rebone-video-smoke-{}-{}",
        std::process::id(),
        storage::unique()
    ));
    std::fs::create_dir(&dir).unwrap();
    let path = dir.join("generated.gif");
    let mut encoder = image::codecs::gif::GifEncoder::new(std::fs::File::create(&path).unwrap());
    for color in [
        [255, 0, 0, 255],
        [0, 0, 255, 255],
        [255, 0, 0, 255],
        [0, 0, 255, 255],
    ] {
        encoder
            .encode_frame(image::Frame::from_parts(
                image::RgbaImage::from_pixel(4, 4, image::Rgba(color)),
                0,
                0,
                image::Delay::from_numer_denom_ms(100, 1),
            ))
            .unwrap();
    }
    drop(encoder);
    let sequence = references::video_frames(
        &path,
        &VideoOptions {
            fps: 10.,
            seconds: 0.4,
            start: 0.,
            size: 32,
            ffmpeg,
        },
    )
    .unwrap();
    assert_eq!(sequence.frames.len(), 4);
    assert_eq!(sequence.fps, 10.);
    assert_eq!(
        (sequence.frames[0].width, sequence.frames[0].height),
        (32, 32)
    );
    assert!(sequence.frames[0].pixels[0] > 200);
    assert!(sequence.frames[1].pixels[2] > 200);
    assert!(
        dir.canonicalize()
            .unwrap()
            .starts_with(std::env::temp_dir().canonicalize().unwrap())
    );
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(dir).unwrap();
}
