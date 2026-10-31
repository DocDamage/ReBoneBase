//! Trusted extracted Lua is a development oracle only, never a runtime dependency.
use mlua::{Lua, Table};
use rebonebase::{
    animation::{BonePose, Channel, Clip, Ease, Pose},
    model::Rig,
    skin,
};

fn source() -> Lua {
    let lua = Lua::new();
    lua.globals()
        .set("ROOT", env!("CARGO_MANIFEST_DIR").replace('\\', "/"))
        .unwrap();
    lua.load(
        r#"
      R=dofile(ROOT..'/extension/src/engine/rig.lua')
      T=dofile(ROOT..'/extension/src/engine/templates.lua').bind(R)
      S=dofile(ROOT..'/extension/src/engine/skin.lua')
      A=dofile(ROOT..'/extension/src/engine/anim.lua')
    "#,
    )
    .exec()
    .unwrap();
    lua
}
fn templates() -> Vec<Rig> {
    serde_json::from_str(include_str!("../assets/templates.json")).unwrap()
}
fn set_pose(lua: &Lua, pose: &Pose) {
    let table = lua.create_table().unwrap();
    for (id, p) in pose {
        let t = lua.create_table().unwrap();
        t.set("rot", p.rotation).unwrap();
        t.set("x", p.x).unwrap();
        t.set("y", p.y).unwrap();
        t.set("s", p.scale).unwrap();
        if let Some(z) = p.z {
            t.set("z", z).unwrap();
        }
        if let Some(v) = p.visibility {
            t.set("vis", v).unwrap();
        }
        table.set(*id, t).unwrap();
    }
    lua.globals().set("POSE", table).unwrap();
}
fn assert_close(actual: f32, expected: f32, label: &str) {
    assert!(
        (actual - expected).abs() < 0.001,
        "{label}: native {actual}, original {expected}"
    );
}

#[test]
fn every_original_template_validates_and_matches_rest_and_posed_fk() {
    let lua = source();
    let fixtures = templates();
    assert_eq!(fixtures.len(), 20);
    for rig in fixtures {
        rig.validate()
            .unwrap_or_else(|e| panic!("{}: {e}", rig.name));
        lua.globals().set("NAME", rig.name.as_str()).unwrap();
        for animated in [false, true] {
            let mut pose = Pose::new();
            if animated {
                for b in &rig.bones {
                    pose.insert(
                        b.id,
                        BonePose {
                            rotation: b.id as f32 * 1.7 - 20.,
                            x: 0.3,
                            y: -0.2,
                            scale: 1.15,
                            ..Default::default()
                        },
                    );
                }
            }
            set_pose(&lua, &pose);
            let original: Table = lua
                .load("return R.solve(T.build(NAME,64,64),POSE)")
                .eval()
                .unwrap();
            for w in rig.world(Some(&pose)) {
                let o: Table = original.get(w.id).unwrap();
                for (native, key) in [
                    (w.head.x, "hx"),
                    (w.head.y, "hy"),
                    (w.tail.x, "tx"),
                    (w.tail.y, "ty"),
                    (w.angle, "angle"),
                ] {
                    assert_close(
                        native,
                        o.get(key).unwrap(),
                        &format!("{} bone{} {key}", rig.name, w.id),
                    );
                }
            }
        }
    }
}

