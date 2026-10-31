use rebonebase::{
    animation::Channel,
    aseprite,
    model::{Rig, Vec2},
    skin,
};
use std::io::Cursor;

fn fixture() -> Rig {
    let mut rig = Rig::new("Layered animation", 24, 24);
    let red = rig.add_part("Body", [255, 0, 0, 255]);
    let blue = rig.add_part("Arm 左", [0, 0, 255, 255]);
    rig.parts[0].z = -2;
    rig.parts[1].z = 3;
    let a = rig
        .add_bone(Vec2::new(5., 12.), Vec2::new(19., 12.), None)
        .unwrap();
    let b = rig
        .add_bone(Vec2::new(12., 5.), Vec2::new(12., 19.), None)
        .unwrap();
    rig.bone_mut(a).unwrap().part = red;
    rig.bone_mut(b).unwrap().part = blue;
    rig.clips[0].length = 3;
    rig.clips[0].fps = 12.5;
    rig.clips[0].set_key(b, Channel::X, 1., 0.).unwrap();
    rig.clips[0].set_key(b, Channel::X, 3., 2.).unwrap();
    rig
}
fn parse(rig: &Rig) -> asefile::AsepriteFile {
    asefile::AsepriteFile::read(Cursor::new(aseprite::encode(rig).unwrap())).unwrap()
}

#[test]
fn independently_decodes_layers_frames_timings_and_rgba_pixels() {
    let rig = fixture();
    let file = parse(&rig);
    assert_eq!((file.width(), file.height()), (24, 24));
    assert_eq!(file.num_frames(), 3);
    assert_eq!(file.num_layers(), 2);
    assert_eq!(file.layer(0).name(), "Body");
    assert_eq!(file.layer(1).name(), "Arm 左");
    for frame in 0..3 {
        let pose = rig.active_clip().sample((frame + 1) as f32);
        assert_eq!(file.frame(frame).duration(), 80);
        assert_eq!(
            file.frame(frame).image().as_raw(),
            skin::render(&rig, Some(&pose)).as_raw()
        );
        for layer in 0..2 {
            let mut single = rig.clone();
            single.parts = vec![rig.parts[layer as usize].clone()];
            assert_eq!(
                file.cel(frame, layer).image().as_raw(),
                skin::render(&single, Some(&pose)).as_raw()
            );
        }
    }
}
fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(bytes[at..at + 2].try_into().unwrap())
}
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}
fn frame_orders(bytes: &[u8]) -> Vec<Vec<(u16, i16)>> {
    assert_eq!(u16_at(bytes, 4), 0xA5E0);
    assert_eq!(u32_at(bytes, 0) as usize, bytes.len());
    let mut at = 128;
    let mut frames = vec![];
    while at < bytes.len() {
        let size = u32_at(bytes, at) as usize;
        assert_eq!(u16_at(bytes, at + 4), 0xF1FA);
        let mut chunk = at + 16;
        let mut order = vec![];
        while chunk < at + size {
            if u16_at(bytes, chunk + 4) == 0x2005 {
                assert_eq!(u16_at(bytes, chunk + 13), 2);
                order.push((
                    u16_at(bytes, chunk + 6),
                    i16::from_le_bytes(bytes[chunk + 15..chunk + 17].try_into().unwrap()),
                ));
            }
            chunk += u32_at(bytes, chunk) as usize;
        }
        assert_eq!(chunk, at + size);
        frames.push(order);
        at += size;
    }
    assert_eq!(at, bytes.len());
    frames
}

#[test]
fn animated_z_and_visibility_keep_independent_part_cels() {
    let mut rig = fixture();
    let blue = rig.parts[1].id;
    for (frame, z, visibility) in [(1., -10., 1.), (2., -10., 0.), (3., 10., 1.)] {
        rig.clips[0].set_key(blue, Channel::Z, frame, z).unwrap();
        rig.clips[0]
            .set_key(blue, Channel::Visibility, frame, visibility)
            .unwrap();
    }
    let bytes = aseprite::encode(&rig).unwrap();
    let file = asefile::AsepriteFile::read(Cursor::new(&bytes)).unwrap();
    let orders = frame_orders(&bytes);
    assert!(file.cel(1, 1).is_empty());
    // asefile 0.3 ignores cel z-index; apply the official ordering to its decoded cels.
    for (frame, mut order) in orders.into_iter().enumerate() {
        order.sort_by_key(|(layer, z)| (i32::from(*layer) + i32::from(*z), *z));
        let mut composite = vec![0u8; 24 * 24 * 4];
        for (layer, _) in order {
            let image = file.cel(frame as u32, u32::from(layer)).image();
            for (out, pixel) in composite
                .chunks_exact_mut(4)
                .zip(image.as_raw().chunks_exact(4))
            {
                if pixel[3] != 0 {
                    out.copy_from_slice(pixel);
                }
            }
        }
        assert_eq!(
            composite,
            skin::render(&rig, Some(&rig.active_clip().sample((frame + 1) as f32))).into_raw()
        );
    }
}

#[test]
fn bad_rigs_and_excessive_work_are_rejected_before_rendering() {
    let mut rig = fixture();
    rig.width = 0;
    assert!(aseprite::encode(&rig).is_err());
    let mut rig = fixture();
    rig.width = 512;
    rig.height = 512;
    for _ in 0..200 {
        rig.add_bone(Vec2::new(1., 1.), Vec2::new(10., 1.), None)
            .unwrap();
    }
    assert!(aseprite::encode(&rig).is_err());
    let mut rig = fixture();
    for _ in 0..250 {
        rig.add_part("Unused", [1, 2, 3, 255]);
    }
    rig.width = 512;
    rig.height = 512;
    assert!(aseprite::encode(&rig).is_err());
}

#[test]
fn slow_original_timing_and_translucent_layer_roundtrip() {
    let mut rig = fixture();
    rig.clips[0].fps = 1. / 60.;
    rig.parts[1].color[3] = 128;
    let file = parse(&rig);
    assert_eq!(file.frame(0).duration(), 60_000);
    let mut single = rig.clone();
    single.parts = vec![rig.parts[1].clone()];
    assert_eq!(
        file.cel(0, 1).image().as_raw(),
        skin::render(&single, Some(&rig.active_clip().sample(1.))).as_raw()
    );
}

#[test]
fn static_layers_follow_z_order_and_keep_equal_z_stable() {
    let mut rig = fixture();
    rig.parts[0].z = 4;
    rig.parts[1].z = -3;
    let file = parse(&rig);
    assert_eq!(file.layer(0).name(), "Arm 左");
    assert_eq!(file.layer(1).name(), "Body");
    assert_eq!(
        file.frame(0).image().as_raw(),
        skin::render(&rig, Some(&rig.active_clip().sample(1.))).as_raw()
    );
    rig.parts[0].z = 0;
    rig.parts[1].z = 0;
    let file = parse(&rig);
    assert_eq!(file.layer(0).name(), "Body");
    assert_eq!(file.layer(1).name(), "Arm 左");
}

#[test]
fn empty_groups_remain_editable_layers_without_ink_cels() {
    let mut rig = fixture();
    rig.add_part("Empty group", [1, 2, 3, 255]);
    let file = parse(&rig);
    assert_eq!(file.num_layers(), 3);
    let layer = file.layer_by_name("Empty group").unwrap();
    assert!(layer.frame(0).is_empty());
    assert!(aseprite::encode(&Rig::new("No groups", 24, 24)).is_err());
}
