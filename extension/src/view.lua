-- view.lua — the Bonebase window as a PURE model (no Aseprite API): one
-- canvas with tabs + panel + footer on the left; mode and tool bar,
-- viewport, timeline and status on the right; a draggable splitter in
-- between (AseKit rules/ui-style.md). `layout(w, h)` splits the window
-- into regions, `paint()` returns one display list for kit/painter_ase.lua
-- (or painter_buf for goldens), and the input handlers route by region
-- and return true when something changed (the host repaints then).
--
-- `M.new(deps)`, `deps = { UI, Rig, Skin, Anim, Editor, Templates?, editor,
--   text?, theme?, uiScale?, version?, onPlay?, confirm?, onExport?,
--   leftW?, onSplit?, reference? }`. `leftW` / `onSplit(leftW)` carry the
-- splitter position in and out (the host keeps it in plugin.preferences).
-- `reference(frame, layer?) -> buffer | nil` gives the sprite's own
-- drawing at a frame (without the Bonebase group), the size of the sprite:
-- every visible layer, or only the layer named `layer`; `layers() ->
-- { name, ... }` lists the sprite's layers. The View tab's Background
-- picks a checker, a solid colour, the sprite or one of its layers, drawn
-- on the sprite's bounds (the canvas can be larger: Rig tab, Canvas).
-- `onExport(opts) -> ok, message` writes the rig into the sprite
-- (src/export.lua); the message lands on the status line.
-- `confirm(text, onYes)` asks before a destructive action (the host shows
-- a native yes / no dialog); without it the action just runs. `editor` is an
-- editor.lua instance: every change to the rig goes through it, so undo
-- and the mirror work the same from the panel and from the viewport.
-- `Ref` (refimg.lua) draws the clip's reference image and `loadRef(kind)`
-- -> buffer | nil, why asks the host for one ("file" | "clipboard" |
-- "sprite"); the View tab has its controls.
-- `Promo` (promo.lua) fills the More tab and `openUrl(url) -> ok` opens
-- its links in the browser (the host's app.command.Launch).
-- `Import` (import3d.lua) is the "Import 3D" mode and `loadMotion() ->
-- bytes, name, readFile | nil, why` asks the host for a .glb / .gltf /
-- .bvh file; while a session is open (`imp`) the panel, viewport and
-- timeline are the import's, and Import / Cancel replace the footer.
-- `Lib` (library.lua) fills the Poses and Library tabs, with `Poses`
-- (engine/poses.lua) and `Thumb` (engine/thumb.lua) for the previews and
-- `library` the host's rig library on disk (library.lua documents it).
-- `Onion` (engine/onion.lua) draws the onion skin of Pose mode; `onion` /
-- `onOnion(settings)` carry its settings in and out (plugin preferences).
-- `onPlay(playing, frameMs)` asks the host to start or stop its Timer,
-- which then calls `V:tick()` once per frame. `Dope` (dopesheet.lua) draws
-- the rows under the timeline; `sheetH` / `onSheet(h)` carry the height of
-- that area (logical px) in and out, like the splitter.
--
-- Viewport camera: `zoom` is continuous (wheel x1.15, Ctrl/Shift x2) but
-- the DRAWN zoom is whole numbers from 1x up, so pixels stay square.
-- Right or middle drag pans; `0` fits, `1` `2` `3` zoom 100/200/300 %.
--
-- Timeline: one cell per frame, the current one in the accent colour and
-- a mark on every frame with keys (of the selected bone, or of any bone
-- when none is selected). Click or drag to scrub; drag a marked frame to
-- move its keys. Using it from Rest mode switches to Pose mode. Under it,
-- the dope sheet (dopesheet.lua), resized by the bar above the timeline.

local M = {}
local floor, max, min = math.floor, math.max, math.min

M.DEF_W, M.DEF_H = 900, 600
M.PANEL_MIN, M.PREVIEW_MIN = 320, 192
M.SPLITTER_W = 6
M.PICK_PX = 5          -- pick tolerance around joints and bones, screen px
M.ZOOM_MIN, M.ZOOM_MAX = 0.25, 16
M.FIT_MARGIN = 24
M.JOINT_PX = 3         -- joint marker size, screen px at 1x ui scale
M.CIRCLE_SEGS = 28
M.PLAY_W = 64          -- Play button of the timeline, logical px
M.KEY_MARK = 2         -- half size of a key mark, logical px

M.TABS = { "Rig", "Bone", "Groups", "Animation", "Poses", "Library", "View", "More" }
M.NO_GROUP = "None"
M.OUTLINES = { "Pixel", "Thick", "None" }
M.BACKGROUNDS = { "Checker", "Color", "Sprite", "Layer" }
M.BOUNDS_ALPHA = 96    -- the sprite's outline on a larger canvas

M.HINTS = {
  Select = "Drag a tail to bend, a body to swing or move; the pink ring moves the whole rig",
  Bone = "Drag from empty space, a joint or a bone",
  Radius = "Drag near a joint to set its thickness",
  Bulge = "Drag a skin edge to bulge it, Delete removes a handle",
  Pan = "Drag to pan, wheel to zoom, 0 to fit",
  Pose = "FK: drag a bone to turn it (limbs too), a root or loose bone to move it",
  IK = "Drag a hand / foot or its dummy, or the elbow / knee one to pick the bend",
  Ref = "Drag the reference to move it, Shift to scale, Alt to turn",
}

--- The kit theme scaled by `uiScale`, plus this window's named colours.
function M.theme(UI, uiScale)
  local s = tonumber(uiScale) or 1
  if s < 1 then s = 1 end
  s = floor(s)
  local th = UI.theme.scaled(UI.theme.default(), s)
  th.checker = 8 * s
  th.checkerA = { 92, 92, 96 }
  th.checkerB = { 76, 76, 80 }
  th.splitterW = M.SPLITTER_W * s
  th.splitter = { 58, 58, 66 }
  th.splitterHot = th.accent
  th.previewBounds = { 255, 255, 255 }
  th.bone = { 235, 235, 240 }
  th.boneDim = { 150, 150, 160 }
  th.mirrorAxis = { 90, 200, 255 }
  th.handle = { 255, 255, 255 }
  th.overlayBg = { 15, 16, 21 }
  th.ikTarget = { 255, 170, 80 }     -- hand / foot dummy
  th.ikPole = { 120, 200, 255 }      -- elbow / knee dummy
  th.ikOff = { 120, 120, 130 }       -- a controller switched off
  th.ground = { 120, 220, 140 }      -- the ground line, and a foot on it
  th.rootHandle = { 236, 120, 220 }  -- the rig root: carries the whole rig
  th.keyMark = { 235, 235, 240 }
  th.cellLine = { 52, 52, 60 }
  return th
end

local V = {}
V.__index = V

function M.new(deps)
  assert(type(deps) == "table" and deps.UI and deps.editor, "view.new: UI and editor required")
  local UI = deps.UI
  local theme = deps.theme or M.theme(UI, deps.uiScale or 1)
  local text = deps.text or UI.text.new()
  local self = setmetatable({
    UI = UI, Rig = deps.Rig, Skin = deps.Skin, Anim = deps.Anim, Editor = deps.Editor,
    Ik = deps.Ik, Dope = deps.Dope, Promo = deps.Promo, launch = deps.openUrl,
    Ref = deps.Ref, loadRef = deps.loadRef,
    Import = deps.Import, loadMotion = deps.loadMotion,
    Lib = deps.Lib, Poses = deps.Poses, Thumb = deps.Thumb, library = deps.library,
    Onion = deps.Onion, onOnion = deps.onOnion,
    onionSet = deps.Onion and deps.Onion.sanitize(deps.onion) or nil, -- onion.lua settings
    poseSel = nil,        -- the pose picked in the Poses tab (id)
    poseOnlySel = false,  -- Apply keys only the selected bones
    libSel = nil,         -- the rig picked in the Library tab (name)
    libName = nil,        -- the name Save to library uses
    libItems = nil,       -- library.list(), read when the tab opens
    libThumbs = {},
    imp = nil,            -- import3d.lua session while "Import 3D" is open
    refTarget = "All frames",  -- where a loaded reference image goes
    sheetH = tonumber(deps.sheetH), -- dope sheet rows area (logical px); nil = default
    onSheet = deps.onSheet,
    sheetScroll = 0,
    sheetDrag = nil,      -- dopesheet.lua: a live move / scale / box
    Templates = deps.Templates, confirm = deps.confirm, templateSel = "Humanoid",
    E = deps.editor,
    theme = theme, text = text,
    ctx = { theme = theme, text = text },
    version = deps.version or "",
    onPlay = deps.onPlay,
    onExport = deps.onExport,
    onSplit = deps.onSplit,
    reference = deps.reference,
    layers = deps.layers,
    help = false,         -- the shortcut overlay (? / F1)
    refToken = 0,         -- bumps to re-read the sprite reference
    w = M.DEF_W, h = M.DEF_H,
    regions = {},
    tab = 1,
    leftW = tonumber(deps.leftW), -- left column width (logical px); nil = default
    capture = nil,        -- "panel" | "footer" | "viewport" | "pan" | "splitter" | "strip" | "play"
                          -- | "sheet" | "hsplit"
    splitHover = false,
    cam = { zoom = 4, px = 0, py = 0, fitted = false, cw = 0, ch = 0 },
    opts = { outline = "Thick", fill = 75, showBones = true, showSkin = true,
      onion = false, lock = true,
      bg = "Checker", bgColor = { 52, 52, 60 }, bgLayer = nil, showBounds = true },
    partSel = nil,
    keyScale = 200,       -- Time scale of the picked keys, %
    playing = false,
    painted = 0,
    notice = nil,         -- one line on the status after an action
    noticeWarn = false,   -- shown in the warning colour
  }, V)
  if self.Lib then
    -- the panel keeps its own copy of the registry
    self.Lib.extend(UI.widgets)
    if UI.panel.Widgets then self.Lib.extend(UI.panel.Widgets) end
    self.thumbs = self.Lib.cache()
  end
  self.modeSpec = { id = "mode", kind = "tabs", items = deps.Editor.MODES }
  self.toolSpec = { id = "tools", kind = "tabs", items = self.E:tools() }
  self.tabSpec = { id = "tabs", kind = "tabs", items = M.TABS }
  local noValue = { get = function() return nil end, set = function() end }
  self.footer = UI.panel.new({
    -- the rig back to its library entry (library.lua saveLinked)
    { id = "libSaveNow", kind = "button", text = "Save rig",
      onPress = function() self.Lib.saveLinked(self) end },
    { id = "export", kind = "button", text = "Export sprite", row = "same", onPress = function() self:export() end },
    { id = "undo", kind = "button", text = "Undo", onPress = function() self:undo() end },
    { id = "redo", kind = "button", text = "Redo", row = "same", onPress = function() self:redo() end },
  }, { get = noValue.get, set = noValue.set,
    -- an asterisk: changes the library entry does not have yet
    label = function(spec)
      if spec.id == "libSaveNow" then return self.E:libDirty() and "Save rig*" or "Save rig" end
      return nil
    end,
    enabled = function(spec)
      if spec.id == "export" then return self.onExport ~= nil and #self.E.doc.bones > 0 end
      if spec.id == "libSaveNow" then
        return self.Lib ~= nil and self.library ~= nil and #self.E.doc.bones > 0
      end
      if spec.id == "undo" then return #self.E.hist.undo > 0 end
      if spec.id == "redo" then return #self.E.hist.redo > 0 end
      return true
    end }, self.ctx)
  self.playPanel = UI.panel.new({
    { id = "play", kind = "button", text = "Play", onPress = function() self:togglePlay() end },
  }, { get = noValue.get, set = noValue.set,
    label = function(spec)
      if spec.id == "play" then
        local on = self.imp and self.imp.playing or (not self.imp and self.playing)
        return on and "Stop" or "Play"
      end
      return nil
    end }, self.ctx)
  self:setTab(1)
  return self
end

---------------------------------------
-- Panels --
---------------------------------------

local function fmtPx(v) return string.format("%.1f px", v) end
local function fmtDeg(v) return string.format("%d deg", floor(v + 0.5)) end
local function fmtPct(v) return string.format("%d %%", floor(v + 0.5)) end
local function fmtMs(v) return string.format("%d ms", floor(v + 0.5)) end

function V:boneSpecs()
  local view = self
  return {
    { id = "noBone", kind = "label", text = "Select a bone, or add one with the Bone tool" },
    { id = "name", kind = "textField", label = "Name", needsBone = true, maxLen = 32 },
    { id = "len", kind = "slider", label = "Length", min = 1, max = 256, step = 0.5,
      format = fmtPx, needsBone = true, rest = true },
    { id = "angle", kind = "slider", label = "Angle", min = -180, max = 180, step = 1,
      format = fmtDeg, needsBone = true, rest = true },
    { id = "rHead", kind = "slider", label = "Head radius", min = 0.5, max = 64, step = 0.5,
      format = fmtPx, needsBone = true, rest = true },
    { id = "rTail", kind = "slider", label = "Tail radius", min = 0.5, max = 64, step = 0.5,
      format = fmtPx, needsBone = true, rest = true },
    { id = "poseRot", kind = "slider", label = "Rotation", min = -360, max = 360, step = 1,
      format = fmtDeg, needsBone = true, pose = true, ch = "rot", notDriven = true },
    { id = "driven", kind = "label", text = "Moved by IK: drag its dummies", needsBone = true,
      pose = true, driven = true },
    { id = "poseS", kind = "slider", label = "Length", min = 10, max = 400, step = 5,
      format = fmtPct, needsBone = true, pose = true, ch = "s", scale = 100 },
    { id = "poseX", kind = "slider", label = "Offset X", min = -64, max = 64, step = 0.5,
      format = fmtPx, needsBone = true, pose = true, loose = true, ch = "x" },
    { id = "poseY", kind = "slider", label = "Offset Y", min = -64, max = 64, step = 0.5,
      format = fmtPx, needsBone = true, pose = true, loose = true, ch = "y" },
    { id = "part", kind = "combo", label = "Group", needsBone = true, rest = true },
    { id = "connected", kind = "switch", label = "Connected", needsBone = true, rest = true },
    { id = "bulges", kind = "label", needsBone = true, rest = true },
    { id = "sepBoneIK", kind = "separator", text = "IK", collapsible = false, needsBone = true },
    { id = "ikOn", kind = "switch", label = "IK", needsIK = true },
    { id = "ikStretch", kind = "switch", label = "Stretch", needsIK = true },
    { id = "ikMax", kind = "slider", label = "Max stretch", min = 100, max = 300, step = 5,
      format = fmtPct, needsIK = true, needsStretch = true },
    { id = "ikLock", kind = "switch", label = "Keep hand / foot angle", needsIK = true, needsTip = true },
    { id = "addIK", kind = "button", text = "Add IK", needsBone = true, canAddIK = true,
      onPress = function()
        if view.E:addIK() then view:setNotice("IK added: drag its dummies in Pose") end
      end },
    { id = "removeIK", kind = "button", text = "Remove IK", needsIK = true, onPress = function()
      local c = view:selCtrl()
      if c then view.E:removeIK(c.id) end
    end },
    { id = "noIK", kind = "label", text = "IK needs a bone connected to its parent",
      needsBone = true, noIK = true },
    { id = "clearBulges", kind = "button", text = "Clear bulges", needsBone = true, rest = true,
      onPress = function() view:clearBulges() end },
    { id = "deleteBone", kind = "button", text = "Delete bone", needsBone = true, row = "same",
      rest = true, onPress = function() view.E:deleteSelection() end },
  }
end

function V:partSpecs()
  local view = self
  return {
    { id = "partList", kind = "list", label = "Groups", rows = 6 },
    { id = "groupHint", kind = "label", text = "A bone's skin shows once it is in a group" },
    { id = "newPart", kind = "button", text = "New group", onPress = function()
      local n = #view.E:selectedIds()
      local p = view.E:addPart()
      view.partSel = p.id
      if n > 0 then
        view:setNotice(string.format("%s: %d %s", p.name, n, n == 1 and "bone" or "bones"))
      end
    end },
    { id = "deletePart", kind = "button", text = "Delete group", row = "same", needsPart = true,
      onPress = function()
        if view.partSel and view.E:removePart(view.partSel) then view.partSel = nil end
      end },
    { id = "assignPart", kind = "button", text = "Add selected bones", needsPart = true,
      onPress = function()
        local n = view.partSel and view.E:assignPart(view.partSel) or 0
        if n == 0 then view:setNotice("Select bones first (Shift+click adds more)", true) end
      end },
    { id = "unassignPart", kind = "button", text = "Take them out", row = "same", needsPart = true,
      onPress = function() view.E:assignPart(0) end },
    { id = "partName", kind = "textField", label = "Name", needsPart = true, maxLen = 32 },
    { id = "partColor", kind = "color", label = "Color", needsPart = true },
    { id = "partZ", kind = "number", label = "Order", min = -99, max = 99, step = 1, needsPart = true,
      rest = true },
    { id = "partBlend", kind = "slider", label = "Blend", min = 0, max = 12, step = 0.5,
      format = fmtPx, needsPart = true, rest = true },
    { id = "partVis", kind = "switch", label = "Visible", needsPart = true, inPose = true, partCh = "vis" },
    { id = "partZKey", kind = "number", label = "Order", min = -99, max = 99, step = 1,
      needsPart = true, inPose = true, partCh = "z" },
    { id = "deletePartKey", kind = "button", text = "Delete group key", needsPart = true, inPose = true,
      onPress = function()
        if view.partSel then view.E:deletePartKeys(view.partSel) end
      end },
    { id = "partKeyHint", kind = "label", text = "Visible and Order are keyed on this frame",
      needsPart = true, inPose = true },
  }
end

function V:animSpecs()
  local view = self
  local list = {
    { id = "sepClips", kind = "separator", text = "Clips" },
    { id = "clipList", kind = "list", label = "Clips", rows = 4 },
    { id = "clipName", kind = "textField", label = "Name", maxLen = 32 },
    { id = "newClip", kind = "button", text = "New", onPress = function() view.E:newClip() end },
    { id = "duplicateClip", kind = "button", text = "Duplicate", row = "same",
      onPress = function() view.E:duplicateClip() end },
    { id = "deleteClip", kind = "button", text = "Delete", row = "same", onPress = function()
      local name = view.E.doc.anim.name
      if view.E:deleteClip() then view:setNotice("Clip " .. name .. " deleted (Ctrl+Z brings it back)") end
    end },
    { id = "clipUp", kind = "button", text = "Move up", onPress = function() view.E:moveClip(-1) end },
    { id = "clipDown", kind = "button", text = "Move down", row = "same",
      onPress = function() view.E:moveClip(1) end },
    { id = "import3d", kind = "button", text = "Import 3D", row = "same",
      onPress = function() view:startImport() end },
    { id = "sepTiming", kind = "separator", text = "Timing" },
    { id = "frames", kind = "number", label = "Frames", min = 1, max = 240, step = 1 },
    { id = "frameMs", kind = "slider", label = "All frames", min = 20, max = 500, step = 10,
      format = fmtMs },
    { id = "frameHold", kind = "slider", label = "This frame", min = 20, max = 1000, step = 10,
      format = fmtMs },
    { id = "loop", kind = "switch", label = "Loop" },
    { id = "pingpong", kind = "switch", label = "Ping-pong" },
    { id = "ease", kind = "combo", label = "Ease", options = view.Anim.EASES },
    { id = "addKey", kind = "button", text = "Add key", onPress = function()
      view.E:setMode("Pose")
      view.E:addKey()
    end },
    { id = "deleteKey", kind = "button", text = "Delete key", row = "same",
      onPress = function()
        view:setMode("Pose")
        view.E:deleteKeys()
      end },
    { id = "copyPose", kind = "button", text = "Copy pose", onPress = function() view:copyPose() end },
    { id = "pastePose", kind = "button", text = "Paste pose", row = "same",
      onPress = function() view:pastePose(false) end },
    { id = "pasteMirrored", kind = "button", text = "Paste mirror", row = "same",
      onPress = function() view:pastePose(true) end },
    { id = "resetPose", kind = "button", text = "Reset pose", onPress = function() view:resetPose() end },
    { id = "insertFrame", kind = "button", text = "Insert frame", row = "same", onPress = function()
      view:setMode("Pose")
      view.E:insertFrame()
    end },
    { id = "deleteFrame", kind = "button", text = "Delete frame", row = "same", onPress = function()
      view:setMode("Pose")
      view.E:deleteFrame()
    end },
    { id = "reverseKeys", kind = "button", text = "Reverse keys", onPress = function()
      view:setMode("Pose")
      local n = view.E:reverseKeys()
      if n == 0 then view:setNotice("Reverse needs keys on two frames or more", true) end
    end },
    { id = "keyScale", kind = "slider", label = "Time scale", min = 25, max = 400, step = 5,
      format = fmtPct, needsKeys = true },
    { id = "scaleKeys", kind = "button", text = "Scale picked keys", needsKeys = true,
      onPress = function()
        local ok, why = view.E:scaleSelKeys(view.keyScale / 100)
        if not ok then
          view:setNotice(why == "overlap" and "Keys of one row would land on the same frame"
            or why == "out" and "The keys would leave the clip"
            or "Nothing to scale: the keys would not move", true)
        end
      end },
    { id = "keyHint", kind = "label", text = "Keys go on the selected bones, or on all of them" },
    { id = "pinLimbs", kind = "switch", label = "Pin hands and feet" },
    { id = "sepGround", kind = "separator", text = "Ground" },
    { id = "groundOn", kind = "switch", label = "Ground" },
    { id = "groundY", kind = "slider", label = "Ground height", min = 0,
      max = view.E.doc.canvas.h, step = 0.5, format = fmtPx, needsGround = true },
    { id = "groundStop", kind = "switch", label = "Feet stop at ground", needsGround = true },
    { id = "sepOnion", kind = "separator", text = "Onion skin" },
    { id = "onion", kind = "switch", label = "Onion skin" },
  }
  for _, spec in ipairs(self:onionSpecs()) do list[#list + 1] = spec end
  return list
end

--- The Onion skin section after its switch: every setting of onion.lua,
--- shown while the onion skin is on.
function V:onionSpecs()
  local view = self
  local O = self.Onion
  if not O then return {} end
  local function on() return view.opts.onion end
  local function flat() return view.opts.onion and view.onionSet.style ~= "Skin" end
  local function setting(id, kind, label, key, extra)
    local spec = { id = id, kind = kind, label = label, visible = on,
      get = function()
        local v = view.onionSet[key]
        if kind == "color" then return { r = v[1], g = v[2], b = v[3], a = 255 } end
        return v
      end,
      set = function(_, v)
        local t = {}
        for k, x in pairs(view.onionSet) do t[k] = x end
        if kind == "color" and type(v) == "table" then v = { v.r, v.g, v.b } end
        if kind == "switch" then v = v == true end
        t[key] = v
        view:setOnion(t)
      end }
    for k, x in pairs(extra or {}) do spec[k] = x end
    return spec
  end
  local fmtFrames = function(v) return string.format("%d", floor(v + 0.5)) end
  return {
    setting("onionBefore", "number", "Frames before", "before", { min = 0, max = O.MAX, step = 1 }),
    setting("onionAfter", "number", "Frames after", "after", { min = 0, max = O.MAX, step = 1 }),
    setting("onionKeys", "switch", "Keyframes only", "keys"),
    setting("onionStep", "number", "Every", "step", { min = 1, max = O.MAX, step = 1,
      format = fmtFrames, visible = function() return on() and not view.onionSet.keys end }),
    setting("onionStyle", "combo", "Style", "style", { options = O.STYLES }),
    setting("onionBeforeColor", "color", "Before", "colorBefore", { visible = flat }),
    setting("onionAfterColor", "color", "After", "colorAfter", { visible = flat }),
    setting("onionOpacity", "slider", "Opacity", "opacity", { min = 5, max = 100, step = 1, format = fmtPct }),
    setting("onionFade", "slider", "Fade per frame", "fade", { min = 0, max = 90, step = 1, format = fmtPct }),
    setting("onionWrap", "switch", "Wrap around the loop", "wrap"),
    setting("onionFront", "switch", "Over the rig", "front"),
    { id = "onionReset", kind = "button", text = "Reset onion skin", visible = on,
      onPress = function() view:setOnion(nil) end },
  }
end

--- New onion skin settings (nil: the defaults), kept by the host.
function V:setOnion(t)
  self.onionSet = self.Onion.sanitize(t)
  if self.onOnion then self.onOnion(self.onionSet) end
end

function V:rigSpecs()
  local view = self
  return {
    { id = "sepTemplate", kind = "separator", text = "Start from", collapsible = false },
    { id = "template", kind = "combo", label = "Template" },
    { id = "useTemplate", kind = "button", text = "Use template",
      onPress = function() view:useTemplate() end },
    { id = "newRig", kind = "button", text = "New rig", row = "same",
      onPress = function() view:newRig() end },
    { id = "templateHint", kind = "label", text = "Replaces the current rig" },
    { id = "sepCanvas", kind = "separator", text = "Canvas", collapsible = false },
    { id = "canvasW", kind = "number", label = "Width", min = 1, max = view.Rig.MAX_CANVAS, step = 1 },
    { id = "canvasH", kind = "number", label = "Height", min = 1, max = view.Rig.MAX_CANVAS, step = 1 },
    { id = "canvasInfo", kind = "label" },
    { id = "canvasFit", kind = "button", text = "Fit to sprite", onPress = function()
      local bd = view.Rig.bounds(view.E.doc)
      if view.E:setCanvasSize(bd.w, bd.h) then view:fit() end
    end },
    { id = "sepMirror", kind = "separator", text = "Symmetry", collapsible = false, rest = true },
    { id = "mirror", kind = "switch", label = "Mirror", rest = true },
    { id = "mirrorX", kind = "slider", label = "Mirror axis", min = 0,
      max = view.E.doc.canvas.w, step = 0.5, format = fmtPx, needsMirror = true, rest = true },
    { id = "symmetrize", kind = "button", text = "Symmetrize", rest = true,
      onPress = function() view:symmetrize() end },
    { id = "sepRoot", kind = "separator", text = "Root", collapsible = false },
    { id = "rootX", kind = "number", label = "Root X", min = 0, max = view.E.doc.canvas.w, step = 0.5,
      get = function() return (view.Rig.rootHandle(view.E.doc)) end,
      set = function(_, v)
        local _, y = view.Rig.rootHandle(view.E.doc)
        view.E:beginGesture()
        view.E:setRootPos(tonumber(v), y, true)
      end,
      -- the rest place: in Pose the ring also carries this frame's offset
      visible = function() return #view.E.doc.bones > 0 and view.E.mode == "Rest" end },
    { id = "rootY", kind = "number", label = "Root Y", min = 0, max = view.E.doc.canvas.h, step = 0.5,
      get = function() return select(2, view.Rig.rootHandle(view.E.doc)) end,
      set = function(_, v)
        local x = view.Rig.rootHandle(view.E.doc)
        view.E:beginGesture()
        view.E:setRootPos(x, tonumber(v), true)
      end,
      -- the rest place: in Pose the ring also carries this frame's offset
      visible = function() return #view.E.doc.bones > 0 and view.E.mode == "Rest" end },
    { id = "rootReset", kind = "button", text = "Root under the rig",
      enabled = function() return view.E.doc.rootPos ~= nil end,
      onPress = function()
        view.E:beginGesture()
        view.E:setRootPos(nil, nil)
      end },
    { id = "rootHint", kind = "label", dim = true, text = "Alt+click or Alt+drag moves only the root" },
    { id = "sepIK", kind = "separator", text = "IK", collapsible = false },
    { id = "ikLimbs", kind = "button", text = "IK on limbs", onPress = function()
      local n = view.E:addIKLimbs()
      if n == 0 then
        view:setNotice("No limb without IK (a limb is a branch bone + 2 connected)", true)
      else
        view:setNotice(string.format("IK on %d %s", n, n == 1 and "limb" or "limbs"))
      end
    end },
    { id = "mirrorPose", kind = "label", text = "Mirror shapes the skeleton, poses stay free",
      pose = true },
    { id = "sepExport", kind = "separator", text = "Export", collapsible = false },
    { id = "lock", kind = "switch", label = "Lock layers" },
    { id = "exportHint", kind = "label", text = "Export adds a Bonebase group under your layers" },
  }
end

function V:viewSpecs()
  local view = self
  return {
    { id = "outline", kind = "combo", label = "Outline", options = M.OUTLINES },
    { id = "fill", kind = "slider", label = "Fill opacity", min = 0, max = 100, step = 1,
      format = fmtPct },
    { id = "showSkin", kind = "switch", label = "Show skin" },
    { id = "showBones", kind = "switch", label = "Show bones" },
    { id = "bg", kind = "combo", label = "Background", options = M.BACKGROUNDS },
    { id = "bgColor", kind = "color", label = "Color", needsBg = "Color" },
    { id = "bgLayer", kind = "combo", label = "Layer", needsBg = "Layer" },
    { id = "showBounds", kind = "switch", label = "Sprite bounds" },
    { id = "showHelp", kind = "button", text = "Show shortcuts",
      onPress = function() view.help = true end },
    { id = "sepRef", kind = "separator", text = "Reference image" },
    { id = "refTarget", kind = "combo", label = "Load into", options = M.REF_TARGETS },
    { id = "refFile", kind = "button", text = "From file", onPress = function() view:loadReference("file") end },
    { id = "refPaste", kind = "button", text = "Paste", row = "same",
      onPress = function() view:loadReference("clipboard") end },
    { id = "refSprite", kind = "button", text = "From sprite", row = "same",
      onPress = function() view:loadReference("sprite") end },
    { id = "refOn", kind = "switch", label = "Show reference", needsRef = true },
    { id = "refFront", kind = "switch", label = "Over the skin", needsRef = true },
    { id = "refO", kind = "slider", label = "Opacity", min = 0, max = 100, step = 1, format = fmtPct,
      needsRef = true, refCh = "ro", scale = 100 },
    { id = "refS", kind = "slider", label = "Scale", min = 2, max = 800, step = 1, format = fmtPct,
      needsRef = true, refCh = "rs", scale = 100 },
    { id = "refR", kind = "slider", label = "Rotation", min = -180, max = 180, step = 1, format = fmtDeg,
      needsRef = true, refCh = "rr" },
    { id = "refX", kind = "slider", label = "X", min = -view.E.doc.canvas.w, max = 2 * view.E.doc.canvas.w,
      step = 0.5, format = fmtPx, needsRef = true, refCh = "rx" },
    { id = "refY", kind = "slider", label = "Y", min = -view.E.doc.canvas.h, max = 2 * view.E.doc.canvas.h,
      step = 0.5, format = fmtPx, needsRef = true, refCh = "ry" },
    { id = "refF", kind = "switch", label = "Flip", needsRef = true, refCh = "rf" },
    { id = "refHint", kind = "label", text = "In Pose these are keyed on this frame", needsRef = true },
    { id = "refClearFrame", kind = "button", text = "Clear this frame", needsRef = true,
      onPress = function() view.E:clearRefFrame() end },
    { id = "refRemove", kind = "button", text = "Remove", row = "same", needsRef = true,
      onPress = function() view.E:removeRef() end },
  }
end

M.REF_TARGETS = { "All frames", "This frame" }

--- Asks the host for an image ("file" | "clipboard" | "sprite") and puts
--- it in as the reference of every frame or of this one.
function V:loadReference(kind)
  if not self.loadRef or not self.Ref then
    self:setNotice("Reference images need Aseprite", true)
    return true
  end
  local buf, why = self.loadRef(kind)
  if not buf then
    if why then self:setNotice(why, true) end
    return true
  end
  local frame = self.refTarget == "This frame"
  if frame and self.E.mode ~= "Pose" then self:setMode("Pose") end
  self.E:setRefImage(buf, frame and "frame" or "global")
  self:setNotice(string.format("Reference %d x %d px%s", buf.w, buf.h,
    frame and (" on frame " .. self.E.frame) or ""))
  return true
end

--- The More tab: the author's other extensions (promo.lua), each with a
--- button to its store page. Nothing opens unless clicked.
function V:poseSpecs()
  if not (self.Lib and self.Poses and self.Thumb) then
    return { { id = "posesNone", kind = "label", dim = true, text = "Poses are not available" } }
  end
  return self.Lib.poseSpecs(self)
end

function V:librarySpecs()
  if not (self.Lib and self.Thumb) then
    return { { id = "libNone", kind = "label", dim = true, text = "The rig library is not available" } }
  end
  return self.Lib.librarySpecs(self)
end

function V:moreSpecs()
  local view = self
  local P = self.Promo
  local specs = {
    { id = "moreIntro", kind = "label", text = "Other Aseprite extensions by icemaan", dim = true },
  }
  for i, it in ipairs(P and P.items or {}) do
    specs[#specs + 1] = { id = "promoName" .. i, kind = "separator", text = it.name, collapsible = false }
    specs[#specs + 1] = { id = "promoTag" .. i, kind = "label", text = it.tagline, dim = true }
    specs[#specs + 1] = { id = "promoOpen" .. i, kind = "button", text = "Open store page",
      onPress = function() view:openUrl(it.url) end }
  end
  if P and P.profile then
    specs[#specs + 1] = { id = "promoSepAll", kind = "separator" }
    specs[#specs + 1] = { id = "promoAll", kind = "button", text = P.profileText or "View all extensions",
      onPress = function() view:openUrl(P.profile) end }
  end
  return specs
end

--- Opens `url` in the browser through the host; says so on the status.
function V:openUrl(url)
  if self.launch and self.launch(url) then
    self:setNotice("Opened in your browser: " .. url)
  else
    self:setNotice("Could not open the browser. The page is " .. url, true)
  end
  return true
end

--- Replaces the rig with the chosen template (one undo step), asking
--- first when there is something to lose.
function V:useTemplate()
  local T = self.Templates
  if not T then return false end
  local E = self.E
  local function apply()
    -- built for the sprite and placed on it (the canvas may be larger)
    local bd = self.Rig.bounds(E.doc)
    local doc = T.build(self.templateSel, bd.w, bd.h)
    if not doc then return false end
    self.Rig.placeLike(doc, E.doc)
    if self.playing then self:togglePlay() end
    self:setMode("Rest")
    if self.Ik then E:loadTemplate(doc) else E:loadDoc(doc) end
    self.partSel = nil
    self:fit()
    self:setNotice(self.templateSel .. " template ready")
    return true
  end
  if #E.doc.bones > 0 and self.confirm then
    self.confirm("Replace the current rig with the " .. self.templateSel .. " template?", apply)
    return true
  end
  return apply()
end

function V:partNames()
  local out = {}
  for i, p in ipairs(self.E.doc.parts) do out[i] = p.name end
  return out
end

--- The IK controller of the selected bone's chain, or nil.
function V:selCtrl()
  local b = self.E:selected()
  if not b or not self.Ik then return nil end
  return self.Ik.forBone(self.E.doc, b.id)
end

function V:selPart()
  return self.partSel and self.Rig.part(self.E.doc, self.partSel) or nil
end

local function bulgeCount(b)
  return #b.bulgeL + #b.bulgeR
end

local function isLoose(b)
  return b.parent == 0 or not b.connected
end

--- One adapter for every tab: the spec id says what it edits.
function V:newAdapter()
  local view, E = self, self.E
  local anim = function() return E.doc.anim end
  return {
    get = function(spec)
      local id = spec.id
      if spec.get then return spec.get(spec) end
      if spec.pose then
        local v = E:getPoseValue(spec.ch)
        return v and spec.scale and v * spec.scale or v
      end
      if spec.partCh then
        local v = view.partSel and E:getPartValue(view.partSel, spec.partCh)
        if spec.partCh == "vis" then return v ~= nil and v >= 0.5 end
        return v
      end
      if E.PROPS[id] then
        if id == "part" then
          local b = E:selected()
          local p = b and view.Rig.part(E.doc, b.part)
          return p and p.name or M.NO_GROUP
        end
        return E:getProp(id)
      end
      local p = view:selPart()
      if id == "partList" then
        for i, q in ipairs(E.doc.parts) do
          if p and q.id == p.id then return i end
        end
        return nil
      end
      if id == "partName" then return p and p.name end
      if id == "partColor" then
        return p and { r = p.color[1], g = p.color[2], b = p.color[3], a = 255 }
      end
      if id == "partZ" then return p and p.z end
      if id == "partBlend" then return p and p.blend end
      if id == "frames" or id == "frameMs" or id == "loop" or id == "pingpong" then return anim()[id] end
      if id == "frameHold" then return E:frameMs() end
      if id == "refTarget" then return view.refTarget end
      if spec.refCh then
        local v = E:getRefValue(spec.refCh)
        if spec.refCh == "rf" then return v == 1 end
        return v and spec.scale and v * spec.scale or v
      end
      if id == "refOn" then return E:ref() and E:ref().on end
      if id == "refFront" then return E:ref() and E:ref().front end
      if id == "keyScale" then return view.keyScale end
      if id == "clipList" then return E.doc.clipIndex end
      if id == "clipName" then return E.doc.anim.name end
      if id == "ease" then return E:getEase() end
      if id == "onion" then return view.opts.onion end
      if id == "lock" then return view.opts.lock end
      local c = view:selCtrl()
      if id == "ikOn" then return c and c.on end
      if id == "ikStretch" then return c and c.stretch end
      if id == "ikMax" then return c and floor(c.maxStretch * 100 + 0.5) end
      if id == "ikLock" then return c and c.lockTip end
      if id == "groundOn" then return E.doc.ground.on end
      if id == "groundY" then return E.doc.ground.y end
      if id == "groundStop" then return E.doc.ground.stop end
      if id == "pinLimbs" then return E.pinLimbs end
      if id == "template" then return view.templateSel end
      if id == "mirror" then return E.doc.mirror.on end
      if id == "mirrorX" then return E.doc.mirror.x end
      if id == "outline" then return view.opts.outline end
      if id == "fill" then return view.opts.fill end
      if id == "showSkin" then return view.opts.showSkin end
      if id == "showBones" then return view.opts.showBones end
      if id == "bg" then return view.opts.bg end
      if id == "bgColor" then
        local c = view.opts.bgColor
        return { r = c[1], g = c[2], b = c[3], a = 255 }
      end
      if id == "bgLayer" then return view.opts.bgLayer or "" end
      if id == "showBounds" then return view.opts.showBounds end
      if id == "canvasW" then return E.doc.canvas.w end
      if id == "canvasH" then return E.doc.canvas.h end
      return nil
    end,
    set = function(spec, v)
      local id = spec.id
      if spec.set then spec.set(spec, v) return end
      if spec.pose then
        if spec.scale then v = (tonumber(v) or spec.scale) / spec.scale end
        E:setPoseValue(spec.ch, v)
        return
      end
      if spec.partCh then
        if view.partSel then
          if spec.partCh == "vis" then v = v == true and 1 or 0 end
          E:setPartValue(view.partSel, spec.partCh, v)
        end
        return
      end
      if id == "part" then
        if v == M.NO_GROUP then E:setProp("part", 0) return end
        for _, p in ipairs(E.doc.parts) do
          if p.name == v then E:setProp("part", p.id) view.partSel = p.id return end
        end
        return
      end
      if E.PROPS[id] then E:setProp(id, v) return end
      if id == "partList" then
        local p = E.doc.parts[v]
        if p then view.partSel = p.id end
        return
      end
      local p = view:selPart()
      if id == "partName" and p then E:setPartProp(p.id, "name", tostring(v)) return end
      if id == "partColor" and p and type(v) == "table" then
        E:setPartProp(p.id, "color", { v.r or 0, v.g or 0, v.b or 0 })
        return
      end
      if id == "partZ" and p then E:setPartProp(p.id, "z", floor(tonumber(v) or 0)) return end
      if id == "partBlend" and p then E:setPartProp(p.id, "blend", tonumber(v) or 0) return end
      if id == "clipList" then
        if view.playing then view:togglePlay() end
        E:setClip(tonumber(v) or E.doc.clipIndex)
        return
      end
      if id == "clipName" then E:renameClip(v) return end
      if id == "frames" or id == "frameMs" or id == "loop" or id == "pingpong" then
        E:setAnimProp(id, v)
        if id == "frameMs" and view.playing and view.onPlay then
          view.onPlay(true, E:frameMs())
        end
        return
      end
      if id == "frameHold" then
        E:setFrameMs(v)
        return
      end
      if id == "keyScale" then view.keyScale = max(25, min(400, tonumber(v) or 200)) return end
      if id == "refTarget" then view.refTarget = v return end
      if spec.refCh then
        if spec.refCh == "rf" then v = v == true and 1 or 0
        elseif spec.scale then v = (tonumber(v) or 0) / spec.scale end
        E:setRefValue(spec.refCh, v)
        return
      end
      if id == "refOn" or id == "refFront" then
        E:setRefProp(id == "refOn" and "on" or "front", v == true)
        return
      end
      if id == "ease" then E:setEase(v) return end
      if id == "onion" then view.opts.onion = v == true return end
      if id == "lock" then view.opts.lock = v == true return end
      local c = view:selCtrl()
      if id == "ikOn" and c then E:setIKProp(c.id, "on", v == true) return end
      if id == "ikStretch" and c then E:setIKProp(c.id, "stretch", v == true) return end
      if id == "ikMax" and c then E:setIKProp(c.id, "maxStretch", (tonumber(v) or 125) / 100) return end
      if id == "ikLock" and c then E:setIKProp(c.id, "lockTip", v == true) return end
      if id == "groundOn" then E:setGround("on", v == true) return end
      if id == "groundY" then E:setGround("y", tonumber(v)) return end
      if id == "groundStop" then E:setGround("stop", v == true) return end
      if id == "pinLimbs" then E.pinLimbs = v == true return end
      if id == "template" then view.templateSel = v return end
      if id == "mirror" then E:setMirror(v == true) return end
      if id == "mirrorX" then E:setMirror(E.doc.mirror.on, tonumber(v)) return end
      if id == "outline" then view.opts.outline = v return end
      if id == "fill" then view.opts.fill = tonumber(v) or view.opts.fill return end
      if id == "showSkin" then view.opts.showSkin = v == true return end
      if id == "showBones" then view.opts.showBones = v == true return end
      if id == "bg" then
        view.opts.bg = v
        view.refToken = view.refToken + 1
        if v == "Layer" and not view.opts.bgLayer and view.layers then
          view.opts.bgLayer = (view.layers() or {})[1]
        end
        if (v == "Sprite" or v == "Layer") and not view:referenceBuffer() then
          view:setNotice("This background needs an open RGB sprite", true)
        end
        return
      end
      if id == "bgColor" and type(v) == "table" then
        view.opts.bgColor = { v.r or 0, v.g or 0, v.b or 0 }
        return
      end
      if id == "bgLayer" then
        view.opts.bgLayer = v
        view.refToken = view.refToken + 1
        return
      end
      if id == "showBounds" then view.opts.showBounds = v == true return end
      if id == "canvasW" or id == "canvasH" then
        local c = E.doc.canvas
        local w = id == "canvasW" and tonumber(v) or c.w
        local h = id == "canvasH" and tonumber(v) or c.h
        if E:setCanvasSize(w, h) then view:fit() end
        return
      end
    end,
    visible = function(spec)
      if type(spec.visible) == "function" then return spec.visible(spec) end
      local b = E:selected()
      local pose = E.mode == "Pose"
      if spec.id == "noBone" then return b == nil end
      if spec.needsBone and not b then return false end
      if spec.rest and pose then return false end
      if spec.pose and not pose then return false end
      if spec.inPose and not pose then return false end
      if spec.loose and not (b and isLoose(b)) then return false end
      if spec.needsPart and not view:selPart() then return false end
      if spec.needsMirror and not E.doc.mirror.on then return false end
      if spec.needsOnion and not view.opts.onion then return false end
      if spec.needsBg and view.opts.bg ~= spec.needsBg then return false end
      if spec.needsGround and not E.doc.ground.on then return false end
      if spec.needsKeys and E:keySelCount() == 0 then return false end
      if spec.needsRef and not (view.Ref and E:ref() and view.Ref.used(E.doc.anim)) then return false end
      if spec.notDriven or spec.driven then
        -- a bone a live IK chain turns: its rotation slider would do nothing
        local c = view:selCtrl()
        local isDriven = c and c.on and b and (c.upper == b.id or c.lower == b.id)
        if spec.notDriven and isDriven then return false end
        if spec.driven and not isDriven then return false end
      end
      if spec.needsIK or spec.canAddIK or spec.noIK then
        local c = view:selCtrl()
        local can = b and view.Ik and view.Ik.chainFor(E.doc, b.id) ~= nil
        if spec.needsIK and not c then return false end
        if spec.canAddIK and (c or not can) then return false end
        if spec.noIK and (c or can) then return false end
        if spec.needsStretch and not (c and c.stretch) then return false end
        if spec.needsTip and not (c and c.tip ~= 0) then return false end
      end
      return true
    end,
    enabled = function(spec)
      if type(spec.enabled) == "function" then return spec.enabled(spec) end
      if spec.id == "connected" then
        local b = E:selected()
        return b ~= nil and b.parent ~= 0
      end
      if spec.id == "clearBulges" then
        local b = E:selected()
        return b ~= nil and bulgeCount(b) > 0
      end
      if spec.id == "assignPart" or spec.id == "unassignPart" then return E:selected() ~= nil end
      if spec.id == "refClearFrame" then
        local r = E:ref()
        return r ~= nil and r.frames[E.frame] ~= nil
      end
      if spec.id == "refFile" or spec.id == "refPaste" or spec.id == "refSprite" then
        return view.loadRef ~= nil and view.Ref ~= nil
      end
      if spec.id == "deleteClip" then return #E.doc.clips > 1 end
      if spec.id == "newClip" or spec.id == "duplicateClip" then return #E.doc.clips < view.Rig.MAX_CLIPS end
      if spec.id == "clipUp" then return E.doc.clipIndex > 1 end
      if spec.id == "clipDown" then return E.doc.clipIndex < #E.doc.clips end
      if spec.id == "deletePartKey" then
        return view.partSel ~= nil and view.Anim.hasKey(E.doc.anim, E.frame, { [view.partSel] = true })
      end
      if spec.id == "addKey" then return #E.doc.bones > 0 end
      if spec.id == "useTemplate" then return view.Templates ~= nil end
      if spec.id == "pastePose" or spec.id == "pasteMirrored" then return E.clip ~= nil end
      if spec.id == "copyPose" or spec.id == "resetPose" or spec.id == "symmetrize" then
        return #E.doc.bones > 0
      end
      if spec.id == "insertFrame" then return E.doc.anim.frames < view.Anim.MAX_FRAMES end
      if spec.id == "deleteFrame" then return E.doc.anim.frames > 1 end
      if spec.id == "canvasFit" then
        local bd = view.Rig.bounds(E.doc)
        return E.doc.canvas.w ~= bd.w or E.doc.canvas.h ~= bd.h
      end
      if spec.id == "deleteKey" then
        return view.Anim.hasKey(E.doc.anim, E.frame, E:targets())
      end
      return true
    end,
    options = function(spec)
      if spec.id == "partList" then
        -- each group with how many bones it holds
        local count = {}
        for _, b in ipairs(E.doc.bones) do count[b.part] = (count[b.part] or 0) + 1 end
        local out = {}
        for i, p in ipairs(E.doc.parts) do out[i] = string.format("%s (%d)", p.name, count[p.id] or 0) end
        return out
      end
      if spec.id == "part" then
        local out = { M.NO_GROUP }
        for _, n in ipairs(view:partNames()) do out[#out + 1] = n end
        return out
      end
      if spec.id == "clipList" then
        local out = {}
        for i, c in ipairs(E.doc.clips) do out[i] = c.name end
        return out
      end
      if spec.id == "template" then return view.Templates and view.Templates.NAMES or {} end
      if spec.id == "bgLayer" then return view.layers and view.layers() or {} end
      return spec.options
    end,
    label = function(spec)
      if spec.labelOf then return spec.labelOf(spec) end
      if spec.id == "bulges" then
        local b = E:selected()
        if not b then return "" end
        return string.format("Bulges  left %d  right %d", #b.bulgeL, #b.bulgeR)
      end
      if spec.id == "canvasInfo" then
        local bd = view.Rig.bounds(E.doc)
        if bd.w == E.doc.canvas.w and bd.h == E.doc.canvas.h then
          return string.format("The sprite: %d x %d px", bd.w, bd.h)
        end
        return string.format("Sprite %d x %d px in the middle", bd.w, bd.h)
      end
      return nil
    end,
    palette = function()
      local out = {}
      for i, c in ipairs(view.Rig.PALETTE) do out[i] = { c[1], c[2], c[3] } end
      return out
    end,
    -- what the kit does not know (a grid's activate): the spec decides
    action = function(spec, a)
      if spec.onAction then spec.onAction(spec, a) end
    end,
  }
end

function V:clearBulges()
  local E = self.E
  local b = E:selected()
  if not b or bulgeCount(b) == 0 then return false end
  E:beginGesture()
  E:touch()
  b.bulgeL, b.bulgeR = {}, {}
  E:syncTwin(b.id)
  E.selBulge = nil
  E:changed()
  return true
end

--- Opens tab `i`: its index in TABS or its name.
function V:setTab(i)
  if type(i) == "string" then
    local want = i
    i = nil
    for k, t in ipairs(M.TABS) do
      if t == want then i = k end
    end
  end
  local name = i and M.TABS[i]
  if not name then return false end
  self.tab = i
  -- the bar scrolls when the tabs do not fit: keep this one in sight
  if self.regions.tabs and not self.imp then
    self.UI.widgets.tabs.scrollTo(self.tabSpec, self.regions.tabs, i, self.ctx)
  end
  local specs
  if name == "Rig" then specs = self:rigSpecs()
  elseif name == "Bone" then specs = self:boneSpecs()
  elseif name == "Groups" then specs = self:partSpecs()
  elseif name == "Animation" then specs = self:animSpecs()
  elseif name == "Poses" then specs = self:poseSpecs()
  elseif name == "Library" then
    if self.Lib then self.Lib.refresh(self) end
    specs = self:librarySpecs()
  elseif name == "View" then specs = self:viewSpecs()
  else specs = self:moreSpecs() end
  self.panel = self.UI.panel.new(specs, self:newAdapter(), self.ctx)
  if self.regions.panel then self.panel:layout(self.regions.panel) end
  self.panel.bounds = { x = 0, y = 0, w = self.w, h = self.h }
  return true
end

--- Writes the rig into the sprite, looking like the preview.
function V:export()
  if not self.onExport then return false end
  if self.playing then self:togglePlay() end
  local ok, msg = self.onExport{
    outline = self.opts.outline:lower(),
    fillAlpha = floor(self.opts.fill * 255 / 100 + 0.5),
    lock = self.opts.lock,
  }
  self:setNotice(msg, not ok)
  return true
end

---------------------------------------
-- Import 3D --
---------------------------------------

--- Asks the host for a 3D file and opens the import mode on it.
function V:startImport()
  if not self.Import or not self.loadMotion then
    self:setNotice("Importing 3D needs Aseprite", true)
    return true
  end
  local bytes, name, readFile = self.loadMotion()
  if not bytes then
    if name then self:setNotice(name, true) end
    return true
  end
  return self:openImport(bytes, name, readFile)
end

--- The import mode on a file's bytes (startImport, and the tests).
function V:openImport(bytes, name, readFile)
  local s, why = self.Import.open(bytes, name, readFile, self.E.doc)
  if not s then
    self:setNotice(why, true)
    return true
  end
  if self.playing then self:togglePlay() end
  self.imp = s
  self._saved = { panel = self.panel, footer = self.footer, tabs = self.tabSpec.items }
  self.tabSpec.items = { "Import 3D" }
  local view = self
  local noValue = function() return nil end
  self.panel = self.UI.panel.new(self.Import.specs(self), self.Import.adapter(self), self.ctx)
  self.footer = self.UI.panel.new({
    { id = "impApply", kind = "button", text = "Import", onPress = function() view:endImport(true) end },
    { id = "impCancel", kind = "button", text = "Cancel", row = "same",
      onPress = function() view:endImport(false) end },
  }, { get = noValue, set = function() end }, self.ctx)
  self:setNotice(string.format("%s: %d joints, %d animation%s", s.name, #s.skel.joints,
    #s.skel.clips, #s.skel.clips == 1 and "" or "s"))
  self:layout(self.w, self.h)
  return true
end

--- The import panel again (its Bones section follows "Onto").
function V:refreshImport()
  if not self.imp then return false end
  self.panel = self.UI.panel.new(self.Import.specs(self), self.Import.adapter(self), self.ctx)
  if self.regions.panel then self.panel:layout(self.regions.panel) end
  self.panel.bounds = { x = 0, y = 0, w = self.w, h = self.h }
  return true
end

--- Leaves the import mode, building the clip first when `apply`. A new
--- rig over one that has bones asks first (Ctrl+Z still brings it back).
function V:endImport(apply, sure)
  local s = self.imp
  if not s then return false end
  if apply and not sure and s.target == "New rig" and #self.E.doc.bones > 0 and self.confirm then
    self.confirm("Replace this rig and its clips with the one in the file?", function()
      self:endImport(true, true)
    end)
    return true
  end
  local ok, msg = true, "Import cancelled"
  if apply then ok, msg = self.Import.apply(s, self.E) end
  if apply and not ok then
    self:setNotice(msg, true)
    return true
  end
  if s.playing and self.onPlay then self.onPlay(false) end
  self.imp = nil
  local sv = self._saved
  self.panel, self.footer, self.tabSpec.items = sv.panel, sv.footer, sv.tabs
  self._saved = nil
  if apply then
    self:setMode("Pose")
    self.cam.fitted = false
  end
  self:setTab(self.tab)
  self:layout(self.w, self.h)
  self:setNotice(msg, not ok)
  return true
end

---------------------------------------
-- Modes and playback --
---------------------------------------

function V:setMode(mode)
  if not self.E:setMode(mode) then return false end
  self.toolSpec.items = self.E:tools()
  self.toolSpec._scroll = 0
  if mode == "Rest" and self.playing then self:togglePlay() end
  return true
end

function V:togglePlay()
  local s = self.imp
  if s then
    s.playing = not s.playing
    if self.onPlay then self.onPlay(s.playing, self.Import.shown(s).frameMs) end
    return true
  end
  if not self.playing and self.E.mode ~= "Pose" then self:setMode("Pose") end
  self.playing = not self.playing
  self.playDir = 1
  if self.onPlay then self.onPlay(self.playing, self.E:frameMs()) end
  return true
end

--- How long the frame on screen lasts (the host's Timer follows it).
function V:frameMs()
  return self.E:frameMs()
end

--- One playback step (the host's Timer). True when the frame moved. A
--- ping-pong clip turns round at both ends.
function V:tick()
  if self.imp then return self.Import.tick(self.imp) end
  if not self.playing then return false end
  local E = self.E
  local a = E.doc.anim
  if a.pingpong and a.frames > 1 then
    local f = E.frame + (self.playDir or 1)
    if f < 1 or f > a.frames then
      self.playDir = -(self.playDir or 1)
      f = E.frame + self.playDir
    end
    return E:setFrame(f)
  end
  return E:stepFrame(1)
end

---------------------------------------
-- Layout --
---------------------------------------

function V:splitBounds(w)
  local s = self.theme.uiScale
  local lo = M.PANEL_MIN
  local hi = floor(w / s) - M.PREVIEW_MIN - M.SPLITTER_W
  if hi < lo then hi = lo end
  return lo, hi
end

function V:tabsWidth(items)
  local w = 0
  for _, t in ipairs(items) do w = w + self.text:width(t) + 2 * self.theme.tabPad end
  return w
end

function V:layout(w, h)
  local th = self.theme
  local s = th.uiScale
  self.w, self.h = w, h
  local pad, gap = th.pad, th.gap
  local lo, hi = self:splitBounds(w)
  local lw = self.leftW or M.PANEL_MIN
  if lw < lo then lw = lo elseif lw > hi then lw = hi end
  local leftW = floor(lw * s) - pad
  -- as tall as its rows (two here, one while importing)
  self.footer:layout({ x = 0, y = 0, w = max(1, leftW), h = 0 })
  local footerH = self.footer.contentH
  local tabsR = { x = pad, y = pad, w = leftW, h = th.tabH }
  local footerR = { x = pad, y = h - pad - footerH, w = leftW, h = footerH }
  local panelY = tabsR.y + tabsR.h + gap
  local panelR = { x = pad, y = panelY, w = leftW, h = max(0, footerR.y - gap - panelY) }
  local splitterR = { x = pad + leftW, y = pad, w = th.splitterW, h = max(0, h - 2 * pad) }
  local rx = splitterR.x + splitterR.w
  local rw = max(0, w - rx - pad)
  local modeW = min(rw, self:tabsWidth(self.Editor.MODES))
  local modeR = { x = rx, y = pad, w = modeW, h = th.tabH }
  local toolsR = { x = rx + modeW + 2 * gap, y = pad, w = max(0, rw - modeW - 2 * gap), h = th.tabH }
  local statusR = { x = rx, y = h - pad - th.labelH, w = rw, h = th.labelH }
  local tlH = th.rowH + 2 * pad
  local vpY = toolsR.y + toolsR.h + gap
  -- the dope sheet takes what the viewport can spare
  local sheetH = 0
  if self.Dope then
    local most = max(0, statusR.y - gap - tlH - th.splitterW - vpY - M.PREVIEW_MIN * s)
    sheetH = max(0, min(most, floor((self.sheetH or self.Dope.DEFAULT_H) * s)))
  end
  local sheetR = { x = rx, y = statusR.y - gap - sheetH, w = rw, h = sheetH }
  local timelineR = { x = rx, y = sheetR.y - tlH, w = rw, h = tlH }
  local nameW = self.Dope and self.Dope.NAME_W or M.PLAY_W
  -- a narrow column keeps two thirds for the frame cells
  local playW = min(nameW * s, floor(rw / 3))
  local playR = { x = rx, y = timelineR.y, w = playW, h = tlH }
  local stripR = { x = rx + playW + gap, y = timelineR.y + pad, w = max(0, rw - playW - gap),
    h = th.rowH }
  local namesR = { x = rx, y = sheetR.y, w = playW, h = sheetH }
  local hsplitR = self.Dope and { x = rx, y = timelineR.y - th.splitterW, w = rw, h = th.splitterW } or nil
  local viewportR = { x = rx, y = vpY,
    w = rw, h = max(0, (hsplitR and hsplitR.y or timelineR.y - gap) - vpY) }
  self.regions = {
    tabs = tabsR, panel = panelR, footer = footerR, splitter = splitterR,
    mode = modeR, tools = toolsR, viewport = viewportR, timeline = timelineR,
    play = playR, strip = stripR, status = statusR,
    sheet = self.Dope and sheetR or nil, names = namesR, hsplit = hsplitR,
  }
  if self.Dope then self.Dope.clampScroll(self) end
  self.panel:layout(panelR)
  self.panel.bounds = { x = 0, y = 0, w = w, h = h }
  self.footer:layout(footerR)
  self.footer.bounds = { x = 0, y = 0, w = w, h = h }
  self.playPanel:layout(playR)
  self.playPanel.bounds = { x = 0, y = 0, w = w, h = h }
  local doc = self.E.doc
  local cam = self.cam
  if not cam.fitted or cam.cw ~= doc.canvas.w or cam.ch ~= doc.canvas.h then
    self:fit()
  end
  return self.regions
end

---------------------------------------
-- Camera --
---------------------------------------

--- The zoom actually drawn: whole numbers from 1x up.
function V:z()
  local z = self.cam.zoom
  if z >= 1 then return floor(z) end
  return z
end

function V:toScreen(x, y)
  local r = self.regions.viewport
  local z = self:z()
  return r.x + self.cam.px + x * z, r.y + self.cam.py + y * z
end

function V:toRig(sx, sy)
  local r = self.regions.viewport
  local z = self:z()
  return (sx - r.x - self.cam.px) / z, (sy - r.y - self.cam.py) / z
end

function V:fit()
  local r = self.regions.viewport
  if not r or r.w <= 0 or r.h <= 0 then return false end
  local doc = self.E.doc
  local cw, ch = doc.canvas.w, doc.canvas.h
  local m = M.FIT_MARGIN * self.theme.uiScale
  local z = min((r.w - 2 * m) / cw, (r.h - 2 * m) / ch)
  z = max(M.ZOOM_MIN, min(M.ZOOM_MAX, z))
  if z >= 1 then z = floor(z) end
  local cam = self.cam
  cam.zoom = z
  cam.px = floor((r.w - cw * z) / 2)
  cam.py = floor((r.h - ch * z) / 2)
  cam.fitted, cam.cw, cam.ch = true, cw, ch
  return true
end

--- Zooms to `zoom`, keeping the rig point under (sx, sy) where it is.
function V:zoomAt(zoom, sx, sy)
  zoom = max(M.ZOOM_MIN, min(M.ZOOM_MAX, zoom))
  local rx, ry = self:toRig(sx, sy)
  self.cam.zoom = zoom
  local z = self:z()
  local r = self.regions.viewport
  self.cam.px = floor(sx - r.x - rx * z + 0.5)
  self.cam.py = floor(sy - r.y - ry * z + 0.5)
  return true
end

---------------------------------------
-- Paint --
---------------------------------------

local function append(ops, more)
  for i = 1, #more do ops[#ops + 1] = more[i] end
end

local function rectOp(ops, x, y, w, h, c, a)
  ops[#ops + 1] = { "rect", floor(x), floor(y), floor(w), floor(h), c[1], c[2], c[3], a }
end

local function lineOp(ops, c, width, pts)
  local op = { "line", c[1], c[2], c[3], width }
  for i = 1, #pts do op[#op + 1] = pts[i] end
  ops[#ops + 1] = op
end

function V:statusText()
  local E = self.E
  local n = #E.doc.bones
  local s = "Bonebase " .. tostring(self.version) .. "  |  " .. E.mode
  if E.mode == "Pose" then
    if #E.doc.clips > 1 then s = s .. "  |  " .. E.doc.anim.name end
    s = s .. string.format("  |  Frame %d / %d", E.frame, E.doc.anim.frames)
    if E.doc.anim.durations[E.frame] then s = s .. string.format(" (%d ms)", E:frameMs()) end
  end
  s = s .. "  |  " .. n .. (n == 1 and " bone" or " bones")
  local b = E:selected()
  if b then
    local more = #E:selectedIds() - 1
    s = s .. "  |  " .. b.name .. (more > 0 and string.format(" +%d", more) or "")
  end
  return s .. "  |  " .. (M.HINTS[E.tool] or "")
end

--- The composed skin, cached by revision, mode, frame and view options.
function V:skinBuffer()
  local E = self.E
  local key = E.rev .. "|" .. E.mode .. "|" .. E.frame .. "|" .. self.opts.outline
    .. "|" .. self.opts.fill
  if self._skinKey == key and self._skin then return self._skin end
  local doc = E.doc
  local world = E:world()
  local layers = self.Skin.render(doc, world, { outline = self.opts.outline:lower(), pose = E:pose() })
  self._skin = self.Skin.compose(layers, doc.canvas.w, doc.canvas.h,
    { fillAlpha = floor(self.opts.fill * 255 / 100 + 0.5) })
  self._skinKey = key
  return self._skin
end

--- The onion skin of the frames around the current one (onion.lua, with
--- the settings of the Animation tab). Cached: the whole picture by rig,
--- frame and settings, and what each frame covers by rig and style (so
--- playback and the settings only composite again). While a drag in the
--- viewport goes on the picture stays as it was; it follows on release.
function V:onionBuffer()
  local E, O = self.E, self.Onion
  if not O then return nil end
  local o = self.onionSet
  local settings = E.frame .. "|" .. O.key(o)
  local key = E.rev .. "|" .. settings
  if self._onionKey == key then return self._onion end
  if E.drag and self._onion and self._onionSettings == settings then return self._onion end
  local tag = E.rev .. "|" .. o.style
  if not self._onionCache or self._onionCache.tag ~= tag then self._onionCache = O.newCache(tag) end
  local list = O.frames(E.doc.anim, E.frame, o)
  self._onion = O.render({ Skin = self.Skin, Rig = self.Rig, Anim = self.Anim, Ik = self.Ik },
    E.doc, list, o, self._onionCache)
  self._onionKey, self._onionSettings = key, settings
  return self._onion
end

function V:paintChecker(ops, x0, y0, x1, y1)
  local th = self.theme
  local cell = th.checker
  rectOp(ops, x0, y0, x1 - x0, y1 - y0, th.checkerA)
  -- cells are anchored to the canvas corner so they do not crawl on pan
  local cx, cy = self:toScreen(0, 0)
  local i0 = floor((x0 - cx) / cell)
  local j0 = floor((y0 - cy) / cell)
  local i1 = floor((x1 - 1 - cx) / cell)
  local j1 = floor((y1 - 1 - cy) / cell)
  for j = j0, j1 do
    for i = i0, i1 do
      if (i + j) % 2 == 1 then
        local ax, ay = max(x0, cx + i * cell), max(y0, cy + j * cell)
        local bx, by = min(x1, cx + (i + 1) * cell), min(y1, cy + (j + 1) * cell)
        if bx > ax and by > ay then rectOp(ops, ax, ay, bx - ax, by - ay, th.checkerB) end
      end
    end
  end
end

function V:circle(ops, c, cx, cy, r)
  local pts = {}
  for k = 0, M.CIRCLE_SEGS do
    local a = k / M.CIRCLE_SEGS * 2 * math.pi
    local sx, sy = self:toScreen(cx + r * math.cos(a), cy + r * math.sin(a))
    pts[#pts + 1] = sx
    pts[#pts + 1] = sy
  end
  lineOp(ops, c, 1, pts)
end

function V:joint(ops, c, x, y)
  local s = self.theme.uiScale
  -- small when zoomed out, so markers never hide the figure
  local k = max(s, min(M.JOINT_PX * s, floor(self:z() / 2) * s))
  local sx, sy = self:toScreen(x, y)
  rectOp(ops, sx - k, sy - k, 2 * k + 1, 2 * k + 1, self.theme.overlayBg)
  rectOp(ops, sx - k + s, sy - k + s, 2 * k + 1 - 2 * s, 2 * k + 1 - 2 * s, c)
end

--- Where bulge handle (t, d) of `side` sits on the skin edge, rig space.
function V:handlePoint(b, w, side, t)
  local c, s = (w.tx - w.hx) / b.len, (w.ty - w.hy) / b.len
  local dist = self.Skin.edgeOffset(b, t, side)
  local ax, ay = w.hx + c * t * b.len, w.hy + s * t * b.len
  if side == "left" then return ax + s * dist, ay - c * dist end
  return ax - s * dist, ay + c * dist
end

function V:paintOverlays(ops)
  local th = self.theme
  local E = self.E
  local doc = E.doc
  local world = E:world()
  local s = th.uiScale
  local x0, y0 = self:toScreen(0, 0)
  local x1, y1 = self:toScreen(doc.canvas.w, doc.canvas.h)
  local b = th.previewBounds
  ops[#ops + 1] = { "frame", floor(x0) - 1, floor(y0) - 1, floor(x1 - x0) + 2, floor(y1 - y0) + 2,
    b[1], b[2], b[3], 90 }
  if doc.mirror.on and E.mode == "Rest" then
    local ax = self:toScreen(doc.mirror.x, 0)
    lineOp(ops, th.mirrorAxis, s, { ax, y0, ax, y1 })
  end
  if self.opts.showBones then
    local sel = E.sel
    for _, bone in ipairs(doc.bones) do
      if not E:isSelected(bone.id) then
        local w = world[bone.id]
        local hx, hy = self:toScreen(w.hx, w.hy)
        local tx, ty = self:toScreen(w.tx, w.ty)
        lineOp(ops, th.overlayBg, 3 * s, { hx, hy, tx, ty })
        lineOp(ops, th.bone, s, { hx, hy, tx, ty })
      end
    end
    for _, bone in ipairs(doc.bones) do
      local w = world[bone.id]
      self:joint(ops, th.boneDim, w.hx, w.hy)
      self:joint(ops, E:isSelected(bone.id) and th.accent or th.bone, w.tx, w.ty)
    end
    for _, id in ipairs(E:selectedIds()) do
      if id ~= sel then
        local w = world[id]
        local hx, hy = self:toScreen(w.hx, w.hy)
        local tx, ty = self:toScreen(w.tx, w.ty)
        lineOp(ops, th.overlayBg, 4 * s, { hx, hy, tx, ty })
        lineOp(ops, th.accent, 2 * s, { hx, hy, tx, ty })
        self:joint(ops, th.accent, w.tx, w.ty)
      end
    end
    local sb = E:selected()
    if sb then
      local w = world[sb.id]
      local hx, hy = self:toScreen(w.hx, w.hy)
      local tx, ty = self:toScreen(w.tx, w.ty)
      lineOp(ops, th.overlayBg, 4 * s, { hx, hy, tx, ty })
      lineOp(ops, th.accent, 2 * s, { hx, hy, tx, ty })
      self:joint(ops, th.accent, w.tx, w.ty)
      if E.tool == "Radius" or E.tool == "Select" then
        self:circle(ops, th.accent, w.hx, w.hy, sb.rHead)
        self:circle(ops, th.accent, w.tx, w.ty, sb.rTail)
      end
    end
  end
  self:paintIK(ops, x0, x1)
  if E.tool == "Bulge" then
    for _, bone in ipairs(doc.bones) do
      local w = world[bone.id]
      for _, side in ipairs({ "left", "right" }) do
        local list = side == "left" and bone.bulgeL or bone.bulgeR
        for i, q in ipairs(list) do
          local sbz = E.selBulge
          local on = sbz and sbz.bone == bone.id and sbz.side == side and sbz.i == i
          local px, py = self:handlePoint(bone, w, side, q[1])
          self:joint(ops, on and th.accent or th.handle, px, py)
        end
      end
    end
  end
  local d = E.drag
  if d and d.kind == "new" then
    local hx, hy = self:toScreen(d.hx, d.hy)
    local tx, ty = self:toScreen(d.tx, d.ty)
    lineOp(ops, th.overlayBg, 4 * s, { hx, hy, tx, ty })
    lineOp(ops, th.accent, 2 * s, { hx, hy, tx, ty })
    if doc.mirror.on then
      local mhx = self:toScreen(2 * doc.mirror.x - d.hx, d.hy)
      local mtx = self:toScreen(2 * doc.mirror.x - d.tx, d.ty)
      lineOp(ops, th.accent, s, { mhx, hy, mtx, ty })
    end
  end
end

--- The clip's reference image on the canvas (refimg.lua), and its box
--- while the Ref tool is out.
function V:paintReference(ops)
  local E = self.E
  local dr = self.Ref.draw(E.doc, E.doc.anim, E:refFrame())
  if not dr then return end
  local x0, y0 = self:toScreen(dr.x, dr.y)
  local x1, y1 = self:toScreen(dr.x + dr.w, dr.y + dr.h)
  ops[#ops + 1] = { "image", floor(x0), floor(y0), floor(x1) - floor(x0), floor(y1) - floor(y0), dr.buf }
  if E.tool == "Ref" then
    local c = self.theme.accent
    ops[#ops + 1] = { "frame", floor(x0), floor(y0), floor(x1) - floor(x0), floor(y1) - floor(y0),
      c[1], c[2], c[3] }
  end
end

function V:paintViewport(ops)
  local th = self.theme
  local r = self.regions.viewport
  if r.w <= 0 or r.h <= 0 then return end
  local E = self.E
  local doc = E.doc
  ops[#ops + 1] = { "save" }
  ops[#ops + 1] = { "clip", r.x, r.y, r.w, r.h }
  rectOp(ops, r.x, r.y, r.w, r.h, th.bg)
  local cx0, cy0 = self:toScreen(0, 0)
  local cx1, cy1 = self:toScreen(doc.canvas.w, doc.canvas.h)
  local x0, y0 = max(r.x, floor(cx0)), max(r.y, floor(cy0))
  local x1, y1 = min(r.x + r.w, floor(cx1)), min(r.y + r.h, floor(cy1))
  if x1 > x0 and y1 > y0 then self:paintChecker(ops, x0, y0, x1, y1) end
  local iw, ih = floor(cx1) - floor(cx0), floor(cy1) - floor(cy0)
  local bg = self.opts.bg
  if bg == "Color" and x1 > x0 and y1 > y0 then
    rectOp(ops, x0, y0, x1 - x0, y1 - y0, self.opts.bgColor)
  elseif (bg == "Sprite" or bg == "Layer") and self.reference then
    local rb = self:referenceBuffer()
    if rb then
      local bd = self.Rig.bounds(doc)
      local sx, sy = self:toScreen(bd.x, bd.y)
      local ex, ey = self:toScreen(bd.x + rb.w, bd.y + rb.h)
      ops[#ops + 1] = { "image", floor(sx), floor(sy), floor(ex) - floor(sx), floor(ey) - floor(sy), rb }
    end
  end
  local ob = self.opts.onion and E.mode == "Pose" and #doc.bones > 0 and self:onionBuffer()
  local front = self.onionSet and self.onionSet.front
  if ob and not front then ops[#ops + 1] = { "image", floor(cx0), floor(cy0), iw, ih, ob } end
  local ref = self.Ref and doc.anim.ref
  if ref and not ref.front then self:paintReference(ops) end
  if self.opts.showSkin and #doc.bones > 0 then
    ops[#ops + 1] = { "image", floor(cx0), floor(cy0), iw, ih, self:skinBuffer() }
  end
  if ob and front then ops[#ops + 1] = { "image", floor(cx0), floor(cy0), iw, ih, ob } end
  if ref and ref.front then self:paintReference(ops) end
  self:paintBounds(ops)
  self:paintOverlays(ops)
  self:paintRoot(ops)
  if #doc.bones == 0 then
    local msg = "Drag with the Bone tool, or pick a template in Rig"
    local tw = self.text:width(msg)
    local th2 = self.text:height()
    local bx = r.x + floor((r.w - tw) / 2) - th.pad
    local by = r.y + th.pad
    rectOp(ops, bx, by, tw + 2 * th.pad, th2 + 2 * th.gap, th.overlayBg)
    ops[#ops + 1] = { "text", bx + th.pad, by + th.gap, th.textDim[1], th.textDim[2],
      th.textDim[3], msg }
  end
  if self.help then self:paintHelp(ops) end
  ops[#ops + 1] = { "restore" }
end

--- The rig root's handle: a ring with a cross under the rig. Dragging it
--- with Select (Rest) moves the whole rig, with Pose or IK it keys the
--- offset that carries everything, IK dummies included.
function V:paintRoot(ops)
  if not self.opts.showBones then return end
  local E = self.E
  local tool = E.tool
  local usable = (E.mode == "Rest" and tool == "Select") or (E.mode == "Pose" and (tool == "Pose" or tool == "IK"))
  if not usable then return end
  local hx, hy = E:rootHandle()
  if not hx then return end
  local sx, sy = self:toScreen(hx, hy)
  local s = self.theme.uiScale
  local c = self.theme.rootHandle
  local r = 5 * s
  local pts = {}
  for k = 0, 16 do
    local a = k / 16 * 2 * math.pi
    pts[#pts + 1] = sx + r * math.cos(a)
    pts[#pts + 1] = sy + r * math.sin(a)
  end
  lineOp(ops, self.theme.overlayBg, 3 * s, pts)
  lineOp(ops, c, s, pts)
  lineOp(ops, c, s, { sx - r - 3 * s, sy, sx + r + 3 * s, sy })
  lineOp(ops, c, s, { sx, sy - r - 3 * s, sx, sy + r + 3 * s })
end

--- The sprite's own edges when the canvas is larger than it: a thin,
--- quiet outline just outside them (View tab, Sprite bounds).
function V:paintBounds(ops)
  if not self.opts.showBounds then return end
  local doc = self.E.doc
  local bd = self.Rig.bounds(doc)
  if bd.w == doc.canvas.w and bd.h == doc.canvas.h then return end
  local x0, y0 = self:toScreen(bd.x, bd.y)
  local x1, y1 = self:toScreen(bd.x + bd.w, bd.y + bd.h)
  x0, y0, x1, y1 = floor(x0), floor(y0), floor(x1), floor(y1)
  local s = self.theme.uiScale
  local c, a = self.theme.previewBounds, M.BOUNDS_ALPHA
  rectOp(ops, x0 - s, y0 - s, x1 - x0 + 2 * s, s, c, a)
  rectOp(ops, x0 - s, y1, x1 - x0 + 2 * s, s, c, a)
  rectOp(ops, x0 - s, y0, s, y1 - y0, c, a)
  rectOp(ops, x1, y0, s, y1 - y0, c, a)
end

--- The sprite's drawing at the frame shown (all of it, or the Background
--- layer), cached until the frame, the choice or the sprite changes (the
--- host composes it; see ui.lua).
function V:referenceBuffer()
  if not self.reference then return nil end
  local layer = self.opts.bg == "Layer" and self.opts.bgLayer or nil
  local key = self.E.frame .. "|" .. self.refToken .. "|" .. tostring(layer)
  if self._refKey == key then return self._ref end
  local ok, buf = pcall(self.reference, self.E.frame, layer)
  self._ref = ok and buf or nil
  self._refKey = key
  return self._ref
end

--- The shortcut overlay over the viewport.
function V:paintHelp(ops)
  local th = self.theme
  local r = self.regions.viewport
  local lh = self.text:height() + th.gap
  local keyW = 0
  for _, s in ipairs(M.SHORTCUTS) do keyW = max(keyW, self.text:width(s[1])) end
  local w = min(r.w - 2 * th.pad, keyW + th.colGap + 320 * th.uiScale)
  local h = min(r.h - 2 * th.pad, (#M.SHORTCUTS + 2) * lh + 2 * th.pad)
  local x = r.x + floor((r.w - w) / 2)
  local y = r.y + floor((r.h - h) / 2)
  rectOp(ops, x, y, w, h, th.overlayBg, 235)
  ops[#ops + 1] = { "frame", x, y, w, h, th.frame[1], th.frame[2], th.frame[3] }
  local ty = y + th.pad
  ops[#ops + 1] = { "text", x + th.pad, ty, th.text[1], th.text[2], th.text[3], "Shortcuts" }
  ops[#ops + 1] = { "text", x + w - th.pad - self.text:width("Esc closes"), ty,
    th.textDim[1], th.textDim[2], th.textDim[3], "Esc closes" }
  ty = ty + lh * 2
  ops[#ops + 1] = { "save" }
  ops[#ops + 1] = { "clip", x, y, w, h }
  for i, s in ipairs(M.SHORTCUTS) do
    if ty + 2 * lh > y + h and i < #M.SHORTCUTS then
      -- no room for the rest: say so instead of cutting silently
      ops[#ops + 1] = { "text", x + th.pad, ty, th.warning[1], th.warning[2], th.warning[3],
        "Enlarge the window to see all shortcuts" }
      break
    end
    ops[#ops + 1] = { "text", x + th.pad, ty, th.text[1], th.text[2], th.text[3], s[1] }
    ops[#ops + 1] = { "text", x + th.pad + keyW + th.colGap, ty, th.textDim[1], th.textDim[2],
      th.textDim[3], self.text:ellipsize(s[2], max(0, w - 2 * th.pad - keyW - th.colGap)) }
    ty = ty + lh
  end
  ops[#ops + 1] = { "restore" }
end

--- A dashed line between two rig points (screen dashes of 3 px).
function V:dashed(ops, c, ax, ay, bx, by)
  local s = self.theme.uiScale
  local sx, sy = self:toScreen(ax, ay)
  local ex, ey = self:toScreen(bx, by)
  local dx, dy = ex - sx, ey - sy
  local len = math.sqrt(dx * dx + dy * dy)
  if len < 1 then return end
  local step = 3 * s
  local n = floor(len / step)
  for i = 0, n - 1, 2 do
    local t0, t1 = i * step / len, min(1, (i + 1) * step / len)
    lineOp(ops, c, s, { sx + dx * t0, sy + dy * t0, sx + dx * t1, sy + dy * t1 })
  end
end

--- A dummy marker: a target is a hollow square with a dot, a pole a
--- small solid square; `hot` draws it bigger.
function V:dummy(ops, c, x, y, pole, hot)
  local s = self.theme.uiScale
  local k = (hot and 5 or 4) * s
  local sx, sy = self:toScreen(x, y)
  sx, sy = floor(sx), floor(sy)
  local bg = self.theme.overlayBg
  if pole then
    rectOp(ops, sx - k + s, sy - k + s, 2 * k - s, 2 * k - s, bg)
    rectOp(ops, sx - k + 2 * s, sy - k + 2 * s, 2 * k - 3 * s, 2 * k - 3 * s, c)
    return
  end
  rectOp(ops, sx - k, sy - k, 2 * k + 1, 2 * k + 1, bg)
  ops[#ops + 1] = { "frame", sx - k + s, sy - k + s, 2 * k + 1 - 2 * s, 2 * k + 1 - 2 * s,
    c[1], c[2], c[3] }
  ops[#ops + 1] = { "frame", sx - k + 2 * s, sy - k + 2 * s, 2 * k + 1 - 4 * s,
    2 * k + 1 - 4 * s, c[1], c[2], c[3] }
  rectOp(ops, sx - s, sy - s, 2 * s + 1, 2 * s + 1, c)
end

--- The ground line, and in Pose the IK chains with their dummies.
function V:paintIK(ops, sx0, sx1)
  local th = self.theme
  local E = self.E
  local doc = E.doc
  local s = th.uiScale
  local g = doc.ground
  if g and g.on then
    local _, gy = self:toScreen(0, g.y)
    lineOp(ops, th.overlayBg, 3 * s, { sx0, gy, sx1, gy })
    lineOp(ops, th.ground, s, { sx0, gy, sx1, gy })
  end
  if E.mode ~= "Pose" or not self.Ik or #doc.ik == 0 then return end
  local d = E.drag
  local sel = self:selCtrl()
  -- the dummies are the IK tool's; with the others they only show, dimmed
  local ikTool = E.tool == "IK"
  for _, h in ipairs(self.Ik.handles(doc, E.frame, E:world())) do
    local on = h.c.on and ikTool
    local hot = sel == h.c or (d and d.kind == "dummy" and d.ctrl == h.c.id)
    local ct = on and (h.onGround and th.ground or th.ikTarget) or th.ikOff
    local cp = on and th.ikPole or th.ikOff
    -- the chain: base to target, joint to pole
    self:dashed(ops, ct, h.bx, h.by, h.tx, h.ty)
    self:dashed(ops, cp, h.jx, h.jy, h.px, h.py)
    if d and d.kind == "dummy" and d.ctrl == h.c.id and d.which == "t" then
      -- how far the chain reaches (stretch included)
      self:circle(ops, th.ikOff, h.bx, h.by, h.reach)
    end
    self:dummy(ops, cp, h.px, h.py, true, hot)
    self:dummy(ops, ct, h.tx, h.ty, false, hot)
  end
end

--- Left edge and width of frame `f`'s cell in the strip.
function V:cell(f)
  local r = self.regions.strip
  local n = self.E.doc.anim.frames
  local x0 = r.x + floor((f - 1) * r.w / n)
  local x1 = r.x + floor(f * r.w / n)
  return x0, x1 - x0
end

function V:frameAt(x)
  local r = self.regions.strip
  local n = self.E.doc.anim.frames
  if r.w <= 0 then return 1 end
  local f = floor((x - r.x) * n / r.w) + 1
  return max(1, min(n, f))
end

function V:paintTimeline(ops)
  local th = self.theme
  local r = self.regions.strip
  if r.w <= 0 then return end
  local E = self.E
  local anim = E.doc.anim
  local n = anim.frames
  local s = th.uiScale
  local pose = E.mode == "Pose"
  rectOp(ops, r.x, r.y, r.w, r.h, th.box)
  local keyed = {}
  for _, f in ipairs(self.Anim.keyFrames(anim, E:targets())) do keyed[f] = true end
  local numW = self.text:width(tostring(n)) + 2 * s
  local every = 1
  for _, e in ipairs({ 1, 2, 5, 10, 20, 50 }) do
    every = e
    local _, cw = self:cell(1)
    if cw * e >= numW then break end
  end
  local sd = self.stripDrag
  for f = 1, n do
    local x, cw = self:cell(f)
    local cur = pose and f == E.frame
    if cur then
      rectOp(ops, x, r.y, cw, r.h, th.accent)
    elseif f > 1 then
      rectOp(ops, x, r.y, s, r.h, th.cellLine)
    end
    local ghost = sd and sd.moving and f == sd.to
    if ghost then ops[#ops + 1] = { "frame", x, r.y, cw, r.h, th.focus[1], th.focus[2], th.focus[3] } end
    if f == 1 or f % every == 0 then
      local c = cur and th.accentText or th.textDim
      ops[#ops + 1] = { "text", x + 2 * s, r.y + 2 * s, c[1], c[2], c[3], tostring(f) }
    end
    if anim.durations[f] then
      rectOp(ops, x + 2 * s, r.y + s, max(s, cw - 4 * s), s, cur and th.accentText or th.textDim)
    end
    if keyed[f] then
      local k = M.KEY_MARK * s
      local mx = x + floor(cw / 2)
      local my = r.y + r.h - k - 2 * s
      local c = cur and th.accentText or th.keyMark
      rectOp(ops, mx - k, my - k, 2 * k + 1, 2 * k + 1, c)
    end
  end
  ops[#ops + 1] = { "frame", r.x, r.y, r.w, r.h, th.frame[1], th.frame[2], th.frame[3] }
  if not pose then
    -- Rest mode: the strip is still live (a click enters Pose mode)
    rectOp(ops, r.x, r.y, r.w, r.h, th.bg, 110)
  end
end

function V:paintSplitter(ops)
  local th = self.theme
  local r = self.regions.splitter
  if not r or r.h <= 0 then return end
  local s = floor(th.uiScale)
  local hot = self.splitHover or self.capture == "splitter"
  local c = hot and th.splitterHot or th.splitter
  local cx = r.x + floor(r.w / 2)
  rectOp(ops, cx, r.y, s, r.h, c)
  local cy = r.y + floor(r.h / 2)
  for i = -1, 1 do rectOp(ops, cx - s, cy + i * 4 * s - s, 2 * s, 2 * s, c) end
  -- the bar that resizes the dope sheet
  local hr = self.regions.hsplit
  if hr and hr.w > 0 then
    local hh = self.hsplitHover or self.capture == "hsplit"
    local hc = hh and th.splitterHot or th.splitter
    local my = hr.y + floor(hr.h / 2)
    rectOp(ops, hr.x, my, hr.w, s, hc)
    local mx = hr.x + floor(hr.w / 2)
    for i = -1, 1 do rectOp(ops, mx + i * 4 * s - s, my - s, 2 * s, 2 * s, hc) end
  end
end

local function indexOf(list, v)
  for i, x in ipairs(list) do if x == v then return i end end
  return nil
end

--- What a notice is about: it goes away once any of this changes.
function V:noticeKey()
  local E = self.E
  return table.concat({ E.rev, E.mode, E.frame, E.tool, self.tab }, "|")
end

function V:setNotice(msg, warn)
  self.notice, self.noticeWarn = msg, warn == true
  self._noticeKey = self:noticeKey()
end

function V:paint()
  self.painted = self.painted + 1
  -- the pose thumbnails no longer asked for go, but only while their tab
  -- is the one painted (coming back to it finds them ready)
  if self.thumbs and M.TABS[self.tab] == "Poses" then self.thumbs.sweep() end
  if self.notice and self._noticeKey and self._noticeKey ~= self:noticeKey() then
    self.notice, self.noticeWarn = nil, false
  end
  local th = self.theme
  local R = self.regions
  local E = self.E
  local ops = {}
  rectOp(ops, 0, 0, self.w, self.h, th.bg)
  if self.imp then
    append(ops, self.UI.widgets.tabs.paint(self.tabSpec, R.tabs, 1, {}, self.ctx))
    self.Import.paintViewport(self, ops)
    local t = R.mode
    ops[#ops + 1] = { "text", t.x, t.y + floor((t.h - self.text:height()) / 2), th.text[1],
      th.text[2], th.text[3], self.text:ellipsize("Import 3D animation: " .. self.imp.name,
        R.mode.w + R.tools.w) }
    append(ops, self.playPanel:paint())
    self.Import.paintTimeline(self, ops)
  else
    append(ops, self.UI.widgets.tabs.paint(self.tabSpec, R.tabs, self.tab, {}, self.ctx))
    self:paintViewport(ops)
    append(ops, self.UI.widgets.tabs.paint(self.modeSpec, R.mode,
      indexOf(self.Editor.MODES, E.mode), {}, self.ctx))
    self.toolSpec.items = E:tools()
    append(ops, self.UI.widgets.tabs.paint(self.toolSpec, R.tools,
      indexOf(self.toolSpec.items, E.tool), {}, self.ctx))
    append(ops, self.playPanel:paint())
    self:paintTimeline(ops)
    if self.Dope then self.Dope.paint(self, ops) end
  end
  local st = R.status
  if self.notice then
    local c = self.noticeWarn and th.warning or th.textDim
    ops[#ops + 1] = { "text", st.x, st.y, c[1], c[2], c[3],
      self.text:ellipsize(tostring(self.notice), st.w) }
  else
    ops[#ops + 1] = { "text", st.x, st.y, th.textDim[1], th.textDim[2], th.textDim[3],
      self.text:ellipsize(self:statusText(), st.w) }
  end
  append(ops, self.footer:paint())
  self:paintSplitter(ops)
  -- the panel last: its dropdowns and colour picker float over the rest
  append(ops, self.panel:paint())
  return ops
end

---------------------------------------
-- Input --
---------------------------------------

local function inside(r, x, y)
  return r and x >= r.x and y >= r.y and x < r.x + r.w and y < r.y + r.h
end

function V:undo()
  if self.E:undo() then self.notice = nil return true end
  return false
end

function V:redo()
  if self.E:redo() then self.notice = nil return true end
  return false
end

--- Keeps the Groups tab on the group of the bone just selected.
function V:followSelection()
  local b = self.E:selected()
  if b and b.id ~= self._lastSel and b.part ~= 0 then self.partSel = b.part end
  self._lastSel = b and b.id or nil
end

function V:pickTol()
  return M.PICK_PX * self.theme.uiScale / self:z()
end

function V:mouseDown(x, y, button, mods)
  mods = mods or {}
  if self.help then
    -- any click closes the shortcut overlay
    self.help = false
    return true
  end
  local R = self.regions
  local p = self.panel
  if p.dropdown or p.picker or p.entry then
    self.capture = "panel"
    return p:mouseDown(x, y, button, mods) or true
  end
  if inside(R.splitter, x, y) and button == "left" then
    self.capture = "splitter"
    self.splitDrag = { x0 = x, w0 = self.leftW or M.PANEL_MIN }
    return true
  end
  if inside(R.hsplit, x, y) and button == "left" then
    self.capture = "hsplit"
    self.hsplitDrag = { y0 = y, h0 = floor(R.sheet.h / self.theme.uiScale) }
    return true
  end
  if self.imp then
    if inside(R.strip, x, y) and button == "left" then
      local f = self.Import.frameAt(self, x)
      self.imp.frame = f
      -- Shift+click marks or unmarks the frame (Import: Marked frames)
      if mods.shift then
        self.notice = nil
        if self.Import.previewing(self.imp) then
          self:setNotice("Turn Preview result off to mark frames", true)
          return true
        end
        return self.Import.toggleMark(self.imp, f)
      end
      self.capture = "impStrip"
      return true
    end
    if inside(R.play, x, y) then
      self.capture = "play"
      return self.playPanel:mouseDown(x, y, button, mods)
    end
    if inside(R.viewport, x, y) then
      if self.Import.down(self, x, y, button) then self.capture = "impView" end
      return true
    end
    if inside(R.footer, x, y) then
      self.capture = "footer"
      return self.footer:mouseDown(x, y, button, mods)
    end
    if inside(R.panel, x, y) then
      self.capture = "panel"
      return p:mouseDown(x, y, button, mods)
    end
    return false
  end
  if inside(R.sheet, x, y) then
    if self.Dope.down(self, x, y, button, mods) then
      self.capture = "sheet"
      return true
    end
    return false
  end
  if inside(R.tabs, x, y) then
    local act = self.UI.widgets.tabs.down(self.tabSpec, R.tabs, x, y, self.tab, self.ctx)
    if act and act.type == "select" then return self:setTab(act.index) end
    return true
  end
  if inside(R.mode, x, y) then
    -- the CURRENT index: tabs.down only selects a tab that is not it
    local cur = indexOf(self.Editor.MODES, self.E.mode)
    local act = self.UI.widgets.tabs.down(self.modeSpec, R.mode, x, y, cur, self.ctx)
    if act and act.type == "select" then return self:setMode(self.Editor.MODES[act.index]) end
    return true
  end
  if inside(R.tools, x, y) then
    self.toolSpec.items = self.E:tools()
    local cur = indexOf(self.toolSpec.items, self.E.tool)
    local act = self.UI.widgets.tabs.down(self.toolSpec, R.tools, x, y, cur, self.ctx)
    if act and act.type == "select" then
      return self.E:setTool(self.toolSpec.items[act.index])
    end
    return true
  end
  if inside(R.strip, x, y) and button == "left" then
    local E = self.E
    if E.mode ~= "Pose" then self:setMode("Pose") end
    local f = self:frameAt(x)
    self.capture = "strip"
    local keyed = self.Anim.hasKey(E.doc.anim, f, E:targets())
    self.stripDrag = { from = f, to = f, keys = keyed, moving = false }
    E:setFrame(f)
    return true
  end
  if inside(R.play, x, y) then
    self.capture = "play"
    return self.playPanel:mouseDown(x, y, button, mods)
  end
  if inside(R.viewport, x, y) then
    if button ~= "left" or self.E.tool == "Pan" then
      self.capture = "pan"
      self.panDrag = { x = x, y = y, px = self.cam.px, py = self.cam.py }
      return true
    end
    self.capture = "viewport"
    self.notice, self.noticeWarn = nil, false
    local rx, ry = self:toRig(x, y)
    local changed = self.E:down(rx, ry, self:pickTol(), mods)
    self:followSelection()
    return changed or true
  end
  if inside(R.footer, x, y) then
    self.capture = "footer"
    return self.footer:mouseDown(x, y, button, mods)
  end
  if inside(R.panel, x, y) then
    self.capture = "panel"
    return p:mouseDown(x, y, button, mods)
  end
  return false
end

function V:mouseMove(x, y, mods)
  local R = self.regions
  local cap = self.capture
  if cap == "splitter" then
    local d = self.splitDrag
    local lo, hi = self:splitBounds(self.w)
    local lw = floor(d.w0 + (x - d.x0) / self.theme.uiScale)
    if lw < lo then lw = lo elseif lw > hi then lw = hi end
    if lw == self.leftW then return false end
    self.leftW = lw
    self:layout(self.w, self.h)
    return true
  end
  if cap == "hsplit" then
    local d = self.hsplitDrag
    local h = max(0, floor(d.h0 + (d.y0 - y) / self.theme.uiScale))
    if h == self.sheetH then return false end
    d.moved = true
    self.sheetH = h
    self:layout(self.w, self.h)
    return true
  end
  if cap == "sheet" then return self.Dope.move(self, x, y) end
  if cap == "impView" then return self.Import.move(self, x, y) end
  if cap == "impStrip" then
    local f = self.Import.frameAt(self, x)
    if f == self.imp.frame then return false end
    self.imp.frame = f
    return true
  end
  if cap == "pan" then
    local d = self.panDrag
    self.cam.px = d.px + (x - d.x)
    self.cam.py = d.py + (y - d.y)
    return true
  end
  if cap == "strip" then
    local d = self.stripDrag
    local f = self:frameAt(x)
    if d.keys then
      -- a marked frame: dragging moves its keys (shown as a ghost cell)
      local moving = f ~= d.from
      local changed = d.to ~= f or d.moving ~= moving
      d.to, d.moving = f, moving
      return changed
    end
    return self.E:setFrame(f)
  end
  if cap == "viewport" then
    local rx, ry = self:toRig(x, y)
    return self.E:move(rx, ry)
  end
  if cap == "panel" then return self.panel:mouseMove(x, y, mods) or false end
  if cap == "footer" then return self.footer:mouseMove(x, y, mods) or false end
  if cap == "play" then return self.playPanel:mouseMove(x, y, mods) or false end
  local over = inside(R.splitter, x, y) and true or false
  local changed = over ~= self.splitHover
  self.splitHover = over
  local hover = inside(R.hsplit, x, y) and true or false
  if hover ~= (self.hsplitHover or false) then changed = true end
  self.hsplitHover = hover
  local target
  if inside(R.panel, x, y) then target = self.panel
  elseif inside(R.footer, x, y) then target = self.footer
  elseif inside(R.play, x, y) then target = self.playPanel end
  -- every OTHER panel loses its hover, even when one move jumps between them
  for _, pnl in ipairs({ self.panel, self.footer, self.playPanel }) do
    if pnl ~= target and pnl.hover then pnl.hover = nil changed = true end
  end
  if target then return target:mouseMove(x, y) or changed end
  return changed
end

function V:mouseUp(x, y, button, mods)
  local cap = self.capture
  self.capture = nil
  if cap == "splitter" or cap == "pan" then
    if cap == "splitter" and self.onSplit and self.leftW then self.onSplit(self.leftW) end
    self.splitDrag, self.panDrag = nil, nil
    return true
  end
  if cap == "hsplit" then
    local d = self.hsplitDrag
    self.hsplitDrag = nil
    -- what the drag asked for, not what this window size could give: a
    -- small window must not shrink the saved height for good
    if d and d.moved and self.onSheet then self.onSheet(self.sheetH) end
    return true
  end
  if cap == "sheet" then return self.Dope.up(self, x, y) end
  if cap == "impView" then return self.Import.up(self) end
  if cap == "impStrip" then return true end
  if cap == "strip" then
    local d = self.stripDrag
    self.stripDrag = nil
    if d and d.keys and d.moving then return self.E:moveKeys(d.from, d.to) end
    return true
  end
  if cap == "viewport" then
    local rx, ry = self:toRig(x, y)
    local changed = self.E:up(rx, ry)
    self:followSelection()
    return changed
  end
  if cap == "panel" then return self.panel:mouseUp(x, y, button, mods) or false end
  if cap == "footer" then return self.footer:mouseUp(x, y, button, mods) or false end
  if cap == "play" then return self.playPanel:mouseUp(x, y, button, mods) or false end
  return false
end

function V:wheel(x, y, dy, mods)
  mods = mods or {}
  -- a horizontal trackpad scroll arrives as dy = 0: nothing to do
  if not dy or dy == 0 then return false end
  if self.help then return true end -- nothing moves behind the overlay
  local R = self.regions
  local p = self.panel
  if p.dropdown or p.picker then return p:wheel(x, y, dy) end
  if self.imp then
    if inside(R.viewport, x, y) then return self.Import.wheel(self, x, y, dy) end
    if inside(R.strip, x, y) then
      local n = self.Import.shown(self.imp).frames
      self.imp.frame = (self.imp.frame - 1 + (dy > 0 and 1 or -1)) % n + 1
      return true
    end
    if inside(R.panel, x, y) then return p:wheel(x, y, dy) end
    return false
  end
  if inside(R.viewport, x, y) then
    local step = (mods.ctrl or mods.shift) and 2 or 1.15
    local z = self.cam.zoom
    if dy < 0 then z = z * step else z = z / step end
    return self:zoomAt(z, x, y)
  end
  if inside(R.strip, x, y) then
    if self.E.mode ~= "Pose" then self:setMode("Pose") end
    return self.E:stepFrame(dy > 0 and 1 or -1)
  end
  if inside(R.sheet, x, y) then return self.Dope.wheel(self, dy) end
  if inside(R.tabs, x, y) then
    local act = self.UI.widgets.tabs.wheel(self.tabSpec, R.tabs, dy, self.tab, self.ctx)
    if act then return self:setTab(act.index) end
    return true
  end
  if inside(R.panel, x, y) then return p:wheel(x, y, dy) end
  return false
end

M.TOOL_KEYS = { v = "Select", b = "Bone", r = "Radius", g = "Bulge", m = "Ref", h = "Pan" }
M.POSE_KEYS = { p = "Pose", i = "IK", m = "Ref", h = "Pan" }

--- The shortcut list of the help overlay (? or F1): { keys, what }.
M.SHORTCUTS = {
  { "V B R G H", "Select, Bone, Radius, Bulge, Pan (Rest)" },
  { "P I H", "Pose, IK, Pan (Pose)" },
  { "M", "The Ref tool: move the reference (Shift scale, Alt turn)" },
  { "Shift+click", "Add a bone to the selection, or take it out" },
  { "Alt+click", "Move only the root (pink ring) there; Alt+drag it" },
  { "K  /  Shift+K", "Key the selected bones / every bone" },
  { "Delete  /  Shift+Delete", "Delete keys of the selected bone / every bone" },
  { "Left  Right", "Previous / next frame" },
  { "Alt+Left  Alt+Right", "Previous / next key" },
  { "Home  End", "First / last frame" },
  { "Insert  /  Ctrl+Delete", "Insert a frame / delete the frame" },
  { "Ctrl+C  Ctrl+V", "Copy / paste the pose (the picked keys when there are)" },
  { "Dope sheet", "Click a key to pick it, Shift adds, drag to move, Alt+drag to scale" },
  { "Ctrl+Shift+V", "Paste the pose mirrored" },
  { "Alt+R  Alt+G", "Reset rotation / offset to rest" },
  { "Space", "Play / stop" },
  { "O", "Onion skin on / off" },
  { "[  ]", "Select the parent / a child bone" },
  { "F", "Frame the selection" },
  { "F2", "Rename the bone" },
  { "Ctrl+N", "New rig" },
  { "Ctrl+E", "Export sprite" },
  { "Ctrl+Z  Ctrl+Y", "Undo / redo (also Ctrl+Shift+Z)" },
  { "Ctrl+Tab", "Next tab (Shift: previous)" },
  { "0  1  2  3", "Fit / zoom 100, 200, 300 %" },
  { "Esc", "Cancel the drag, clear the selection" },
  { "?  F1", "This list" },
}

--- Pose clipboard and frame commands from the keyboard or a button: they
--- enter Pose like the timeline does and say what happened.
function V:copyPose()
  if #self.E.doc.bones == 0 then self:setNotice("Nothing to copy: the rig is empty", true) return true end
  self:setMode("Pose")
  local n = self.E:copyPose()
  self:setNotice(string.format("Pose copied (%d %s)", n, n == 1 and "bone" or "bones"))
  return true
end

function V:copyKeys()
  local n = self.E:copyKeys()
  self:setNotice(string.format("%d %s copied: Ctrl+V pastes them from the current frame", n,
    n == 1 and "key" or "keys"))
  return true
end

function V:pasteKeys()
  self:setMode("Pose")
  if not self.E:pasteKeys() then
    self:setNotice("Nothing fits from this frame on", true)
  end
  return true
end

function V:pastePose(mirrored)
  if not self.E.clip then self:setNotice("Copy a pose first (Ctrl+C)", true) return true end
  self:setMode("Pose")
  local n = self.E:pastePose(mirrored)
  self:setNotice(string.format("Pose pasted%s (%d %s)", mirrored and " mirrored" or "", n,
    n == 1 and "bone" or "bones"))
  return true
end

function V:resetPose(which)
  if #self.E.doc.bones == 0 then self:setNotice("Nothing to reset: the rig is empty", true) return true end
  self:setMode("Pose")
  local n = self.E:resetPose(which)
  if n == 0 then
    self:setNotice("Only a root or a loose bone has an offset", true)
  else
    local what = which == "rot" and "Rotation" or which == "offset" and "Offset" or "Pose"
    self:setNotice(string.format("%s reset (%d %s)", what, n, n == 1 and "bone" or "bones"))
  end
  return true
end

--- A frame command from the keyboard: same conditions as its button, and
--- a notice when it cannot run.
function V:frameCommand(fn, refusal)
  self:setMode("Pose")
  if not fn(self.E) then self:setNotice(refusal, true) end
  return true
end

function V:newRig()
  local E = self.E
  local function apply()
    if self.playing then self:togglePlay() end
    self:setMode("Rest")
    E:newRig()
    self.partSel = nil
    self:fit()
    self:setNotice("New rig")
    return true
  end
  if #E.doc.bones > 0 and self.confirm then
    self.confirm("Start a new rig? The current one goes (Ctrl+Z brings it back).", apply)
    return true
  end
  return apply()
end

function V:symmetrize()
  local n = self.E:symmetrize()
  if n == 0 then
    self:setNotice("No pairs found: name them _L / _R or place them mirrored", true)
  else
    self:setNotice(string.format("%d %s paired, mirror on", n, n == 1 and "pair" or "pairs"))
  end
  return true
end

--- Zooms onto the selected bone (with its skin), or the whole rig.
function V:frameSelection()
  local E = self.E
  local world = E:world()
  local x0, y0, x1, y1 = math.huge, math.huge, -math.huge, -math.huge
  local sel = E:selected()
  for _, b in ipairs(E.doc.bones) do
    if not sel or b.id == sel.id then
      local w = world[b.id]
      local r = math.max(b.rHead, b.rTail) + 1
      x0, y0 = min(x0, w.hx - r, w.tx - r), min(y0, w.hy - r, w.ty - r)
      x1, y1 = max(x1, w.hx + r, w.tx + r), max(y1, w.hy + r, w.ty + r)
    end
  end
  if x0 == math.huge then return self:fit() end
  local r = self.regions.viewport
  local m = M.FIT_MARGIN * self.theme.uiScale
  local z = min((r.w - 2 * m) / (x1 - x0), (r.h - 2 * m) / (y1 - y0))
  z = max(M.ZOOM_MIN, min(M.ZOOM_MAX, z))
  if z >= 1 then z = floor(z) end
  self.cam.zoom = z
  local zz = self:z()
  self.cam.px = floor(r.w / 2 - (x0 + x1) / 2 * zz)
  self.cam.py = floor(r.h / 2 - (y0 + y1) / 2 * zz)
  return true
end

--- F2: the Bone tab with the Name entry open.
function V:rename()
  if not self.E:selected() then self:setNotice("Select a bone first", true) return true end
  if self.tab ~= 2 then self:setTab(2) end
  self.panel:layout(self.regions.panel)
  for _, row in ipairs(self.panel.rows) do
    for _, c in ipairs(row.cells) do
      if c.spec.id == "name" then self.panel:openEntry(c) return true end
    end
  end
  return true
end

local function lowerOf(key)
  if type(key) == "string" and #key == 1 then return key:lower() end
  return key
end

function V:key(key, mods)
  mods = mods or {}
  local p = self.panel
  if p.entry or p.dropdown or p.picker then return p:key(key, mods) end
  local E = self.E
  local k = lowerOf(key)
  if self.help then
    -- the overlay eats keys: ? / F1 / Esc close it
    if key == "?" or key == "F1" or key == "Escape" then self.help = false end
    return true
  end
  if self.imp then
    -- the import mode: Esc leaves it, Enter imports, Space plays
    if key == "Escape" then return self:endImport(false) end
    if key == "Enter" then return self:endImport(true) end
    if key == "Space" then return self:togglePlay() end
    if k == "m" and not mods.ctrl then
      self.notice = nil
      if self.Import.previewing(self.imp) then
        self:setNotice("Turn Preview result off to mark frames", true)
        return true
      end
      return self.Import.toggleMark(self.imp, self.imp.frame)
    end
    if key == "Left" or key == "Right" then
      local n = self.Import.shown(self.imp).frames
      self.imp.frame = (self.imp.frame - 1 + (key == "Right" and 1 or -1)) % n + 1
      return true
    end
    return false
  end
  if key == "?" or key == "F1" then self.help = true return true end
  if mods.ctrl then
    if k == "z" then
      if mods.shift then return self:redo() end
      return self:undo()
    end
    if k == "y" then return self:redo() end
    if k == "c" then
      if E:keySelCount() > 0 then return self:copyKeys() end
      return self:copyPose()
    end
    if k == "v" then
      if not mods.shift and E.lastCopy == "keys" then return self:pasteKeys() end
      return self:pastePose(mods.shift == true)
    end
    if k == "n" then return self:newRig() end
    if k == "e" then return self:export() end
    if key == "Delete" or key == "Backspace" then
      return self:frameCommand(function(e) return e:deleteFrame() end, "Only one frame left")
    end
    if key == "Tab" then
      local n = #M.TABS
      return self:setTab((self.tab - 1 + (mods.shift and -1 or 1)) % n + 1)
    end
    return false
  end
  if mods.alt then
    if key == "Left" or key == "Right" then
      return self:frameCommand(function(e) return e:jumpKey(key == "Right" and 1 or -1) end,
        "No key that way")
    end
    if k == "r" then return self:resetPose("rot") end
    if k == "g" then return self:resetPose("offset") end
    return false
  end
  if key == "Delete" or key == "Backspace" then
    -- always consumed: an unhandled Delete would reach Aseprite and clear
    -- the active cel
    if mods.shift then
      return self:frameCommand(function(e) return e:deleteKeysAll() end, "No keys on this frame")
    end
    if not E:deleteSelection() then
      self:setNotice(E.mode == "Pose" and "No keys here (pick keys in the dope sheet, or go to a keyed frame)"
        or "Select a bone or a bulge handle first", true)
    end
    self:followSelection()
    return true
  end
  if key == "Escape" then
    if self.playing then return self:togglePlay() end
    if self.capture == "sheet" then
      -- a dope sheet drag: drop it, nothing moved yet
      self.sheetDrag, self.capture = nil, nil
      return true
    end
    if E.drag then
      E:cancelGesture()
      self.capture = nil
      return true
    end
    if E:clearKeySel() then return true end
    if E.sel then E.sel, E.selBulge = nil, nil return true end
    return false
  end
  if key == "Space" then return self:togglePlay() end
  if key == "0" then return self:fit() end
  if key == "1" or key == "2" or key == "3" then
    local r = self.regions.viewport
    return self:zoomAt(tonumber(key), r.x + floor(r.w / 2), r.y + floor(r.h / 2))
  end
  if key == "Left" or key == "Right" or k == "k" or key == "Home" or key == "End"
    or key == "Insert" then
    -- timeline keys act like the timeline: from Rest they enter Pose
    self:setMode("Pose")
    if k == "k" then
      if mods.shift then return E:addKeyAll() or true end
      return E:addKey() or true
    end
    if key == "Home" then return E:setFrame(1) or true end
    if key == "End" then return E:setFrame(E.doc.anim.frames) or true end
    if key == "Insert" then
      if not E:insertFrame() then self:setNotice("Frame limit reached", true) end
      return true
    end
    return E:stepFrame(key == "Right" and 1 or -1) or true
  end
  -- [ and ] are always ours (Aseprite would resize its brush)
  if key == "[" or key == "]" then
    if not E:selectRelative(key == "[" and "parent" or "child") then
      self:setNotice(key == "[" and "No parent bone" or "No child bone", true)
    end
    self:followSelection()
    return true
  end
  if key == "F2" then return self:rename() end
  -- plain letters only from here: Shift+letter is not a tool switch
  if mods.shift then return p:key(key, mods) or false end
  if k == "o" then
    self.opts.onion = not self.opts.onion
    return true
  end
  if k == "f" then return self:frameSelection() end
  if E.mode == "Pose" then
    local tool = M.POSE_KEYS[k]
    if tool then return E:setTool(tool) end
  else
    local tool = M.TOOL_KEYS[k]
    if tool then return E:setTool(tool) end
  end
  return p:key(key, mods) or false
end

return M
