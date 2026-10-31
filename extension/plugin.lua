-- Bonebase - entry point.
--
-- 2D skeleton with a skin envelope to build animation bases you can draw over.
--
-- State lives in _G so re-running the command re-enters the same session;
-- the package version invalidates it after a reinstall (see kit/host.lua).

-- Kept in sync with package.json by scripts/build-extension.ps1. Hardcoded
-- so opening Bonebase never trips Aseprite's script-security prompt.
local VERSION = "2.2.2"

local function scriptRoot()
  local src = debug.getinfo(1, "S").source
  if src:sub(1, 1) == "@" then src = src:sub(2) end
  return app.fs.filePath(src)
end

local ROOT = scriptRoot()
local Host = dofile(app.fs.joinPath(ROOT, "src", "kit/host.lua"))

local H = Host.boot{
  id = "bonebase",
  version = VERSION,
  root = ROOT,

  command = {
    id = "bonebase_Open",
    title = "Bonebase",
    group = "file_scripts",
  },

  modules = function(State, load)
    State.Buffer = State.Buffer or load("kit/buffer.lua")
    State.Painter = State.Painter or load("kit/painter_ase.lua")
    State.UI = State.UI or load("kit/ui/init.lua")
    State.Undo = State.Undo or load("kit/undo.lua")
    State.Rig = State.Rig or load("engine/rig.lua")
    State.Skin = State.Skin or load("engine/skin.lua")
    State.Anim = State.Anim or load("engine/anim.lua")
    State.Templates = State.Templates or load("engine/templates.lua")
    State.Templates.bind(State.Rig)
    State.Plan = State.Plan or load("engine/export_plan.lua")
    State.Ik = State.Ik or load("engine/ik.lua")
    State.Ik.bind(State.Rig, State.Anim)
    State.Ref = State.Ref or load("engine/refimg.lua")
    State.Ref.bind(State.Anim)
    State.Plan.bind(State.Rig, State.Skin, State.Anim, State.Ik)
    State.Skel3d = State.Skel3d or load("engine/skel3d.lua")
    State.M3d = State.M3d or load("engine/m3d.lua")
    State.Json = State.Json or load("engine/json.lua")
    State.Bvh = State.Bvh or load("engine/bvh.lua")
    State.Bvh.bind(State.Skel3d, State.M3d)
    State.Gltf = State.Gltf or load("engine/gltf.lua")
    State.Gltf.bind(State.Skel3d, State.M3d, State.Json)
    State.Retarget = State.Retarget or load("engine/retarget.lua")
    State.Retarget.bind(State.Rig, State.Anim, State.Ik)
    State.Serial = State.Serial or load("kit/serial.lua")
    State.Png = State.Png or load("kit/png.lua")
    State.Poses = State.Poses or load("engine/poses.lua")
    State.Poses.bind(State.Rig, State.Anim, State.Ik)
    State.Thumb = State.Thumb or load("engine/thumb.lua")
    State.Onion = State.Onion or load("engine/onion.lua").bind(State.Anim)
    State.Lib = State.Lib or load("library.lua")
    State.Export = State.Export or load("export.lua")
    State.Export.bind{ Buffer = State.Buffer, Plan = State.Plan, Rig = State.Rig,
      Serial = State.Serial }
    State.Editor = State.Editor or load("editor.lua")
    State.Editor.bind(State.Rig, State.Skin, State.Undo, State.Anim, State.Ik, State.Ref, State.Poses)
    State.Dope = State.Dope or load("dopesheet.lua")
    State.Promo = State.Promo or load("promo.lua")
    State.Import = State.Import or load("import3d.lua")
    State.Import.bind{ R = State.Retarget, Bvh = State.Bvh, Gltf = State.Gltf, Rig = State.Rig,
      Anim = State.Anim, Ik = State.Ik, Skin = State.Skin }
    State.View = State.View or load("view.lua")
    State.Ui = State.Ui or load("ui.lua")
    State.Ui.bind{
      Painter = State.Painter, UI = State.UI, View = State.View, Buffer = State.Buffer,
      Rig = State.Rig, Skin = State.Skin, Anim = State.Anim, Editor = State.Editor, Ik = State.Ik,
      Dope = State.Dope, Promo = State.Promo, Ref = State.Ref, Import = State.Import,
      Templates = State.Templates, Export = State.Export,
      Lib = State.Lib, Poses = State.Poses, Thumb = State.Thumb, Png = State.Png, Serial = State.Serial,
      Onion = State.Onion,
    }
  end,

  open = function(State, plugin)
    State.plugin = plugin
    State.root = ROOT
    State.Ui.open(State)
  end,

  close = function(State)
    if State.Ui and State.Ui.close then pcall(State.Ui.close, State) end
  end,
}

function init(plugin) H.init(plugin) end
function exit(plugin) H.exit(plugin) end
