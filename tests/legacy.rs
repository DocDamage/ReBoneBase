use mlua::Lua;
use rebonebase::legacy;

fn original_file() -> String {
    let lua = Lua::new();
    lua.globals()
        .set("ROOT", env!("CARGO_MANIFEST_DIR").replace('\\', "/"))
        .unwrap();
    lua.load(
        r#"
      local R=dofile(ROOT..'/extension/src/engine/rig.lua')
      local S=dofile(ROOT..'/extension/src/kit/serial.lua')
      local A=dofile(ROOT..'/extension/src/engine/anim.lua')
      local T=dofile(ROOT..'/extension/src/engine/templates.lua').bind(R)
      local d=T.build('Humanoid',64,64)
      d.libName='A "quoted" rig\nwith Unicode: 雪'
      A.setKey(d.anim,d.bones[1].id,'rot',1,10,'Linear')
      A.setKey(d.anim,d.bones[1].id,'rot',7,40,'Ease')
      return R.encode(S,d)
    "#,
    )
    .eval()
    .unwrap()
}
#[test]
fn imports_actual_original_rig_and_animation_without_execution() {
    let (rig, warnings) = legacy::import(&original_file()).unwrap();
    assert_eq!((rig.width, rig.height), (64, 64));
    assert_eq!(rig.bones.len(), 16);
    assert_eq!(rig.clips[0].length, 12);
    assert_eq!(rig.clips[0].fps, 10.);
    assert_eq!(rig.clips[0].tracks[0].keys.len(), 2);
    assert!((rig.clips[0].sample(4.)[&rig.bones[0].id].rotation - 25.).abs() < 0.001);
    assert!(warnings.iter().all(|w| !w.contains("animation tracks")));
    rig.validate().unwrap();
}
#[test]
fn rejects_code_expressions_deep_or_oversized_inputs() {
    for text in [
        "return os.execute('bad')",
        "while true do end",
        "return {canvas={w=64,h=64},bones={},parts={}}; print('bad')",
        "return {x=1+2}",
        "return (function() return {} end)()",
        "return {x=0/0}",
        "return {x=1e309}",
    ] {
        assert!(legacy::import(text).is_err(), "accepted {text}");
    }
    let deep = format!("return {}{}", "{".repeat(70), "}".repeat(70));
    assert!(legacy::import(&deep).is_err());
    assert!(legacy::import(&" ".repeat(16 * 1024 * 1024 + 1)).is_err());
}
#[test]
fn warns_for_unsupported_motion_and_reference_features() {
    let text = original_file()
        .replace("images={}", "images={asset='bytes'}")
        .replace("poses={}", "poses={{name='pose'}}");
    let (_, warnings) = legacy::import(&text).unwrap();
    assert!(warnings.iter().any(|s| s.contains("reference")));
    assert!(warnings.iter().any(|s| s.contains("pose")));
}

#[test]
fn preserves_original_slow_clip_timing() {
    let text = original_file().replace("frameMs=100,", "frameMs=60000,");
    let (rig, _) = legacy::import(&text).unwrap();
    assert!((rig.clips[0].fps - 1. / 60.).abs() < 0.000001);
}

#[test]
fn rejects_duplicate_keys_sparse_arrays_and_non_finite_geometry() {
    for text in [
        "return {canvas={},canvas={},bones={},parts={}}",
        "return {canvas={w=64,h=64},bones={[2]={}},parts={}}",
    ] {
        assert!(legacy::import(text).is_err());
    }
    let text = original_file().replacen("len=6,", "len=1e308,", 1);
    assert!(legacy::import(&text).is_err());
}

