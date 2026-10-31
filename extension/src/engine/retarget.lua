-- retarget.lua — turns a 3D skeleton animation (bvh.lua / gltf.lua, the
-- `skel` format below) into a Bonebase clip: an orthographic camera
-- projects the joints, and every rig bone takes the DIRECTION of the
-- source segment it is assigned to. Pure Lua, dual-VM.
--
--   skel = { joints = { { name, parent = 0 | index, isEnd }, ... },
--            rest = { x1, y1, z1, ... },
--            clips = { { name, frameTime, count, pos = { [f] = flat } } } }
--
-- Why directions and not positions: the user's rig keeps its own
-- proportions and skin, the 3D clip only says where each bone points. The
-- length a bone loses to the projection becomes its `s` key (foreshortening),
-- and the depth of each part its `z` key (who is in front).
--
-- A MAP is boneId -> joint index (the bone's head). The bone's tail is the
-- joint of its connected child when that child is mapped below the head,
-- else the head's source child with the tallest subtree (the average when
-- several tie, like the fingers of a hand).
--
-- CAMERA: { yaw, pitch, roll (degrees), zUp }. A point goes through
-- Rz(roll) Rx(pitch) Ry(yaw) (after Z-up -> Y-up when zUp); the screen is
-- x right, y down, and depth grows towards the viewer. Yaw 90 shows a
-- character that faces +Z (the glTF convention) from its side, facing
-- right; positive pitch looks from above.

local M = {}

M.Rig, M.Anim, M.Ik = nil, nil, nil

function M.bind(Rig, Anim, Ik)
  M.Rig, M.Anim, M.Ik = Rig, Anim, Ik
  return M
end

local cos, sin, pi, sqrt, abs, floor = math.cos, math.sin, math.pi, math.sqrt, math.abs, math.floor
local min, max = math.min, math.max
local RAD, DEG = pi / 180, 180 / pi

local function angleOf(dx, dy)
  if math.atan2 then return math.atan2(dy, dx) * DEG end
  return math.atan(dy, dx) * DEG
end

local function round(v) return floor(v + 0.5) end

M.PRESETS = { "Side", "Front", "Back", "3/4" }
M.PRESET = {
  Side = { yaw = 90, pitch = 0, roll = 0 },
  Front = { yaw = 0, pitch = 0, roll = 0 },
  Back = { yaw = 180, pitch = 0, roll = 0 },
  ["3/4"] = { yaw = 45, pitch = 10, roll = 0 },
}
M.HOLD_RATIO = 0.2    -- below this projected / 3D length the angle is held
M.FPS = 12            -- default frame rate of the resulting clip
-- IK dummies keep a key on every frame: near full reach a hair of target
-- error is degrees of knee
M.TOL = { rot = 0.5, s = 0.01, x = 0.25, y = 0.25 }
M.DEPTH_MARGIN = 0.03 -- of the skeleton height: closer than this to the
                      -- depth reference, a part keeps its own z
M.DEPTH_HOLD = 1.5    -- a part leaves its side only past this many margins

---------------------------------------
-- Camera --
---------------------------------------

--- A camera table with its rotation matrix (rows) precomputed.
function M.camera(yaw, pitch, roll, zUp)
  local cy, sy = cos((yaw or 0) * RAD), sin((yaw or 0) * RAD)
  local cp, sp = cos((pitch or 0) * RAD), sin((pitch or 0) * RAD)
  local cr, sr = cos((roll or 0) * RAD), sin((roll or 0) * RAD)
  -- Ry
  local a = { { cy, 0, sy }, { 0, 1, 0 }, { -sy, 0, cy } }
  local rx = { { 1, 0, 0 }, { 0, cp, -sp }, { 0, sp, cp } }
  local rz = { { cr, -sr, 0 }, { sr, cr, 0 }, { 0, 0, 1 } }
  local function mul(p, q)
    local r = {}
    for i = 1, 3 do
      r[i] = {}
      for j = 1, 3 do
        r[i][j] = p[i][1] * q[1][j] + p[i][2] * q[2][j] + p[i][3] * q[3][j]
      end
    end
    return r
  end
  return { yaw = yaw or 0, pitch = pitch or 0, roll = roll or 0, zUp = zUp and true or false,
    m = mul(rz, mul(rx, a)) }
end

--- Screen x, y (y down) and depth (towards the viewer) of a world point.
function M.project(cam, x, y, z)
  if cam.zUp then y, z = z, -y end
  local m = cam.m
  local px = m[1][1] * x + m[1][2] * y + m[1][3] * z
  local py = m[2][1] * x + m[2][2] * y + m[2][3] * z
  local pz = m[3][1] * x + m[3][2] * y + m[3][3] * z
  return px, -py, pz
end

--- Every joint of a flat position list projected: a flat list sx, sy, depth.
function M.projectAll(cam, flat)
  local out = {}
  for i = 1, #flat, 3 do
    out[i], out[i + 1], out[i + 2] = M.project(cam, flat[i], flat[i + 1], flat[i + 2])
  end
  return out
end

--- True when the rest pose stands along Z (Blender, some BVH) instead of Y.
function M.isZUp(skel)
  local r = skel.rest
  local y0, y1, z0, z1 = math.huge, -math.huge, math.huge, -math.huge
  for i = 1, #r, 3 do
    y0, y1 = min(y0, r[i + 1]), max(y1, r[i + 1])
    z0, z1 = min(z0, r[i + 2]), max(z1, r[i + 2])
  end
  return (z1 - z0) > (y1 - y0) * 1.2
end

---------------------------------------
-- Skeleton tree --
---------------------------------------