#[test]
fn every_original_template_matches_solid_rgba_raster_with_float_boundary_tolerance() {
    let lua = source();
    let mut boundary_pixels = 0;
    let mut checked_pixels = 0;
    for rig in templates() {
        lua.globals().set("NAME", rig.name.as_str()).unwrap();
        let pixels: Table = lua
            .load(
                r#"
          local d=T.build(NAME,64,64)
          DOC=d; WORLD=R.solve(d)
          local layers=S.render(d,R.solve(d),{outline='none'})
          return S.compose(layers,64,64,{fillAlpha=255}).d
        "#,
            )
            .eval()
            .unwrap();
        let original: Vec<u8> = pixels
            .sequence_values::<u8>()
            .collect::<mlua::Result<_>>()
            .unwrap();
        let native = skin::render(&rig, None);
        let mismatches = native
            .as_raw()
            .as_chunks::<4>()
            .0
            .iter()
            .zip(original.as_chunks::<4>().0.iter())
            .filter(|(a, b)| a != b)
            .count();
        checked_pixels += 64 * 64;
        if mismatches != 0 {
            let world = rig.world(None);
            for (index, (actual, expected)) in native
                .as_raw()
                .as_chunks::<4>()
                .0
                .iter()
                .zip(original.as_chunks::<4>().0.iter())
                .enumerate()
                .filter(|(_, (a, b))| a != b)
            {
                let point = rebonebase::model::Vec2::new(
                    (index % 64) as f32 + 0.5,
                    (index / 64) as f32 + 0.5,
                );
                let mut at_boundary = false;
                for part in rig
                    .parts
                    .iter()
                    .filter(|p| p.color.as_slice() == actual || p.color.as_slice() == expected)
                {
                    let distance = rig
                        .bones
                        .iter()
                        .zip(&world)
                        .filter(|(b, _)| b.part == part.id)
                        .map(|(b, w)| skin::bone_distance(b, w, point))
                        .reduce(|a, b| skin::smooth_min(a, b, part.blend))
                        .unwrap_or(f32::INFINITY);
                    lua.globals().set("PID", part.id).unwrap();
                    lua.globals().set("PX", point.x).unwrap();
                    lua.globals().set("PY", point.y).unwrap();
                    let source_distance:f64=lua.load("local bones={} local blend=0 for _,p in ipairs(DOC.parts) do if p.id==PID then blend=p.blend end end for _,b in ipairs(DOC.bones) do if b.part==PID then bones[#bones+1]=b end end return S.partDistance(bones,WORLD,blend,PX,PY)").eval().unwrap();
                    at_boundary |= distance.abs() < 0.0001 && source_distance.abs() < 0.0001;
                }
                assert!(
                    at_boundary,
                    "{} has non-boundary raster mismatch at ({},{}): {actual:?}/{expected:?}",
                    rig.name,
                    index % 64,
                    index / 64
                );
                boundary_pixels += 1;
            }
        }
    }
    assert_eq!(checked_pixels, 81_920);
    assert!(
        boundary_pixels <= 16,
        "Unexpected boundary drift: {boundary_pixels} pixels"
    );
    eprintln!(
        "Raster oracle: {checked_pixels} pixels, {boundary_pixels} float-boundary classifications differ; all other RGBA pixels exact"
    );
}

#[test]
fn animation_matches_original_easing_and_rotation_seam() {
    let lua = source();
    for (ease, name) in [
        (Ease::Linear, "Linear"),
        (Ease::Ease, "Ease"),
        (Ease::EaseIn, "Ease in"),
        (Ease::EaseOut, "Ease out"),
        (Ease::Back, "Back"),
        (Ease::Bounce, "Bounce"),
        (Ease::Step, "Step"),
    ] {
        for channel in [Channel::Rotation, Channel::X, Channel::Scale] {
            let mut clip = Clip::default();
            let (a, b) = if channel == Channel::Scale {
                (0.2, 3.9)
            } else {
                (170., -170.)
            };
            clip.set_key(1, channel, 3., a).unwrap();
            clip.set_key(1, channel, 9., b).unwrap();
            clip.tracks[0].keys[0].easing = ease;
            clip.tracks[0].keys[1].easing = ease;
            lua.globals()
                .set(
                    "CH",
                    match channel {
                        Channel::Rotation => "rot",
                        Channel::Scale => "s",
                        _ => "x",
                    },
                )
                .unwrap();
            lua.globals().set("EASE", name).unwrap();
            lua.globals().set("AV", a).unwrap();
            lua.globals().set("BV", b).unwrap();
            for frame in [1., 2., 3., 4.5, 6., 8., 9., 10., 11.5, 12.] {
                lua.globals().set("F", frame).unwrap();
                let original:f32=lua.load("return A.value({frames=12,loop=true},{ch=CH,keys={{3,AV,EASE},{9,BV,EASE}}},F)").eval().unwrap();
                assert_close(
                    clip.value(&clip.tracks[0], frame),
                    original,
                    &format!("{name} {channel:?} frame{frame}"),
                );
            }
        }
    }
}

#[test]
fn analytic_ik_matches_original_reach_and_bend() {
    let lua = source();
    for (x, y) in [(6., 2.), (100., 0.), (0., 0.), (1., 0.)] {
        for bend in [-1., 1.] {
            lua.globals().set("X", x).unwrap();
            lua.globals().set("Y", y).unwrap();
            lua.globals().set("BEND", bend).unwrap();
            let (upper, lower): (f32, f32) = lua
                .load("return A.solve2(0,0,5,4,X,Y,BEND)")
                .eval()
                .unwrap();
            let native = rebonebase::ik::solve_two_bone(
                Default::default(),
                rebonebase::model::Vec2::new(x, y),
                5.,
                4.,
                bend,
            );
            assert_close(native.angle1, upper, "IK upper angle");
            assert_close(
                rebonebase::animation::wrap_degrees(native.angle1 + native.angle2 - lower),
                0.,
                "IK lower angle",
            );
        }
    }
}
