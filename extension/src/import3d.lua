-- import3d.lua — the "Import 3D" mode of the Bonebase window. Pure Lua: a
-- SESSION (the parsed file and every choice made on it) plus the panel,
-- viewport, timeline and input the view hands over while it is open (the
-- way dopesheet.lua owns the sheet).
--
-- A session: { skel, name, clip (index), view (preset or "Custom"), yaw,
-- pitch, roll, zUp, target ("This rig" | "New rig"), frames, loop,
-- inPlace, foreshorten, depthOrder, motion (%), map (bone -> joint),
-- doc (what the preview animates: the editor's rig, or a scratch rig for
-- "New rig"), anim (the preview clip, a key on every frame), frame,
-- playing, orbit = { zoom, px, py }, rev, marks (frame -> true), useMarks,
-- length }. `rebuild` redoes doc / anim after any change; `apply` builds
-- the real clip (keys simplified) through the editor in one undo step.
-- Nothing of the 3D file is kept in the sprite.
--
-- MARKED FRAMES: with "Import: Marked frames" only the frames marked on
-- the timeline (Shift+click, M) become keys, spread evenly over a clip
-- `length` frames long (never fewer than the marks), eased in between;
-- the clip lasts as long as the file's (remap).
--
-- The viewport splits in two: the skeleton in 3D on the left (drag orbits,
-- right drag pans, wheel zooms), the rig it drives on the right.

local M = {}

M.R, M.Bvh, M.Gltf, M.Rig, M.Anim, M.Ik, M.Skin = nil, nil, nil, nil, nil, nil, nil

function M.bind(deps)
  for k, v in pairs(deps) do M[k] = v end
  return M
end

local floor, max, min, abs = math.floor, math.max, math.min, math.abs

M.TARGETS = { "This rig", "New rig" }
M.USE = { "All frames", "Marked frames" }
M.VIEWS = { "Side", "Front", "Back", "3/4", "Custom" }
M.ORBIT_DEG = 0.5      -- degrees of orbit per screen px
M.NONE = "-"           -- the map combo's "no joint"

---------------------------------------
-- Session --
---------------------------------------

local function lowerExt(name)
  return (tostring(name or ""):match("%.([%w]+)$") or ""):lower()
end

--- A session from a file's bytes. `readFile(uri)` loads a .gltf's
--- external buffers. Returns the session, or nil and why.
function M.open(bytes, name, readFile, doc)
  local ext = lowerExt(name)
  local skel, err
  if ext == "bvh" then
    skel, err = M.Bvh.parse(bytes)
  elseif ext == "glb" or ext == "gltf" then
    skel, err = M.Gltf.parse(bytes, { readFile = readFile })
  else
    return nil, "Pick a .glb, .gltf or .bvh file"
  end
  if not skel then return nil, "Could not read the file: " .. tostring(err) end
  if #skel.clips == 0 then return nil, "The file has no animation" end
  local R = M.R
  local s = {
    skel = skel, name = tostring(name or ""):match("([^/\\]+)$") or "",
    clip = 1, view = "Side", yaw = 90, pitch = 0, roll = 0, zUp = R.isZUp(skel),
    target = #doc.bones == 0 and "New rig" or "This rig",
    loop = true, inPlace = true, foreshorten = true, depthOrder = true, motion = 100,
    parts = {}, thick = 100, follow = true,
    depthFrom = nil,          -- Depth order's 0 (a joint); nil = the hips  -- New rig: part (or false: left out) per joint, skin %
    map = {}, frame = 1, playing = false, orbit = { zoom = 1, px = 0, py = 0 }, rev = 0,
    marks = {}, moments = {}, useMarks = false, length = nil,
  }
  s.frames = R.defaultFrames(skel.clips[1], s.loop)
  if s.target == "This rig" then s.map = R.autoMap(doc, skel) end
  M.rebuild(s, doc)
  return s
end

function M.camera(s)
  return M.R.camera(s.yaw, s.pitch, s.roll, s.zUp)
end

function M.rigOptions(s)
  return { parts = s.parts, thick = s.thick / 100 }
end

function M.options(s)
  return { frames = s.frames, loop = s.loop, inPlace = s.inPlace, foreshorten = s.foreshorten,
    depthOrder = s.depthOrder, motion = s.motion / 100, followJoints = s.follow,
    rootFrom = s.target == "New rig" and "first" or "rest", depthFrom = s.depthFrom }
end

--- Exporters' own names that say nothing ("mixamo.com", Blender's
--- "ArmatureAction", "Take 001"): the clip takes the file's name instead.
local function generic(n)
  local l = n:lower()
  return l == "" or l == "mixamo.com" or l:match("^armature") ~= nil or l:match("^take ?%d*$") ~= nil
    or l:match("^animation ?%d*$") ~= nil or l:match("^action ?%d*$") ~= nil
