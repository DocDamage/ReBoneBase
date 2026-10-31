//! Development-only source oracle. Lua is not linked into the application.
use mlua::{Lua, LuaSerdeExt};
fn main() -> anyhow::Result<()> {
    let lua = Lua::new();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    lua.globals()
        .set("ROOT", root.to_string_lossy().replace('\\', "/"))?;
    let values: mlua::Value = lua.load(r#"
local R = dofile(ROOT .. '/extension/src/engine/rig.lua')
local T = dofile(ROOT .. '/extension/src/engine/templates.lua').bind(R)
local out = {}
for _, name in ipairs(T.NAMES) do
 local d = assert(T.build(name, 64, 64))
 local rig = {name=name,width=64,height=64,bones={},parts={},mirror=d.mirror}
 for _,b in ipairs(d.bones) do
  local function bulges(a) local v={} for _,p in ipairs(a) do v[#v+1]={t=p[1],d=p[2]} end return v end
  rig.bones[#rig.bones+1]={id=b.id,name=b.name,parent=b.parent~=0 and b.parent or nil,
   connected=b.connected,x=b.x,y=b.y,length=b.len,angle=b.angle,radius_head=b.rHead,radius_tail=b.rTail,
   part=b.part,twin=b.twin~=0 and b.twin or nil,bulges_left=bulges(b.bulgeL),bulges_right=bulges(b.bulgeR)}
 end
 for _,p in ipairs(d.parts) do
  rig.parts[#rig.parts+1]={id=p.id,name=p.name,color={p.color[1],p.color[2],p.color[3],255},z=p.z,blend=p.blend}
 end
 out[#out+1]=rig
end
return out
"#).eval()?;
    let mut data: serde_json::Value = lua.from_value(values)?;
    for rig in data.as_array_mut().unwrap() {
        for bone in rig["bones"].as_array_mut().unwrap() {
            for key in ["bulges_left", "bulges_right"] {
                if bone[key].is_object() {
                    bone[key] = serde_json::json!([]);
                }
            }
        }
    }
    std::fs::create_dir_all(root.join("assets"))?;
    std::fs::write(
        root.join("assets/templates.json"),
        serde_json::to_string_pretty(&data)?,
    )?;
    println!(
        "Extracted {} original templates",
        data.as_array().unwrap().len()
    );
    Ok(())
}
