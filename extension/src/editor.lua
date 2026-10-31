-- editor.lua — the rest-pose editor of Bonebase as a pure model: the
-- document, its undo history, the selection, the active tool and the
-- gestures, all in RIG coordinates (canvas pixels, floats). The window
-- (view.lua) converts the mouse to rig space and back; nothing here knows
-- about zoom, widgets or Aseprite.
--
-- Tools (left button):
--   Select  click a bone to select it; drag a TAIL to change its length
--           and angle (a joint shared by a chain is the parent's tail);
--           drag the body of a connected bone to swing it around its
--           head; drag the body or head of a root / loose bone to move it.
--   Bone    drag from empty space for a new root, from a joint to grow a
--           connected child, from a body or a loose head for a branch.
--           A connected child keeps its parent's group; a root or a
--           branch has none (no skin) until one is given (Groups tab).
--   Radius  drag near a joint: the radius follows the pointer. Every bone
--           end sitting on that joint takes it, so chains stay smooth.
--   Bulge   drag on the side of a bone: grabs the nearest handle on that
--           side or adds one; releasing it at ~0 px removes it.
--   Pan     handled by the view (camera), listed here for the toolbar.
--
-- One gesture = one undo step: the snapshot is taken lazily on the first
-- change of a gesture (a click that changes nothing leaves no step).
--
-- MIRROR (doc.mirror = { on, x }): while on, a bone created off the axis
-- gets a twin reflected across x = mirror.x (`bone.twin` both ways) and
-- every later edit of either one is copied to the other, left and right
-- bulges swapped (a reflection turns the left side into the right one).
--
-- MODES: "Rest" edits the skeleton (the tools above); "Pose" animates it
-- at `E.frame` with its own tools, and every pose drag writes keys there
-- (auto-key, anim.lua):
--   Pose  FK: drag a tail or a body to turn the bone around its head;
--         drag the body of a root or loose bone to move it. Whatever hangs
--         below goes along, live IK chains too (their dummies are keyed
--         where the FK shape puts them). A bone of a live chain turns the
--         same way (the upper one swings the whole limb from the shoulder
--         or hip, the lower one bends it at the elbow or knee). Dummies
--         are not grabbed here. The Bone tab's sliders follow the same
--         rule while Pose is the tool.
--   IK    drag a hand / foot or its dummies: the end follows the pointer
--         (a live chain through its target; a chain without a controller
--         by turning both bones). Moving a body (a root or loose bone)
--         keeps every hand and foot under it pinned while `pinLimbs` is
--         on; grabbing anything else acts like Pose.
--   Pan   as in Rest.

local M = {}

M.Rig, M.Skin, M.Undo, M.Anim, M.Ik, M.Ref, M.Poses = nil, nil, nil, nil, nil, nil, nil

--- `Ref` (refimg.lua) is optional: without it there are no reference images.
function M.bind(Rig, Skin, Undo, Anim, Ik, Ref, Poses)
  M.Rig, M.Skin, M.Undo, M.Anim, M.Ik, M.Ref, M.Poses = Rig, Skin, Undo, Anim, Ik, Ref, Poses
  return M
end

M.MODES = { "Rest", "Pose" }
M.TOOLS = { "Select", "Bone", "Radius", "Bulge", "Ref", "Pan" }
M.TOOLS_POSE = { "Pose", "IK", "Ref", "Pan" }
M.SNAP = 0.5        -- dragged joints land on half pixels
M.MIN_NEW = 2       -- shorter drags with the Bone tool create nothing
M.BULGE_DROP = 0.5  -- a handle released closer than this to 0 goes away

local sqrt, abs, floor, max, min = math.sqrt, math.abs, math.floor, math.max, math.min

local function snap(v)
  return floor(v / M.SNAP + 0.5) * M.SNAP
end

local E = {}
E.__index = E

function M.new(doc)
  local Rig, Undo = M.Rig, M.Undo
  local self = setmetatable({
    doc = doc,
    hist = Undo.new(100, Rig.copy, Rig.restore),
    rev = 0,          -- bumps on every change: the view caches renders by it
    tool = "Select",
    sel = nil,        -- selected bone id (the primary one: the panel edits it)
    multi = {},       -- other selected bones (Shift+click), id -> true ...
    multiSel = nil,   -- ... valid while `sel` is still this id
    keySel = {},      -- dope sheet: picked keys (anim.lua key set)
    keyClip = nil,    -- copied keys (anim.lua copyKeys)
    lastCopy = nil,   -- "pose" | "keys": what Ctrl+V pastes
    selBulge = nil,   -- { bone, side, i } last touched bulge handle
    drag = nil,
    gesture = 0,
    mode = "Rest",
    pinLimbs = true,  -- IK tool: moving a body keeps hands and feet in place
    frame = 1,
  }, E)
  return self
end

---------------------------------------
-- Bookkeeping --
---------------------------------------

--- The pose shown: the animation with the live IK applied in Pose mode
--- (ik.lua), nil (rest) else.
function E:pose()
  if self.mode ~= "Pose" then return nil end
  self:world()
  return self._pose
end

--- World transforms of what is shown (posed in Pose mode), cached.
function E:world()
  local key = self.rev .. self.mode .. self.frame
  if self._worldKey ~= key or not self._world then
    if self.mode == "Pose" then
      self._pose, self._world = M.Ik.solve(self.doc, self.frame)
    else
      self._pose, self._world = nil, M.Rig.solve(self.doc)
    end
    self._worldKey = key
  end
  return self._world
end

--- True when `id` is a bone, a part or an IK controller of the doc (what
--- keys may belong to).
function E:exists(id)
  return M.Rig.bone(self.doc, id) ~= nil or M.Ik.find(self.doc, id) ~= nil
    or M.Rig.part(self.doc, id) ~= nil or id == -1   -- -1: the reference image
    or id == M.Rig.ROOT                              -- -2: the rig root
end

--- Drops, in EVERY clip, the keys of things that no longer exist.
function E:pruneAnims()
  for _, anim in ipairs(self.doc.clips) do
    M.Anim.prune(anim, function(id) return self:exists(id) end)
  end
end

--- Called before the first change of a gesture or command.
function E:touch(tag)
  if tag == nil then self.dirtyGesture = self.gesture end
  M.Undo.push(self.hist, self.doc, tag or ("g" .. self.gesture))
end

--- Escape during a drag: whatever the gesture already changed goes back,
--- and its step leaves no redo behind. Returns true when a drag was live.
function E:cancelGesture()
  if not self.drag then return false end
  self.drag = nil
  if self.dirtyGesture == self.gesture and #self.hist.undo > 0 then
    M.Undo.undo(self.hist, self.doc)
    self.hist.redo = {}
    self:afterHistory()
  end
  self.dirtyGesture = nil
  return true
end

function E:changed()
  self.rev = self.rev + 1
  -- what the library entry has no longer is this (undo and redo restore
  -- the flag with the rest: back on the saved state it is clean again)
  self.doc.libChanged = true
end

--- A new gesture: its changes never merge with the previous one.
function E:beginGesture()
  self.gesture = self.gesture + 1
  -- a command started mid-drag ends that drag: Esc must never undo the
  -- command thinking it was the drag
  self.drag = nil
  M.Undo.breakTag(self.hist)
end

function E:undo()
  if #self.hist.undo == 0 then return false end
  M.Undo.undo(self.hist, self.doc)
  self:afterHistory()
  return true
end

function E:redo()
  if #self.hist.redo == 0 then return false end
  M.Undo.redo(self.hist, self.doc)
  self:afterHistory()
  return true
end

function E:afterHistory()
  if self.sel and not M.Rig.bone(self.doc, self.sel) then self.sel = nil end
  if self.frame > self.doc.anim.frames then self.frame = self.doc.anim.frames end
  self.selBulge = nil
  self.drag = nil
  -- not a change of the doc: the snapshot brought its own libChanged
  self.rev = self.rev + 1
end

function E:tools()
  return self.mode == "Pose" and M.TOOLS_POSE or M.TOOLS
end

function E:setMode(mode)
  if mode ~= "Rest" and mode ~= "Pose" then return false end
  if mode == self.mode then return false end
  self.mode = mode
  self.drag, self.selBulge = nil, nil
  if self.tool ~= "Pan" then self.tool = self:tools()[1] end
  return true
end

function E:setTool(name)
  for _, t in ipairs(self:tools()) do
    if t == name then
      self.tool = name
      self.drag = nil
      -- Delete acts on bulge handles only while they are on screen
      if name ~= "Bulge" then self.selBulge = nil end
      return true
    end
  end
  return false
end

function E:selected()
  return self.sel and M.Rig.bone(self.doc, self.sel) or nil
end

--- Every selected bone id: the primary first, then the rest in rig
--- order. The extra ones only count while the primary is the one they
--- were picked with: any other way of changing `sel` drops them.
function E:selectedIds()
  if not self:selected() then return {} end
  local out = { self.sel }
  if self.multiSel == self.sel then
    for _, b in ipairs(self.doc.bones) do
      if self.multi[b.id] and b.id ~= self.sel then out[#out + 1] = b.id end
    end
  end
  return out
end

function E:isSelected(id)
  if id == nil or not self.sel then return false end
  return id == self.sel or (self.multiSel == self.sel and self.multi[id] == true)
end

--- Shift+click: adds bone `id` to the selection (it becomes the primary)
--- or takes it out.
function E:toggleSelect(id)
  if not M.Rig.bone(self.doc, id) then return false end
  local ids = self:selectedIds()
  local set = {}
  for _, k in ipairs(ids) do set[k] = true end
  if set[id] then
    set[id] = nil
    if id == self.sel then
      self.sel = nil
      for _, k in ipairs(ids) do
        if set[k] then self.sel = k break end
      end
    end
  else
    set[id] = true
    self.sel = id
  end
  if self.sel then set[self.sel] = nil end
  self.multi, self.multiSel = set, self.sel
  self.selBulge = nil
  return true
end

--- After a plain click: clicking another selected bone keeps the group
--- (it becomes the primary); clicking anything else selects just that.
function E:keepMulti(prev)
  if not self.sel then self.multi, self.multiSel = {}, nil return end
  if self.sel == prev then return end
  if prev and self.multiSel == prev and self.multi[self.sel] then
    self.multi[self.sel] = nil
    self.multi[prev] = true
    self.multiSel = self.sel
  else
    self.multi, self.multiSel = {}, nil
  end
end

---------------------------------------
-- Mirror --
---------------------------------------

function E:mirrorOn()
  return self.doc.mirror and self.doc.mirror.on == true
end

function E:mx(x)
  return 2 * self.doc.mirror.x - x
end

local function copyBulges(list)
  local out = {}
  for i, q in ipairs(list) do out[i] = { q[1], q[2] } end
  return out
end

--- Writes `t` as the reflection of `b` (geometry; `full` adds radii,
--- bulges with sides swapped, and the connected flag).
function E:reflect(b, t, full)
  local Rig = M.Rig
  local w = Rig.solve(self.doc)[b.id]
  if full then t.connected = b.connected and t.parent ~= 0 end
  Rig.place(self.doc, t.id, self:mx(w.hx), w.hy, self:mx(w.tx), w.ty)
  if full then
    t.rHead, t.rTail = b.rHead, b.rTail
    t.bulgeL, t.bulgeR = copyBulges(b.bulgeR), copyBulges(b.bulgeL)
  end
end

--- After an edit of bone `id` with the mirror on: its twin copies it in
--- full, then every OTHER pair is reflected again in rig order (parents
--- first), the first bone of each pair leading. Editing a bone on the axis
--- (a chest, a hip) moves both sides of what hangs from it; this puts
--- them back in mirror.
function E:syncTwin(id)
  if not self:mirrorOn() then return end
  local Rig = M.Rig
  local doc = self.doc
  local done = {}
  local b = Rig.bone(doc, id)
  if b and b.twin ~= 0 then
    local t = Rig.bone(doc, b.twin)
    if t then
      self:reflect(b, t, true)
      done[b.id], done[t.id] = true, true
    else
      b.twin = 0
    end
  end
  for _, x in ipairs(doc.bones) do
    if x.twin ~= 0 and not done[x.id] then
      local y = Rig.bone(doc, x.twin)
      if y then
        self:reflect(x, y, false)
        done[x.id], done[y.id] = true, true
      end
    end
  end
end

--- The bone whose twin should be the new bone's parent.
function E:twinParent(parentId)
  if parentId == 0 then return 0 end
  local p = M.Rig.bone(self.doc, parentId)
  if p and p.twin ~= 0 then return p.twin end
  return parentId
end

function E:onAxis(x)
  return abs(x - self.doc.mirror.x) <= 0.5
end

---------------------------------------
-- Commands (panel buttons, keys) --
---------------------------------------

--- Deletes the selected bulge handle if one is selected, else the
--- selected bone (and its twin, with their subtrees).
function E:deleteSelection()
  local Rig = M.Rig
  if self.mode == "Pose" then
    if self:keySelCount() > 0 then return self:deleteSelKeys() end
    return self:deleteKeys()
  end
  local sb = self.selBulge
  if sb then
    local b = Rig.bone(self.doc, sb.bone)
    if b then
      self:beginGesture()
      self:touch()
      M.Skin.removeBulge(b, sb.side, sb.i)
      self:syncTwin(b.id)
      self.selBulge = nil
      self:changed()
      return true
    end
  end
  local ids = self:selectedIds()
  if #ids == 0 then return false end
  self:beginGesture()
  self:touch()
  for _, id in ipairs(ids) do
    local b = Rig.bone(self.doc, id)   -- may be gone with an earlier subtree
    if b then
      local twin = b.twin
      Rig.removeBone(self.doc, b.id)
      if twin ~= 0 and Rig.bone(self.doc, twin) then Rig.removeBone(self.doc, twin) end
    end
  end
  local doc = self.doc
  M.Ik.prune(doc)
  self:pruneAnims()
  self.sel = nil
  self.multi, self.multiSel = {}, nil
  self.selBulge = nil
  self:changed()
  return true
end

--- Bone properties the panel edits. Coalesces per (field, bone) so a
--- slider drag is one undo step.
E.PROPS = { name = true, len = true, angle = true, rHead = true, rTail = true,
  part = true, connected = true }

function E:getProp(field)
  local b = self:selected()
  if not b then return nil end
  return b[field]
end

function E:setProp(field, v)
  local Rig = M.Rig
  local b = self:selected()
  if not b or not E.PROPS[field] then return false end
  if b[field] == v then return false end
  -- refusals go before the snapshot: a no-op must not leave an undo step
  if field == "connected" and b.parent == 0 then return false end
  self:touch("prop:" .. field .. ":" .. b.id)
  if field == "len" then
    b.len = max(Rig.MIN_LEN, tonumber(v) or b.len)
  elseif field == "angle" then
    b.angle = Rig.wrap(tonumber(v) or b.angle)
  elseif field == "rHead" or field == "rTail" then
    b[field] = max(Rig.MIN_RADIUS, tonumber(v) or b[field])
  elseif field == "connected" then
    if b.parent == 0 then return false end
    if v then
      b.connected = true
      -- a connected bone ignores x / y: drop those tracks in this same
      -- step, or they would come back to life on a later disconnect
      for _, anim in ipairs(self.doc.clips) do
        local kept = {}
        for _, tr in ipairs(anim.tracks) do
          if not (tr.bone == b.id and (tr.ch == "x" or tr.ch == "y")) then kept[#kept + 1] = tr end
        end
        anim.tracks = kept
      end
    else
      -- keep the head where it is in the REST pose (world() is posed in Pose)
      local w = Rig.solve(self.doc)[b.id]
      Rig.setHead(self.doc, b.id, w.hx, w.hy)
    end
  elseif field == "name" then
    b.name = tostring(v)
    -- a twin follows a name with a side: arm_L <-> arm_R
    local tw = b.twin ~= 0 and Rig.bone(self.doc, b.twin) or nil
    local base, side = b.name:match("^(.*)_([LR])$")
    if tw and base then tw.name = base .. (side == "L" and "_R" or "_L") end
  else
    b[field] = v
  end
  if field ~= "name" and field ~= "part" then self:syncTwin(b.id) end
  self:changed()
  return true
end

--- Replaces the whole document (a template), as one undo step.
function E:loadDoc(doc)
  self:beginGesture()
  self:touch()
  -- another rig: its own generation, so linking it to the library never
  -- touches the snapshots of the one it replaced
  local gen = (self.doc.gen or 0) + 1
  M.Rig.restore(self.doc, doc)
  self.doc.gen = gen
  self.sel, self.selBulge, self.drag = nil, nil, nil
  self.clip = nil -- its bone ids mean nothing in another rig
  self.keyClip, self.keySel, self.lastCopy = nil, {}, nil
  self.frame = 1
  self:changed()
  return true
end

--- Ties the rig to library entry `name` (nil: to none), as it is now
--- saved there; `stamp` (library.lua stamp) tells that file from a later
--- one. Not an undo step: undo and redo keep the link, on every snapshot
--- of this same rig (not those of a rig it took the place of), and those
--- other states count as changed.
function E:linkLibrary(name, stamp)
  local gen = self.doc.gen
  self.doc.libName, self.doc.libStamp, self.doc.libChanged = name, stamp, false
  for _, stack in ipairs({ self.hist.undo, self.hist.redo }) do
    for _, snap in ipairs(stack) do
      if snap.gen == gen then
        snap.libName, snap.libStamp, snap.libChanged = name, stamp, true
      end
    end
  end
end

--- True when the rig changed since it was saved to its library entry.
function E:libDirty()
  return self.doc.libName ~= nil and self.doc.libChanged == true
end

--- A template: the doc plus controllers on every limb, one undo step.
function E:loadTemplate(doc)
  self:loadDoc(doc)
  M.Ik.addLimbs(self.doc)
  self:changed()
  return true
end

--- Groups (doc.parts; the Groups tab). A bone's group is the layer its
--- skin goes to; a bone without one (part 0) has no skin.

--- A new group, named "group N"; the selected bones (if any) go in it.
function E:addPart(name)
  self:beginGesture()
  self:touch()
  local doc = self.doc
  local n, used = #doc.parts + 1, {}
  for _, p in ipairs(doc.parts) do used[p.name] = true end
  while used["group " .. n] do n = n + 1 end
  local p = M.Rig.addPart(doc, name or ("group " .. n))
  for _, id in ipairs(self:selectedIds()) do
    local b = M.Rig.bone(doc, id)
    if b then b.part = p.id end
  end
  self:changed()
  return p
end

--- Puts the selected bones in group `id` (0 = out of any group). Returns
--- how many changed; no step when none does.
function E:assignPart(id)
  local doc = self.doc
  if id ~= 0 and not M.Rig.part(doc, id) then return 0 end
  local list = {}
  for _, bid in ipairs(self:selectedIds()) do
    local b = M.Rig.bone(doc, bid)
    if b and b.part ~= id then list[#list + 1] = b end
  end
  if #list == 0 then return 0 end
  self:beginGesture()
  self:touch()
  for _, b in ipairs(list) do b.part = id end
  self:changed()
  return #list
end

--- Deletes a group: its bones are left without one (no skin) until they
--- get another; its z / vis keys go with it.
function E:removePart(id)
  if not M.Rig.part(self.doc, id) then return false end
  self:beginGesture()
  self:touch()
  M.Rig.removePart(self.doc, id)
  self:pruneAnims()
  self:changed()
  return true
end

--- A part's animated value on `ch` ("z" | "vis") at this frame: its key,
--- else the part's own z / visible.
function E:getPartValue(id, ch)
  local p = M.Rig.part(self.doc, id)
  if not p then return nil end
  local q = M.Anim.sample(self.doc.anim, self.frame)[id]
  if q and q[ch] ~= nil then return q[ch] end
  return ch == "z" and p.z or 1
end

--- Keys a part's z / vis at this frame (Pose mode, one undo step per
--- frame and channel).
function E:setPartValue(id, ch, v)
  local p = M.Rig.part(self.doc, id)
  if not p or self.mode ~= "Pose" or not M.Anim.STEPPED[ch] then return false end
  v = tonumber(v) or 0
  if ch == "vis" then v = v >= 0.5 and 1 or 0 else v = max(-99, min(99, floor(v + 0.5))) end
  local t = M.Anim.track(self.doc.anim, id, ch)
  if t and self:getPartValue(id, ch) == v and M.Anim.hasKey(self.doc.anim, self.frame, { [id] = true }) then
    return false
  end
  self:touch("partkey:" .. ch .. ":" .. id .. ":" .. self.frame)
  M.Anim.setKey(self.doc.anim, id, ch, self.frame, v, "Step")
  self:changed()
  return true
end

--- Removes a part's keys at this frame.
function E:deletePartKeys(id)
  local only = { [id] = true }
  if not M.Anim.hasKey(self.doc.anim, self.frame, only) then return false end
  self:beginGesture()
  self:touch()
  M.Anim.removeKeys(self.doc.anim, self.frame, only)
  self:changed()
  return true
end

function E:setPartProp(id, field, v)
  local p = M.Rig.part(self.doc, id)
  if not p or p[field] == v then return false end
  self:touch("part:" .. field .. ":" .. id)
  p[field] = v
  self:changed()
  return true
end

function E:setMirror(on, x)
  local m = self.doc.mirror
  if m.on == on and (x == nil or m.x == x) then return false end
  self:touch("mirror")
  m.on = on and true or false
  if x then m.x = x end
  self:changed()
  return true
end

---------------------------------------
-- Gestures --
---------------------------------------

local function dist(ax, ay, bx, by)
  local dx, dy = ax - bx, ay - by
  return sqrt(dx * dx + dy * dy)
end

--- A shared joint means the parent's tail, not the child's head.
function E:resolveHit(hit)
  if not hit then return nil end
  local b = M.Rig.bone(self.doc, hit.bone)
  if hit.what == "head" and b.connected and b.parent ~= 0 then
    return { bone = b.parent, what = "tail", t = 1 }
  end
  return hit
end

--- Left button down at rig point (x, y). `tol` is the pick tolerance in
--- rig px (the view passes a few screen px divided by the zoom).
function E:down(x, y, tol, mods)
  self:beginGesture()
  local tool = self.tool
  local picks = (self.mode == "Pose" and (tool == "Pose" or tool == "IK"))
    or (self.mode == "Rest" and tool == "Select")
  if mods and mods.shift and picks then
    -- Shift+click adds a bone to the selection or takes it out
    local hit = self:resolveHit(M.Rig.pick(self.doc, self:world(), x, y, tol))
    if hit then return self:toggleSelect(hit.bone) end
  end
  local prev = self.sel
  self.downMods = mods or {}
  self.downIds = self:selectedIds()   -- the group a drag may carry along
  local r = self:downTool(x, y, tol)
  self:keepMulti(prev)
  return r
end

function E:downTool(x, y, tol)
  local tool = self.tool
  if tool == "Ref" then return self:downRef(x, y) end
  -- the rig root's handle comes before any bone under it; Alt moves the
  -- handle alone, from wherever the click is
  if (self.mode == "Pose" and (tool == "Pose" or tool == "IK"))
    or (self.mode == "Rest" and tool == "Select") then
    local alone = self.downMods and self.downMods.alt
    if self:downRoot(x, y, tol, alone) then return true end
  end
  if self.mode == "Pose" then
    if tool == "Pose" then return self:downPose(x, y, tol) end
    if tool == "IK" then return self:downIK(x, y, tol) end
    return false
  end
  if tool == "Select" then return self:downSelect(x, y, tol) end
  if tool == "Bone" then return self:downBone(x, y, tol) end
  if tool == "Radius" then return self:downRadius(x, y, tol) end
  if tool == "Bulge" then return self:downBulge(x, y, tol) end
  return false
end

function E:move(x, y)
  local d = self.drag
  if not d then return false end
  local k = d.kind
  if k == "tail" then return self:dragTail(d, x, y) end
  if k == "swing" then return self:dragSwing(d, x, y) end
  if k == "move" then return self:dragMove(d, x, y) end
  if k == "new" then
    d.tx, d.ty = snap(x), snap(y)
    return true
  end
  if k == "radius" then return self:dragRadius(d, x, y) end
  if k == "bulge" then return self:dragBulge(d, x, y) end
  if k == "rot" then return self:dragRot(d, x, y) end
  if k == "trans" then return self:dragTrans(d, x, y) end
  if k == "ik" then return self:dragIK(d, x, y) end
  if k == "dummy" then return self:dragDummy(d, x, y) end
  if k == "ground" then return self:dragGround(d, x, y) end
  if k == "ref" then return self:dragRef(d, x, y) end
  if k == "root" then return self:dragRoot(d, x, y) end
  if k == "rootAlone" then return self:setRootPos(x, y) end
  if k == "chain" then return self:dragChain(d, x, y) end
  return false
end

---------------------------------------
-- FK and IK kept apart --
---------------------------------------
-- Pose is FK: what it turns or moves carries everything below, and a live
-- chain below rides along (its dummies are keyed where the FK shape puts
-- them). IK keeps hands and feet where they are. Turning a chain's IK or
-- its locked hand / foot on or off rewrites the keys of every keyed frame
-- so nothing moves there (setIKProp).

--- True when bone `id` hangs (at any depth) from bone `anc`.
local function hangsFrom(doc, id, anc)
  local b = M.Rig.bone(doc, id)
  local p = b and b.parent or 0
  local guard = 0
  while p ~= 0 and guard <= #doc.bones do
    if p == anc then return true end
    local pb = M.Rig.bone(doc, p)
    p = pb and pb.parent or 0
    guard = guard + 1
  end
  return false
end

--- Where a chain's pole goes for base (bx, by), joint (jx, jy), end
--- (ex, ey): out from the joint, away from the base-end line. `side`
--- (+1 / -1) picks it when the chain is straight.
local function poleOut(bx, by, jx, jy, ex, ey, len, side)
  local mx, my = (bx + ex) / 2, (by + ey) / 2
  local vx, vy = jx - mx, jy - my
  local l = math.sqrt(vx * vx + vy * vy)
  if l < 1e-6 then
    local ux, uy = ex - bx, ey - by
    local ul = math.max(1e-9, math.sqrt(ux * ux + uy * uy))
    vx, vy, l = -uy / ul * (side or 1), ux / ul * (side or 1), 1
  end
  local k = M.Ik.POLE_DIST * len / l
  return jx + vx * k, jy + vy * k
end

--- Keys the dummies of `c` in `anim` at `f` with canvas positions (the
--- keys are relative to the rig root there).
local function keyDummiesAt(doc, anim, f, c, tx, ty, px, py)
  local gx, gy = M.Ik.offset(doc, f, anim)
  M.Anim.setKey(anim, c.id, "tx", f, tx - gx)
  M.Anim.setKey(anim, c.id, "ty", f, ty - gy)
  M.Anim.setKey(anim, c.id, "px", f, px - gx)
  M.Anim.setKey(anim, c.id, "py", f, py - gy)
end

--- A rotation key in `anim` at `f`, a whole number of turns from what is
--- there, so a converted key never spins the long way.
local function keyRotAt(anim, id, f, v)
  local p = M.Anim.sample(anim, f)[id]
  M.Anim.setKey(anim, id, "rot", f, M.Anim.unwrap(p and p.rot or 0, v))
end

--- True when a press `hit` on chain `c` grabs the body of its upper bone
--- and that bone is loose (a root, or not connected: a shoulder, a hip):
--- it moves, as it would without IK.
function E:movesLoose(c, hit)
  if hit.what == "tail" or hit.bone ~= c.upper then return false end
  local b = M.Rig.bone(self.doc, hit.bone)
  return b ~= nil and (b.parent == 0 or not b.connected)
end

--- The live chains hanging below any of `ids` (not the chains those
--- bones belong to, unless `own`: an upper bone that moves carries its
--- chain), with their shape as it is now: applyCarry keeps it.
function E:captureCarry(ids, own)
  local doc = self.doc
  local out = {}
  if not M.Ik or #doc.ik == 0 then return out end
  local world = self:world()
  local sampled = M.Anim.sample(doc.anim, self.frame)
  for _, c in ipairs(doc.ik) do
    local hit = false
    if c.on then
      for _, id in ipairs(ids) do
        if (own and id == c.upper) or (id ~= c.upper and id ~= c.lower and hangsFrom(doc, c.upper, id)) then
          hit = true
          break
        end
      end
    end
    if hit then
      local a = M.Rig.bone(doc, c.upper)
      local wa, wb = world[c.upper], world[c.lower]
      local tx, ty, px, py = M.Ik.dummies(doc, c, self.frame)
      local cross = (tx - wa.hx) * (py - wa.hy) - (ty - wa.hy) * (px - wa.hx)
      out[#out + 1] = {
        c = c, relU = wa.angle - self:parentAngle(a, world), relL = wb.angle - wa.angle,
        lenU = math.sqrt((wa.tx - wa.hx) ^ 2 + (wa.ty - wa.hy) ^ 2),
        lenL = math.sqrt((wb.tx - wb.hx) ^ 2 + (wb.ty - wb.hy) ^ 2),
        lower0 = wb.angle, side = cross < 0 and 1 or -1,
        own0 = (c.tip ~= 0 and c.lockTip) and (sampled[c.tip] and sampled[c.tip].rot or 0) or nil,
      }
    end
  end
  return out
end

--- Keys the captured chains so they keep their shape on the body that
--- just moved (FK): their base is wherever their parent now puts it.
function E:applyCarry(list)
  if not list or #list == 0 then return end
  local doc, f = self.doc, self.frame
  local fk = M.Rig.solve(doc, M.Anim.sample(doc.anim, f))
  for _, e in ipairs(list) do
    local c = e.c
    local a = M.Rig.bone(doc, c.upper)
    local h = fk[c.upper]
    local au = (a.parent ~= 0 and fk[a.parent].angle or 0) + e.relU
    local al = au + e.relL
    local ru, rl = math.rad(au), math.rad(al)
    local jx, jy = h.hx + e.lenU * math.cos(ru), h.hy + e.lenU * math.sin(ru)
    local tx, ty = jx + e.lenL * math.cos(rl), jy + e.lenL * math.sin(rl)
    local px, py = poleOut(h.hx, h.hy, jx, jy, tx, ty, a.len, e.side)
    keyDummiesAt(doc, doc.anim, f, c, tx, ty, px, py)
    if e.own0 then keyRotAt(doc.anim, c.tip, f, e.own0 + (al - e.lower0)) end
  end
end

--- The live chain whose locked hand / foot is bone `id`, if any: its
--- rotation key is a turn in the WORLD over its rest angle (ik.lua).
function E:lockedTipChain(id)
  for _, c in ipairs(self.doc.ik) do
    if c.on and c.lockTip and c.tip == id then return c end
  end
  return nil
end

--- Keys bone `b` so its world angle is `target` (degrees) at this frame,
--- whatever its rotation key means (FK, or a locked hand / foot).
function E:keyWorldAngle(b, target, world)
  if self:lockedTipChain(b.id) then
    keyRotAt(self.doc.anim, b.id, self.frame, target - M.Rig.solve(self.doc)[b.id].angle)
  else
    self:keyRot(b.id, target - self:parentAngle(b, world) - b.angle)
  end
end

--- The frames of `anim` with any key (frame 1 when it has none).
local function keyedFrames(anim)
  local set, list = {}, {}
  for _, t in ipairs(anim.tracks) do
    for _, k in ipairs(t.keys) do
      if not set[k[1]] then set[k[1]], list[#list + 1] = true, k[1] end
    end
  end
  if #list == 0 then list[1] = 1 end
  table.sort(list)
  return list
end

--- Rewrites chain `c`'s keys in every clip so each keyed frame looks the
--- same once `field` (on | lockTip) is `v`: what IK showed becomes FK
--- rotations, what FK showed becomes dummies, a locked hand's world turn
--- becomes a plain one and back.
function E:matchChain(c, field, v)
  local doc = self.doc
  local Rig = M.Rig
  local a, b = Rig.bone(doc, c.upper), Rig.bone(doc, c.lower)
  local tip = c.tip ~= 0 and Rig.bone(doc, c.tip) or nil
  local rest = Rig.solve(doc)
  local seen = {}
  for _, anim in ipairs(doc.clips) do
    seen[#seen + 1] = { anim = anim, frames = {} }
    for _, f in ipairs(keyedFrames(anim)) do
      local pose, world = M.Ik.solve(doc, f, anim)
      local tx, ty, px, py = M.Ik.dummies(doc, c, f, nil, anim)
      local cross = (tx - world[a.id].hx) * (py - world[a.id].hy) - (ty - world[a.id].hy) * (px - world[a.id].hx)
      table.insert(seen[#seen].frames, { f = f, pose = pose, world = world, side = cross < 0 and 1 or -1 })
    end
  end
  c[field] = v
  for _, e in ipairs(seen) do
    local anim = e.anim
    for _, q in ipairs(e.frames) do
      local f, w = q.f, q.world
      local wa, wb = w[a.id], w[b.id]
      if c.on then
        local px, py = poleOut(wa.hx, wa.hy, wb.hx, wb.hy, wb.tx, wb.ty, a.len, q.side)
        keyDummiesAt(doc, anim, f, c, wb.tx, wb.ty, px, py)
      else
        local pa = a.parent ~= 0 and w[a.parent].angle or 0
        keyRotAt(anim, a.id, f, wa.angle - pa - a.angle)
        keyRotAt(anim, b.id, f, wb.angle - wa.angle - b.angle)
        -- a stretched chain keeps its length as FK
        for _, bone in ipairs({ a, b }) do
          local ww = w[bone.id]
          local s = math.sqrt((ww.tx - ww.hx) ^ 2 + (ww.ty - ww.hy) ^ 2) / bone.len
          local own = q.pose[bone.id] and q.pose[bone.id].ownS or 1
          if math.abs(s - own) > 1e-6 then M.Anim.setKey(anim, bone.id, "s", f, s) end
        end
      end
      if tip then
        local wt = w[tip.id]
        if c.on and c.lockTip then
          keyRotAt(anim, tip.id, f, wt.angle - rest[tip.id].angle)
        else
          keyRotAt(anim, tip.id, f, wt.angle - wb.angle - tip.angle)
        end
      end
    end
  end
end

---------------------------------------
-- The rig root --
---------------------------------------

--- Where the rig root's handle is now: its rest place plus, in Pose, the
--- offset keyed at this frame. nil without bones.
function E:rootHandle()
  local doc = self.doc
  local hx, hy = M.Rig.rootHandle(doc)
  if not hx then return nil end
  if self.mode == "Pose" then
    local gx, gy = M.Ik.offset(doc, self.frame)
    hx, hy = hx + gx, hy + gy
  end
  return hx, hy
end

--- A press on the root handle: in Rest the drag moves the whole rig, in
--- Pose it keys the root's offset at this frame. With Alt (`alone`) only
--- the handle moves, in either mode; Alt anywhere else puts the handle
--- under the pointer and goes on dragging it.
function E:downRoot(x, y, tol, alone)
  local hx, hy = self:rootHandle()
  if not hx then return false end
  if alone then
    self:setRootPos(x, y)
    self.drag = { kind = "rootAlone" }
    return true
  end
  -- a foot dummy right by it wins: the IK tool is for them
  if self.tool == "IK" and self:pickDummy(x, y, tol) then return false end
  local r = math.max(tol or 1, 1) * 1.6
  if (x - hx) ^ 2 + (y - hy) ^ 2 > r * r then return false end
  local gx, gy = M.Ik.offset(self.doc, self.mode == "Pose" and self.frame or nil)
  self.drag = { kind = "root", x0 = x, y0 = y, gx = gx, gy = gy, dx = 0, dy = 0 }
  return true
end

--- Puts the root's handle at (x, y) as it shows now (in Pose the offset of
--- this frame comes off: the place is the rest one; `rest` gives the rest
--- place itself), snapped and kept on the canvas. Nothing else moves.
--- nil, nil: back under the rig. One undo step per gesture.
function E:setRootPos(x, y, rest)
  local doc = self.doc
  if #doc.bones == 0 then return false end
  local pos
  if x and y then
    if self.mode == "Pose" and not rest then
      local gx, gy = M.Ik.offset(doc, self.frame)
      x, y = x - gx, y - gy
    end
    x = math.max(0, math.min(doc.canvas.w, snap(x)))
    y = math.max(0, math.min(doc.canvas.h, snap(y)))
    pos = { x = x, y = y }
  end
  local cur = doc.rootPos
  if (cur == nil and pos == nil) or (cur and pos and cur.x == pos.x and cur.y == pos.y) then return false end
  self:touch()
  doc.rootPos = pos
  self:changed()
  return true
end

function E:dragRoot(d, x, y)
  local dx, dy = snap(x - d.x0), snap(y - d.y0)
  if dx == d.dx and dy == d.dy then return false end
  self:touch()
  if self.mode == "Pose" then
    local Anim, R = M.Anim, M.Rig.ROOT
    Anim.setKey(self.doc.anim, R, "x", self.frame, d.gx + dx)
    Anim.setKey(self.doc.anim, R, "y", self.frame, d.gy + dy)
  else
    M.Rig.moveRig(self.doc, dx - d.dx, dy - d.dy)
  end
  d.dx, d.dy = dx, dy
  self:changed()
  return true
end

function E:up(x, y)
  local d = self.drag
  self.drag = nil
  if not d then return false end
  if d.kind == "new" then return self:finishNew(d) end
  if d.kind == "bulge" then return self:finishBulge(d) end
  return d.kind ~= nil
end

-- Select --------------------------------------------------------------

function E:downSelect(x, y, tol)
  local Rig = M.Rig
  local hit = self:resolveHit(Rig.pick(self.doc, self:world(), x, y, tol))
  if not hit and self:downGround(x, y, tol) then return true end
  if not hit then
    local had = self.sel ~= nil
    self.sel, self.selBulge = nil, nil
    return had
  end
  self.sel = hit.bone
  self.selBulge = nil
  local b = Rig.bone(self.doc, hit.bone)
  local loose = b.parent == 0 or not b.connected
  if hit.what == "tail" then
    self.drag = { kind = "tail", id = b.id }
  elseif loose then
    self.drag = { kind = "move", id = b.id, lx = x, ly = y, ox = 0, oy = 0 }
  else
    self.drag = { kind = "swing", id = b.id }
  end
  return true
end

function E:dragTail(d, x, y)
  self:touch()
  if not M.Rig.setTail(self.doc, d.id, snap(x), snap(y)) then return false end
  self:syncTwin(d.id)
  self:changed()
  return true
end

function E:dragSwing(d, x, y)
  local w = self:world()[d.id]
  local b = M.Rig.bone(self.doc, d.id)
  local l = dist(x, y, w.hx, w.hy)
  if l < 1e-6 then return false end
  self:touch()
  M.Rig.setTail(self.doc, d.id, w.hx + (x - w.hx) / l * b.len, w.hy + (y - w.hy) / l * b.len)
  self:syncTwin(d.id)
  self:changed()
  return true
end

function E:dragMove(d, x, y)
  -- whole snapped steps so a slow drag still lands on half pixels
  local tx, ty = snap(x - d.lx), snap(y - d.ly)
  local dx, dy = tx - d.ox, ty - d.oy
  if dx == 0 and dy == 0 then return false end
  self:touch()
  M.Rig.moveBone(self.doc, d.id, dx, dy)
  d.ox, d.oy = tx, ty
  self:syncTwin(d.id)
  self:changed()
  return true
end

-- Bone ----------------------------------------------------------------

function E:downBone(x, y, tol)
  local Rig = M.Rig
  local world = self:world()
  local hit = self:resolveHit(Rig.pick(self.doc, world, x, y, tol))
  local d = { kind = "new", parent = 0, connected = false }
  if hit and hit.what == "tail" then
    local w = world[hit.bone]
    d.parent, d.connected, d.hx, d.hy = hit.bone, true, w.tx, w.ty
  elseif hit and hit.what == "head" then
    local w = world[hit.bone]
    d.parent, d.hx, d.hy = hit.bone, w.hx, w.hy
  elseif hit then
    d.parent, d.hx, d.hy = hit.bone, snap(x), snap(y)
  else
    d.hx, d.hy = snap(x), snap(y)
  end
  d.tx, d.ty = d.hx, d.hy
  self.drag = d
  return true
end

function E:finishNew(d)
  local Rig = M.Rig
  if dist(d.hx, d.hy, d.tx, d.ty) < M.MIN_NEW then return true end
  self:touch()
  local doc = self.doc
  local parent = d.parent ~= 0 and Rig.bone(doc, d.parent) or nil
  local part = (parent and d.connected) and parent.part or 0
  local b = Rig.addBone(doc, d.hx, d.hy, d.tx, d.ty,
    { parent = d.parent, connected = d.connected, part = part })
  if not b then return true end
  self.sel = b.id
  if self:mirrorOn() and not (self:onAxis(d.hx) and self:onAxis(d.tx)) then
    local tp = self:twinParent(d.parent)
    local tparent = tp ~= 0 and Rig.bone(doc, tp) or nil
    local tpart = 0
    if parent and d.connected then tpart = tparent and tparent.part or part end
    local t = Rig.addBone(doc, self:mx(d.hx), d.hy, self:mx(d.tx), d.ty,
      { parent = tp, connected = d.connected, part = tpart,
        rHead = b.rHead, rTail = b.rTail })
    if t then
      b.twin, t.twin = t.id, b.id
      -- the templates' convention: _L is the character's left, which is
      -- the screen's right in a front view
      local right = (d.hx + d.tx) / 2 > doc.mirror.x
      local sb, st = right and "_L" or "_R", right and "_R" or "_L"
      b.name, t.name = b.name .. sb, b.name .. st
    end
  end
  self:changed()
  return true
end

-- Radius --------------------------------------------------------------

--- Every bone end within 1e-6 of (jx, jy): { { id, which }, ... }.
function E:jointsAt(jx, jy)
  local out = {}
  local world = self:world()
  for _, b in ipairs(self.doc.bones) do
    local w = world[b.id]
    if dist(w.hx, w.hy, jx, jy) < 1e-6 then out[#out + 1] = { b.id, "head" } end
    if dist(w.tx, w.ty, jx, jy) < 1e-6 then out[#out + 1] = { b.id, "tail" } end
  end
  return out
end

--- The joint to resize: the nearest bone end whose disc (plus `tol`)
--- contains the point.
function E:pickJoint(x, y, tol)
  local world = self:world()
  local best, bestD
  for _, b in ipairs(self.doc.bones) do
    local w = world[b.id]
    local dh, dt = dist(x, y, w.hx, w.hy), dist(x, y, w.tx, w.ty)
    if dh <= b.rHead + tol and (not bestD or dh < bestD) then
      best, bestD = { id = b.id, x = w.hx, y = w.hy }, dh
    end
    if dt <= b.rTail + tol and (not bestD or dt < bestD) then
      best, bestD = { id = b.id, x = w.tx, y = w.ty }, dt
    end
  end
  return best
end

--- A click that grabs nothing deselects, with every tool.
function E:deselect()
  local had = self.sel ~= nil or self.selBulge ~= nil
  self.sel, self.selBulge = nil, nil
  return had
end

function E:downRadius(x, y, tol)
  local j = self:pickJoint(x, y, tol)
  if not j then return self:deselect() end
  self.sel = j.id
  self.drag = { kind = "radius", joints = self:jointsAt(j.x, j.y), cx = j.x, cy = j.y }
  return true
end

function E:dragRadius(d, x, y)
  local r = max(M.Rig.MIN_RADIUS, snap(dist(x, y, d.cx, d.cy)))
  self:touch()
  for _, j in ipairs(d.joints) do
    M.Rig.setRadius(self.doc, j[1], j[2], r)
  end
  for _, j in ipairs(d.joints) do self:syncTwin(j[1]) end
  self:changed()
  return true
end

-- Bulge ---------------------------------------------------------------

--- The bone, side and position along it for a point on a skin edge:
--- { id, side, t, sd } with sd the distance from the axis on that side.
function E:bulgeTarget(x, y, tol)
  local world = self:world()
  local best, bestScore
  for _, b in ipairs(self.doc.bones) do
    local w = world[b.id]
    local c, s = (w.tx - w.hx) / b.len, (w.ty - w.hy) / b.len
    local qx, qy = x - w.hx, y - w.hy
    local along = qx * c + qy * s
    local t = along / b.len
    if t > 0 and t < 1 then
      local side = -qx * s + qy * c
      local name = side < 0 and "left" or "right"
      local list = name == "left" and b.bulgeL or b.bulgeR
      local edge = M.Skin.edgeOffset(b, t, name)
      local score = abs(abs(side) - edge)
      if score <= tol + 2 and (not bestScore or score < bestScore) then
        best, bestScore = { id = b.id, side = name, t = t, sd = abs(side), len = b.len }, score
      end
    end
  end
  return best
end

function E:downBulge(x, y, tol)
  local Rig = M.Rig
  local g = self:bulgeTarget(x, y, tol)
  if not g then return self:deselect() end
  local b = Rig.bone(self.doc, g.id)
  local list = g.side == "left" and b.bulgeL or b.bulgeR
  local idx
  for i, q in ipairs(list) do
    if abs(q[1] - g.t) * g.len <= tol + 1 then idx = i break end
  end
  self.sel = b.id
  self.drag = { kind = "bulge", id = b.id, side = g.side, i = idx }
  if not idx then
    -- a new handle starts flat: dragging outwards makes it grow
    self:touch()
    self.drag.i = M.Skin.addBulge(b, g.side, g.t, 0)
    self:changed()
  end
  self.selBulge = { bone = b.id, side = g.side, i = self.drag.i }
  return true
end

function E:dragBulge(d, x, y)
  local Rig = M.Rig
  local b = Rig.bone(self.doc, d.id)
  local w = self:world()[d.id]
  local c, s = (w.tx - w.hx) / b.len, (w.ty - w.hy) / b.len
  local qx, qy = x - w.hx, y - w.hy
  local t = (qx * c + qy * s) / b.len
  local side = -qx * s + qy * c
  local sd = d.side == "left" and -side or side
  t = min(M.Skin.BULGE_T_MAX, max(M.Skin.BULGE_T_MIN, t))
  -- the bulge that puts the drawn edge under the pointer (skin.edgeOffset)
  local _, a, lin = M.Skin.edgeOffset(b, t, d.side)
  local dd = snap(sd * a - lin)
  self:touch()
  d.i = M.Skin.setBulge(b, d.side, d.i, t, dd)
  self.selBulge = { bone = b.id, side = d.side, i = d.i }
  self:syncTwin(b.id)
  self:changed()
  return true
end

function E:finishBulge(d)
  local b = M.Rig.bone(self.doc, d.id)
  if not b then return true end
  local list = d.side == "left" and b.bulgeL or b.bulgeR
  local q = list[d.i]
  if q and abs(q[2]) < M.BULGE_DROP then
    self:touch()
    M.Skin.removeBulge(b, d.side, d.i)
    self:syncTwin(b.id)
    self.selBulge = nil
    self:changed()
  end
  return true
end

---------------------------------------
-- Pose mode: frames and keys --
---------------------------------------

--- What key commands act on: the selected bone and its IK controller,
--- or everything (nil).
function E:targets()
  local ids = self:selectedIds()
  if #ids == 0 then return nil end
  local t = {}
  for _, id in ipairs(ids) do
    t[id] = true
    local c = M.Ik.forBone(self.doc, id)
    if c then t[c.id] = true end
  end
  return t
end

--- The IK controllers key commands act on (same rule as targets()).
function E:targetCtrls()
  local ids = self:selectedIds()
  if #ids == 0 then return self.doc.ik end
  local out, seen = {}, {}
  for _, id in ipairs(ids) do
    local c = M.Ik.forBone(self.doc, id)
    if c and not seen[c] then
      seen[c] = true
      out[#out + 1] = c
    end
  end
  return out
end

--- Keys the dummies of controller `c` at this frame with (tx, ty, px, py),
--- canvas positions: the keys are relative to the rig root.
function E:keyDummies(c, tx, ty, px, py)
  local anim, f = self.doc.anim, self.frame
  local gx, gy = M.Ik.offset(self.doc, f)
  tx, px, ty, py = tx - gx, px - gx, ty - gy, py - gy
  M.Anim.setKey(anim, c.id, "tx", f, tx)
  M.Anim.setKey(anim, c.id, "ty", f, ty)
  M.Anim.setKey(anim, c.id, "px", f, px)
  M.Anim.setKey(anim, c.id, "py", f, py)
end

function E:setFrame(f)
  local n = self.doc.anim.frames
  f = floor(tonumber(f) or 1)
  if f < 1 then f = 1 elseif f > n then f = n end
  if f == self.frame then return false end
  self.frame = f
  self.drag = nil
  return true
end

--- Next / previous frame, wrapping (playback and the arrow keys).
function E:stepFrame(d)
  local n = self.doc.anim.frames
  return self:setFrame((self.frame - 1 + d) % n + 1) or n == 1
end

--- The channels K keys on a bone: rot, x / y when it is loose, and its
--- length factor once it has one (an untouched s stays out of the way).
local function channelsOf(b, anim)
  local out = { "rot" }
  if b.parent == 0 or not b.connected then out[2], out[3] = "x", "y" end
  if anim and M.Anim.track(anim, b.id, "s") then out[#out + 1] = "s" end
  return out
end

--- Keys the target bones at the current frame with what they show now.
function E:addKey()
  local doc, Anim = self.doc, M.Anim
  if #doc.bones == 0 then return false end
  local pose = M.Anim.sample(doc.anim, self.frame)
  local targets = self:targets()
  self:beginGesture()
  self:touch()
  for _, b in ipairs(doc.bones) do
    if not targets or targets[b.id] then
      local p = pose[b.id]
      for _, ch in ipairs(channelsOf(b, doc.anim)) do
        Anim.setKey(doc.anim, b.id, ch, self.frame, p and p[ch] or Anim.neutral(ch))
      end
    end
  end
  if not targets and M.Ref and doc.anim.ref then
    -- everything: the reference's transform too, once it is animated
    local anyRef = false
    for _, tr in ipairs(doc.anim.tracks) do if tr.bone == -1 then anyRef = true end end
    if anyRef then
      local tf = M.Ref.transform(doc.anim, self.frame)
      for _, ch in ipairs(M.Ref.CHANNELS) do
        Anim.setKey(doc.anim, -1, ch, self.frame, tf[ch], ch == "rf" and "Step" or nil)
      end
    end
  end
  if not targets then
    -- everything: the animated parts too
    for _, part in ipairs(doc.parts) do
      for _, ch in ipairs(Anim.PART_CHANNELS) do
        if Anim.track(doc.anim, part.id, ch) then
          Anim.setKey(doc.anim, part.id, ch, self.frame, self:getPartValue(part.id, ch), "Step")
        end
      end
    end
  end
  for _, c in ipairs(self:targetCtrls()) do
    self:keyDummies(c, M.Ik.dummies(doc, c, self.frame))
  end
  self:changed()
  return true
end

function E:deleteKeys()
  local anim = self.doc.anim
  if not M.Anim.hasKey(anim, self.frame, self:targets()) then return false end
  self:beginGesture()
  self:touch()
  M.Anim.removeKeys(anim, self.frame, self:targets())
  self:changed()
  return true
end

function E:moveKeys(from, to)
  local anim = self.doc.anim
  to = max(1, min(anim.frames, floor(tonumber(to) or from)))
  if from == to or not M.Anim.hasKey(anim, from, self:targets()) then return false end
  self:beginGesture()
  self:touch()
  M.Anim.moveKeys(anim, from, to, self:targets())
  self.frame = to
  self:changed()
  return true
end

--- The keys the Ease combo acts on: the picked ones in the dope sheet,
--- else those at this frame (of the targets).
function E:easeKeys()
  local anim = self.doc.anim
  local live, n = M.Anim.setLive(anim, self.keySel)
  if n > 0 then return M.Anim.keysIn(anim, live) end
  return M.Anim.keysAt(anim, self.frame, self:targets())
end

--- The ease of those keys, else the default.
function E:getEase()
  local keys = self:easeKeys()
  if keys[1] then return keys[1][3] end
  return self.doc.anim.ease
end

--- Sets the ease of those keys, or the default for new keys when there
--- are none.
function E:setEase(e)
  local anim = self.doc.anim
  local keys = self:easeKeys()
  if e == self:getEase() then
    local same = true
    for _, k in ipairs(keys) do if k[3] ~= e then same = false end end
    if same then return false end
  end
  self:touch("ease")
  if #keys == 0 then
    anim.ease = e
  else
    for _, k in ipairs(keys) do k[3] = e end
  end
  self:changed()
  return true
end

--- frames | frameMs | loop | pingpong. Fewer frames drop the keys past
--- the end. frameMs is "All frames": it also clears the frames' own times.
function E:setAnimProp(field, v)
  local doc = self.doc
  local anim = doc.anim
  -- the value it would really get: a clamped no-op leaves no undo step
  if field == "frames" then
    v = max(1, min(M.Anim.MAX_FRAMES, floor(tonumber(v) or anim.frames)))
  elseif field == "frameMs" then
    v = max(10, min(60000, floor(tonumber(v) or anim.frameMs)))
  elseif field == "loop" or field == "pingpong" then
    v = v == true
  else
    return false
  end
  if anim[field] == v and (field ~= "frameMs" or next(anim.durations) == nil) then return false end
  self:touch("anim:" .. field .. ":" .. doc.clipIndex)
  anim[field] = v
  if field == "frames" then
    M.Anim.prune(anim, function(id) return self:exists(id) end)
    for f in pairs(anim.durations) do
      if f > anim.frames then anim.durations[f] = nil end
    end
    if anim.ref then
      for f in pairs(anim.ref.frames) do
        if f > anim.frames then anim.ref.frames[f] = nil end
      end
    end
    if self.frame > anim.frames then self.frame = anim.frames end
  elseif field == "frameMs" then
    anim.durations = {}
  end
  self:changed()
  return true
end

--- How long frame `f` (default: this one) of the active clip lasts, ms.
function E:frameMs(f)
  local a = self.doc.anim
  return a.durations[f or self.frame] or a.frameMs
end

--- "This frame": frame `f`'s own duration (the clip's when equal).
function E:setFrameMs(ms, f)
  local a = self.doc.anim
  f = f or self.frame
  ms = max(10, min(60000, floor(tonumber(ms) or a.frameMs)))
  local own = ms ~= a.frameMs and ms or nil
  if a.durations[f] == own then return false end
  self:touch("dur:" .. self.doc.clipIndex .. ":" .. f)
  a.durations[f] = own
  self:changed()
  return true
end

--- Reverses the keys of the targets between `from` and `to` (default:
--- their first and last key): the motion plays backwards. One step.
function E:reverseKeys(from, to)
  local anim = self.doc.anim
  local targets = self:targets()
  local live, n = M.Anim.setLive(anim, self.keySel)
  local pick
  if not from and n > 0 then
    -- only the picked keys, mirrored as one block in time
    local owners
    from, to, owners = M.Anim.setRange(live)
    targets, pick = owners, live
  end
  if not from then
    local frames = M.Anim.keyFrames(anim, targets)
    from, to = frames[1], frames[#frames]
  end
  if not from or to <= from then return 0 end
  self:beginGesture()
  self:touch()
  local n = M.Anim.reverseKeys(anim, from, to, targets, pick)
  if pick then
    local moved = {}
    for id, r in pairs(pick) do
      for f in pairs(r) do M.Anim.setAdd(moved, id, from + to - f) end
    end
    self.keySel = moved
  end
  self:changed()
  return n
end

---------------------------------------
-- Dope sheet: picked keys --
---------------------------------------

--- How many picked keys still exist (undo may have taken some).
function E:keySelCount()
  local live, n = M.Anim.setLive(self.doc.anim, self.keySel)
  self.keySel = live
  return n
end

function E:clearKeySel()
  if next(self.keySel) == nil then return false end
  self.keySel = {}
  return true
end

--- Picks the key cell (id, f): "only" makes it the whole pick, "toggle"
--- adds or takes it out, "add" adds it. A cell without keys is ignored.
function E:pickKey(id, f, how)
  local anim = self.doc.anim
  if not M.Anim.hasKey(anim, f, { [id] = true }) then return false end
  if how == "toggle" then
    if M.Anim.setHas(self.keySel, id, f) then M.Anim.setRemove(self.keySel, id, f)
    else M.Anim.setAdd(self.keySel, id, f) end
  elseif how == "add" then
    M.Anim.setAdd(self.keySel, id, f)
  else
    self.keySel = {}
    M.Anim.setAdd(self.keySel, id, f)
  end
  return true
end

--- Picks every key of `ids` (a list) between frames f0 and f1. `add`
--- keeps what was picked.
function E:pickKeyBox(ids, f0, f1, add)
  if f1 < f0 then f0, f1 = f1, f0 end
  if not add then self.keySel = {} end
  local want = {}
  for _, id in ipairs(ids) do want[id] = true end
  for _, t in ipairs(self.doc.anim.tracks) do
    if want[t.bone] then
      for _, k in ipairs(t.keys) do
        if k[1] >= f0 and k[1] <= f1 then M.Anim.setAdd(self.keySel, t.bone, k[1]) end
      end
    end
  end
  return true
end

--- Moves the picked keys `delta` frames (held inside the clip). One step.
function E:moveSelKeys(delta)
  local anim = self.doc.anim
  local live, n = M.Anim.setLive(anim, self.keySel)
  if n == 0 then return false end
  local lo, hi = M.Anim.setRange(live)
  delta = max(1 - lo, min(anim.frames - hi, floor(tonumber(delta) or 0)))
  if delta == 0 then return false end
  self:beginGesture()
  self:touch()
  self.keySel = M.Anim.remapKeys(anim, live, function(f) return f + delta end)
  self:changed()
  return true
end

--- Scales the timing of the picked keys by `k` around frame `pivot`
--- (default their first frame), rounded to whole frames. Refused (false,
--- why) when a key would leave the clip or two of one row would merge:
--- scaling never deletes keys.
function E:scaleSelKeys(k, pivot)
  local anim = self.doc.anim
  local live, n = M.Anim.setLive(anim, self.keySel)
  k = tonumber(k)
  if n == 0 or not k or k <= 0 then return false end
  local lo = M.Anim.setRange(live)
  pivot = pivot or lo
  local function map(f)
    return floor(pivot + (f - pivot) * k + 0.5)
  end
  local same = true
  for _, r in pairs(live) do
    local used = {}
    for f in pairs(r) do
      local g = map(f)
      if g < 1 or g > anim.frames then return false, "out" end
      if used[g] then return false, "overlap" end
      used[g] = true
      if g ~= f then same = false end
    end
  end
  if same then return false end
  self:beginGesture()
  self:touch()
  self.keySel = M.Anim.remapKeys(anim, live, map)
  self:changed()
  return true
end

function E:deleteSelKeys()
  local anim = self.doc.anim
  local live, n = M.Anim.setLive(anim, self.keySel)
  if n == 0 then return false end
  self:beginGesture()
  self:touch()
  M.Anim.deleteKeySet(anim, live)
  self.keySel = {}
  self:changed()
  return true
end

--- Copies the picked keys. Returns how many.
function E:copyKeys()
  local live, n = M.Anim.setLive(self.doc.anim, self.keySel)
  if n == 0 then return 0 end
  self.keyClip = M.Anim.copyKeys(self.doc.anim, live)
  self.lastCopy = "keys"
  return n
end

--- Pastes the copied keys with the first one on this frame; they become
--- the pick. One step.
function E:pasteKeys()
  if not self.keyClip then return false end
  local any = false
  for _, it in ipairs(self.keyClip.items) do
    local f = self.frame + it[3]
    if f <= self.doc.anim.frames and self:exists(it[1]) then any = true break end
  end
  if not any then return false end
  self:beginGesture()
  self:touch()
  self.keySel = M.Anim.pasteKeys(self.doc.anim, self.keyClip, self.frame,
    function(id) return self:exists(id) end)
  self:changed()
  return next(self.keySel) ~= nil
end

---------------------------------------
-- Reference image (refimg.lua) --
---------------------------------------

--- The active clip's reference; `create` makes an empty one.
function E:ref(create)
  local a = self.doc.anim
  if not a.ref and create and M.Ref then
    local bd = M.Rig.bounds(self.doc)
    a.ref = M.Ref.new(bd.w, bd.h)
    a.ref.base.rx, a.ref.base.ry = bd.x + bd.w / 2, bd.y + bd.h / 2
  end
  return a.ref
end

--- The frame the reference shows: this one in Pose, the base in Rest.
function E:refFrame()
  return self.mode == "Pose" and self.frame or nil
end

--- Puts `buf` (a kit/buffer image) in as the reference: `target` "frame"
--- for this frame only, else the image of every frame. A first global
--- image is centred and fitted to the sprite's bounds. One undo step.
function E:setRefImage(buf, target)
  if not M.Ref or type(buf) ~= "table" or not buf.w or buf.w < 1 or buf.h < 1 then return false end
  -- a frame's own image only shows in Pose: in Rest it would be a step
  -- with nothing to see
  if target == "frame" and self.mode ~= "Pose" then return false end
  local doc = self.doc
  self:beginGesture()
  self:touch()
  local ref = self:ref(true)
  local fresh = ref.image == nil and next(ref.frames) == nil
  local key = M.Ref.store(doc, buf)
  if target == "frame" then
    ref.frames[self.frame] = key
  else
    ref.image = key
  end
  if fresh then
    -- the first picture: centred and fitted to the sprite
    local e = doc.images[key]
    local bd = M.Rig.bounds(doc)
    ref.base.rs = M.Ref.fitScale(e.w, e.h, bd.w, bd.h)
    ref.base.rx, ref.base.ry = bd.x + bd.w / 2, bd.y + bd.h / 2
  end
  ref.on = true
  self:changed()
  return true
end

--- This frame goes back to the global image.
function E:clearRefFrame()
  local ref = self:ref()
  if not ref or not ref.frames[self.frame] then return false end
  self:beginGesture()
  self:touch()
  ref.frames[self.frame] = nil
  self:changed()
  return true
end

--- Removes the clip's reference: images, frames and keys.
function E:removeRef()
  local a = self.doc.anim
  local any = a.ref ~= nil
  for _, tr in ipairs(a.tracks) do if tr.bone == -1 then any = true end end
  if not any then return false end
  self:beginGesture()
  self:touch()
  a.ref = nil
  local kept = {}
  for _, tr in ipairs(a.tracks) do
    if tr.bone ~= -1 then kept[#kept + 1] = tr end
  end
  a.tracks = kept
  self:changed()
  return true
end

function E:getRefValue(ch)
  if not M.Ref or not self.doc.anim.ref then return nil end
  return M.Ref.transform(self.doc.anim, self:refFrame())[ch]
end

--- Writes a channel without a new step (gestures and setRefValue).
function E:writeRef(ch, v)
  local ref = self:ref(true)
  v = M.Ref.clampValue(ch, v)
  if self.mode == "Pose" then
    M.Anim.setKey(self.doc.anim, -1, ch, self.frame, v, ch == "rf" and "Step" or nil)
  else
    ref.base[ch] = v
  end
end

--- A transform channel: the base in Rest, a key on this frame in Pose.
function E:setRefValue(ch, v)
  if not M.Ref or not M.Ref.IS_CHANNEL[ch] or not self.doc.anim.ref then return false end
  v = M.Ref.clampValue(ch, v)
  local keyed = M.Anim.hasKey(self.doc.anim, self.frame, { [-1] = true })
  if v == self:getRefValue(ch) and (self.mode ~= "Pose" or keyed) then return false end
  self:touch("ref:" .. ch .. ":" .. self.mode .. ":" .. self.frame)
  self:writeRef(ch, v)
  self:changed()
  return true
end

--- on | front (not animated).
function E:setRefProp(field, v)
  local ref = self:ref()
  if not ref or (field ~= "on" and field ~= "front") or ref[field] == (v == true) then return false end
  self:touch("ref:" .. field)
  ref[field] = v == true
  self:changed()
  return true
end

local function refAngle(dx, dy)
  if math.atan2 then return math.deg(math.atan2(dy, dx)) end
  return math.deg(math.atan(dy, dx))
end

--- The Ref tool: drag moves the reference, Shift+drag scales it, Alt+drag
--- turns it, around its centre.
function E:downRef(x, y)
  local ref = self:ref()
  if not M.Ref or not ref or not M.Ref.imageAt(self.doc.anim, self:refFrame()) then return false end
  local mods = self.downMods or {}
  local tf = M.Ref.transform(self.doc.anim, self:refFrame())
  self.drag = { kind = "ref", mode = mods.shift and "scale" or mods.alt and "rotate" or "move",
    x0 = x, y0 = y, tf = tf }
  return true
end

function E:dragRef(d, x, y)
  local tf = d.tf
  local want = {}
  if d.mode == "move" then
    want.rx, want.ry = tf.rx + (x - d.x0), tf.ry + (y - d.y0)
  elseif d.mode == "scale" then
    local r0 = math.sqrt((d.x0 - tf.rx) ^ 2 + (d.y0 - tf.ry) ^ 2)
    local r1 = math.sqrt((x - tf.rx) ^ 2 + (y - tf.ry) ^ 2)
    if r0 > 0.5 then want.rs = tf.rs * r1 / r0 end
  else
    local a0 = refAngle(d.x0 - tf.rx, d.y0 - tf.ry)
    local a1 = refAngle(x - tf.rx, y - tf.ry)
    want.rr = tf.rr + (a1 - a0 + 540) % 360 - 180
  end
  -- a step only when a value really changes (a click is not an edit)
  local now = M.Ref.transform(self.doc.anim, self:refFrame())
  local changed = false
  for ch, v in pairs(want) do
    if M.Ref.clampValue(ch, v) ~= now[ch] then changed = true end
  end
  if not changed then return false end
  self:touch()
  for _, ch in ipairs(M.Ref.CHANNELS) do
    if want[ch] then self:writeRef(ch, want[ch]) end
  end
  self:changed()
  return true
end

---------------------------------------
-- Clips --
---------------------------------------

--- Shows clip `i` (not an undo step: undo brings back whichever clip
--- was active when the undone change happened).
function E:setClip(i)
  local doc = self.doc
  if i == doc.clipIndex or not doc.clips[i] then return false end
  -- a drag on the old clip ends here (its step stays with that clip)
  self:beginGesture()
  M.Rig.setClip(doc, i)
  self.keySel = {}
  if self.frame > doc.anim.frames then self.frame = doc.anim.frames end
  self:changed()
  return true
end

--- A new empty clip after the active one, which becomes active.
function E:newClip(name)
  local doc = self.doc
  if #doc.clips >= M.Rig.MAX_CLIPS then return nil end
  self:beginGesture()
  self:touch()
  local a = M.Rig.newAnim(M.Rig.clipName(doc, name or "Animation"))
  a.frameMs, a.ease = doc.anim.frameMs, doc.anim.ease
  table.insert(doc.clips, doc.clipIndex + 1, a)
  M.Rig.setClip(doc, doc.clipIndex + 1)
  self.frame = 1
  self.keySel = {}
  self:changed()
  return a
end

--- A clip made outside the editor (import3d.lua) in one undo step.
--- `build(doc) -> anim | nil, why` may add bones and parts first (a new
--- rig). The clip goes after the active one and becomes active; an empty
--- rig's lone empty clip is replaced instead of kept.
function E:importClip(build)
  local doc = self.doc
  if #doc.clips >= M.Rig.MAX_CLIPS then return nil, "No room for another clip" end
  self:beginGesture()
  self:touch()
  local a, why = build(doc)
  if not a then
    M.Undo.undo(self.hist, doc)
    self.hist.redo = {}
    self:afterHistory()
    return nil, why
  end
  local only = doc.clips[1]
  if #doc.clips == 1 and #only.tracks == 0 and not only.ref then
    doc.clips[1] = a
    M.Rig.setClip(doc, 1)
  else
    table.insert(doc.clips, doc.clipIndex + 1, a)
    M.Rig.setClip(doc, doc.clipIndex + 1)
  end
  self.frame = 1
  self.keySel = {}
  self.sel, self.multi = nil, {}
  self:changed()
  return a
end

--- A copy of the active clip right after it, which becomes active.
function E:duplicateClip()
  local doc = self.doc
  if #doc.clips >= M.Rig.MAX_CLIPS then return nil end
  self:beginGesture()
  self:touch()
  local a = M.Rig.copy(doc.anim)
  a.name = M.Rig.clipName(doc, doc.anim.name .. " copy")
  table.insert(doc.clips, doc.clipIndex + 1, a)
  M.Rig.setClip(doc, doc.clipIndex + 1)
  self.keySel = {}
  self:changed()
  return a
end

--- Deletes the active clip (never the last one).
function E:deleteClip()
  local doc = self.doc
  if #doc.clips <= 1 then return false end
  self:beginGesture()
  self:touch()
  table.remove(doc.clips, doc.clipIndex)
  M.Rig.setClip(doc, min(doc.clipIndex, #doc.clips))
  self.keySel = {}
  if self.frame > doc.anim.frames then self.frame = doc.anim.frames end
  self.drag = nil
  self:changed()
  return true
end

--- Renames the active clip; names stay unique ("Run 2").
function E:renameClip(name)
  local doc = self.doc
  name = tostring(name or ""):gsub("^%s+", ""):gsub("%s+$", "")
  if name == "" or name == doc.anim.name then return false end
  local free = M.Rig.clipName(doc, name, doc.anim)
  if free == doc.anim.name then return false end
  self:touch("clip:name:" .. doc.clipIndex)
  doc.anim.name = free
  self:changed()
  return true
end

--- Moves the active clip one place up (-1) or down (+1) in the list (the
--- export order).
function E:moveClip(dir)
  local doc = self.doc
  local i, j = doc.clipIndex, doc.clipIndex + dir
  if j < 1 or j > #doc.clips then return false end
  self:beginGesture()
  self:touch()
  doc.clips[i], doc.clips[j] = doc.clips[j], doc.clips[i]
  M.Rig.setClip(doc, j)
  self:changed()
  return true
end

--- The selected bone's pose value on `ch` at this frame (panel).
function E:getPoseValue(ch)
  local b = self:selected()
  if not b then return nil end
  local p = M.Anim.sample(self.doc.anim, self.frame)[b.id]
  return p and p[ch] or M.Anim.neutral(ch)
end

function E:setPoseValue(ch, v)
  local b = self:selected()
  if not b or self.mode ~= "Pose" then return false end
  -- x / y only move a root or a loose bone (rig.lua solve ignores them else)
  if (ch == "x" or ch == "y") and b.parent ~= 0 and b.connected then return false end
  v = tonumber(v) or M.Anim.neutral(ch)
  if ch == "s" then v = max(M.Anim.MIN_SCALE, min(M.Anim.MAX_SCALE, v)) end
  -- the same key again (a slider let go where it was): no step
  local tr = M.Anim.track(self.doc.anim, b.id, ch)
  if tr then
    for _, k in ipairs(tr.keys) do
      if k[1] == self.frame and k[2] == v then return false end
    end
  end
  local carry = self.tool ~= "IK" and self:captureCarry({ b.id }) or nil
  self:touch("pose:" .. ch .. ":" .. b.id .. ":" .. self.frame)
  M.Anim.setKey(self.doc.anim, b.id, ch, self.frame, v)
  self:applyCarry(carry)
  self:changed()
  return true
end

---------------------------------------
-- Pose mode: gestures --
---------------------------------------

local function angleOf(dx, dy)
  if math.atan2 then return math.deg(math.atan2(dy, dx)) end
  return math.deg(math.atan(dy, dx))
end

function E:parentAngle(b, world)
  if b.parent == 0 then return 0 end
  return world[b.parent].angle
end

function E:downPose(x, y, tol)
  local Rig = M.Rig
  local world = self:world()
  local hit = self:resolveHit(Rig.pick(self.doc, world, x, y, tol))
  if not hit then
    if self:downGround(x, y, tol) then return true end
    local had = self.sel ~= nil
    self.sel = nil
    return had
  end
  self.sel = hit.bone
  -- a bone a live IK chain drives turns like FK, through its dummies; the
  -- body of a loose upper bone (a shoulder, a hip) moves like any loose
  -- bone, below
  local c = M.Ik.forBone(self.doc, hit.bone)
  if c and c.on and (c.upper == hit.bone or c.lower == hit.bone)
    and not self:movesLoose(c, hit) then
    return self:grabChain(c, hit.bone, x, y)
  end
  local b = Rig.bone(self.doc, hit.bone)
  local loose = b.parent == 0 or not b.connected
  -- the rest of the selection moves along (a bone a live chain turns
  -- is left to its IK)
  local others = {}
  local group = self.downIds or {}
  local keep = false
  for _, id in ipairs(group) do
    if id == b.id then keep = true end
  end
  if keep then
    local pose = M.Anim.sample(self.doc.anim, self.frame)
    local inGroup = {}
    for _, id in ipairs(group) do inGroup[id] = true end
    -- a bone under another of the group already turns / moves with it
    local function underGroup(id)
      local o = Rig.bone(self.doc, id)
      local p = o and o.parent or 0
      while p ~= 0 do
        if inGroup[p] then return true end
        local pb = Rig.bone(self.doc, p)
        p = pb and pb.parent or 0
      end
      return false
    end
    for _, id in ipairs(group) do
      local o = Rig.bone(self.doc, id)
      local oc = M.Ik.forBone(self.doc, id)
      local driven = oc and oc.on and (oc.upper == id or oc.lower == id)
      -- nor an ancestor of the dragged bone: turning it would turn the
      -- dragged one away from the pointer
      local above = false
      local p = b.parent
      while p ~= 0 do
        if p == id then above = true break end
        local pb = Rig.bone(self.doc, p)
        p = pb and pb.parent or 0
      end
      if id ~= b.id and o and not driven and not underGroup(id) and not above then
        local p = pose[id]
        others[#others + 1] = { id = id, loose = o.parent == 0 or not o.connected,
          vx = p and p.x or 0, vy = p and p.y or 0 }
      end
    end
  end
  if hit.what ~= "tail" and loose then
    local p = M.Anim.sample(self.doc.anim, self.frame)[b.id]
    self.drag = { kind = "trans", id = b.id, x0 = x, y0 = y,
      vx = p and p.x or 0, vy = p and p.y or 0, others = others }
  else
    self.drag = { kind = "rot", id = b.id, others = others }
  end
  -- Pose is FK: live chains below ride along (with IK they stay planted),
  -- and so does the chain of a loose upper bone that is moved
  if self.tool ~= "IK" then
    local ids = { b.id }
    for _, o in ipairs(others) do ids[#ids + 1] = o.id end
    self.drag.carry = self:captureCarry(ids, self.drag.kind == "trans")
  end
  return true
end

--- Keys the rotation of bone `id` at this frame, `v` unwrapped near the
--- value it shows now so the key never spins the long way round.
function E:keyRot(id, v)
  local p = M.Anim.sample(self.doc.anim, self.frame)[id]
  local cur = p and p.rot or 0
  M.Anim.setKey(self.doc.anim, id, "rot", self.frame, M.Anim.unwrap(cur, v))
end

function E:dragRot(d, x, y)
  local b = M.Rig.bone(self.doc, d.id)
  local world = self:world()
  local w = world[d.id]
  if (x - w.hx) ^ 2 + (y - w.hy) ^ 2 < 1e-6 then return false end
  local target = angleOf(x - w.hx, y - w.hy)
  self:touch()
  local before = M.Anim.sample(self.doc.anim, self.frame)
  local cur = before[d.id] and before[d.id].rot or 0
  self:keyWorldAngle(b, target, world)
  -- the same turn for every other selected bone, each around its head
  local after = M.Anim.sample(self.doc.anim, self.frame)[d.id].rot
  local delta = after - cur
  if delta ~= 0 then
    for _, o in ipairs(d.others or {}) do
      local p = before[o.id]
      self:keyRot(o.id, (p and p.rot or 0) + delta)
    end
  end
  self:applyCarry(d.carry)
  self:changed()
  return true
end

function E:dragTrans(d, x, y)
  local b = M.Rig.bone(self.doc, d.id)
  local world = self:world()
  local dx, dy = snap(x - d.x0), snap(y - d.y0)
  -- the delta lives in the parent's frame (a root: the canvas)
  local a = math.rad(self:parentAngle(b, world))
  local c, s = math.cos(a), math.sin(a)
  local lx, ly = dx * c + dy * s, -dx * s + dy * c
  self:touch()
  M.Anim.setKey(self.doc.anim, d.id, "x", self.frame, d.vx + lx)
  M.Anim.setKey(self.doc.anim, d.id, "y", self.frame, d.vy + ly)
  -- every other loose selected bone moves by the same canvas delta, in
  -- its own parent's frame
  for _, o in ipairs(d.others or {}) do
    if o.loose then
      local ob = M.Rig.bone(self.doc, o.id)
      local oa = math.rad(self:parentAngle(ob, world))
      local oc, os = math.cos(oa), math.sin(oa)
      M.Anim.setKey(self.doc.anim, o.id, "x", self.frame, o.vx + dx * oc + dy * os)
      M.Anim.setKey(self.doc.anim, o.id, "y", self.frame, o.vy - dx * os + dy * oc)
    end
  end
  self:applyCarry(d.carry)
  self:changed()
  if d.pins then self:applyPins(d.pins) end
  return true
end

--- The limbs under bone `id`: a branch bone (not connected), its
--- connected child and a third connected bone at the end (a hand, a
--- foot). Two bones without a third (a tail) are not a limb.
--- { { upper, lower, tip }, ... } in rig order.
function E:limbsUnder(id)
  local Rig = M.Rig
  local doc = self.doc
  local inside = { [id] = true }
  local out = {}
  for _, b in ipairs(doc.bones) do
    if inside[b.parent] then inside[b.id] = true end
    if inside[b.id] and b.parent ~= 0 and not b.connected then
      local lower
      for _, c in ipairs(Rig.children(doc, b.id)) do
        if c.connected then lower = c break end
      end
      local tip
      if lower then
        for _, c in ipairs(Rig.children(doc, lower.id)) do
          if c.connected then tip = c break end
        end
      end
      if tip then out[#out + 1] = { upper = b, lower = lower, tip = tip } end
    end
  end
  return out
end

--- Where every limb under `id` ends now: what applyPins holds still.
function E:pinsFor(id)
  local world = self:world()
  local pins = {}
  for _, l in ipairs(self:limbsUnder(id)) do
    local live = M.Ik.forBone(self.doc, l.lower.id)
    if live and live.on then goto continue end
    local wl, wu, wt = world[l.lower.id], world[l.upper.id], world[l.tip.id]
    pins[#pins + 1] = {
      upper = l.upper.id, lower = l.lower.id, tip = l.tip.id,
      x = wl.tx, y = wl.ty, tipAngle = wt.angle,
      bend = M.Anim.bendOf(wu.angle, wl.angle),
    }
    ::continue::
  end
  return pins
end

--- Re-solves every pinned limb so its end stays on its pin and the hand
--- or foot keeps its angle in the world. Keys like any IK drag.
function E:applyPins(pins)
  local Rig, Anim = M.Rig, M.Anim
  local world = self:world()
  for _, p in ipairs(pins) do
    local a = Rig.bone(self.doc, p.upper)
    local b = Rig.bone(self.doc, p.lower)
    local c = Rig.bone(self.doc, p.tip)
    if a and b and c then
      local wa = world[a.id]
      local a1, a2 = Anim.solve2(wa.hx, wa.hy, a.len, b.len, p.x, p.y, p.bend)
      self:keyRot(a.id, a1 - self:parentAngle(a, world) - a.angle)
      self:keyRot(b.id, a2 - a1 - b.angle)
      self:keyRot(c.id, p.tipAngle - a2 - c.angle)
    end
  end
  self:changed()
end

--- IK grabs the END of a chain: a tail whose bone hangs, connected, from
--- a parent. Anything else does what the Pose tool does (turn a bone,
--- move a root or loose bone), so the root can be moved without
--- switching tools.
function E:downIK(x, y, tol)
  local Rig = M.Rig
  local world = self:world()
  local dummy = self:pickDummy(x, y, tol)
  if dummy then return self:grabDummy(dummy.c, dummy.which, x, y) end
  local hit = self:resolveHit(Rig.pick(self.doc, world, x, y, tol))
  local live = hit and M.Ik.forBone(self.doc, hit.bone)
  if live and not live.on then live = nil end
  -- a bone of a live chain: the hand or foot follows the pointer (the
  -- body of a loose upper bone moves it, the hand staying put)
  if live and (live.upper == hit.bone or live.lower == hit.bone) and not self:movesLoose(live, hit) then
    self:grabDummy(live, "t", x, y)
    self.sel = hit.bone
    return true
  end
  if hit and hit.what == "tail" and not live then
    local b = Rig.bone(self.doc, hit.bone)
    if b.parent ~= 0 and b.connected then
      local a = Rig.bone(self.doc, b.parent)
      self.sel = b.id
      self.drag = { kind = "ik", lower = b.id, upper = a.id,
        bend = M.Anim.bendOf(world[a.id].angle, world[b.id].angle) }
      return true
    end
  end
  local did = self:downPose(x, y, tol)
  local d = self.drag
  if d and d.kind == "trans" and self.pinLimbs then d.pins = self:pinsFor(d.id) end
  return did
end

--- The Pose tool on a bone of a live chain: it turns round its own head
--- like any FK bone (the upper one round the shoulder / hip, carrying the
--- whole limb; the lower one round the elbow / knee). What is keyed is
--- the chain's dummies (target and pole where that pose puts them) and,
--- for a locked hand or foot, its own turn, so it goes along as in FK.
function E:grabChain(c, boneId, x, y)
  local world = self:world()
  local wa, wb = world[c.upper], world[c.lower]
  local tx, ty, px, py = M.Ik.dummies(self.doc, c, self.frame)
  local upper = boneId == c.upper
  local cx, cy = upper and wa.hx or wb.hx, upper and wa.hy or wb.hy
  local tipRot
  if c.tip ~= 0 and c.lockTip then
    local p = M.Anim.sample(self.doc.anim, self.frame)[c.tip]
    tipRot = p and p.rot or 0
  end
  self.drag = { kind = "chain", ctrl = c.id, upper = upper, cx = cx, cy = cy,
    a0 = angleOf(x - cx, y - cy), tx = tx, ty = ty, px = px, py = py,
    bx = wa.hx, by = wa.hy, ex = wb.hx, ey = wb.hy, tipRot = tipRot, da = 0 }
  return true
end

function E:dragChain(d, x, y)
  local c = M.Ik.find(self.doc, d.ctrl)
  if not c then return false end
  local da = angleOf(x - d.cx, y - d.cy) - d.a0
  if math.abs(da - d.da) < 1e-9 then return false end
  local r = math.rad(da)
  local cs, sn = math.cos(r), math.sin(r)
  local function turn(px, py)
    local vx, vy = px - d.cx, py - d.cy
    return d.cx + vx * cs - vy * sn, d.cy + vx * sn + vy * cs
  end
  local tx, ty = turn(d.tx, d.ty)
  local ex, ey = d.ex, d.ey
  if d.upper then ex, ey = turn(d.ex, d.ey) end
  -- the pole out from the elbow / knee on the side it bends to
  local px, py
  local mx, my = (d.bx + tx) / 2, (d.by + ty) / 2
  local vx, vy = ex - mx, ey - my
  local l = math.sqrt(vx * vx + vy * vy)
  local a = M.Rig.bone(self.doc, c.upper)
  if l < 1e-6 then
    px, py = d.px, d.py
    if d.upper then px, py = turn(d.px, d.py) end
  else
    local k = M.Ik.POLE_DIST * a.len / l
    px, py = ex + vx * k, ey + vy * k
  end
  self:touch()
  self:keyDummies(c, tx, ty, px, py)
  if d.tipRot then self:keyRot(c.tip, d.tipRot + da) end
  d.da = da
  self:changed()
  return true
end

function E:dragIK(d, x, y)
  local Rig = M.Rig
  local world = self:world()
  local a, b = Rig.bone(self.doc, d.upper), Rig.bone(self.doc, d.lower)
  local wa = world[a.id]
  local a1, a2 = M.Anim.solve2(wa.hx, wa.hy, a.len, b.len, x, y, d.bend)
  self:touch()
  self:keyRot(a.id, a1 - self:parentAngle(a, world) - a.angle)
  self:keyRot(b.id, a2 - a1 - b.angle)
  self:changed()
  return true
end

---------------------------------------
-- Quality of life: poses, frames, rig --
---------------------------------------

local function isLoose(b)
  return b.parent == 0 or not b.connected
end

--- The targets as a list in rig order (the selected bone, or all).
function E:targetBones()
  local out = {}
  local t = self:targets()
  for _, b in ipairs(self.doc.bones) do
    if not t or t[b.id] then out[#out + 1] = b end
  end
  return out
end

--- Copies the pose shown at this frame (selected bone, or all) into the
--- editor's clipboard. Returns how many bones went in.
function E:copyPose()
  local pose = M.Anim.sample(self.doc.anim, self.frame)
  local clip = {}
  for _, b in ipairs(self:targetBones()) do
    local p = pose[b.id]
    clip[#clip + 1] = { bone = b.id, rot = p and p.rot or 0, x = p and p.x or 0, y = p and p.y or 0,
      s = p and p.s or 1 }
  end
  for _, c in ipairs(self:targetCtrls()) do
    local tx, ty, px, py = M.Ik.dummies(self.doc, c, self.frame)
    clip[#clip + 1] = { ctrl = c.id, tx = tx, ty = ty, px = px, py = py }
  end
  if #clip == 0 then return 0 end
  self.clip = clip
  self.lastCopy = "pose"
  return #clip
end

--- Pastes the clipboard as keys at this frame. `mirrored`: each value goes
--- to the bone's twin (or to the bone itself on the axis) reflected, which
--- turns a left step into a right step. One undo step. Returns how many
--- bones got keys.
function E:pastePose(mirrored)
  return self:pasteClip(self.clip, mirrored)
end

--- The twin of group `p` by name (`arm_L` <-> `arm_R`), or `p`.
function E:twinPart(p)
  local base, side = p.name:match("^(.-)_([LR])$")
  if not base then return p end
  local want = base .. "_" .. (side == "L" and "R" or "L")
  for _, q in ipairs(self.doc.parts) do
    if q.name == want then return q end
  end
  return p
end

--- How "mirrored" (paste, apply a pose) works on this rig. A front view
--- links its sides as twins: a value reflects across the mirror axis onto
--- the twin. A side view has no twins, both sides drawn (Profile): it swaps
--- the sides instead, what the `_L` limb does going to the `_R` one as it
--- is, so the character keeps facing the same way.
function E:sideSwap()
  for _, b in ipairs(self.doc.bones) do
    if b.twin ~= 0 then return false end
  end
  return true
end

--- The bone a mirrored value of `b` lands on: its twin, or in a side view
--- the bone of the same name on the other side; else `b` itself.
function E:mirrorBone(b)
  if b.twin ~= 0 then return M.Rig.bone(self.doc, b.twin) or b end
  if not self:sideSwap() then return b end
  local base, side = b.name:match("^(.-)_([LR])$")
  if not base then return b end
  local want = base .. "_" .. (side == "L" and "R" or "L")
  for _, q in ipairs(self.doc.bones) do
    if q.name == want then return q end
  end
  return b
end

--- pastePose's work on any clipboard-shaped list: { bone, rot, x, y, s }
--- and { ctrl, tx, ty, px, py } (canvas positions). `parts` = { { id, z,
--- vis } } keys the groups too (to their twins when mirrored).
function E:pasteClip(clip, mirrored, parts)
  if not clip or #clip == 0 then return 0 end
  local Rig, Anim = M.Rig, M.Anim
  local doc = self.doc
  local writes, dummies = {}, {}
  -- the dummies are canvas positions of a rig the root may have moved:
  -- they reflect across the axis where the root puts it at this frame
  local ax = doc.mirror.x + (M.Ik.offset(doc, self.frame))
  -- a side view swaps its sides instead of reflecting them
  local swap = mirrored and self:sideSwap()
  local restWorld = swap and Rig.solve(doc)
  for _, c in ipairs(clip) do
    local src = c.bone and Rig.bone(doc, c.bone)
    local ctrl = c.ctrl and M.Ik.find(doc, c.ctrl)
    if ctrl then
      -- a mirrored dummy goes to the twin limb's controller, x reflected
      local dst = ctrl
      if mirrored then
        local up = Rig.bone(doc, ctrl.upper)
        if up then dst = M.Ik.forBone(doc, self:mirrorBone(up).id) or ctrl end
      end
      if swap then
        -- the other limb's dummies, as far from its rest ones as these were
        local stx, sty, spx, spy = M.Ik.restDummies(doc, ctrl, restWorld)
        local dtx, dty, dpx, dpy = M.Ik.restDummies(doc, dst, restWorld)
        dummies[#dummies + 1] = { c = dst, c.tx + dtx - stx, c.ty + dty - sty,
          c.px + dpx - spx, c.py + dpy - spy }
      else
        local mx = function(v) return mirrored and (2 * ax - v) or v end
        dummies[#dummies + 1] = { c = dst, mx(c.tx), c.ty, mx(c.px), c.py }
      end
    end
    if src then
      local dst = mirrored and self:mirrorBone(src) or src
      local root = dst.parent == 0
      local w = { bone = dst, rot = c.rot, x = c.x, y = c.y, s = c.s or 1 }
      if mirrored and not swap then
        w.rot = Anim.mirrorValue("rot", c.rot, root)
        w.x = Anim.mirrorValue("x", c.x, root)
        w.y = Anim.mirrorValue("y", c.y, root)
      end
      writes[#writes + 1] = w
    end
  end
  if #writes == 0 and #dummies == 0 then return 0, 0 end
  self:beginGesture()
  self:touch()
  for _, e in ipairs(parts or {}) do
    local p = Rig.part(doc, e.id)
    if p then
      -- a side view keeps its depth: the near limb's group stays over
      if mirrored and not swap then p = self:twinPart(p) end
      if e.z then Anim.setKey(doc.anim, p.id, "z", self.frame, e.z) end
      if e.vis then Anim.setKey(doc.anim, p.id, "vis", self.frame, e.vis) end
    end
  end
  for _, w in ipairs(writes) do
    Anim.setKey(doc.anim, w.bone.id, "rot", self.frame, w.rot)
    if isLoose(w.bone) then
      Anim.setKey(doc.anim, w.bone.id, "x", self.frame, w.x)
      Anim.setKey(doc.anim, w.bone.id, "y", self.frame, w.y)
    end
    if w.s ~= 1 or Anim.track(doc.anim, w.bone.id, "s") then
      Anim.setKey(doc.anim, w.bone.id, "s", self.frame, w.s)
    end
  end
  for _, d in ipairs(dummies) do self:keyDummies(d.c, d[1], d[2], d[3], d[4]) end
  self:changed()
  return #writes, #dummies
end

---------------------------------------
-- The pose library (doc.poses, poses.lua) --
---------------------------------------

--- This frame as a new pose of the library, named `name` (or "pose N").
--- One undo step; nil when the rig is empty or the library is full.
function E:addPose(name)
  local P = M.Poses
  if #self.doc.bones == 0 or #self.doc.poses >= M.Rig.MAX_POSES then return nil end
  name = (name and name ~= "") and name or P.freeName(self.doc)
  local pose = P.capture(self.doc, self.frame, P.freeName(self.doc, name))
  self:beginGesture()
  self:touch()
  P.add(self.doc, pose)
  self:changed()
  return pose
end

--- Pose `id` takes what this frame shows now (same id and name).
function E:updatePose(id)
  local P = M.Poses
  local old = P.find(self.doc, id)
  if not old then return false end
  local pose = P.capture(self.doc, self.frame, old.name)
  self:beginGesture()
  self:touch()
  old = P.find(self.doc, id) -- the snapshot keeps the table, but be safe
  old.bones, old.ik, old.parts = pose.bones, pose.ik, pose.parts
  self:changed()
  return true
end

function E:renamePose(id, name)
  local P = M.Poses
  local pose = P.find(self.doc, id)
  name = tostring(name or ""):gsub("^%s+", ""):gsub("%s+$", ""):sub(1, M.Rig.MAX_NAME)
  if not pose or name == "" or name == pose.name then return false end
  self:beginGesture()
  self:touch()
  P.find(self.doc, id).name = P.freeName(self.doc, name)
  self:changed()
  return true
end

function E:removePose(id)
  if not M.Poses.find(self.doc, id) then return false end
  self:beginGesture()
  self:touch()
  M.Poses.remove(self.doc, id)
  self:changed()
  return true
end

--- Moves pose `id` one place (`delta` -1 / 1) in the library.
function E:movePose(id, delta)
  local _, i = M.Poses.find(self.doc, id)
  local j = i and i + delta
  if not j or j < 1 or j > #self.doc.poses then return false end
  self:beginGesture()
  self:touch()
  local list = self.doc.poses
  list[i], list[j] = list[j], list[i]
  self:changed()
  return true
end

--- Keys pose `id` at this frame, like a paste: `opts.mirrored` puts each
--- value on the twin, `opts.selected` only keys the selected bones (and
--- their chains) when there are some. One undo step; returns how many
--- bones got keys.
function E:applyPose(id, opts)
  opts = opts or {}
  local pose = M.Poses.find(self.doc, id)
  if not pose then return 0, 0 end
  local only = opts.selected and self:targets() or nil
  local gx, gy = M.Ik.offset(self.doc, self.frame)
  local driven = M.Poses.driven(self.doc)
  if opts.mirrored then
    -- a value lands on the twin: whether a live chain turns it is the
    -- twin's business (the left arm may have IK and the right one not)
    local at = {}
    for _, b in ipairs(self.doc.bones) do
      at[b.id] = driven[self:mirrorBone(b).id]
    end
    driven = at
  end
  -- a chain added after the pose was kept: its dummies (and a locked
  -- hand's key) where the pose's FK shape puts them
  local missing = M.Poses.missingDummies(self.doc, pose)
  local tipRot = M.Poses.tipRots(missing, driven)
  local clip = {}
  for _, e in ipairs(pose.bones) do
    if not only or only[e.id] then
      clip[#clip + 1] = { bone = e.id, rot = tipRot[e.id] or M.Poses.rotOf(e, driven), x = e.x, y = e.y,
        s = M.Poses.sOf(e, driven) }
    end
  end
  local dummies = {}
  for _, e in ipairs(pose.ik) do dummies[#dummies + 1] = e end
  for _, e in ipairs(missing) do dummies[#dummies + 1] = e end
  for _, e in ipairs(dummies) do
    if not only or only[e.id] then
      clip[#clip + 1] = { ctrl = e.id, tx = e.tx + gx, ty = e.ty + gy, px = e.px + gx, py = e.py + gy }
    end
  end
  return self:pasteClip(clip, opts.mirrored, not only and pose.parts or nil)
end

--- Puts the targets back to their rest pose at this frame, as keys:
--- which = "rot", "offset" or nil (both). Returns how many bones changed.
function E:resetPose(which)
  local Anim = M.Anim
  local list = {}
  for _, b in ipairs(self:targetBones()) do
    -- an offset reset only means something on a root or loose bone
    if which ~= "offset" or isLoose(b) then list[#list + 1] = b end
  end
  -- the dummies of a live chain go back to where the rest pose has them
  local ctrls = which ~= "offset" and self:targetCtrls() or {}
  if #list == 0 and #ctrls == 0 then return 0 end
  self:beginGesture()
  self:touch()
  for _, c in ipairs(ctrls) do self:keyDummies(c, M.Ik.restDummies(self.doc, c)) end
  local n = 0
  for _, b in ipairs(list) do
    if which ~= "offset" then
      Anim.setKey(self.doc.anim, b.id, "rot", self.frame, 0)
      n = n + 1
    end
    if which == nil and Anim.track(self.doc.anim, b.id, "s") then
      Anim.setKey(self.doc.anim, b.id, "s", self.frame, 1)
    end
    if which ~= "rot" and isLoose(b) then
      Anim.setKey(self.doc.anim, b.id, "x", self.frame, 0)
      Anim.setKey(self.doc.anim, b.id, "y", self.frame, 0)
      if which == "offset" then n = n + 1 end
    end
  end
  self:changed()
  return n
end

--- Keys or deletes for EVERY bone, whatever is selected.
function E:addKeyAll()
  local sel = self.sel
  self.sel = nil
  local ok = self:addKey()
  self.sel = sel
  return ok
end

function E:deleteKeysAll()
  local sel = self.sel
  self.sel = nil
  local ok = self:deleteKeys()
  self.sel = sel
  return ok
end

--- Jumps to the previous (-1) or next (+1) frame holding a key of the
--- targets. False when there is none that way.
function E:jumpKey(dir)
  local frames = M.Anim.keyFrames(self.doc.anim, self:targets())
  local pick
  if dir > 0 then
    for _, f in ipairs(frames) do
      if f > self.frame then pick = f break end
    end
  else
    for i = #frames, 1, -1 do
      if frames[i] < self.frame then pick = frames[i] break end
    end
  end
  if not pick and self.doc.anim.loop and #frames > 0 then
    -- looping: past the last key comes the first one, like stepFrame
    pick = dir > 0 and frames[1] or frames[#frames]
    if pick == self.frame then pick = nil end
  end
  if not pick then return false end
  return self:setFrame(pick)
end

--- A new empty frame right after the current one (keys after it move one
--- frame later) and the current frame moves onto it.
function E:insertFrame()
  local anim = self.doc.anim
  if anim.frames >= M.Anim.MAX_FRAMES then return false end
  self:beginGesture()
  self:touch()
  anim.frames = anim.frames + 1
  M.Anim.shiftKeys(anim, self.frame, 1)
  local d = anim.durations
  for f = anim.frames - 1, self.frame + 1, -1 do d[f + 1] = d[f] end
  d[self.frame + 1] = nil
  local rf = anim.ref and anim.ref.frames
  if rf then
    for f = anim.frames - 1, self.frame + 1, -1 do rf[f + 1] = rf[f] end
    rf[self.frame + 1] = nil
  end
  self.frame = self.frame + 1
  self:changed()
  return true
end

--- Removes the current frame: its keys go and the later ones move back.
function E:deleteFrame()
  local anim = self.doc.anim
  if anim.frames <= 1 then return false end
  self:beginGesture()
  self:touch()
  M.Anim.removeKeys(anim, self.frame)
  -- every key after the deleted frame moves back onto it and beyond
  M.Anim.shiftKeys(anim, self.frame - 1, -1)
  local d = anim.durations
  for f = self.frame, anim.frames - 1 do d[f] = d[f + 1] end
  d[anim.frames] = nil
  local rf = anim.ref and anim.ref.frames
  if rf then
    for f = self.frame, anim.frames - 1 do rf[f] = rf[f + 1] end
    rf[anim.frames] = nil
  end
  anim.frames = anim.frames - 1
  if self.frame > anim.frames then self.frame = anim.frames end
  self:changed()
  return true
end

--- An empty rig of the same canvas (one undo step).
function E:newRig()
  local bd = M.Rig.bounds(self.doc)
  return self:loadDoc(M.Rig.placeLike(M.Rig.new(bd.w, bd.h), self.doc))
end

--- A canvas of w x h around the sprite (one undo step): room to work
--- past the sprite's edges. The sprite stays in its middle and the rig
--- on the sprite. False when nothing changes.
function E:setCanvasSize(w, h)
  local doc = self.doc
  local bd = M.Rig.bounds(doc)
  w = math.max(bd.w, math.min(M.Rig.MAX_CANVAS, math.floor(tonumber(w) or doc.canvas.w)))
  h = math.max(bd.h, math.min(M.Rig.MAX_CANVAS, math.floor(tonumber(h) or doc.canvas.h)))
  if w == doc.canvas.w and h == doc.canvas.h then return false end
  self:beginGesture()
  self:touch()
  M.Rig.setCanvas(doc, w, h)
  self:changed()
  return true
end

--- Pairs up an existing rig as mirror twins across doc.mirror.x: a pair
--- must REFLECT (head and tail within `tol` px, or `nameTol` px when the
--- names already say so: base_L with base_R), neither bone may lie on the
--- axis, and each bone takes its closest candidate. The pairs are then
--- reflected exactly in this same undo step (so a near-mirror snaps now,
--- visibly, not at some later edit) and the mirror turns on. Returns the
--- number of new pairs.
function E:symmetrize(tol, nameTol)
  tol, nameTol = tol or 0.75, nameTol or 3
  local Rig = M.Rig
  local doc = self.doc
  local world = Rig.solve(doc)
  local ax = doc.mirror.x
  local function onAxis(b)
    local w = world[b.id]
    return math.abs(w.hx - ax) <= tol and math.abs(w.tx - ax) <= tol
  end
  --- How far `o` is from being `b` reflected (largest coordinate error).
  local function gap(b, o)
    local wb, wo = world[b.id], world[o.id]
    return math.max(math.abs(wo.hx - (2 * ax - wb.hx)), math.abs(wo.hy - wb.hy),
      math.abs(wo.tx - (2 * ax - wb.tx)), math.abs(wo.ty - wb.ty))
  end
  local free = {}
  for _, b in ipairs(doc.bones) do
    if b.twin == 0 and not onAxis(b) then free[#free + 1] = b end
  end
  local pairs_, taken = {}, {}
  local byName = {}
  for _, b in ipairs(free) do byName[b.name] = b end
  for _, b in ipairs(free) do
    local base, side = b.name:match("^(.*)_([LR])$")
    if base and side == "L" and not taken[b.id] then
      local o = byName[base .. "_R"]
      if o and not taken[o.id] and gap(b, o) <= nameTol then
        pairs_[#pairs_ + 1] = { b, o }
        taken[b.id], taken[o.id] = true, true
      end
    end
  end
  for _, b in ipairs(free) do
    if not taken[b.id] then
      local best, bestGap
      for _, o in ipairs(free) do
        if o ~= b and not taken[o.id] then
          local g = gap(b, o)
          if g <= tol and (not bestGap or g < bestGap) then best, bestGap = o, g end
        end
      end
      if best then
        pairs_[#pairs_ + 1] = { b, best }
        taken[b.id], taken[best.id] = true, true
      end
    end
  end
  if #pairs_ == 0 then return 0 end
  self:beginGesture()
  self:touch()
  for _, p in ipairs(pairs_) do p[1].twin, p[2].twin = p[2].id, p[1].id end
  doc.mirror.on = true
  self:syncTwin(nil)
  self:changed()
  return #pairs_
end

--- Walks the hierarchy from the selection: "parent", "child" (the first
--- one), or with nothing selected, the first root.
function E:selectRelative(which)
  local Rig = M.Rig
  local b = self:selected()
  if not b then
    for _, x in ipairs(self.doc.bones) do
      if x.parent == 0 then self.sel = x.id return true end
    end
    return false
  end
  if which == "parent" then
    if b.parent == 0 then return false end
    self.sel = b.parent
  else
    local kids = Rig.children(self.doc, b.id)
    if #kids == 0 then return false end
    self.sel = kids[1].id
  end
  self.selBulge = nil
  return true
end

---------------------------------------
-- IK controllers: dummies, ground, settings --
---------------------------------------

--- The dummy under (x, y): { c, which = "t" | "p" } or nil. Targets win
--- over poles on a tie.
function E:pickDummy(x, y, tol)
  if self.mode ~= "Pose" or #self.doc.ik == 0 then return nil end
  local best, bestD
  local r = (tol or 1) * 1.6
  for _, h in ipairs(M.Ik.handles(self.doc, self.frame, self:world())) do
    if h.c.on then
      for _, d in ipairs({ { "t", h.tx, h.ty, 0 }, { "p", h.px, h.py, 0.01 } }) do
        local dist = math.sqrt((x - d[2]) ^ 2 + (y - d[3]) ^ 2) + d[4]
        if dist <= r and (not bestD or dist < bestD) then
          best, bestD = { c = h.c, which = d[1] }, dist
        end
      end
    end
  end
  return best
end

function E:grabDummy(c, which, x, y)
  local tx, ty, px, py = M.Ik.dummies(self.doc, c, self.frame)
  local dx, dy = which == "t" and tx or px, which == "t" and ty or py
  self.sel = which == "t" and c.lower or c.upper
  self.drag = { kind = "dummy", ctrl = c.id, which = which, ox = dx - x, oy = dy - y }
  return true
end

--- Moves a dummy; both keep their keys in step (a dummy keyed alone would
--- leave the other one interpolating from somewhere else).
function E:dragDummy(d, x, y)
  local c = M.Ik.find(self.doc, d.ctrl)
  if not c then return false end
  local tx, ty, px, py = M.Ik.dummies(self.doc, c, self.frame)
  local nx, ny = snap(x + d.ox), snap(y + d.oy)
  if d.which == "t" then tx, ty = nx, ny else px, py = nx, ny end
  self:touch()
  self:keyDummies(c, tx, ty, px, py)
  self:changed()
  return true
end

--- The ground line, when it is on and near the pointer.
function E:downGround(x, y, tol)
  local g = self.doc.ground
  if not (g and g.on) or math.abs(y - g.y) > (tol or 1) then return false end
  self.drag = { kind = "ground" }
  return true
end

function E:dragGround(d, x, y)
  local g = self.doc.ground
  local ny = snap(y)
  if ny == g.y then return false end
  self:touch()
  g.y = ny
  self:changed()
  return true
end

function E:setGround(field, v)
  local g = self.doc.ground
  if g[field] == v then return false end
  self:touch("ground:" .. field)
  if field == "y" then g.y = tonumber(v) or g.y else g[field] = v == true end
  self:changed()
  return true
end

--- A controller on the selected bone's chain (the bone as the lower one).
function E:addIK()
  local b = self:selected()
  if not b then return nil end
  local up, lo, tip = M.Ik.chainFor(self.doc, b.id)
  if not up then return nil end
  if M.Ik.forBone(self.doc, up) or M.Ik.forBone(self.doc, lo) then return nil end
  self:beginGesture()
  self:touch()
  local c = M.Ik.add(self.doc, up, lo, tip)
  self:changed()
  return c
end

function E:removeIK(id)
  if not M.Ik.find(self.doc, id) then return false end
  self:beginGesture()
  self:touch()
  M.Ik.remove(self.doc, id)
  self:changed()
  return true
end

--- Controllers on every limb that has none yet. Returns how many.
function E:addIKLimbs()
  local probe = M.Rig.copy(self.doc)
  local n = M.Ik.addLimbs(probe)
  if n == 0 then return 0 end
  self:beginGesture()
  self:touch()
  M.Ik.addLimbs(self.doc)
  self:changed()
  return n
end

--- on | stretch | maxStretch | lockTip of controller `id`.
function E:setIKProp(id, field, v)
  local c = M.Ik.find(self.doc, id)
  if not c or c[field] == v then return false end
  if field == "on" or field == "lockTip" then
    v = v == true
    if c[field] == v then return false end
    self:beginGesture()
    self:touch()
    -- a locked hand matters only while the IK is on
    if field == "on" or c.on then self:matchChain(c, field, v) else c[field] = v end
    self:changed()
    return true
  end
  self:touch("ik:" .. field .. ":" .. id)
  if field == "maxStretch" then
    c.maxStretch = math.max(1, math.min(4, tonumber(v) or c.maxStretch))
  else
    c[field] = v == true
  end
  self:changed()
  return true
end

return M