#[test]
fn imports_every_original_template_file() {
    let lua = Lua::new();
    lua.globals()
        .set("ROOT", env!("CARGO_MANIFEST_DIR").replace('\\', "/"))
        .unwrap();
    let files:mlua::Table=lua.load(r#"
      local R=dofile(ROOT..'/extension/src/engine/rig.lua')
      local S=dofile(ROOT..'/extension/src/kit/serial.lua')
      local T=dofile(ROOT..'/extension/src/engine/templates.lua').bind(R)
      local files={} for _,name in ipairs(T.NAMES) do files[#files+1]=R.encode(S,T.build(name,64,64)) end return files
    "#).eval().unwrap();
    let mut count = 0;
    for file in files.sequence_values::<String>() {
        legacy::import(&file.unwrap()).unwrap();
        count += 1;
    }
    assert_eq!(count, 20);
}

fn small_file(extra: &str) -> String {
    format!(
        "return {{canvas={{w=32,h=32}},bones={{{{id=2,parent=0,x=3,y=4,len=5,angle=0,part=1}}}},parts={{{{id=1,name='Body',color={{100,150,200}}}}}},{extra}}}"
    )
}

#[test]
fn accepts_data_only_lua_format_variants_and_escaped_names() {
    let literal = small_file(
        r#"libName='A\a\b\f\n\r\t\v\\\"\'\065\x42\z
       C',images=nil,mirror={on=false,x=+1.6e+1},metadata={true,false,nil,[-2]='root',["a b"]='quoted key'}"#,
    );
    let bare = literal
        .strip_prefix("return ")
        .unwrap()
        .replace("canvas=", r#"["canvas"]="#)
        .replace("bones={{", "bones={[1]={")
        .replace("parts={{", "parts={[1]={");
    let input = format!("-- source file\n--[[ multiline\ncomment ]]\n{bare} -- end of file");
    let (rig, warnings) = legacy::import(&input).unwrap();
    assert_eq!(rig.name, "A\x07\x08\x0c\n\r\t\x0b\\\"'ABC");
    assert_eq!(rig.mirror.x, 16.);
    assert!(warnings.is_empty());
    let continued = small_file("libName=\"First\\\nSecond\\\r\nThird\"");
    assert_eq!(
        legacy::import(&continued).unwrap().0.name,
        "First\nSecond\nThird"
    );
}

#[test]
fn predecessor_anim_without_clips_keeps_all_supported_tracks_and_easing() {
    use rebonebase::animation::{Channel, Ease};
    let source = small_file(
        r#"anim={name='Old clip',frames=7,frameMs=40,loop=false,ease='Ease out',tracks={
      {bone=2,ch='rot',keys={{7,70,'Step'},{6,60,'Bounce'},{5,50,'Back'},{4,40,'Ease out'},{3,30,'Ease in'},{2,20,'Ease'},{1,10,'Linear'}}},
      {bone=2,ch='x',keys={{1,3}}},{bone=2,ch='y',keys={{1,-2}}},{bone=2,ch='s',keys={{1,1.5}}},
      {bone=1,ch='z',keys={{1,4}}},{bone=1,ch='vis',keys={{1,0}}}}}"#,
    );
    let (rig, warnings) = legacy::import(&source).unwrap();
    let clip = &rig.clips[0];
    assert!(warnings.is_empty());
    assert_eq!(clip.name, "Old clip");
    assert_eq!(clip.fps, 25.);
    assert!(!clip.looped);
    let rotations = clip
        .tracks
        .iter()
        .find(|t| t.channel == Channel::Rotation)
        .unwrap();
    assert_eq!(
        rotations.keys.iter().map(|k| k.easing).collect::<Vec<_>>(),
        vec![
            Ease::Linear,
            Ease::Ease,
            Ease::EaseIn,
            Ease::EaseOut,
            Ease::Back,
            Ease::Bounce,
            Ease::Step
        ]
    );
    let pose = clip.sample(1.);
    assert_eq!((pose[&2].x, pose[&2].y, pose[&2].scale), (3., -2., 1.5));
    assert_eq!(pose[&1].z, Some(4.));
    assert_eq!(pose[&1].visibility, Some(0.));
    assert_eq!(
        clip.tracks
            .iter()
            .find(|t| t.channel == Channel::X)
            .unwrap()
            .keys[0]
            .easing,
        Ease::EaseOut
    );
}

#[test]
fn selects_active_clip_and_reports_every_unsupported_feature_once() {
    let source = small_file(
        r#"bounds={x=1,y=1,w=20,h=20},ground={on=true},rootPos={x=0,y=0},
      images={asset='bytes'},poses={{name='Saved'}},ik={{id=3}},clipIndex=2,
      clips={{name='First'},{name='Second',pingpong=true,ref={image='asset'},durations={{1,200}},tracks={
        {bone=-2,ch='x',keys={{1,1}}},{bone=-1,ch='rx',keys={{1,1}}},{bone=3,ch='tx',keys={{1,1}}},
        {bone=1,ch='rot',keys={{1,1}}},{bone=2,ch='vis',keys={{1,1}}}}}}"#,
    );
    let (rig, warnings) = legacy::import(&source).unwrap();
    assert_eq!(rig.active_clip, 1);
    assert_eq!(rig.active_clip().name, "Second");
    assert_eq!(
        rig.active_clip().playback,
        rebonebase::animation::Playback::PingPong
    );
    assert_eq!(rig.active_clip().duration_ms(1), 200);
    assert!(rig.active_clip().tracks.is_empty());
    for feature in [
        "Export subregion",
        "Ground",
        "root handle",
        "reference images",
        "pose-library",
        "Live IK",
        "reference-image settings",
        "animation tracks",
    ] {
        assert!(
            warnings.iter().any(|w| w.contains(feature)),
            "Missing warning for {feature}: {warnings:?}"
        );
    }
    assert_eq!(
        warnings
            .iter()
            .filter(|w| w.contains("animation tracks"))
            .count(),
        1
    );
}

#[test]
fn rejects_malformed_strings_comments_numbers_and_table_keys() {
    for suffix in [
        "libName='unterminated",
        "libName='a\\",
        "libName='raw\nnewline'",
        r"libName='\x'",
        r"libName='\xGG'",
        r"libName='\999'",
        r"libName='\255'",
        r"libName='\q'",
        "metadata=--[[unfinished",
        "metadata=1e+",
        "metadata=.",
        "metadata=+",
        "metadata={[true]=1}",
        "metadata={[{}]=1}",
        "metadata={[1+1]=1}",
        "metadata={['x' 1}",
    ] {
        let input = small_file(suffix);
        assert!(legacy::import(&input).is_err(), "Accepted {suffix}");
    }
    assert!(legacy::import("return {canvas={w=32,h=32} bones={},parts={}}").is_err());
    assert!(legacy::import("return {canvas={w=32,h=32},bones={},parts={},['a']=1,a=2}").is_err());
}

#[test]
fn rejects_wrong_schema_types_invalid_geometry_and_unknown_easing() {
    for input in [
        small_file("version=2"),
        small_file("libName=false"),
        small_file("mirror={on=1}"),
        small_file("").replace("w=32", "w='large'"),
        small_file("").replace(
            "bones={{id=2,parent=0,x=3,y=4,len=5,angle=0,part=1}}",
            "bones=1",
        ),
        small_file("").replace("bones={{", "bones={named={"),
        small_file("").replace("color={100,150,200}", "color={256,0,0}"),
        small_file("").replace("name='Body'", "name='Body',z=0.5"),
        small_file("").replace("id=2,parent", "id=-2,parent"),
        small_file("").replace("id=2,parent", "id=4294967296,parent"),
        small_file("").replace("len=5", "len=5,bulgeL={{0.5}}"),
        small_file("anim={frameMs=0}"),
        small_file("anim={frames=3,tracks={{bone=2,ch='rot',keys={{1,3,'Unknown'}}}}}"),
        small_file("anim={tracks={{bone=2,ch='rot',keys={{1}}}}}"),
        small_file("anim={tracks={{bone=2,ch='rot',keys={{1,0,'Ease',9}}}}}"),
    ] {
        assert!(
            legacy::import(&input).is_err(),
            "Accepted invalid schema: {input}"
        );
    }
}

#[test]
fn enforces_structural_budgets_before_converting_or_applying_values() {
    let enormous_values = small_file(&format!("metadata={{{}}}", "nil,".repeat(250_001)));
    assert!(
        legacy::import(&enormous_values)
            .unwrap_err()
            .contains("Too many values")
    );
    let bones = small_file("").replace(
        "bones={{id=2,parent=0,x=3,y=4,len=5,angle=0,part=1}}",
        &format!("bones={{{}}}", "{},".repeat(2049)),
    );
    assert!(legacy::import(&bones).is_err());
    let parts = small_file("").replace(
        "parts={{id=1,name='Body',color={100,150,200}}}",
        &format!("parts={{{}}}", "{},".repeat(257)),
    );
    assert!(legacy::import(&parts).is_err());
    for extra in [
        format!("clips={{{}}}", "{},".repeat(33)),
        format!("anim={{tracks={{{}}}}}", "{},".repeat(16385)),
        format!(
            "anim={{tracks={{{{bone=2,ch='rot',keys={{{}}}}}}}}}",
            "{1,0},".repeat(241)
        ),
    ] {
        assert!(legacy::import(&small_file(&extra)).is_err());
    }
}