--- Children lists and subtree heights of a skel (cached on it).
function M.tree(skel)
  if skel._tree then return skel._tree end
  local n = #skel.joints
  local kids, height, depth = {}, {}, {}
  for j = 1, n do kids[j], height[j] = {}, 0 end
  for j = 1, n do
    local p = skel.joints[j].parent or 0
    depth[j] = p > 0 and (depth[p] or 0) + 1 or 0
    if p > 0 then kids[p][#kids[p] + 1] = j end
  end
  -- parents come before children: one backwards pass
  for j = n, 1, -1 do
    local p = skel.joints[j].parent or 0
    if p > 0 then height[p] = max(height[p], height[j] + 1) end
  end
  local sided, slot = {}, {}
  for j = 1, n do
    local sl, side = M.classify(skel.joints[j].name)
    sided[j], slot[j] = side ~= nil, sl
  end
  -- joints sitting on their parent (3ds Max's Biped stacks Neck and Neck01
  -- on one point): a segment of length zero has no direction to follow
  local same = {}
  local r = skel.rest
  if r and #r >= 3 * n then
    local lo, hi = math.huge, -math.huge
    for i = 1, 3 * n do lo, hi = min(lo, r[i]), max(hi, r[i]) end
    local eps = max(1e-9, (hi - lo) * 1e-3)
    for j = 1, n do
      local p = skel.joints[j].parent or 0
      if p > 0 then
        local d = sqrt((r[3 * j - 2] - r[3 * p - 2]) ^ 2 + (r[3 * j - 1] - r[3 * p - 1]) ^ 2
          + (r[3 * j] - r[3 * p]) ^ 2)
        same[j] = d < eps
      end
    end
  end
  skel._tree = { kids = kids, height = height, depth = depth, sided = sided, slot = slot,
    same = same }
  return skel._tree
end

--- True when joint `j` is `anc` or below it.
function M.isBelow(skel, j, anc)
  local guard = 0
  while j and j > 0 and guard <= #skel.joints do
    if j == anc then return true end
    j = skel.joints[j].parent
    guard = guard + 1
  end
  return false
end

--- The joints that stand for the tail of a segment starting at `j`: its
--- child (or children, when they tie) with the tallest subtree. A joint
--- with no side carries on through a child with no side when it has one
--- (the chest goes on to the neck, not to the shoulders), and a named
--- trunk joint through a named child. When nothing fits (a leaf, or
--- Rigify's head with only its face below) a named joint carries on its
--- parent's way: `{ ext = parent, head = j }`. Else empty.
function M.defaultTail(skel, j)
  local t = M.tree(skel)
  local kids = t.kids[j]
  local slot = t.slot[j]
  -- the parent to carry on from: the first one not on the same point
  local p, q = skel.joints[j].parent or 0, j
  while p > 0 and t.same[q] do q, p = p, skel.joints[p].parent or 0 end
  local ext = slot and slot ~= "end" and p > 0 and { ext = p, head = j } or {}
  if #kids == 0 then return ext end
  if not t.sided[j] then
    local central = {}
    for _, c in ipairs(kids) do
      if not t.sided[c] then central[#central + 1] = c end
    end
    if #central > 0 then
      kids = central
      if slot and slot ~= "end" then
        local named = {}
        for _, c in ipairs(central) do
          -- the face hangs off the head, it does not carry it on
          if t.slot[c] and t.slot[c] ~= "face" then named[#named + 1] = c end
        end
        if #named == 0 then return ext end
        kids = named
      end
    end
  end
  local best, list = -1, {}
  for _, c in ipairs(kids) do
    local h = t.height[c]
    if h > best then best, list = h, { c }
    elseif h == best then list[#list + 1] = c end
  end
  -- a child on the same point is no tail: go on through it
  if #list == 1 and t.same[list[1]] then
    local deeper = M.defaultTail(skel, list[1])
    if deeper.ext then return ext end
    return deeper
  end
  return list
end

---------------------------------------
-- Names --
---------------------------------------

-- body words a side letter may stick to ("lfemur", "rhand", "LUpperArm")
local WORDS = { "arm", "uparm", "upperarm", "forearm", "lowerarm", "hand", "wrist", "humerus",
  "radius", "ulna", "clavicle", "collar", "shoulder", "leg", "upleg", "upperleg", "lowerleg",
  "thigh", "femur", "tibia", "shin", "calf", "knee", "foot", "ankle", "toe", "toes", "ball",
  "hip", "elbow", "finger", "thumb", "index", "middle", "ring", "pinky", "eye", "ear", "wing" }
local WORD = {}
for _, w in ipairs(WORDS) do WORD[w] = true end

--- A joint or bone name without namespace and rig prefixes, lowercase.
function M.normName(name)
  local s = tostring(name or "")
  s = s:gsub("^.*:", "")                       -- "mixamorig:Hips"
  s = s:lower()
  s = s:gsub("^mixamorig%d*_?", ""):gsub("^bip%d*[ _]?", ""):gsub("^def[%-_]", "")
  s = s:gsub("^j_bip_c_", ""):gsub("^j_bip_l_", "l_"):gsub("^j_bip_r_", "r_")
  return s
end

local SEP = "[_%.%s%-]"

--- The side of a normalized name ("L" | "R" | nil) and the name without it.
local function sideOf(n)
  local core = n
  for _, p in ipairs({ { "left", "L" }, { "right", "R" } }) do
    local a, b = core:find(p[1], 1, true)
    if a then return p[2], (core:sub(1, a - 1) .. core:sub(b + 1)) end
  end
  for _, p in ipairs({ { "l", "L" }, { "r", "R" } }) do
    local s = p[1]
    if core:match("^" .. s .. SEP) then return p[2], core:sub(3) end
    if core:match(SEP .. s .. "$") then return p[2], core:sub(1, -3) end
    local a = core:find(SEP .. s .. SEP)
    if a then return p[2], core:sub(1, a) .. core:sub(a + 3) end
  end
  -- a side letter glued to a body word: "lfemur", "rhand", "lupperarm"
  local first, rest = core:sub(1, 1), core:sub(2)
  if (first == "l" or first == "r") then
    local w = rest:gsub(SEP, ""):gsub("%d+", "")
    for word in pairs(WORD) do
      if w:sub(1, #word) == word and #w - #word <= 6 then
        return first == "l" and "L" or "R", rest
      end
    end
  end
  return nil, core
end

-- slot rules, checked in order on the side-less name (separators gone)
local RULES = {
  { "end", { "end$", "top$", "nub$", "tip$", "site$" } },
  { "finger", { "thumb", "index", "middle", "ring", "pinky", "finger", "digit", "fing" } },
  { "twist", { "twist", "roll" } },
  { "hipjoint", { "hipjoint" } },
  { "toe", { "toe", "ball" } },
  { "foot", { "foot", "ankle", "paw" } },
  { "hand", { "hand", "wrist", "palm" } },
  { "forearm", { "forearm", "lowerarm", "lowarm", "elbow", "radius", "ulna" } },
  { "clavicle", { "clavicle", "collar", "shoulder" } },
  { "upperarm", { "upperarm", "uparm", "humerus", "arm" } },
  { "thigh", { "upleg", "upperleg", "thigh", "femur", "hip" } },
  { "shin", { "lowerleg", "lowleg", "shin", "calf", "knee", "tibia", "leg" } },
  { "neck", { "neck" } },
  { "face", { "forehead", "temple", "jaw", "cheek", "brow", "lid", "lip", "nose", "ear", "eye",
    "teeth", "tongue", "chin", "face", "mouth" } },
  { "head", { "head", "skull" } },
  { "hips", { "hips", "pelvis", "hip" } },
  { "spine", { "spine", "chest", "back", "torso", "thorax", "abdomen", "waist", "belly", "body" } },
  { "tail", { "tail" } },
  { "wing", { "wing" } },
}

--- The canonical slot of a name and its side: "thigh", "L". Unknown
--- names give nil. A sided "hip" is a thigh; a centred one the hips.
function M.classify(name)
  local side, core = sideOf(M.normName(name))
  local flat = core:gsub(SEP, "")
  local bare = flat:gsub("%d+", "")
  for _, r in ipairs(RULES) do
    for _, pat in ipairs(r[2]) do
      if bare:find(pat) then
        local slot = r[1]
        if slot == "thigh" and pat == "hip" and not side then slot = "hips" end
        if slot == "hips" and side then slot = pat == "hip" and "thigh" or "hipjoint" end
        if slot == "end" then
          -- "headtop_end": still an end; the slot of what it ends
          return "end", side
        end
        return slot, side
      end
    end
  end
  return nil, side
end

-- slots mapped one to one (the torso is shared out apart)
local LIMBS = { "neck", "head", "clavicle", "upperarm", "forearm", "hand", "thigh", "shin",
  "foot", "toe", "tail", "wing" }

---------------------------------------
-- Mapping --
---------------------------------------

--- The source joints of each slot..side, shallowest first.
local function jointsBySlot(skel)
  local t = M.tree(skel)
  local by = {}
  for j, jt in ipairs(skel.joints) do
    -- a joint stacked on a named parent duplicates it (Neck01 on Neck); on
    -- an unnamed one (Pelvis on the Bip01 root) it is the real one
    local dupOfParent = t.same[j] and t.slot[jt.parent] ~= nil
    if not jt.isEnd and not dupOfParent then
      local slot, side = M.classify(jt.name)
      if slot then
        local key = slot .. (side or "")
        by[key] = by[key] or {}
        by[key][#by[key] + 1] = j
      end
    end
  end
  for _, list in pairs(by) do
    table.sort(list, function(a, b)
      if t.depth[a] ~= t.depth[b] then return t.depth[a] < t.depth[b] end
      return a < b
    end)
  end
  return by
end

--- An automatic map of the rig's bones onto the skel's joints by names.
--- Returns the map and how many bones it assigned.
local TRUNK = { hips = true, spine = true, neck = true, head = true }

--- The source's trunk: from its hips (else its shallowest spine joint) on
--- through central, named trunk children (the tallest when several).
local function sourceTrunk(skel, by)
  local t = M.tree(skel)
  local j = by.hips and by.hips[1] or by.spine and by.spine[1]
  local chain = {}
  while j do
    chain[#chain + 1] = j
    local nextJ, best = nil, -1
    for _, c in ipairs(t.kids[j]) do
      if not t.sided[c] and TRUNK[t.slot[c] or ""] and t.height[c] > best then
        nextJ, best = c, t.height[c]
      end
    end
    j = nextJ
  end
  return chain
end

--- An automatic map of the rig's bones onto the skel's joints by names.
--- Returns the map and how many bones it assigned.
---
--- Limbs go one to one by slot and side. The trunk (hips, spine, neck,
--- head bones with no side) is shared out along the source's trunk: a neck
--- or head found on both sides is an anchor, and the bones between anchors
--- spread evenly over the joints between them (Mixamo has three spine
--- joints and a template one; Rigify names its whole trunk spine.00N).
function M.autoMap(doc, skel)
  local Rig = M.Rig
  local by = jointsBySlot(skel)
  local map, n = {}, 0
  local function first(key) return by[key] and by[key][1] end
  local rigTrunk = {}
  for _, b in ipairs(doc.bones) do
    local slot, side = M.classify(b.name)
    if TRUNK[slot or ""] and not side then
      rigTrunk[#rigTrunk + 1] = { b = b, slot = slot }
    elseif slot then
      local j = first(slot .. (side or ""))
      if j then map[b.id], n = j, n + 1 end
    end
  end
  local chain = sourceTrunk(skel, by)
  local N = #chain
  if N > 0 and #rigTrunk > 0 then
    local t = M.tree(skel)
    local where = {}
    for i, j in ipairs(chain) do
      local sl = t.slot[j]
      if (sl == "neck" or sl == "head") and not where[sl] then where[sl] = i end
    end
    -- anchors: rig bones whose slot the source trunk has (in order)
    local fixed, last = {}, 0
    for i, e in ipairs(rigTrunk) do
      local k = where[e.slot]
      if k and k > last then fixed[i], last = k, k end
    end
    local i = 1
    local m = #rigTrunk
    while i <= m do
      if fixed[i] then
        map[rigTrunk[i].b.id], n = chain[fixed[i]], n + 1
        i = i + 1
      else
        -- a run of free bones up to the next anchor (or the end)
        local r = i
        while r + 1 <= m and not fixed[r + 1] do r = r + 1 end
        local lo = 1
        for q = i - 1, 1, -1 do if fixed[q] then lo = fixed[q] + 1 break end end
        local hi = r < m and fixed[r + 1] and fixed[r + 1] - 1 or N
        local cnt = r - i + 1
        for q = 0, cnt - 1 do
          local idx
          if r < m then idx = lo + round(q * (hi - lo + 1) / cnt)
          elseif cnt == 1 then idx = lo
          else idx = lo + round(q * (hi - lo) / (cnt - 1)) end
          idx = max(1, min(N, idx))
          map[rigTrunk[i + q].b.id], n = chain[idx], n + 1
        end
        i = r + 1
      end
    end
  end
  -- drop what does not exist any more
  for id in pairs(map) do
    if not Rig.bone(doc, id) then map[id] = nil end
  end
  return map, n
end

--- The tail joints of bone `b` under `map` (a list; empty = no tail).
function M.tailOf(doc, skel, map, b)
  local Rig = M.Rig
  local j = map[b.id]
  if not j then return {} end
  for _, c in ipairs(Rig.children(doc, b.id)) do
    local k = c.connected and map[c.id]
    if k and k ~= j and M.isBelow(skel, k, j) then return { k } end
  end
  return M.defaultTail(skel, j)
end

---------------------------------------
-- Sampling --
---------------------------------------

--- Joint positions of `clip` at time `t` (seconds), linear between its
--- baked frames. Returns a flat list.
function M.positionsAt(clip, t)
  local n = clip.count
  if n <= 1 then return clip.pos[1] end
  local u = t / clip.frameTime + 1
  if u <= 1 then return clip.pos[1] end
  if u >= n then return clip.pos[n] end
  local i = floor(u)
  local k = u - i
  local a, b = clip.pos[i], clip.pos[i + 1]
  if k < 1e-9 then return a end
  local out = {}
  for q = 1, #a do out[q] = a[q] + (b[q] - a[q]) * k end
  return out
end

--- Seconds the clip lasts (from its first baked frame to its last).
function M.duration(clip)
  return max(0, (clip.count - 1) * clip.frameTime)
end

--- The default frame count of the resulting clip: the source at M.FPS.
function M.defaultFrames(clip, loop)
  local d = M.duration(clip)
  local n = round(d * M.FPS) + (loop and 0 or 1)
  return max(2, min(M.Anim.MAX_FRAMES, n))
end

--- True when a tail list (defaultTail) gives a point.
function M.hasTail(list)
  return #list > 0 or list.ext ~= nil
end

local function avg(flat, list)
  if list.ext then
    local h, p = list.head, list.ext
    return 2 * flat[3 * h - 2] - flat[3 * p - 2], 2 * flat[3 * h - 1] - flat[3 * p - 1],
      2 * flat[3 * h] - flat[3 * p]
  end
  local x, y, z = 0, 0, 0
  for _, j in ipairs(list) do
    x, y, z = x + flat[3 * j - 2], y + flat[3 * j - 1], z + flat[3 * j]
  end
  local n = #list
  return x / n, y / n, z / n
end

local function at(flat, j) return flat[3 * j - 2], flat[3 * j - 1], flat[3 * j] end

---------------------------------------
-- Retarget --
---------------------------------------

--- Everything the per-frame solve needs, worked out once.
--- opts: frames, loop, inPlace, foreshorten, depthOrder, motion (factor),
--- followJoints (loose bones' heads follow their joints, default on),
--- rootFrom ("rest" default, or "first" for a rig built from frame 1),
--- depthFrom (the joint whose depth is 0 for Depth order; default the
--- root bone's joint, the hips).
function M.prepare(doc, skel, clipIndex, map, cam, opts)
  local Rig = M.Rig
  opts = opts or {}
  local clip = skel.clips[clipIndex or 1]
  local tails, l3, l3sum, l2sum = {}, {}, 0, 0
  local ref = clip.pos[1]
  for _, b in ipairs(doc.bones) do
    local list = M.tailOf(doc, skel, map, b)
    if M.hasTail(list) then
      tails[b.id] = list
      local hx, hy, hz = at(ref, map[b.id])
      local tx, ty, tz = avg(ref, list)
      local l = sqrt((tx - hx) ^ 2 + (ty - hy) ^ 2 + (tz - hz) ^ 2)
      l3[b.id] = l
      l3sum, l2sum = l3sum + l, l2sum + b.len
    end
  end
  -- loose bones (a shoulder or a thigh hanging off the trunk) whose head
  -- follows its joint: where the joint sits from the parent's, in the
  -- parent bone's own px per unit, so the rig's proportions hold
  local loose = {}
  if opts.followJoints ~= false then
    for _, b in ipairs(doc.bones) do
      local p = b.parent ~= 0 and Rig.bone(doc, b.parent)
      if p and not b.connected and map[b.id] and map[p.id] and map[b.id] ~= map[p.id]
        and (l3[p.id] or 0) > 1e-9 then
        loose[#loose + 1] = { b = b, p = p, k = p.len / l3[p.id] }
      end
    end
  end
  local loop = opts.loop ~= false
  local frames = opts.frames or M.defaultFrames(clip, loop)
  local dur = M.duration(clip)
  -- the parent's own px per unit, kept near the rig's average: a parent
  -- bone far longer or shorter than its source segment (a stub over a
  -- tiny segment) must not fling the shoulders off the canvas
  local g = l3sum > 1e-9 and l2sum / l3sum or 1
  for _, e in ipairs(loose) do e.k = max(0.5 * g, min(2 * g, e.k)) end
  local ctx = {
    doc = doc, skel = skel, clip = clip, map = map, cam = cam, tails = tails,
    scale = (l3sum > 1e-9 and l2sum / l3sum or 1) * (opts.motion or 1),
    frames = frames, loop = loop, dur = dur,
    inPlace = opts.inPlace and true or false,
    -- where the root's height is measured from: the rest pose (a rig of
    -- its own, standing) or the clip's first frame (a rig built from it)
    rootFromFirst = opts.rootFrom == "first",
    foreshorten = opts.foreshorten ~= false,
    depthOrder = opts.depthOrder ~= false,
    depthFrom = opts.depthFrom,
    restWorld = Rig.solve(doc),
    loose = loose,
  }
  -- the skeleton's height on screen, for the z bias
  local p = M.projectAll(cam, ref)
  local y0, y1 = math.huge, -math.huge
  for i = 2, #p, 3 do y0, y1 = min(y0, p[i]), max(y1, p[i]) end
  ctx.height = max(1e-6, y1 - y0)
  -- the root(s): they follow their joint (or only its height, in place)
  local roots = {}
  for _, b in ipairs(doc.bones) do
    if b.parent == 0 and map[b.id] then roots[#roots + 1] = b end
  end
  ctx.roots = roots
  -- Depth order's 0: the chosen joint, else the root's (the hips), else
  -- the first mapped bone's
  if not (ctx.depthFrom and skel.joints[ctx.depthFrom]) then
    ctx.depthFrom = roots[1] and map[roots[1].id]
    if not ctx.depthFrom then
      for _, b in ipairs(doc.bones) do
        if map[b.id] then ctx.depthFrom = map[b.id] break end
      end
    end
  end
  ctx.time = function(f)
    if frames <= 1 then return 0 end
    if loop then return (f - 1) / frames * dur end
    return (f - 1) / (frames - 1) * dur
  end
  ctx.frameMs = max(10, round(dur * 1000 / (loop and frames or max(1, frames - 1))))
  if dur <= 0 then ctx.frameMs = 100 end
  return ctx
end

--- The pose (rig.lua) of output frame `f` and each part's depth.
--- `prev` (the previous frame's result) holds angles that the projection
--- makes unstable and keeps rotations from jumping by whole turns.
function M.solveFrame(ctx, f, prev)
  local doc, map, cam = ctx.doc, ctx.map, ctx.cam
  local t = ctx.time(f)
  local P = M.positionsAt(ctx.clip, t)
  local pose, worldAngle, theta = {}, {}, {}
  local partDepth = {}  -- where each part joins the body: its first mapped bone's head
  for _, b in ipairs(doc.bones) do
    local parentAngle = b.parent ~= 0 and worldAngle[b.parent] or 0
    local list = ctx.tails[b.id]
    local d = { rot = 0, x = 0, y = 0, s = 1 }
    if list then
      local hx, hy, hz = at(P, map[b.id])
      local tx, ty, tz = avg(P, list)
      local l3 = sqrt((tx - hx) ^ 2 + (ty - hy) ^ 2 + (tz - hz) ^ 2)
      local ax, ay, ad = M.project(cam, hx, hy, hz)
      local bx, by, bd = M.project(cam, tx, ty, tz)
      local l2 = sqrt((bx - ax) ^ 2 + (by - ay) ^ 2)
      local ratio = l3 > 1e-9 and min(1, l2 / l3) or 1
      local th
      if ratio < M.HOLD_RATIO and prev and prev.theta[b.id] then
        th = prev.theta[b.id]
      elseif l2 > 1e-9 then
        th = angleOf(bx - ax, by - ay)
        if prev and prev.theta[b.id] then th = M.Anim.unwrap(prev.theta[b.id], th) end
      else
        th = parentAngle + b.angle
      end
      theta[b.id] = th
      d.rot = th - parentAngle - b.angle
      if prev and prev.pose[b.id] then d.rot = M.Anim.unwrap(prev.pose[b.id].rot, d.rot) end
      if ctx.foreshorten then d.s = max(M.Anim.MIN_SCALE, ratio) end
      worldAngle[b.id] = th
      local pid = b.part
      if partDepth[pid] == nil then partDepth[pid] = ad end
    else
      worldAngle[b.id] = parentAngle + b.angle
    end
    pose[b.id] = d
  end
  -- loose heads: the joint's offset from the parent's joint on screen,
  -- turned into the parent's frame (what a disconnected head stores)
  for _, e in ipairs(ctx.loose) do
    local b, p = e.b, e.p
    local jx, jy = M.project(cam, at(P, map[b.id]))
    local qx, qy = M.project(cam, at(P, map[p.id]))
    local dx, dy = (jx - qx) * e.k, (jy - qy) * e.k
    local a = worldAngle[p.id] * RAD
    local c, sn = cos(a), sin(a)
    pose[b.id].x = dx * c + dy * sn - b.x
    pose[b.id].y = -dx * sn + dy * c - b.y
  end
  -- the roots follow their joint (scaled into canvas px)
  for _, b in ipairs(ctx.roots) do
    local j = map[b.id]
    local x, y, z = at(P, j)
    local x0, y0, z0 = at(ctx.clip.pos[1], j)
    local r = ctx.skel.rest
    if not ctx.rootFromFirst and r and #r >= 3 * j then
      -- a clip that starts sitting: its hips sit below the rig's standing ones
      if cam.zUp then z0 = r[3 * j] else y0 = r[3 * j - 1] end
    end
    if ctx.inPlace then
      -- in place: the root keeps its spot on the ground plane (x z, or x y
      -- when Z is up) and only bobs; a clip that wanders (a cutscene, a
      -- turn) stays on the canvas, not only a straight walk
      if cam.zUp then x, y = x0, y0 else x, z = x0, z0 end
    end
    local sx, sy = M.project(cam, x, y, z)
    local rx, ry = M.project(cam, x0, y0, z0)
    pose[b.id].x = (sx - rx) * ctx.scale
    pose[b.id].y = (sy - ry) * ctx.scale
  end
  -- Depth order, measured from the reference joint (the hips): a part
  -- whose joint with the body (a hip, a shoulder, the neck) is clearly in
  -- front of it goes over everything level with it, one clearly behind
  -- under it, and inside each side the rig's own z order holds. Not the
  -- average depth of the whole body or limb: a pelvis that moves back or a
  -- leg that swings forward does not swap legs and torso; only a body
  -- that really turns does.
  local zs, side
  if ctx.depthOrder and ctx.depthFrom then
    local _, _, dref = M.project(cam, at(P, ctx.depthFrom))
    local margin = M.DEPTH_MARGIN * ctx.height
    local list = {}
    side = {}
    for i, p in ipairs(doc.parts) do
      local sd = 0
      if partDepth[p.id] then
        local rel = partDepth[p.id] - dref
        local was = prev and prev.side and prev.side[p.id] or 0
        -- a part already on a side stays there until it is well past
        local m = (was ~= 0 and (rel > 0) == (was > 0)) and margin / M.DEPTH_HOLD or margin
        if rel > m then sd = 1 elseif rel < -m then sd = -1 end
      end
      side[p.id] = sd
      list[#list + 1] = { p = p, sd = sd, i = i }
    end
    table.sort(list, function(a, b)
      if a.sd ~= b.sd then return a.sd < b.sd end
      if a.p.z ~= b.p.z then return a.p.z < b.p.z end
      return a.i < b.i
    end)
    zs = {}
    for i, e in ipairs(list) do zs[e.p.id] = i - 1 end
  end
  return { pose = pose, theta = theta, z = zs, side = side }
end

-- Ramer-Douglas-Peucker on a track's keys: the fewest keys whose linear
-- segments stay within `tol` of every dropped key.
local function simplify(keys, tol)
  local n = #keys
  if n <= 2 then return keys end
  local keep = { [1] = true, [n] = true }
  local stack = { { 1, n } }
  while #stack > 0 do
    local seg = table.remove(stack)
    local a, b = seg[1], seg[2]
    local fa, va, fb, vb = keys[a][1], keys[a][2], keys[b][1], keys[b][2]
    local worst, wi = -1, nil
    for i = a + 1, b - 1 do
      local u = (keys[i][1] - fa) / (fb - fa)
      local e = abs(keys[i][2] - (va + (vb - va) * u))
      if e > worst then worst, wi = e, i end
    end
    if wi and worst > tol then
      keep[wi] = true
      stack[#stack + 1] = { a, wi }
      stack[#stack + 1] = { wi, b }
    end
  end
  local out = {}
  for i = 1, n do if keep[i] then out[#out + 1] = keys[i] end end
  return out
end

local function setKey(anim, id, ch, f, v)
  M.Anim.setKey(anim, id, ch, f, v, M.Anim.STEPPED[ch] and "Step" or "Linear")
end

--- Keys for the live IK controllers whose chains are mapped: the target
--- on the end of the FK chain, the pole out from its middle joint. The
--- tip's key turns into its world offset when it is locked.
local function keyIK(ctx, anim, f, sol, prevPole)
  local doc, Rig, Ik = ctx.doc, M.Rig, M.Ik
  if not Ik or #doc.ik == 0 then return end
  local world = Rig.solve(doc, sol.pose)
  for _, c in ipairs(doc.ik) do
    if c.on and ctx.tails[c.upper] and ctx.tails[c.lower] then
      local wa, wb = world[c.upper], world[c.lower]
      local a = Rig.bone(doc, c.upper)
      local mx, my = (wa.hx + wb.tx) / 2, (wa.hy + wb.ty) / 2
      local dx, dy = wb.hx - mx, wb.hy - my
      local l = sqrt(dx * dx + dy * dy)
      local last = prevPole[c.id]
      if l < 0.05 * a.len and last then
        dx, dy, l = last[1], last[2], 1
      elseif l < 1e-9 then
        local ux, uy = wb.tx - wa.hx, wb.ty - wa.hy
        local ul = max(1e-9, sqrt(ux * ux + uy * uy))
        dx, dy, l = uy / ul, -ux / ul, 1
      end
      dx, dy = dx / l, dy / l
      prevPole[c.id] = { dx, dy }
      local k = Ik.POLE_DIST * a.len
      setKey(anim, c.id, "tx", f, wb.tx)
      setKey(anim, c.id, "ty", f, wb.ty)
      setKey(anim, c.id, "px", f, wb.hx + dx * k)
      setKey(anim, c.id, "py", f, wb.hy + dy * k)
      if c.tip ~= 0 and c.lockTip and sol.theta[c.tip] then
        local tr = M.Anim.track(anim, c.tip, "rot")
        local key = tr and tr.keys[#tr.keys]
        if key and key[1] == f then
          key[2] = sol.theta[c.tip] - ctx.restWorld[c.tip].angle
        end
      end
    end
  end
end

--- The whole clip: a new anim table (not yet in the doc).
--- `simplifyKeys` false keeps a key on every frame (the live preview).
function M.buildClip(ctx, name, simplifyKeys)
  local Rig, Anim = M.Rig, M.Anim
  local doc = ctx.doc
  local anim = Rig.newAnim(Rig.clipName(doc, name or "Imported"))
  anim.frames, anim.frameMs, anim.loop, anim.ease = ctx.frames, ctx.frameMs, ctx.loop, "Linear"
  local prev, prevPole, lastZ = nil, {}, {}
  for f = 1, ctx.frames do
    local sol = M.solveFrame(ctx, f, prev)
    for _, b in ipairs(doc.bones) do
      local d = sol.pose[b.id]
      if ctx.tails[b.id] then
        setKey(anim, b.id, "rot", f, d.rot)
        if ctx.foreshorten then setKey(anim, b.id, "s", f, d.s) end
      end
    end
    for _, b in ipairs(ctx.roots) do
      setKey(anim, b.id, "x", f, sol.pose[b.id].x)
      setKey(anim, b.id, "y", f, sol.pose[b.id].y)
    end
    for _, e in ipairs(ctx.loose) do
      setKey(anim, e.b.id, "x", f, sol.pose[e.b.id].x)
      setKey(anim, e.b.id, "y", f, sol.pose[e.b.id].y)
    end
    if sol.z then
      for _, p in ipairs(doc.parts) do
        local z = sol.z[p.id]
        if z and z ~= lastZ[p.id] then
          setKey(anim, p.id, "z", f, z)
          lastZ[p.id] = z
        end
      end
    end
    keyIK(ctx, anim, f, sol, prevPole)
    prev = sol
  end
  if simplifyKeys ~= false then
    local kept = {}
    for _, t in ipairs(anim.tracks) do
      local tol = M.TOL[t.ch]
      if tol then t.keys = simplify(t.keys, tol) end
      -- a track that never leaves its rest value says nothing
      local neutral = Anim.neutral(t.ch)
      local flat = true
      for _, k in ipairs(t.keys) do
        if abs(k[2] - neutral) > (tol or 0) then flat = false break end
      end
      local ikCh = t.ch == "tx" or t.ch == "ty" or t.ch == "px" or t.ch == "py"
      if not flat or ikCh or t.ch == "z" then kept[#kept + 1] = t end
    end
    anim.tracks = kept
  end
  return anim
end

---------------------------------------
-- New rig --
---------------------------------------

local PART_OF = {
  clavicle = "arm", upperarm = "arm", forearm = "arm", hand = "arm",
  thigh = "leg", shin = "leg", foot = "leg",
  head = "head", tail = "tail", wing = "wing",
}
local PART_Z = { leg = -1, tail = -1, wing = -2, torso = 0, body = 0, head = 1, arm = 2 }
-- skin radius (head end) as a share of the rig's height
local RADIUS = { hips = 0.08, spine = 0.08, neck = 0.035, head = 0.085, clavicle = 0.04,
  upperarm = 0.04,
  forearm = 0.034, hand = 0.032, thigh = 0.055, shin = 0.045, foot = 0.032, tail = 0.04,
  wing = 0.035 }
-- joints that never become bones of their own (the clavicle does: it is
-- what lifts and swings the shoulder)
local SKIP = { finger = true, twist = true, toe = true, hipjoint = true,
  face = true, ["end"] = true }

M.LEAVE_OUT = "Leave out"
M.PART_NAMES = { "torso", "head", "arm_L", "arm_R", "leg_L", "leg_R", "tail", "wing_L",
  "wing_R", "body" }

--- The draw order of a part from its name ("arm_L" -> the arms' z).
function M.partZ(name)
  return PART_Z[(tostring(name):match("^(%a+)") or ""):lower()] or 0
end

--- Which joints become bones of a new rig and the part each one gets,
--- before the user changes anything: { { j, slot, side, part, up }, ... }
--- in joint order (parents first; `up` is the nearest kept ancestor's
--- entry). A skeleton with named limbs (a humanoid) leaves out fingers,
--- twist and face bones, toes and its unnamed extras, and parts
--- go by name (arm_L, leg_R, head, torso). Any other keeps every joint,
--- and each branch off a chain is a part of its own (body, part 2...).
function M.rigPlan(skel)
  if skel._plan then return skel._plan end
  local t = M.tree(skel)
  local humanoid = false
  for j = 1, #skel.joints do
    if t.slot[j] == "upperarm" or t.slot[j] == "thigh" then humanoid = true break end
  end
  local plan, byJ = {}, {}
  for j, jt in ipairs(skel.joints) do
    local slot, side = M.classify(jt.name)
    -- a limb slot once per side: Rigify's palms under the hand stay out
    local dup = false
    if slot and side and slot ~= "spine" then
      local q = jt.parent
      while q and q > 0 do
        local s2, d2 = M.classify(skel.joints[q].name)
        if s2 == slot and d2 == side then dup = true break end
        q = skel.joints[q].parent
      end
    end
    -- stacked on a kept parent: the same pivot twice (Neck01 on Neck)
    local stacked = t.same[j] and byJ[jt.parent] ~= nil
    local keep = not jt.isEnd and not dup and not stacked and M.hasTail(M.defaultTail(skel, j))
    if humanoid then keep = keep and slot ~= nil and not SKIP[slot] end
    if keep then
      local e = { j = j, slot = slot, side = side }
      local q = jt.parent
      while q and q > 0 and not byJ[q] do q = skel.joints[q].parent end
      e.up = q and q > 0 and byJ[q] or nil
      plan[#plan + 1], byJ[j] = e, e
    end
  end
  if humanoid then
    for _, e in ipairs(plan) do
      local rs = e.slot
      -- the top of the trunk is the head even when named spine.006
      if (rs == "spine" or rs == "hips") and M.defaultTail(skel, e.j).ext and e.up then rs = "head" end
      e.rslot = rs
      local kind = PART_OF[rs or ""]
      if kind then
        local sided = kind ~= "head" and kind ~= "tail" and e.side
        e.part = kind .. (sided and ("_" .. e.side) or "")
      elseif e.up then e.part = e.up.part
      else e.part = "torso" end
    end
  else
    -- each branch is a part: the child carrying the tallest subtree goes
    -- on with its parent's part, the others start one
    local kids = {}
    for _, e in ipairs(plan) do
      if e.up then
        kids[e.up] = kids[e.up] or {}
        table.insert(kids[e.up], e)
      end
    end
    local n = 0
    for _, e in ipairs(plan) do
      if not e.up then
        n = n + 1
        e.part = n == 1 and "body" or ("part " .. n)
      end
      local main
      for _, c in ipairs(kids[e] or {}) do
        if not main or t.height[c.j] > t.height[main.j] then main = c end
      end
      for _, c in ipairs(kids[e] or {}) do
        if c == main then c.part = e.part
        else
          n = n + 1
          c.part = "part " .. n
        end
      end
    end
  end
  skel._plan = plan
  return plan
end

--- Builds the rig's bones and parts from the skeleton seen through `cam`
--- at the first frame of clip `clipIndex`, its size (the longest side of
--- its 3D box, whatever the pose: a clip may start lying down) at 80 % of
--- the sprite's height (its bounds in the canvas) and its lowest point
--- near the sprite's bottom. `doc` must have no bones. opts: `parts` { [joint] = part name
--- | false (left out) } over the plan's (rigPlan), `thick` a factor on
--- the skin radii. Returns the map (bone -> joint).
function M.buildRig(doc, skel, clipIndex, cam, opts)
  local Rig = M.Rig
  opts = opts or {}
  local over = opts.parts or {}
  local thick = opts.thick or 1
  local clip = skel.clips[clipIndex or 1]
  local P = clip.pos[1]
  local S = M.projectAll(cam, P)
  local x0, x1, y0, y1 = math.huge, -math.huge, math.huge, -math.huge
  for i = 1, #S, 3 do
    x0, x1 = min(x0, S[i]), max(x1, S[i])
    y0, y1 = min(y0, S[i + 1]), max(y1, S[i + 1])
  end
  local bd = Rig.bounds(doc)
  local W, H = bd.w, bd.h
  -- over the whole clip: one that starts sitting and then stands up with
  -- its arms raised must still fit, and stand on the same ground
  local size, low = 0, y1
  local step = max(1, floor(clip.count / 60))
  for f = 1, clip.count, step do
    local Q = clip.pos[f]
    for axis = 0, 2 do
      local lo, hi = math.huge, -math.huge
      for i = 1 + axis, #Q, 3 do lo, hi = min(lo, Q[i]), max(hi, Q[i]) end
      size = max(size, hi - lo)
    end
    for i = 1, #Q, 3 do
      local _, sy = M.project(cam, Q[i], Q[i + 1], Q[i + 2])
      low = max(low, sy)
    end
  end
  local k = 0.8 * H / max(1e-6, size)
  local ox = bd.x + W / 2 - (x0 + x1) / 2 * k
  local oy = bd.y + H * 0.9 - low * k
  local function screen(j) return S[3 * j - 2] * k + ox, S[3 * j - 1] * k + oy end

  local parts = {}
  local function partFor(name)
    if not parts[name] then
      local p = Rig.addPart(doc, name)
      p.z = M.partZ(name)
      parts[name] = p
    end
    return parts[name].id
  end

  local map, boneOf, bySlot = {}, {}, {}
  for _, e in ipairs(M.rigPlan(skel)) do
    local j = e.j
    local partName = over[j]
    if partName == nil then partName = e.part end
    if partName then
      local jt = skel.joints[j]
      local tails = M.defaultTail(skel, j)
      -- the nearest ancestor that is a bone
      local pj = jt.parent
      while pj and pj > 0 and not boneOf[pj] do pj = skel.joints[pj].parent end
      local parent = pj and pj > 0 and boneOf[pj] or nil
      local hx, hy = screen(j)
      local tx, ty, tz = avg(P, tails)
      local px, py, pz = at(P, j)
      local l3 = sqrt((tx - px) ^ 2 + (ty - py) ^ 2 + (tz - pz) ^ 2) * k
      local sx, sy = M.project(cam, tx, ty, tz)
      sx, sy = sx * k + ox, sy * k + oy
      local dx, dy = sx - hx, sy - hy
      local dl = sqrt(dx * dx + dy * dy)
      if dl < 1e-6 then dx, dy, dl = 0, 1, 1 end
      local len = max(Rig.MIN_LEN * 1.001, l3)
      local connected = false
      if parent then
        local pt = M.tailOf(doc, skel, map, parent)
        connected = #pt == 1 and pt[1] == j
        if connected then
          local w = Rig.solve(doc)[parent.id]
          hx, hy = w.tx, w.ty
        end
      end
      local r = max(1, 0.8 * H * (RADIUS[e.rslot or e.slot or ""] or 0.03) * thick)
      local b = Rig.addBone(doc, hx, hy, hx + dx / dl * len, hy + dy / dl * len, {
        parent = parent and parent.id or 0, connected = connected,
        name = tostring(jt.name):gsub("^.*:", ""):sub(1, Rig.MAX_NAME),
        part = partFor(partName), rHead = r, rTail = r * 0.85,
      })
      if b then
        map[b.id], boneOf[j] = j, b
        if e.slot and e.side then bySlot[e.slot .. e.side] = b end
      end
    end
  end
  -- twins for the mirror tools
  for key, b in pairs(bySlot) do
    local other = bySlot[key:sub(1, -2) .. (key:sub(-1) == "L" and "R" or "L")]
    if other then b.twin = other.id end
  end
  return map
end

return M
