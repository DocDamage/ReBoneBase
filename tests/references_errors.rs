use rebonebase::{
    references::{self, VideoOptions},
    storage,
};
use std::path::PathBuf;

struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("rebone-reference-errors-{}", storage::unique()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        if self
            .0
            .canonicalize()
            .is_ok_and(|p| p.starts_with(std::env::temp_dir().canonicalize().unwrap()))
        {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[test]
fn extraction_settings_and_missing_inputs_fail_before_starting_a_decoder() {
    let missing = std::env::temp_dir().join(format!("missing-video-{}", storage::unique()));
    for settings in [
        VideoOptions {
            fps: f32::NAN,
            ..Default::default()
        },
        VideoOptions {
            seconds: 0.,
            ..Default::default()
        },
        VideoOptions {
            start: -1.,
            ..Default::default()
        },
        VideoOptions {
            size: 31,
            ..Default::default()
        },
    ] {
        assert!(
            references::video_frames(&missing, &settings)
                .unwrap_err()
                .to_string()
                .contains("Invalid video extraction settings")
        );
    }
    assert!(
        references::video_frames(&missing, &VideoOptions::default())
            .unwrap_err()
            .to_string()
            .contains("does not exist")
    );
}

#[test]
fn unavailable_ffmpeg_reports_an_actionable_error_and_keeps_the_source() {
    let files = Files::new();
    let source = files.0.join("source.mp4");
    std::fs::write(&source, b"source bytes must survive").unwrap();
    let options = VideoOptions {
        ffmpeg: files.0.join("missing-ffmpeg-executable"),
        ..Default::default()
    };
    let error = references::video_frames(&source, &options)
        .unwrap_err()
        .to_string();
    assert!(error.contains("Could not start FFmpeg"));
    assert!(error.contains("Select its executable"));
    assert_eq!(std::fs::read(source).unwrap(), b"source bytes must survive");
}

#[test]
fn sequence_loading_uses_natural_order_with_zero_padding_and_case() {
    let files = Files::new();
    let fixtures = [
        ("frame_10.png", [0, 0, 255, 255]),
        ("frame_2.png", [0, 255, 0, 255]),
        ("frame_001.png", [255, 0, 0, 255]),
        ("frame_a.png", [255, 255, 0, 255]),
        ("frame_B.png", [0, 0, 0, 255]),
    ];
    let mut paths = vec![];
    for (name, color) in fixtures {
        let path = files.0.join(name);
        image::RgbaImage::from_pixel(1, 1, image::Rgba(color))
            .save(&path)
            .unwrap();
        paths.push(path);
    }
    let sequence = references::load_sequence(&paths, 12.).unwrap();
    assert_eq!(
        sequence
            .frames
            .iter()
            .map(|r| r.pixels[..4].to_vec())
            .collect::<Vec<_>>(),
        vec![
            vec![255, 0, 0, 255],
            vec![0, 255, 0, 255],
            vec![0, 0, 255, 255],
            vec![255, 255, 0, 255],
            vec![0, 0, 0, 255]
        ]
    );
    assert!(references::load_sequence(&paths, f32::INFINITY).is_err());
}

#[test]
fn sequence_decoder_enforces_aggregate_pixels_even_when_reusing_a_source_image() {
    let files = Files::new();
    let path = files.0.join("large.png");
    image::RgbaImage::from_pixel(1600, 1600, image::Rgba([10, 20, 30, 255]))
        .save(&path)
        .unwrap();
    let error = references::load_sequence(&vec![path; 7], 12.)
        .unwrap_err()
        .to_string();
    assert!(error.contains("16 megapixels"));
}

#[test]
#[ignore = "Runs installed FFmpeg against malformed media; include explicitly on a decoder-equipped host"]
fn a_real_decoder_failure_returns_bounded_diagnostics_without_touching_input() {
    let files = Files::new();
    let path = files.0.join("malformed.mp4");
    std::fs::write(&path, b"not a valid video container").unwrap();
    let ffmpeg = std::env::var_os("REBONEBASE_FFMPEG")
        .map(PathBuf::from)
        .or_else(|| {
            let path = PathBuf::from("C:/Program Files/Krita (x64)/bin/ffmpeg.exe");
            path.is_file().then_some(path)
        })
        .unwrap_or_else(|| PathBuf::from("ffmpeg"));
    let error = references::video_frames(
        &path,
        &VideoOptions {
            ffmpeg,
            ..Default::default()
        },
    )
    .unwrap_err()
    .to_string();
    assert!(error.starts_with("Video decode failed:"), "{error}");
    assert!(error.len() < 4300);
    assert_eq!(std::fs::read(path).unwrap(), b"not a valid video container");
}
