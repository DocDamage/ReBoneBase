use rebonebase::{
    batch::{self, ExportFormat, ExportPreset},
    model::Rig,
    project::{self, Project},
    references,
};

#[test]
fn indexed_png_has_palette_and_preserves_transparency() {
    let mut img = image::RgbaImage::new(2, 1);
    img.put_pixel(1, 0, image::Rgba([255, 0, 0, 255]));
    let bytes = batch::indexed_png(&img, &[[255, 0, 0, 255]]).unwrap();
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().unwrap();
    assert_eq!(reader.info().color_type, png::ColorType::Indexed);
    let mut raw = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut raw).unwrap();
    assert_eq!(info.width, 2);
    assert_ne!(raw[0], raw[1]);
    assert!(reader.info().trns.as_ref().unwrap().contains(&0));
}

#[test]
fn batch_exports_all_clips_with_metadata_and_rejects_overwrite() {
    let mut p = Project::new(Rig::new("hero", 8, 8));
    p.rig.clips[0].length = 2;
    p.rig.clips.push(p.rig.clips[0].clone());
    p.rig.clips[1].name = "../unsafe".into();
    let dir = std::env::temp_dir().join(format!("rebone-batch-{}", std::process::id()));
    let preset = ExportPreset {
        format: ExportFormat::Sheet,
        all_clips: true,
        ..Default::default()
    };
    let report = batch::export(&p, &dir, &preset, false).unwrap();
    assert!(report.files.len() >= 3);
    assert!(dir.join("atlas.json").is_file());
    assert!(report.files.iter().all(|f| f.starts_with(&dir)));
    assert!(batch::export(&p, &dir, &preset, false).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn image_sequences_sort_frames_and_obey_memory_limits() {
    let dir = std::env::temp_dir().join(format!("rebone-refs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let red = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
    let blue = image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 255, 255]));
    red.save(dir.join("002.png")).unwrap();
    blue.save(dir.join("001.png")).unwrap();
    let seq = references::load_sequence(&[dir.join("002.png"), dir.join("001.png")], 12.).unwrap();
    assert_eq!(seq.frames[0].pixels[2], 255);
    let mut p = Project::new(Rig::new("ref", 8, 8));
    p.extras.references = Some(seq);
    p.validate().unwrap();
    project::save(&dir.join("ref.rebone"), &p).unwrap();
    assert_eq!(project::load(&dir.join("ref.rebone")).unwrap(), p);
    assert!(references::load_sequence(&[], 12.).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