end

function M.clipName(s)
  local c = s.skel.clips[s.clip]
  local n = c and c.name or ""
  if generic(n) then n = s.name:gsub("%.[%w]+$", "") end
  if n == "" then n = "Imported" end
  return n
end

--- The preview after any change: a scratch rig for "New rig", the
--- clip with a key on every frame.
function M.rebuild(s, doc)
  local R, Rig = M.R, M.Rig
  local cam = M.camera(s)
  if s.target == "New rig" then
    s.doc = Rig.placeLike(Rig.new(Rig.bounds(doc).w, Rig.bounds(doc).h), doc)
    s.map = R.buildRig(s.doc, s.skel, s.clip, cam, M.rigOptions(s))
  else
    s.doc = doc
    for id in pairs(s.map) do
      if not Rig.bone(doc, id) then s.map[id] = nil end
    end
  end
  s.ctx = R.prepare(s.doc, s.skel, s.clip, s.map, cam, M.options(s))
  s.anim = R.buildClip(s.ctx, M.clipName(s), false)
  s.frame = max(1, min(s.anim.frames, s.frame))
  M.keepMarks(s)
  s.rev = s.rev + 1
  s._result = nil
  s._buf = nil
  return s
end

---------------------------------------
-- Marked frames --
---------------------------------------

