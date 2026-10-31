-- rig.lua — the skeleton model of Bonebase: bones, parts, rest pose, FK
-- and the edit operations the editor calls. Pure Lua, dual-VM, no
-- Aseprite in it.
--
-- A DOCUMENT:
--
--   {
--     version = 1,
--     canvas  = { w = 64, h = 64 },   -- the work area: rig coordinates
--     bounds  = { x, y, w, h },  -- the Aseprite sprite inside it (export
--                                -- area); the whole canvas unless the
--                                -- canvas was grown around the sprite
--     nextId  = 7,
--     bones   = { bone, ... },   -- parents ALWAYS before their children
--     parts   = { part, ... },
--     mirror  = { on = bool, x = px },  -- L/R symmetry axis (editor.lua)
--     clips   = { anim, ... },  -- the animations (CLIPS), see anim.lua
--     clipIndex = 1,            -- the clip being edited
--     anim    = clips[clipIndex],  -- the SAME table, not a copy
--     images  = { [key] = { w, h, data } },  -- reference images (refimg.lua)
--   }
--
-- `doc.anim` is how everything else reads the active clip; `setClip`
-- keeps it pointing at clips[clipIndex], and `copy` keeps that sharing
-- (undo snapshots too). Edits that remove things (a bone, a part, an IK
-- controller) prune EVERY clip.
--
-- A BONE (rest pose):
--
--   { id, name, parent = 0 | id, connected = bool,
--     x, y,          -- head: canvas pixels for a root, else in the
--                    -- parent's local frame (origin at the parent's head,
--                    -- +x along the parent); ignored when connected
--     len, angle,    -- length in px, angle in degrees relative to the
--                    -- parent (absolute for a root); +x is 0 and positive
--                    -- turns clockwise on screen (y grows downwards)
--     rHead, rTail,  -- skin radius at each end, px
--     bulgeL, bulgeR,-- skin bulges per side: { { t, d }, ... } (skin.lua)
--     part,          -- id of the part (a GROUP in the window) whose layer
--                    -- the bone's skin goes to; 0 = none: no skin yet
--     twin }         -- id of its mirror bone (0 = none, see editor.lua)
--
-- A PART: { id, name, color = { r, g, b }, z, blend }. `blend` is the
-- smooth-union radius (px) the skin uses where its bones meet (skin.lua).
--
-- THE RIG ROOT (owner M.ROOT = -2, keys x / y): an offset that carries
-- the whole rig, IK dummies included (their keys are relative to it). Its
-- handle sits under the rig (rootHandle); in Rest dragging it moves the
-- rig (moveRig), in Pose it keys the offset. doc.rootPos = { x, y } puts
-- the handle somewhere else (nil: under the rig): only where you grab the
-- rig, never animated, so moving it alone moves nothing else.
--
-- A POSE is a table boneId -> { rot, x, y, s }: DELTAS over the rest pose
-- (rot in degrees, x/y in the same frame as the bone's head) and `s`, a
-- length factor (its key times the IK stretch). The rest pose is the
-- empty pose. Keys and sampling live in anim.lua, IK in ik.lua.
--
-- doc.ik = { controller, ... } (ik.lua) and doc.ground = { on, y, stop }.
--
-- doc.poses = { { id, name, bones = { { id, rot, x, y, s, fk?, fks? } }, ik = { { id,
-- tx, ty, px, py } }, parts = { { id, z, vis } } }, ... }: the rig's pose
-- library (poses.lua), values as keys hold them (dummies relative to the
-- rig root). Ids come from doc.nextPose, apart from the bones' counter.
--
-- doc.libName: the rig library entry this rig is kept in (library.lua),
-- nil when none; doc.libStamp the stamp of that entry's file when it was
-- saved or loaded, doc.libChanged true once the rig changed after that.
-- doc.gen counts the rigs an editor loaded in place of one another
-- (editor.lua linkLibrary); it is never written to a file.
--
-- Why bones keep their own two radii instead of one radius per joint: a
-- child that branches off its parent (an arm off the chest) wants a
-- shoulder thinner than the chest it starts from. A new child copies the
-- parent's tail radius, so a plain chain still looks like one radius per
-- joint.
--
-- The array order invariant (parent before child) is what lets FK run in
-- one pass with no recursion; every operation here keeps it.

local M = {}

local sqrt = math.sqrt
local cos, sin, pi = math.cos, math.sin, math.pi
local RAD, DEG = pi / 180, 180 / pi

M.VERSION = 1
M.MIN_LEN = 1
M.MIN_RADIUS = 0.5
M.DEFAULT_BLEND = 2
M.MAX_CLIPS = 32
M.MAX_NAME = 32
M.MAX_POSES = 64
M.ROOT = -2               -- the rig root's track owner (no bone, part or IK id)

-- Lua 5.4 has math.atan(y, x) and no atan2; LuaJIT has both.
local function angleOf(dx, dy)
  if math.atan2 then return math.atan2(dy, dx) * DEG end
  return math.atan(dy, dx) * DEG
end

--- An angle folded into (-180, 180].
function M.wrap(a)
  a = a % 360
  if a > 180 then a = a - 360 end
  return a
end

---------------------------------------
-- Document --
---------------------------------------

function M.new(w, h)
  local anim = M.newAnim("Animation")
  return {
    version = M.VERSION,
    canvas = { w = w or 64, h = h or 64 },
    bounds = { x = 0, y = 0, w = w or 64, h = h or 64 },
    nextId = 1,
    bones = {},
    parts = {},
    mirror = { on = false, x = (w or 64) / 2 },
    clips = { anim },
    clipIndex = 1,
    anim = anim,
    images = {},           -- reference images by key (refimg.lua)
    ik = {},
    ground = { on = false, y = (h or 64) - 1, stop = true },
    poses = {},            -- the pose library (poses.lua)
    nextPose = 1,
  }
end

--- An empty animation (anim.lua documents the fields).
function M.newAnim(name)
  return { name = name or "Animation", frames = 12, frameMs = 100, loop = true,
    pingpong = false, ease = "Ease", tracks = {}, durations = {} }
end

--- Makes clip `i` the active one (doc.anim). False when there is none.
function M.setClip(doc, i)
  local c = doc.clips[i]
  if not c then return false end
  doc.clipIndex = i
  doc.anim = c
  return true
end

--- `base` if no clip (but `except`) is called that, else "base 2"...
function M.clipName(doc, base, except)
  base = tostring(base or "Animation"):sub(1, M.MAX_NAME)
  local used = {}
  for _, c in ipairs(doc.clips) do
    if c ~= except then used[c.name] = true end
  end
  if not used[base] then return base end
  local n = 2
  while used[base .. " " .. n] do n = n + 1 end
  return base .. " " .. n
end

---------------------------------------
-- Canvas and sprite bounds --
---------------------------------------

M.MAX_CANVAS = 4096

--- The sprite's rectangle in the canvas (older rigs: the whole canvas).
function M.bounds(doc)
  local b = doc.bounds
  if b then return b end
  return { x = 0, y = 0, w = doc.canvas.w, h = doc.canvas.h }
end

local IK_CH = { tx = "x", ty = "y", px = "x", py = "y" }
local REF_CH = { rx = "x", ry = "y" }

--- The pose library's dummies are canvas positions like the IK keys, and
--- so is the place of the root's handle.
local function movePoses(doc, dx, dy)
  if doc.rootPos then doc.rootPos = { x = doc.rootPos.x + dx, y = doc.rootPos.y + dy } end
  for _, p in ipairs(doc.poses or {}) do
    for _, e in ipairs(p.ik) do
      e.tx, e.px = e.tx + dx, e.px + dx
      e.ty, e.py = e.ty + dy, e.py + dy
    end
  end
end

--- Moves everything that lives in canvas pixels by (dx, dy): roots,
--- IK dummy keys, reference images, the mirror axis, the ground and the
--- sprite bounds. Offsets and rotations are relative and stay.
function M.translate(doc, dx, dy)
  if dx == 0 and dy == 0 then return end
  for _, b in ipairs(doc.bones) do
    if b.parent == 0 then b.x, b.y = b.x + dx, b.y + dy end
  end
  for _, a in ipairs(doc.clips) do
    for _, t in ipairs(a.tracks) do
      local axis = IK_CH[t.ch] or (t.bone == -1 and REF_CH[t.ch])
      if axis then
        local d = axis == "x" and dx or dy
        for _, k in ipairs(t.keys) do k[2] = k[2] + d end
      end
    end
    if a.ref and a.ref.base then
      a.ref.base.rx = (a.ref.base.rx or 0) + dx
      a.ref.base.ry = (a.ref.base.ry or 0) + dy
    end
  end
  movePoses(doc, dx, dy)
  if doc.mirror then doc.mirror.x = doc.mirror.x + dx end
  if doc.ground then doc.ground.y = doc.ground.y + dy end
  local bd = M.bounds(doc)
  doc.bounds = { x = bd.x + dx, y = bd.y + dy, w = bd.w, h = bd.h }
end

--- Moves the rig itself by (dx, dy) in its rest pose: roots, every
--- clip's IK dummy keys and the mirror axis. The canvas, the sprite
--- bounds, the ground and the reference image stay where they are.
function M.moveRig(doc, dx, dy)
  if dx == 0 and dy == 0 then return end
  for _, b in ipairs(doc.bones) do
    if b.parent == 0 then b.x, b.y = b.x + dx, b.y + dy end
  end
  for _, a in ipairs(doc.clips) do
    for _, t in ipairs(a.tracks) do
      local axis = IK_CH[t.ch]
      if axis then
        local d = axis == "x" and dx or dy
        for _, k in ipairs(t.keys) do k[2] = k[2] + d end
      end
    end
  end
  movePoses(doc, dx, dy)
  if doc.mirror then doc.mirror.x = doc.mirror.x + dx end
end

--- Where the rig root's handle sits in the rest pose: doc.rootPos, or under
--- the first root bone's head, a little below the lowest skin. nil without
--- bones.
function M.rootHandle(doc, world)
  local first
  for _, b in ipairs(doc.bones) do
    if b.parent == 0 then first = b break end
  end
  if not first then return nil end
  if doc.rootPos then return doc.rootPos.x, doc.rootPos.y end
  world = world or M.solve(doc)
  local low = -math.huge
  for _, b in ipairs(doc.bones) do
    local w = world[b.id]
    low = math.max(low, w.hy + b.rHead, w.ty + b.rTail)
  end
  return world[first.id].hx, low + 2
end

--- A canvas of w x h around the sprite, which sits in its middle: the
--- rig keeps its place on the sprite. Never smaller than the sprite.
function M.setCanvas(doc, w, h)
  local bd = M.bounds(doc)
  w = math.max(bd.w, math.min(M.MAX_CANVAS, math.floor(w)))
  h = math.max(bd.h, math.min(M.MAX_CANVAS, math.floor(h)))
  local nx, ny = math.floor((w - bd.w) / 2), math.floor((h - bd.h) / 2)
  M.translate(doc, nx - bd.x, ny - bd.y)
  doc.canvas = { w = w, h = h }
  return true
end

--- The sprite is now w x h (resized in Aseprite): the bounds take that
--- size where they are, and the canvas grows if it no longer holds them.
function M.setSpriteSize(doc, w, h)
  local bd = M.bounds(doc)
  doc.bounds = { x = bd.x, y = bd.y, w = w, h = h }
  if bd.x + w > doc.canvas.w or bd.y + h > doc.canvas.h then
    doc.canvas = { w = math.max(doc.canvas.w, bd.x + w), h = math.max(doc.canvas.h, bd.y + h) }
  end
end

--- Puts `doc` (built for a canvas the size of the sprite: a template, an
--- empty rig) into the canvas and bounds of `like`.
function M.placeLike(doc, like)
  local bd = M.bounds(like)
  M.translate(doc, bd.x, bd.y)
  doc.canvas = { w = like.canvas.w, h = like.canvas.h }
  doc.bounds = { x = bd.x, y = bd.y, w = bd.w, h = bd.h }
  return doc
end

--- Puts a saved rig `doc` (any canvas, any sprite size) on the sprite of
--- `like`: its sprite's bottom centre on that sprite's bottom centre (a
--- character keeps its feet on the ground), with `like`'s canvas and bounds.
function M.fitInto(doc, like)
  local from, to = M.bounds(doc), M.bounds(like)
  local dx = (to.x + math.floor(to.w / 2)) - (from.x + math.floor(from.w / 2))
  local dy = (to.y + to.h) - (from.y + from.h)
  M.translate(doc, dx, dy)
  doc.canvas = { w = like.canvas.w, h = like.canvas.h }
  doc.bounds = { x = to.x, y = to.y, w = to.w, h = to.h }
  return doc
end

local function nextId(doc)
  local id = doc.nextId
  doc.nextId = id + 1
  return id
end

--- Index of a bone (or part) by id in its array, or nil.
local function indexOf(list, id)
  for i = 1, #list do
    if list[i].id == id then return i end
  end
  return nil
end

function M.bone(doc, id)
  local i = indexOf(doc.bones, id)
  return i and doc.bones[i] or nil
end

function M.part(doc, id)
  local i = indexOf(doc.parts, id)
  return i and doc.parts[i] or nil
end

function M.children(doc, id)
  local out = {}
  for _, b in ipairs(doc.bones) do
    if b.parent == id then out[#out + 1] = b end
  end
  return out
end

--- A deep copy of any plain table (documents, poses, snapshots).
function M.copy(v, seen)
  if type(v) ~= "table" then return v end
  seen = seen or {}
  if seen[v] then return seen[v] end   -- shared tables stay shared (doc.anim)
  local out = {}
  seen[v] = out
  for k, x in pairs(v) do out[k] = M.copy(x, seen) end
  return out
end

--- Replaces the contents of `doc` with `snap` IN PLACE (undo's restore).
function M.restore(doc, snap)
  for k in pairs(doc) do doc[k] = nil end
  for k, v in pairs(M.copy(snap)) do doc[k] = v end
end

---------------------------------------
-- Parts --
---------------------------------------

M.PALETTE = {
  { 232, 98, 92 }, { 92, 156, 232 }, { 120, 200, 110 }, { 236, 190, 80 },
  { 180, 120, 220 }, { 90, 200, 200 }, { 236, 140, 70 }, { 200, 200, 200 },
}

function M.addPart(doc, name)
  local n = #doc.parts
  local c = M.PALETTE[(n % #M.PALETTE) + 1]
  local part = {
    id = nextId(doc),
    name = name or ("part" .. (n + 1)),
    color = { c[1], c[2], c[3] },
    z = n,
    blend = M.DEFAULT_BLEND,
  }
  doc.parts[n + 1] = part
  return part
end

--- Removes a part; its bones move to `into` (a part id), else they are
--- left without one (no skin until they get a group again).
function M.removePart(doc, id, into)
  local i = indexOf(doc.parts, id)
  if not i then return false end
  table.remove(doc.parts, i)
  for _, b in ipairs(doc.bones) do
    if b.part == id then b.part = into or 0 end
  end
  return true
end

--- Parts sorted by z (ties by array order): the order layers are stacked.
function M.partsByZ(doc)
  local list = {}
  for i, p in ipairs(doc.parts) do list[i] = { p = p, i = i } end
  table.sort(list, function(a, b)
    if a.p.z ~= b.p.z then return a.p.z < b.p.z end
    return a.i < b.i
  end)
  local out = {}
  for i, e in ipairs(list) do out[i] = e.p end
  return out
end

---------------------------------------
-- Forward kinematics --
---------------------------------------

--- World transforms of every bone under `pose` (nil = rest).
--- Returns a table id -> { hx, hy, tx, ty, angle } (angle absolute, deg).
function M.solve(doc, pose)
  local world = {}
  local g = pose and pose[M.ROOT]
  local gx, gy = g and g.x or 0, g and g.y or 0
  for _, b in ipairs(doc.bones) do
    local d = pose and pose[b.id]
    local rot = b.angle + (d and d.rot or 0)
    local hx, hy, angle
    if b.parent == 0 or not world[b.parent] then
      hx = b.x + (d and d.x or 0) + gx
      hy = b.y + (d and d.y or 0) + gy
      angle = rot
    else
      local p = world[b.parent]
      if b.connected then
        hx, hy = p.tx, p.ty
      else
        local ox = b.x + (d and d.x or 0)
        local oy = b.y + (d and d.y or 0)
        local c, s = cos(p.angle * RAD), sin(p.angle * RAD)
        hx = p.hx + ox * c - oy * s
        hy = p.hy + ox * s + oy * c
      end
      angle = p.angle + rot
    end
    local a = angle * RAD
    local len = b.len * (d and d.s or 1)
    world[b.id] = {
      hx = hx, hy = hy,
      tx = hx + len * cos(a), ty = hy + len * sin(a),
      angle = angle,
    }
  end
  return world
end

--- A world point in the local frame of bone `id`'s PARENT (what a
--- disconnected head stores). Root: the point itself.
local function toParentFrame(doc, world, b, wx, wy)
  if b.parent == 0 then return wx, wy end
  local p = world[b.parent]
  local dx, dy = wx - p.hx, wy - p.hy
  local c, s = cos(p.angle * RAD), sin(p.angle * RAD)
  return dx * c + dy * s, -dx * s + dy * c
end

local function parentAngle(world, b)
  if b.parent == 0 then return 0 end
  return world[b.parent].angle
end

---------------------------------------
-- Editing (rest pose) --
---------------------------------------

--- A new bone from world head (hx, hy) to world tail (tx, ty).
--- opts: parent (id, default 0), connected (default: head on the
--- parent's tail), name, part, rHead, rTail.
function M.addBone(doc, hx, hy, tx, ty, opts)
  opts = opts or {}
  local parentId = opts.parent or 0
  local parent = parentId ~= 0 and M.bone(doc, parentId) or nil
  if parentId ~= 0 and not parent then return nil, "no such parent" end
  local world = M.solve(doc)
  local connected = false
  if parent then
    local p = world[parentId]
    if opts.connected == nil then
      connected = math.abs(hx - p.tx) < 1e-6 and math.abs(hy - p.ty) < 1e-6
    else
      connected = opts.connected and true or false
    end
    if connected then hx, hy = p.tx, p.ty end
  end
  local dx, dy = tx - hx, ty - hy
  local len = sqrt(dx * dx + dy * dy)
  if len < M.MIN_LEN then return nil, "too short" end
  local b = {
    id = nextId(doc),
    name = opts.name or ("bone" .. (#doc.bones + 1)),
    parent = parentId,
    connected = connected,
    x = 0, y = 0,
    len = len,
    angle = 0,
    rHead = opts.rHead or (parent and parent.rTail) or 4,
    rTail = opts.rTail or (parent and parent.rTail) or 4,
    bulgeL = {}, bulgeR = {},
    part = opts.part or (parent and parent.part) or 0,
    twin = 0,
  }
  b.angle = M.wrap(angleOf(dx, dy) - (parent and world[parentId].angle or 0))
  if not connected then
    -- the head is stored in the parent's frame; a root keeps canvas px
    local fake = { parent = parentId }
    b.x, b.y = toParentFrame(doc, world, fake, hx, hy)
  end
  -- appended: its parent is already before it
  doc.bones[#doc.bones + 1] = b
  return b
end

--- Removes a bone and its whole subtree. Returns how many went.
function M.removeBone(doc, id)
  local gone = { [id] = true }
  local kept, n = {}, 0
  for _, b in ipairs(doc.bones) do
    if gone[b.id] or gone[b.parent] then
      gone[b.id] = true
      n = n + 1
    else
      kept[#kept + 1] = b
    end
  end
  doc.bones = kept
  for _, b in ipairs(kept) do
    if gone[b.twin] then b.twin = 0 end
  end
  return n
end

--- Moves a bone's TAIL to a world point in the rest pose: changes its
--- length and angle; children follow (connected ones stay attached).
function M.setTail(doc, id, wx, wy)
  local b = M.bone(doc, id)
  if not b then return false end
  local world = M.solve(doc)
  local w = world[id]
  local dx, dy = wx - w.hx, wy - w.hy
  local len = sqrt(dx * dx + dy * dy)
  if len < M.MIN_LEN then return false end
  b.len = len
  b.angle = M.wrap(angleOf(dx, dy) - parentAngle(world, b))
  return true
end

--- Puts a bone so its head and tail land on world points, never refusing:
--- the length is clamped to MIN_LEN (keeping the old angle when the two
--- points coincide). A connected bone keeps its head on the parent's tail
--- and only takes the tail. What the mirror uses to write a twin, where
--- setHead / setTail could refuse a step and leave the pair apart.
function M.place(doc, id, hx, hy, tx, ty)
  local b = M.bone(doc, id)
  if not b then return false end
  local world = M.solve(doc)
  if b.connected and b.parent ~= 0 then
    local p = world[b.parent]
    hx, hy = p.tx, p.ty
  else
    b.x, b.y = toParentFrame(doc, world, b, hx, hy)
  end
  local dx, dy = tx - hx, ty - hy
  local len = sqrt(dx * dx + dy * dy)
  if len > 1e-9 then
    b.angle = M.wrap(angleOf(dx, dy) - parentAngle(world, b))
  end
  b.len = math.max(M.MIN_LEN, len)
  return true
end

--- Moves a bone's HEAD to a world point, keeping its tail where it was.
--- A connected bone first becomes disconnected.
function M.setHead(doc, id, wx, wy)
  local b = M.bone(doc, id)
  if not b then return false end
  local world = M.solve(doc)
  local w = world[id]
  local dx, dy = w.tx - wx, w.ty - wy
  local len = sqrt(dx * dx + dy * dy)
  if len < M.MIN_LEN then return false end
  b.connected = false
  b.x, b.y = toParentFrame(doc, world, b, wx, wy)
  b.len = len
  b.angle = M.wrap(angleOf(dx, dy) - parentAngle(world, b))
  return true
end

--- Translates a whole bone (head and tail) by a world delta. A connected
--- bone becomes disconnected; a root just moves, children with it.
function M.moveBone(doc, id, dx, dy)
  local b = M.bone(doc, id)
  if not b then return false end
  local world = M.solve(doc)
  local w = world[id]
  b.connected = false
  b.x, b.y = toParentFrame(doc, world, b, w.hx + dx, w.hy + dy)
  return true
end

local function clampRadius(r)
  if r < M.MIN_RADIUS then return M.MIN_RADIUS end
  return r
end

--- Sets a radius: which = "head" | "tail".
function M.setRadius(doc, id, which, r)
  local b = M.bone(doc, id)
  if not b then return false end
  if which == "head" then b.rHead = clampRadius(r) else b.rTail = clampRadius(r) end
  return true
end

---------------------------------------
-- Picking --
---------------------------------------

local function distToSegment(px, py, ax, ay, bx, by)
  local vx, vy = bx - ax, by - ay
  local l2 = vx * vx + vy * vy
  local t = 0
  if l2 > 0 then
    t = ((px - ax) * vx + (py - ay) * vy) / l2
    if t < 0 then t = 0 elseif t > 1 then t = 1 end
  end
  local cx, cy = ax + t * vx - px, ay + t * vy - py
  return sqrt(cx * cx + cy * cy), t
end

--- What lies under a world point within `tol` px: returns
--- { bone = id, what = "head" | "tail" | "body", t } or nil. Joints win
--- over bodies; among equals the LAST bone (drawn on top) wins.
function M.pick(doc, world, px, py, tol)
  tol = tol or 2
  local best, bestD, bestRank = nil, math.huge, 3
  for i = #doc.bones, 1, -1 do
    local b = doc.bones[i]
    local w = world[b.id]
    local function try(what, d, rank, t)
      if d <= tol and (rank < bestRank or (rank == bestRank and d < bestD)) then
        best, bestD, bestRank = { bone = b.id, what = what, t = t }, d, rank
      end
    end
    local dt = sqrt((px - w.tx) ^ 2 + (py - w.ty) ^ 2)
    local dh = sqrt((px - w.hx) ^ 2 + (py - w.hy) ^ 2)
    try("tail", dt, 1, 1)
    try("head", dh, 1, 0)
    local ds, t = distToSegment(px, py, w.hx, w.hy, w.tx, w.ty)
    try("body", ds, 2, t)
  end
  return best
end

---------------------------------------
-- Files --
---------------------------------------

--- A finite number from anything, else `default` (NaN and +-inf too:
--- they behave differently in Lua 5.4 and LuaJIT once they reach min/max).
local function num(v, default)
  v = tonumber(v)
  if v == nil or v ~= v or v == math.huge or v == -math.huge then return default end
  return v
end

local function int(v, default)
  v = num(v, nil)
  if v == nil then return default end
  return math.floor(v)
end

--- A bulge list made valid: { t, d } pairs of numbers, t inside the bone,
--- sorted by t.
local function bulgesOf(list)
  local out = {}
  if type(list) ~= "table" then return out end
  for _, q in ipairs(list) do
    if type(q) == "table" then
      local tt, d = num(q[1], nil), num(q[2], nil)
      if tt and d then
        -- same limits as skin.lua BULGE_T_MIN / MAX (test_files_fuzz checks)
        if tt < 0.02 then tt = 0.02 elseif tt > 0.98 then tt = 0.98 end
        out[#out + 1] = { tt, d }
      end
    end
  end
  table.sort(out, function(a, b) return a[1] < b[1] end)
  return out
end

local function colorOf(c)
  if type(c) ~= "table" then return { 200, 200, 200 } end
  local out = {}
  for i = 1, 3 do
    local v = int(c[i], 200)
    out[i] = math.max(0, math.min(255, v))
  end
  return out
end

--- Reads a decoded table back into a valid document, or nil + why.
--- Anything optional that is missing or not a usable number gets its
--- default, so a document that loads also solves and renders. A bone
--- whose parent does not come before it, or a repeated bone id, is
--- rejected (the order invariant above); a repeated part id is dropped.
--- Bones whose part does not exist get a new part, never vanish.
function M.fromTable(t)
  if type(t) ~= "table" or type(t.bones) ~= "table" then
    return nil, "not a Bonebase rig"
  end
  if num(t.version, 1) > M.VERSION then
    return nil, "rig saved by a newer Bonebase"
  end
  local canvas = type(t.canvas) == "table" and t.canvas or {}
  local doc = M.new(math.max(1, math.min(M.MAX_CANVAS, int(canvas.w, 64))),
    math.max(1, math.min(M.MAX_CANVAS, int(canvas.h, 64))))
  if type(t.bounds) == "table" then
    -- the sprite inside the canvas; anything that does not fit is the
    -- whole canvas again
    local bx, by = int(t.bounds.x, 0), int(t.bounds.y, 0)
    local bw, bh = int(t.bounds.w, doc.canvas.w), int(t.bounds.h, doc.canvas.h)
    if bx >= 0 and by >= 0 and bw >= 1 and bh >= 1 and bx + bw <= doc.canvas.w
      and by + bh <= doc.canvas.h then
      doc.bounds = { x = bx, y = by, w = bw, h = bh }
    end
  end
  local seen, parts, maxId = {}, {}, 0
  for i, p in ipairs(type(t.parts) == "table" and t.parts or {}) do
    local id = type(p) == "table" and int(p.id, nil) or nil
    if id and not parts[id] then
      parts[id] = true
      doc.parts[#doc.parts + 1] = {
        id = id, name = tostring(p.name or ("part" .. i)),
        color = colorOf(p.color), z = num(p.z, i - 1),
        blend = math.max(0, num(p.blend, M.DEFAULT_BLEND)),
      }
      if id > maxId then maxId = id end
    end
  end
  local orphans = {}
  for i, b in ipairs(t.bones) do
    if type(b) ~= "table" then return nil, "bad bone #" .. i end
    local id, parent = int(b.id, nil), int(b.parent, nil)
    local len, angle = num(b.len, nil), num(b.angle, nil)
    if not (id and parent and len and angle) then return nil, "bad bone #" .. i end
    if seen[id] then return nil, "bone #" .. i .. " repeats id " .. id end
    if parent ~= 0 and not seen[parent] then
      return nil, "bone #" .. i .. " comes before its parent"
    end
    seen[id] = true
    local bone = {
      id = id, name = tostring(b.name or ("bone" .. i)),
      parent = parent, connected = parent ~= 0 and b.connected == true,
      x = num(b.x, 0), y = num(b.y, 0),
      len = math.max(M.MIN_LEN, len), angle = angle,
      rHead = math.max(M.MIN_RADIUS, num(b.rHead, 4)),
      rTail = math.max(M.MIN_RADIUS, num(b.rTail, 4)),
      bulgeL = bulgesOf(b.bulgeL), bulgeR = bulgesOf(b.bulgeR),
      part = int(b.part, 0),
      twin = int(b.twin, 0),
    }
    if not parts[bone.part] then orphans[#orphans + 1] = bone end
    doc.bones[#doc.bones + 1] = bone
    if id > maxId then maxId = id end
  end
  -- a part with a bone's id (a hand-edited file; Bonebase shares one id
  -- counter): the part gets a new id, its bones and z / vis keys follow
  local partMoved = {}
  for _, p in ipairs(doc.parts) do
    if seen[p.id] then
      maxId = maxId + 1
      partMoved[p.id] = maxId
      parts[maxId] = true
      p.id = maxId
    end
  end
  for _, b in ipairs(doc.bones) do
    if partMoved[b.part] then b.part = partMoved[b.part] end
  end
  doc.nextId = math.max(int(t.nextId, 1), maxId + 1)
  -- a bone whose part is gone has none (no skin until it gets one)
  for _, b in ipairs(orphans) do b.part = 0 end
  -- twins must exist and point back
  for _, b in ipairs(doc.bones) do
    if b.twin ~= 0 then
      local tw = M.bone(doc, b.twin)
      if not tw or tw.twin ~= b.id or tw.id == b.id then b.twin = 0 end
    end
  end
  -- IK controllers (ik.lua): a valid chain of existing bones, own ids
  local ctrls = {}
  for _, c in ipairs(type(t.ik) == "table" and t.ik or {}) do
    if type(c) == "table" then
      local id, up, lo, tip = int(c.id, nil), int(c.upper, nil), int(c.lower, nil), int(c.tip, 0)
      local a, b = up and M.bone(doc, up), lo and M.bone(doc, lo)
      local tb = tip ~= 0 and M.bone(doc, tip) or nil
      if id and not seen[id] and not ctrls[id] and not parts[id] and a and b and b.parent == a.id and b.connected
        and (tip == 0 or (tb and tb.parent == b.id)) then
        ctrls[id] = true
        doc.ik[#doc.ik + 1] = {
          id = id, upper = up, lower = lo, tip = tb and tip or 0,
          on = c.on ~= false, stretch = c.stretch == true,
          maxStretch = math.max(1, math.min(4, num(c.maxStretch, 1.25))),
          lockTip = c.lockTip ~= false,
        }
        if id >= doc.nextId then doc.nextId = id + 1 end
      end
    end
  end
  if type(t.ground) == "table" then
    doc.ground = { on = t.ground.on == true, y = num(t.ground.y, doc.canvas.h - 1),
      stop = t.ground.stop ~= false }
  end
  -- reference images (refimg.lua): sizes and data checked here, the
  -- pixels when first drawn
  local images = {}
  if type(t.images) == "table" then
    for k, e in pairs(t.images) do
      if type(k) == "string" and #k <= 64 and type(e) == "table" and type(e.data) == "string" then
        local w, h = int(e.w, nil), int(e.h, nil)
        if w and h and w >= 1 and h >= 1 and w <= 512 and h <= 512 then
          images[k] = { w = w, h = h, data = e.data }
        end
      end
    end
  end
  doc.images = images
  -- clips; a rig from before clips has one `anim`, which becomes clip 1
  local clips = {}
  if type(t.clips) == "table" then
    for _, a in ipairs(t.clips) do
      if #clips < M.MAX_CLIPS and type(a) == "table" then
        clips[#clips + 1] = M.animFromTable(a, seen, ctrls, parts, partMoved, images)
      end
    end
  end
  if #clips == 0 then clips[1] = M.animFromTable(t.anim, seen, ctrls, parts, partMoved, images) end
  doc.clips = {}
  for _, c in ipairs(clips) do
    c.name = M.clipName(doc, c.name)
    doc.clips[#doc.clips + 1] = c
  end
  for _, c in ipairs(doc.clips) do
    if c.ref then
      c.ref.base.rx = c.ref.base.rx or doc.canvas.w / 2
      c.ref.base.ry = c.ref.base.ry or doc.canvas.h / 2
    end
  end
  local ci = int(t.clipIndex, 1)
  M.setClip(doc, ci >= 1 and ci <= #doc.clips and ci or 1)
  M.posesFromTable(doc, t.poses, t.nextPose, seen, ctrls, parts, partMoved)
  if type(t.libName) == "string" and t.libName ~= "" then
    doc.libName = t.libName:sub(1, 48)
    if type(t.libStamp) == "string" then doc.libStamp = t.libStamp:sub(1, 64) end
    doc.libChanged = t.libChanged == true
  end
  if type(t.mirror) == "table" then
    doc.mirror = { on = t.mirror.on == true, x = num(t.mirror.x, doc.canvas.w / 2) }
  end
  if type(t.rootPos) == "table" then
    local x, y = tonumber(t.rootPos.x), tonumber(t.rootPos.y)
    if x and y and x == x and y == y and math.abs(x) < 1e6 and math.abs(y) < 1e6 then
      doc.rootPos = { x = x, y = y }
    end
  end
  return doc
end

--- Reads the pose library into `doc.poses`: entries of things that do
--- not exist are dropped, a pose with nothing left goes, names are cut to
--- MAX_NAME, ids are unique and below doc.nextPose.
function M.posesFromTable(doc, list, nextPose, bones, ctrls, parts, partMoved)
  doc.poses, doc.nextPose = {}, 1
  local used, maxId = {}, 0
  for i, p in ipairs(type(list) == "table" and list or {}) do
    if #doc.poses >= M.MAX_POSES then break end
    if type(p) == "table" then
      local pose = { name = tostring(p.name or ("pose " .. i)):sub(1, M.MAX_NAME),
        bones = {}, ik = {}, parts = {} }
      for _, e in ipairs(type(p.bones) == "table" and p.bones or {}) do
        local id = type(e) == "table" and int(e.id, nil)
        if id and bones[id] then
          pose.bones[#pose.bones + 1] = { id = id, rot = num(e.rot, 0), x = num(e.x, 0),
            y = num(e.y, 0), s = math.max(0.1, math.min(4, num(e.s, 1))), fk = num(e.fk, nil),
            fks = e.fks ~= nil and math.max(0.1, math.min(4, num(e.fks, 1))) or nil }
        end
      end
      for _, e in ipairs(type(p.ik) == "table" and p.ik or {}) do
        local id = type(e) == "table" and int(e.id, nil)
        local tx, ty, px, py = id and num(e.tx, nil), id and num(e.ty, nil), id and num(e.px, nil),
          id and num(e.py, nil)
        if id and ctrls[id] and tx and ty and px and py then
          pose.ik[#pose.ik + 1] = { id = id, tx = tx, ty = ty, px = px, py = py }
        end
      end
      for _, e in ipairs(type(p.parts) == "table" and p.parts or {}) do
        local id = type(e) == "table" and int(e.id, nil)
        if id and partMoved[id] then id = partMoved[id] end
        if id and parts[id] then
          pose.parts[#pose.parts + 1] = { id = id,
            z = e.z ~= nil and math.max(-99, math.min(99, math.floor(num(e.z, 0) + 0.5))) or nil,
            vis = e.vis ~= nil and (num(e.vis, 1) >= 0.5 and 1 or 0) or nil }
        end
      end
      if #pose.bones + #pose.ik > 0 then
        local id = int(p.id, nil)
        if not id or id < 1 or used[id] then id = nil end
        pose.id = id
        if id then
          used[id] = true
          if id > maxId then maxId = id end
        end
        doc.poses[#doc.poses + 1] = pose
      end
    end
  end
  for _, pose in ipairs(doc.poses) do
    if not pose.id then
      maxId = maxId + 1
      pose.id = maxId
    end
  end
  doc.nextPose = math.max(int(nextPose, 1), maxId + 1)
end

-- these lists mirror anim.lua (EASES, CHANNELS, PART_CHANNELS) and ik.lua
local EASES = { Linear = true, Ease = true, ["Ease in"] = true, ["Ease out"] = true,
  Back = true, Bounce = true, Step = true }
local CHANNELS = { rot = true, x = true, y = true, s = true }
local PART_CHANNELS = { z = true, vis = true }
local IK_CHANNELS = { tx = true, ty = true, px = true, py = true }
-- the reference image's channels, on the owner -1 (refimg.lua)
local REF_OWNER = -1
local ROOT_CHANNELS = { x = true, y = true }
local REF_CHANNELS = { rx = true, ry = true, rs = true, rr = true, ro = true, rf = true }

--- A key value made valid for its channel.
local function channelValue(ch, v)
  if ch == "s" then return math.max(0.1, math.min(4, v)) end
  if ch == "rs" then return math.max(0.02, math.min(32, v)) end
  if ch == "ro" then return math.max(0, math.min(1, v)) end
  if ch == "rf" then return v >= 0.5 and 1 or 0 end
  if ch == "vis" then return v >= 0.5 and 1 or 0 end
  if ch == "z" then return math.max(-99, math.min(99, math.floor(v + 0.5))) end
  return v
end

--- A decoded animation made valid: bad tracks and keys are dropped, keys
--- re-sorted, one per frame, one track per (bone, channel). `bones` = set
--- of existing bone ids, `ctrls` of IK controller ids (channels tx ty px py),
--- `parts` of part ids (z, vis).
function M.animFromTable(a, bones, ctrls, parts, partMoved, images)
  ctrls = ctrls or {}
  parts = parts or {}
  partMoved = partMoved or {}
  images = images or {}
  -- z / vis keys of a part renumbered by fromTable go with it
  local function owner(tr)
    if PART_CHANNELS[tr.ch] and partMoved[tr.bone] then return partMoved[tr.bone] end
    return tr.bone
  end
  local anim = M.newAnim()
  if type(a) ~= "table" then return anim end
  if type(a.name) == "string" and a.name ~= "" then anim.name = a.name:sub(1, M.MAX_NAME) end
  anim.pingpong = a.pingpong == true
  -- 240 = anim.lua MAX_FRAMES (test_files_fuzz checks)
  anim.frames = math.max(1, math.min(240, int(a.frames, anim.frames)))
  anim.frameMs = math.max(10, math.min(60000, int(a.frameMs, anim.frameMs)))
  anim.loop = a.loop ~= false
  anim.ease = EASES[a.ease] and a.ease or "Ease"
  local have = {}
  for _, tr in ipairs(type(a.tracks) == "table" and a.tracks or {}) do
    if type(tr) == "table"
      and ((bones[owner(tr)] and CHANNELS[tr.ch]) or (ctrls[owner(tr)] and IK_CHANNELS[tr.ch])
        or (parts[owner(tr)] and PART_CHANNELS[tr.ch])
        or (owner(tr) == REF_OWNER and REF_CHANNELS[tr.ch])
        or (owner(tr) == M.ROOT and ROOT_CHANNELS[tr.ch]))
      and type(tr.keys) == "table" and not have[owner(tr) .. ":" .. tr.ch] then
      local byFrame, frames2 = {}, {}
      for _, k in ipairs(tr.keys) do
        if type(k) == "table" then
          local f, v = num(k[1], nil), num(k[2], nil)
          if f and v and f == math.floor(f) and f >= 1 and f <= anim.frames then
            if not byFrame[f] then frames2[#frames2 + 1] = f end
            byFrame[f] = { f, channelValue(tr.ch, v), EASES[k[3]] and k[3] or anim.ease }
          end
        end
      end
      table.sort(frames2)
      local keys = {}
      for i, f in ipairs(frames2) do keys[i] = byFrame[f] end
      if #keys > 0 then
        have[owner(tr) .. ":" .. tr.ch] = true
        anim.tracks[#anim.tracks + 1] = { bone = owner(tr), ch = tr.ch, keys = keys }
      end
    end
  end
  -- the reference image (refimg.lua): only keys the store has
  if type(a.ref) == "table" then
    local r = a.ref
    local base = type(r.base) == "table" and r.base or {}
    local ref = { on = r.on ~= false, front = r.front == true, frames = {},
      image = (type(r.image) == "string" and images[r.image]) and r.image or nil,
      -- rx / ry missing: fromTable centres them on the canvas
      base = { rx = num(base.rx, nil), ry = num(base.ry, nil),
        rs = channelValue("rs", num(base.rs, 1)), rr = math.max(-3600, math.min(3600, num(base.rr, 0))),
        ro = channelValue("ro", num(base.ro, 0.5)), rf = channelValue("rf", num(base.rf, 0)) } }
    if type(r.frames) == "table" then
      for _, e in ipairs(r.frames) do
        local f = type(e) == "table" and int(e[1], nil)
        if f and f >= 1 and f <= anim.frames and type(e[2]) == "string" and images[e[2]] then
          ref.frames[f] = e[2]
        end
      end
    end
    anim.ref = ref
  end
  -- per-frame durations (ms) of the frames that differ from frameMs; a
  -- file holds them as { { frame, ms }, ... } (see encode)
  if type(a.durations) == "table" then
    for _, d in ipairs(a.durations) do
      local f, ms = type(d) == "table" and int(d[1], nil), type(d) == "table" and num(d[2], nil)
      if f and ms and f >= 1 and f <= anim.frames then
        anim.durations[f] = math.max(10, math.min(60000, math.floor(ms)))
      end
    end
  end
  return anim
end

--- A clip as a file holds it: durations become a sorted list of pairs (a
--- sparse array would not serialize the same in Lua 5.4 and LuaJIT).
local function clipForFile(c)
  local out = {}
  for k, v in pairs(c) do out[k] = v end
  local list = {}
  for f, ms in pairs(c.durations or {}) do list[#list + 1] = { f, ms } end
  table.sort(list, function(x, y) return x[1] < y[1] end)
  out.durations = list
  if c.ref then
    local r = {}
    for k, v in pairs(c.ref) do r[k] = v end
    local frames = {}
    for f, key in pairs(c.ref.frames or {}) do frames[#frames + 1] = { f, key } end
    table.sort(frames, function(x, y) return x[1] < y[1] end)
    r.frames = frames
    out.ref = r
  end
  return out
end

--- Encodes with kit/serial (passed in: the engine never loads files).
--- `anim` is written too, as a copy of the active clip: an older Bonebase
--- that knows no clips still opens the rig with that animation.
function M.encode(Serial, doc)
  local out = {}
  for k, v in pairs(doc) do out[k] = v end
  out.gen = nil
  out.clips = {}
  for i, c in ipairs(doc.clips) do out.clips[i] = clipForFile(c) end
  out.anim = clipForFile(doc.anim)
  -- only the reference images some clip still uses
  local used = {}
  for _, c in ipairs(doc.clips) do
    if c.ref then
      if c.ref.image then used[c.ref.image] = true end
      for _, k in pairs(c.ref.frames or {}) do used[k] = true end
    end
  end
  out.images = {}
  for k in pairs(used) do out.images[k] = doc.images and doc.images[k] end
  return Serial.encodeFile(out)
end

function M.decode(Serial, text)
  local t, err = Serial.decode(text)
  if not t then return nil, err end
  return M.fromTable(t)
end

return M