--- Where frame `f` of a preview `n` frames long sits in the file's clip,
--- 0..1 (the same sampling as retarget.lua's ctx.time).
local function timeOf(f, n, loop)
  if n <= 1 then return 0 end
  if loop then return (f - 1) / n end
  return (f - 1) / (n - 1)
end

local function frameOf(u, n, loop)
  if n <= 1 then return 1 end
  if loop then return floor(u * n + 0.5) % n + 1 end
  return floor(u * (n - 1) + 0.5) + 1
end

--- After a rebuild: marks are MOMENTS of the file (s.moments, 0..1 of
--- its length), shown on the frame that samples each one now (s.marks).
--- A new sample count or Loop moves them there and back without drift;
--- another animation of the file has other moments, so they go.
function M.keepMarks(s)
  s.moments = s.moments or {}
  if s._markClip and s._markClip ~= s.clip then s.moments = {} end
  s._markClip = s.clip
  local n = s.anim.frames
  s.marks = {}
  for u in pairs(s.moments) do s.marks[frameOf(u, n, s.loop)] = true end
end

--- The marked frames, sorted.
function M.markList(s)
  local out = {}
  for f in pairs(s.marks) do out[#out + 1] = f end
  table.sort(out)
  return out
end

function M.toggleMark(s, f)
  -- the result on screen has its own frames: marks go on the file's
  if M.previewing(s) then return false end
  f = floor(tonumber(f) or 0)
  local n = s.anim.frames
  if f < 1 or f > n then return false end
  s.moments = s.moments or {}
  if s.marks[f] then
    -- every moment this frame shows goes
    for u in pairs(s.moments) do
      if frameOf(u, n, s.loop) == f then s.moments[u] = nil end
    end
    s.marks[f] = nil
  else
    s.moments[timeOf(f, n, s.loop)] = true
    s.marks[f] = true
  end
  s._result = nil
  return true
end

function M.clearMarks(s)
  s.moments, s.marks, s._result = {}, {}, nil
end

--- True while the preview shows the clip Import will build from the marks.
function M.previewing(s)
  return s.useMarks and s.previewResult and next(s.marks) ~= nil
end

--- What the preview shows: the file's clip, or (Preview result) the clip
--- of the marks, rebuilt when the marks, the length or the file change.
function M.shown(s)
  if not M.previewing(s) then return s.anim end
  local marks = M.markList(s)
  local len = M.length(s)
  local key = s.rev .. "|" .. table.concat(marks, ",") .. "|" .. len .. "|" .. tostring(s.loop)
  if s._resultKey ~= key or not s._result then
    s._result, s._resultKey = M.remap(s.anim, marks, len, s.loop), key
  end
  return s._result
end

--- How long the clip of the marked frames is: what was asked, at least
--- one frame per mark, at most Anim.MAX_FRAMES.
function M.length(s)
  local n = #M.markList(s)
  local want = floor(tonumber(s.length) or s.anim.frames)
  return max(max(1, n), min(M.Anim.MAX_FRAMES, want))
end

--- Where `m` keys land in a clip `len` frames long: evenly, the first on
--- frame 1. A loop spaces them len / m apart (the last one eases back
--- into the first); otherwise the last one sits on the last frame.
function M.markPositions(m, len, loop)
  local out = {}
  if m <= 0 then return out end
  for i = 1, m do
    local p
    if loop or m == 1 then
      p = 1 + floor((i - 1) * len / m + 0.5)
    else
      p = 1 + floor((i - 1) * (len - 1) / (m - 1) + 0.5)
    end
    out[i] = max(1, min(len, p))
  end
  return out
end

--- A clip `len` frames long keyed only with the poses `anim` shows at
--- `marks` (sorted), placed by markPositions and eased in between. It
--- lasts as long as `anim` did.
function M.remap(anim, marks, len, loop)
  local Anim = M.Anim
  local out = M.Rig.newAnim(anim.name)
  out.loop, out.pingpong, out.ease = loop, anim.pingpong, "Ease"
  out.frames = len
  local total = loop and anim.frames * anim.frameMs or max(1, anim.frames - 1) * anim.frameMs
  out.frameMs = max(10, floor(total / (loop and len or max(1, len - 1)) + 0.5))
  local pos = M.markPositions(#marks, len, loop)
  local TOL = M.R and M.R.TOL or {}
  for _, t in ipairs(anim.tracks) do
    -- a track that never leaves its rest value at the marks says nothing
    -- (the full import drops those too); IK and z tracks always stay
    local neutral, tol = Anim.neutral(t.ch), TOL[t.ch] or 0
    local keep = t.ch == "tx" or t.ch == "ty" or t.ch == "px" or t.ch == "py" or t.ch == "z"
    local vals = {}
    for i, f in ipairs(marks) do
      vals[i] = Anim.value(anim, t, f)
      if abs(vals[i] - neutral) > tol then keep = true end
    end
    if keep then
      for i in ipairs(marks) do Anim.setKey(out, t.bone, t.ch, pos[i], vals[i]) end
    end
  end
  return out
end

--- Builds the real clip through the editor (one undo step). Returns ok
--- and a line for the status.
function M.apply(s, E)
  local R = M.R
  local cam = M.camera(s)
  local name = M.clipName(s)
  local newRig = s.target == "New rig"
  local marks = s.useMarks and M.markList(s) or nil
  if marks and #marks == 0 then
    return false, "Mark the frames to import first (Shift+click the timeline, or M)"
  end
  local len = marks and M.length(s)
  local a, why = E:importClip(function(doc)
    local map = s.map
    if newRig then
      -- the file's rig takes the place of the one there (same undo step)
      doc.bones, doc.parts, doc.ik = {}, {}, {}
      -- the old rig's poses name bones that are gone
      doc.poses, doc.nextPose = {}, 1
      -- and it is no longer the rig its library entry holds
      doc.libName, doc.gen = nil, (doc.gen or 0) + 1
      doc.rootPos = nil   -- the old rig's handle place means nothing here
      doc.clips = { M.Rig.newAnim("Animation") }
      M.Rig.setClip(doc, 1)
      map = R.buildRig(doc, s.skel, s.clip, cam, M.rigOptions(s))
    end
    local ctx = R.prepare(doc, s.skel, s.clip, map, cam, M.options(s))
    if marks then return M.remap(R.buildClip(ctx, name, false), marks, len, s.loop) end
    return R.buildClip(ctx, name, true)
  end)
  if not a then return false, why or "Nothing imported" end
  local n = 0
  for _ in pairs(s.map) do n = n + 1 end
  return true, string.format("Imported \"%s\": %d frames, %d bones%s", a.name, a.frames, n,
    newRig and " (new rig)" or "")
end

--- The joints a bone can take, by name, in file order ("-" first).
function M.jointOptions(s)
  if s._jopts then return s._jopts end
  local list = { M.NONE }
  for _, j in ipairs(s.skel.joints) do
    if not j.isEnd then list[#list + 1] = j.name end
  end
  s._jopts = list
  return list
end

local function jointIndex(s, name)
  for i, j in ipairs(s.skel.joints) do
    if j.name == name and not j.isEnd then return i end
  end
  return nil
end

M.AUTO = "Hips (auto)"

--- The joints Depth order can measure from: the automatic one first.
function M.depthOptions(s)
  local list = { M.AUTO }
  for i, name in ipairs(M.jointOptions(s)) do
    if i > 1 then list[#list + 1] = name end
  end
  return list
end

--- What a joint of the new rig can be: a part (the plan's, the usual
--- ones, any typed before) or left out.
function M.partOptions(s)
  local list, seen = {}, {}
  local function add(n)
    if n and not seen[n] then list[#list + 1], seen[n] = n, true end
  end
  for _, e in ipairs(M.R.rigPlan(s.skel)) do add(e.part) end
  for _, n in ipairs(M.R.PART_NAMES) do add(n) end
  add(M.R.LEAVE_OUT)
  return list
end

--- A joint's name as a bone gets it (no "mixamorig:").
local function shortName(name)
  return (tostring(name):gsub("^.*:", ""))
end

--- One playback step. True when the frame moved.
function M.tick(s)
  if not s.playing then return false end
  local n = M.shown(s).frames
  s.frame = s.frame % n + 1
  return true
end

---------------------------------------
-- Panel --
---------------------------------------

local function fmtDeg(v) return string.format("%d deg", floor(v + 0.5)) end
local function fmtPct(v) return string.format("%d %%", floor(v + 0.5)) end
local function fmtInt(v) return tostring(floor(v + 0.5)) end

--- The panel specs (view.lua builds the panel from them).
function M.specs(view)
  local s = view.imp
  local specs = {
    { id = "impFile", kind = "label", text = "File: " .. s.name },
  }
  if #s.skel.clips > 1 then
    local names = {}
    for i, c in ipairs(s.skel.clips) do names[i] = (c.name ~= "" and c.name) or ("Take " .. i) end
    specs[#specs + 1] = { id = "impClip", kind = "combo", label = "Animation", options = names }
  end
  local more = {
    { id = "impSepCam", kind = "separator", text = "Camera" },
    { id = "impView", kind = "combo", label = "View", options = M.VIEWS },
    { id = "impYaw", kind = "slider", label = "Turn", min = -180, max = 180, step = 1, format = fmtDeg },
    { id = "impPitch", kind = "slider", label = "Tilt", min = -90, max = 90, step = 1, format = fmtDeg },
    { id = "impRoll", kind = "slider", label = "Roll", min = -180, max = 180, step = 1, format = fmtDeg },
    { id = "impZUp", kind = "switch", label = "Z is up" },
    { id = "impSepClip", kind = "separator", text = "Clip" },
    { id = "impTarget", kind = "combo", label = "Onto", options = M.TARGETS },
    { id = "impFrames", kind = "slider", label = "Frames", min = 2, max = M.Anim.MAX_FRAMES, step = 1,
      format = fmtInt },
    { id = "impUse", kind = "combo", label = "Import", options = M.USE },
    { id = "impLength", kind = "number", label = "Length", min = 1, max = M.Anim.MAX_FRAMES, step = 1,
      needsMarks = true },
    { id = "impMark", kind = "button", text = "Mark frame", needsMarks = true,
      onPress = function() M.toggleMark(s, s.frame) end },
    { id = "impClearMarks", kind = "button", text = "Clear marks", row = "same", needsMarks = true,
      onPress = function() M.clearMarks(s) end },
    { id = "impMarkHint", kind = "label", dim = true, needsMarks = true },
    { id = "impPreview", kind = "switch", label = "Preview result", needsMarks = true },
    { id = "impLoop", kind = "switch", label = "Loop" },
    { id = "impInPlace", kind = "switch", label = "In place" },
    { id = "impFore", kind = "switch", label = "Foreshorten" },
    { id = "impFollow", kind = "switch", label = "Follow joints" },
    { id = "impDepth", kind = "switch", label = "Depth order" },
    { id = "impDepthFrom", kind = "combo", label = "Depth from",
      options = function() return M.depthOptions(s) end },
    { id = "impMotion", kind = "slider", label = "Root motion", min = 0, max = 200, step = 1,
      format = fmtPct },
  }
  for _, sp in ipairs(more) do specs[#specs + 1] = sp end
  if s.target == "New rig" then
    -- the file's own bones: the part each one's skin goes to
    local rows = {
      { id = "impSepParts", kind = "separator", text = "Skin groups" },
      { id = "impThick", kind = "slider", label = "Thickness", min = 25, max = 300, step = 5,
        format = fmtPct },
      { id = "impAutoParts", kind = "button", text = "Auto groups", onPress = function()
        s.parts = {}
        M.rebuild(s, view.E.doc)
      end },
    }
    for _, sp in ipairs(rows) do specs[#specs + 1] = sp end
    for _, e in ipairs(M.R.rigPlan(s.skel)) do
      specs[#specs + 1] = { id = "impPart", kind = "combo", label = shortName(s.skel.joints[e.j].name),
        joint = e.j, options = function() return M.partOptions(s) end }
    end
    return specs
  end
  local rows = {
    { id = "impSepMap", kind = "separator", text = "Bones" },
    { id = "impAuto", kind = "button", text = "Auto map", onPress = function()
      s.map = M.R.autoMap(view.E.doc, s.skel)
      M.rebuild(s, view.E.doc)
    end },
    { id = "impClear", kind = "button", text = "Clear", row = "same", onPress = function()
      s.map = {}
      M.rebuild(s, view.E.doc)
    end },
  }
  for _, sp in ipairs(rows) do specs[#specs + 1] = sp end
  for _, b in ipairs(view.E.doc.bones) do
    specs[#specs + 1] = { id = "impMap", kind = "combo", label = b.name, boneId = b.id,
      options = function() return M.jointOptions(s) end }
  end
  return specs
end

--- The panel adapter: every change rebuilds the preview.
function M.adapter(view)
  local s = view.imp
  local function rebuild() M.rebuild(s, view.E.doc) end
  return {
    get = function(spec)
      local id = spec.id
      if id == "impClip" then
        local c = s.skel.clips[s.clip]
        return (c.name ~= "" and c.name) or ("Take " .. s.clip)
      end
      if id == "impView" then return s.view end
      if id == "impYaw" then return s.yaw end
      if id == "impPitch" then return s.pitch end
      if id == "impRoll" then return s.roll end
      if id == "impZUp" then return s.zUp end
      if id == "impTarget" then return s.target end
      if id == "impFrames" then return s.frames end
      if id == "impUse" then return s.useMarks and M.USE[2] or M.USE[1] end
      if id == "impLength" then return M.length(s) end
      if id == "impPreview" then return s.previewResult == true end
      if id == "impLoop" then return s.loop end
      if id == "impInPlace" then return s.inPlace end
      if id == "impFore" then return s.foreshorten end
      if id == "impFollow" then return s.follow end
      if id == "impDepth" then return s.depthOrder end
      if id == "impDepthFrom" then
        local j = s.ctx and s.ctx.depthFrom
        if not s.depthFrom then return M.AUTO .. (j and (" (" .. s.skel.joints[j].name .. ")") or "") end
        return s.skel.joints[s.depthFrom].name
      end
      if id == "impMotion" then return s.motion end
      if id == "impMap" then
        local j = s.map[spec.boneId]
        return j and s.skel.joints[j].name or M.NONE
      end
      if id == "impThick" then return s.thick end
      if id == "impPart" then
        local v = s.parts[spec.joint]
        if v == false then return M.R.LEAVE_OUT end
        if v then return v end
        for _, e in ipairs(M.R.rigPlan(s.skel)) do
          if e.j == spec.joint then return e.part end
        end
      end
      return nil
    end,
    set = function(spec, v)
      local id = spec.id
      if id == "impClip" then
        for i, c in ipairs(s.skel.clips) do
          if ((c.name ~= "" and c.name) or ("Take " .. i)) == v then s.clip = i end
        end
        s.frames = M.R.defaultFrames(s.skel.clips[s.clip], s.loop)
      elseif id == "impView" then
        s.view = v
        local p = M.R.PRESET[v]
        if p then s.yaw, s.pitch, s.roll = p.yaw, p.pitch, p.roll end
      elseif id == "impYaw" or id == "impPitch" or id == "impRoll" then
        s[id == "impYaw" and "yaw" or id == "impPitch" and "pitch" or "roll"] = v
        s.view = "Custom"
      elseif id == "impZUp" then s.zUp = v and true or false
      elseif id == "impTarget" then
        if v == s.target then return end
        if v == "This rig" and #view.E.doc.bones == 0 then
          view:setNotice("This rig has no bones yet: New rig builds one from the file", true)
          return
        end
        s.target = v
        if v == "This rig" then s.map = M.R.autoMap(view.E.doc, s.skel) end
        rebuild()
        -- the Bones section is a different list for each
        if view.refreshImport then view:refreshImport() end
        return
      elseif id == "impFrames" then s.frames = floor(v + 0.5)
      elseif id == "impUse" then
        s.useMarks = v == M.USE[2]
        s.frame = max(1, min(M.shown(s).frames, s.frame))
        return
      elseif id == "impLength" then
        -- never fewer frames than marks
        s.length = max(#M.markList(s), floor((tonumber(v) or 1) + 0.5))
        s.frame = max(1, min(M.shown(s).frames, s.frame))
        return
      elseif id == "impPreview" then
        s.previewResult = v == true
        s.frame = max(1, min(M.shown(s).frames, s.frame))
        return
      elseif id == "impLoop" then s.loop = v and true or false
      elseif id == "impInPlace" then s.inPlace = v and true or false
      elseif id == "impFore" then s.foreshorten = v and true or false
      elseif id == "impFollow" then s.follow = v and true or false
      elseif id == "impDepth" then s.depthOrder = v and true or false
      elseif id == "impDepthFrom" then
        s.depthFrom = (v ~= M.AUTO) and jointIndex(s, v) or nil
      elseif id == "impMotion" then s.motion = v
      elseif id == "impMap" then
        s.map[spec.boneId] = v ~= M.NONE and jointIndex(s, v) or nil
      elseif id == "impThick" then s.thick = v
      elseif id == "impPart" then
        if v == M.R.LEAVE_OUT then s.parts[spec.joint] = false else s.parts[spec.joint] = v end
      else
        return
      end
      rebuild()
    end,
    enabled = function(spec)
      if spec.id == "impMark" then return not M.previewing(s) and s.frame <= s.anim.frames end
      if spec.id == "impClearMarks" then return not M.previewing(s) and next(s.marks) ~= nil end
      if spec.id == "impPreview" then return next(s.marks) ~= nil end
      -- a left-out joint's children still choose their own part
      return true
    end,
    visible = function(spec)
      if spec.needsMarks then return s.useMarks end
      return true
    end,
    label = function(spec)
      if spec.id == "impMark" then
        return (s.marks[s.frame] and "Unmark frame " or "Mark frame ") .. s.frame
      end
      if spec.id == "impMarkHint" then
        local n = #M.markList(s)
        if n == 0 then return "Shift+click frames on the timeline, or M" end
        if M.previewing(s) then return string.format("Result: %d keys over %d frames", n, M.length(s)) end
        return string.format("%d marked, spread over %d frames", n, M.length(s))
      end
      return nil
    end,
    options = function(spec)
      if type(spec.options) == "function" then return spec.options() end
      return spec.options
    end,
  }
end

---------------------------------------
-- Viewport --
---------------------------------------

--- The two halves of the viewport.
function M.halves(r)
  local w = floor(r.w / 2)
  return { x = r.x, y = r.y, w = w, h = r.h }, { x = r.x + w, y = r.y, w = r.w - w, h = r.h }
end

local function rectOp(ops, x, y, w, h, c, a)
  ops[#ops + 1] = { "rect", floor(x), floor(y), floor(w), floor(h), c[1], c[2], c[3], a }
end

local function mix(a, b, k)
  return { floor(a[1] + (b[1] - a[1]) * k), floor(a[2] + (b[2] - a[2]) * k),
    floor(a[3] + (b[3] - a[3]) * k) }
end

local function label(view, ops, r, text)
  local th = view.theme
  local tw = view.text:width(text)
  rectOp(ops, r.x + th.pad, r.y + th.pad, tw + 2 * th.pad, view.text:height() + 2 * th.gap,
    th.overlayBg)
  ops[#ops + 1] = { "text", r.x + 2 * th.pad, r.y + th.pad + th.gap, th.textDim[1], th.textDim[2],
    th.textDim[3], text }
end

--- Screen placement of the 3D view, fitted to the first frame: its
--- height, and its widest reach around the up axis (so turning the
--- camera never rescales it), then the orbit's own zoom and pan.
local function place3d(s, r, cam)
  local pos = s.ctx.clip.pos[1]
  local n = #pos / 3
  local cx, cy, cz = 0, 0, 0
  local y0, y1 = math.huge, -math.huge
  local function up(i)
    local x, y, z = pos[i], pos[i + 1], pos[i + 2]
    if cam.zUp then y, z = z, -y end
    return x, y, z
  end
  for i = 1, #pos, 3 do
    local x, y, z = up(i)
    cx, cy, cz = cx + x, cy + y, cz + z
    y0, y1 = min(y0, y), max(y1, y)
  end
  cx, cy, cz = cx / n, (y0 + y1) / 2, cz / n
  local reach = 1e-6
  for i = 1, #pos, 3 do
    local x, _, z = up(i)
    reach = max(reach, math.sqrt((x - cx) ^ 2 + (z - cz) ^ 2))
  end
  local k = 0.85 * min(r.w / (2 * reach), r.h / max(1e-6, y1 - y0)) * s.orbit.zoom
  -- the centre goes through the camera like any point (z up undone first)
  local ox, oy = M.R.project(cam, cx, cam.zUp and -cz or cy, cam.zUp and cy or cz)
  return function(x, y)
    return r.x + r.w / 2 + (x - ox) * k + s.orbit.px, r.y + r.h / 2 + (y - oy) * k + s.orbit.py
  end
end

--- The joint In place holds still: the one under the rig's root (as the
--- retarget does), else the skeleton's own root.
local function anchorJoint(s)
  local ctx = s.ctx
  local b = ctx.roots and ctx.roots[1]
  if b and s.map[b.id] then return s.map[b.id] end
  for j, jt in ipairs(s.skel.joints) do
    if not jt.parent or jt.parent == 0 then return j end
  end
  return 1
end

--- Joint positions at frame `f` as the 3D view shows them: with In place
--- the whole skeleton slides back so the anchor keeps its spot of the
--- first frame on the ground plane (it still bobs), like the rig does, and
--- a clip that walks off stays in view.
function M.viewPositions(s, f)
  local ctx = s.ctx
  local P = M.R.positionsAt(ctx.clip, ctx.time(f))
  if not s.inPlace then return P end
  local j = anchorJoint(s)
  local P0 = ctx.clip.pos[1]
  if not (P[3 * j] and P0[3 * j]) then return P end
  local dx = P[3 * j - 2] - P0[3 * j - 2]
  -- the ground plane: x z with Y up, x y with Z up
  local zUp = M.camera(s).zUp
  local dy = zUp and P[3 * j - 1] - P0[3 * j - 1] or 0
  local dz = zUp and 0 or P[3 * j] - P0[3 * j]
  local out = {}
  for i = 1, #P, 3 do
    out[i], out[i + 1], out[i + 2] = P[i] - dx, P[i + 1] - dy, P[i + 2] - dz
  end
  return out
end

function M.paint3d(view, ops, r)
  local s, R, th = view.imp, M.R, view.theme
  local cam = M.camera(s)
  rectOp(ops, r.x, r.y, r.w, r.h, th.overlayBg)
  local toScreen = place3d(s, r, cam)
  local P = M.viewPositions(s, s.frame)
  local Q = R.projectAll(cam, P)
  local d0, d1 = math.huge, -math.huge
  for i = 3, #Q, 3 do d0, d1 = min(d0, Q[i]), max(d1, Q[i]) end
  local span = max(1e-6, d1 - d0)
  local far, near = { 70, 74, 90 }, th.bone
  -- farthest first, so near bones are drawn over far ones
  local segs = {}
  for j, jt in ipairs(s.skel.joints) do
    local p = jt.parent
    if p and p > 0 then
      segs[#segs + 1] = { p, j, (Q[3 * p] + Q[3 * j]) / 2 }
    end
  end
  table.sort(segs, function(a, b) return a[3] < b[3] end)
  local scale = th.uiScale
  for _, e in ipairs(segs) do
    local ax, ay = toScreen(Q[3 * e[1] - 2], Q[3 * e[1] - 1])
    local bx, by = toScreen(Q[3 * e[2] - 2], Q[3 * e[2] - 1])
    local c = mix(far, near, (e[3] - d0) / span)
    ops[#ops + 1] = { "line", c[1], c[2], c[3], scale, ax, ay, bx, by }
  end
  -- the joints the rig's bones take
  local k = 2 * scale
  for _, j in pairs(s.map) do
    local x, y = toScreen(Q[3 * j - 2], Q[3 * j - 1])
    rectOp(ops, x - k, y - k, 2 * k + 1, 2 * k + 1, th.accent)
  end
  -- the world axes in a corner: X red, Y green, Z blue
  local ox, oy = r.x + 28 * scale, r.y + r.h - 28 * scale
  for _, a in ipairs({ { 1, 0, 0, { 230, 90, 90 } }, { 0, 1, 0, { 120, 210, 120 } },
    { 0, 0, 1, { 100, 150, 240 } } }) do
    local x, y = R.project(cam, a[1], a[2], a[3])
    ops[#ops + 1] = { "line", a[4][1], a[4][2], a[4][3], scale, ox, oy, ox + x * 18 * scale,
      oy + y * 18 * scale }
  end
  label(view, ops, r, "3D: drag to turn, wheel to zoom")
end

--- The rig under the preview clip at the current frame, skinned.
function M.previewBuffer(view)
  local s = view.imp
  local shown = M.shown(s)
  local key = s.rev .. "|" .. s.frame .. "|" .. view.opts.outline .. "|" .. view.opts.fill
    .. "|" .. tostring(shown)
  if s._bufKey == key and s._buf then return s._buf, s._world end
  local doc = s.doc
  local pose, world = M.Ik.solve(doc, s.frame, shown)
  local layers = M.Skin.render(doc, world, { outline = view.opts.outline:lower(), pose = pose })
  s._buf = M.Skin.compose(layers, doc.canvas.w, doc.canvas.h,
    { fillAlpha = floor(view.opts.fill * 255 / 100 + 0.5) })
  s._world, s._bufKey = world, key
  return s._buf, world
end

function M.paintResult(view, ops, r)
  local s, th = view.imp, view.theme
  local doc = s.doc
  local cw, ch = doc.canvas.w, doc.canvas.h
  local m = 12 * th.uiScale
  local z = min((r.w - 2 * m) / cw, (r.h - 2 * m) / ch)
  if z >= 1 then z = floor(z) end
  if z <= 0 then return end
  local x0 = floor(r.x + (r.w - cw * z) / 2)
  local y0 = floor(r.y + (r.h - ch * z) / 2)
  view:paintChecker(ops, max(r.x, x0), max(r.y, y0), min(r.x + r.w, x0 + floor(cw * z)),
    min(r.y + r.h, y0 + floor(ch * z)))
  if #doc.bones > 0 then
    local buf, world = M.previewBuffer(view)
    ops[#ops + 1] = { "image", x0, y0, floor(cw * z), floor(ch * z), buf }
    if view.opts.showBones then
      local c = th.bone
      for _, b in ipairs(doc.bones) do
        local w = world[b.id]
        ops[#ops + 1] = { "line", c[1], c[2], c[3], th.uiScale, x0 + w.hx * z, y0 + w.hy * z,
          x0 + w.tx * z, y0 + w.ty * z }
      end
    end
  end
  label(view, ops, r, s.target == "New rig" and "New rig from the file" or "Your rig")
end

function M.paintViewport(view, ops)
  local r = view.regions.viewport
  if r.w <= 0 or r.h <= 0 then return end
  local a, b = M.halves(r)
  ops[#ops + 1] = { "save" }
  ops[#ops + 1] = { "clip", a.x, a.y, a.w, a.h }
  M.paint3d(view, ops, a)
  ops[#ops + 1] = { "restore" }
  ops[#ops + 1] = { "save" }
  ops[#ops + 1] = { "clip", b.x, b.y, b.w, b.h }
  rectOp(ops, b.x, b.y, b.w, b.h, view.theme.bg)
  M.paintResult(view, ops, b)
  ops[#ops + 1] = { "restore" }
  rectOp(ops, b.x, b.y, view.theme.uiScale, b.h, view.theme.splitter)
end

--- The frame cells of the preview clip (no keys to show: every frame has one).
function M.paintTimeline(view, ops)
  local th, s = view.theme, view.imp
  local r = view.regions.strip
  if r.w <= 0 then return end
  local result = M.previewing(s)
  local n = M.shown(s).frames
  -- the result's keys: where the marks land
  local landed = {}
  if result then
    for _, p in ipairs(M.markPositions(#M.markList(s), n, s.loop)) do landed[p] = true end
  end
  rectOp(ops, r.x, r.y, r.w, r.h, th.box)
  local sc = th.uiScale
  local numW = view.text:width(tostring(n)) + 2 * sc
  local cellW = r.w / n
  local every = 1
  for _, e in ipairs({ 1, 2, 5, 10, 20, 50 }) do
    every = e
    if cellW * e >= numW then break end
  end
  for f = 1, n do
    local x = r.x + floor((f - 1) * r.w / n)
    local w = r.x + floor(f * r.w / n) - x
    local cur = f == s.frame
    if cur then rectOp(ops, x, r.y, w, r.h, th.accent)
    elseif f > 1 then rectOp(ops, x, r.y, sc, r.h, th.cellLine) end
    if (result and landed[f]) or (not result and s.marks[f]) then
      -- a marked frame (or, on the result, a key): a bar along the foot
      local c = s.useMarks and th.keyMark or th.textDim
      rectOp(ops, x + sc, r.y + r.h - 3 * sc, max(sc, w - 2 * sc), 2 * sc, c)
    end
    if f == 1 or f % every == 0 then
      local c = cur and th.accentText or th.textDim
      ops[#ops + 1] = { "text", x + 2 * sc, r.y + 2 * sc, c[1], c[2], c[3], tostring(f) }
    end
  end
  ops[#ops + 1] = { "frame", r.x, r.y, r.w, r.h, th.frame[1], th.frame[2], th.frame[3] }
end

function M.frameAt(view, x)
  local r = view.regions.strip
  local n = M.shown(view.imp).frames
  if r.w <= 0 then return 1 end
  return max(1, min(n, floor((x - r.x) * n / r.w) + 1))
end

---------------------------------------
-- Input --
---------------------------------------

local function inside(r, x, y)
  return r and x >= r.x and y >= r.y and x < r.x + r.w and y < r.y + r.h
end

--- A press in the viewport: left on the 3D half orbits, right / middle
--- pans it. The result half only takes the wheel. True when consumed.
function M.down(view, x, y, button)
  local s = view.imp
  local a = M.halves(view.regions.viewport)
  if not inside(a, x, y) then return false end
  s.drag = { x = x, y = y, yaw = s.yaw, pitch = s.pitch, px = s.orbit.px, py = s.orbit.py,
    pan = button ~= "left" }
  return true
end

function M.move(view, x, y)
  local s = view.imp
  local d = s.drag
  if not d then return false end
  if d.pan then
    s.orbit.px, s.orbit.py = d.px + (x - d.x), d.py + (y - d.y)
    return true
  end
  local k = M.ORBIT_DEG / view.theme.uiScale
  local yaw = d.yaw + (x - d.x) * k
  yaw = (yaw + 180) % 360 - 180
  local pitch = max(-90, min(90, d.pitch + (y - d.y) * k))
  if floor(yaw + 0.5) == s.yaw and floor(pitch + 0.5) == s.pitch then return false end
  s.yaw, s.pitch, s.view = floor(yaw + 0.5), floor(pitch + 0.5), "Custom"
  M.rebuild(s, view.E.doc)
  return true
end

function M.up(view)
  local s = view.imp
  local had = s.drag ~= nil
  s.drag = nil
  return had
end

function M.wheel(view, x, y, dy)
  local s = view.imp
  local a = M.halves(view.regions.viewport)
  if not inside(a, x, y) then return false end
  local z = s.orbit.zoom
  if dy < 0 then z = z * 1.15 else z = z / 1.15 end
  s.orbit.zoom = max(0.2, min(8, z))
  return true
end

return M
