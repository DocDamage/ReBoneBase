-- ik.lua — live IK controllers of a Bonebase rig. Pure Lua, dual-VM.
--
-- A CONTROLLER drives a two-bone chain (upper -> lower, connected) and,
-- optionally, the bone at its end (`tip`: a hand, a foot):
--
--   { id, upper, lower, tip = 0 | id,
--     on = true,            -- off: the chain is plain FK again
--     stretch = false,      -- reach further than the bones by stretching
--     maxStretch = 1.25,    -- ... up to this factor of their length
--     lockTip = true }      -- the tip keeps its angle in the WORLD
--
-- It has two dummies, both animated with keys on the controller's id
-- (anim.lua tracks with channels tx ty / px py), in CANVAS coordinates:
--   target (tx, ty)  where the end of `lower` goes (the wrist, the ankle);
--   pole   (px, py)  which side the middle joint bends to (the elbow, the
--                    knee): in 2D, with fixed lengths, base and target fix
--                    the joint up to that side, so the pole picks the side.
-- Without keys, a dummy sits where the chain is in the REST pose (so a new
-- controller changes nothing until it is moved).
--
-- Because the dummies are absolute, a foot stays planted while the hips
-- move, between keys too: the chain is solved again on every frame.
-- `solve` returns the pose with the IK applied (FK rotations of the chain
-- replaced, `s` set when stretched) and its world; export, onion and the
-- editor all use it, so what is drawn is what gets exported.
--
-- doc.ground = { on, y, stop }: with `stop`, a chain's tip never goes
-- below y (the target is lifted until the tip's skin rests on it).

local M = {}

M.Rig, M.Anim = nil, nil

function M.bind(Rig, Anim)
  M.Rig, M.Anim = Rig, Anim
  return M
end

M.CHANNELS = { "tx", "ty", "px", "py" }
M.POLE_DIST = 0.75   -- default pole: this many upper-lengths out from the joint

local sqrt, abs, cos, sin, pi = math.sqrt, math.abs, math.cos, math.sin, math.pi
local RAD = pi / 180

local function angleOf(dx, dy)
  if math.atan2 then return math.deg(math.atan2(dy, dx)) end
  return math.deg(math.atan(dy, dx))
end

---------------------------------------
-- Controllers --
---------------------------------------

function M.find(doc, id)
  for _, c in ipairs(doc.ik) do
    if c.id == id then return c end
  end
  return nil
end

--- The controller that drives (or ends at) bone `id`, or nil.
function M.forBone(doc, id)
  for _, c in ipairs(doc.ik) do
    if c.upper == id or c.lower == id or c.tip == id then return c end
  end
  return nil
end

--- A new controller on upper -> lower (-> tip), or nil + why.
function M.add(doc, upper, lower, tip)
  local Rig = M.Rig
  local a, b = Rig.bone(doc, upper), Rig.bone(doc, lower)
  if not a or not b or b.parent ~= a.id or not b.connected then
    return nil, "needs a bone hanging, connected, from its parent"
  end
  if M.forBone(doc, upper) or M.forBone(doc, lower) then return nil, "already in an IK chain" end
  local tb = tip and tip ~= 0 and Rig.bone(doc, tip) or nil
  if tb and tb.parent ~= b.id then tb = nil end
  local c = {
    id = doc.nextId, upper = upper, lower = lower, tip = tb and tb.id or 0,
    on = true, stretch = false, maxStretch = 1.25, lockTip = true,
  }
  doc.nextId = doc.nextId + 1
  doc.ik[#doc.ik + 1] = c
  return c
end

--- The controller a bone's chain would get: the bone as `lower` with its
--- parent as `upper`, its first connected child as `tip`.
function M.chainFor(doc, boneId)
  local Rig = M.Rig
  local b = Rig.bone(doc, boneId)
  if not b or b.parent == 0 or not b.connected then return nil end
  local tip = 0
  for _, c in ipairs(Rig.children(doc, b.id)) do
    if c.connected then tip = c.id break end
  end
  return b.parent, b.id, tip
end

function M.remove(doc, id)
  for i, c in ipairs(doc.ik) do
    if c.id == id then
      table.remove(doc.ik, i)
      for _, anim in ipairs(doc.clips or { doc.anim }) do
        local kept = {}
        for _, tr in ipairs(anim.tracks) do
          if tr.bone ~= id then kept[#kept + 1] = tr end
        end
        anim.tracks = kept
      end
      return true
    end
  end
  return false
end

--- Drops controllers whose bones are gone or no longer form a chain.
function M.prune(doc)
  local Rig = M.Rig
  local kept = {}
  for _, c in ipairs(doc.ik) do
    local a, b = Rig.bone(doc, c.upper), Rig.bone(doc, c.lower)
    if a and b and b.parent == a.id and b.connected then
      if c.tip ~= 0 then
        local t = Rig.bone(doc, c.tip)
        if not t or t.parent ~= b.id then c.tip = 0 end
      end
      kept[#kept + 1] = c
    end
  end
  doc.ik = kept
end

--- Every limb of the rig (a branch bone, its connected child and a
--- connected tip: arm + forearm + hand, thigh + shin + foot) gets a
--- controller. Returns how many were added.
function M.addLimbs(doc)
  local Rig = M.Rig
  local n = 0
  for _, b in ipairs(doc.bones) do
    if b.parent ~= 0 and not b.connected then
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
      if tip and M.add(doc, b.id, lower.id, tip.id) then n = n + 1 end
    end
  end
  return n
end

---------------------------------------
-- Dummies --
---------------------------------------

--- Where the dummies sit in the rest pose: the chain's end, and a pole out
--- from the middle joint on the side it bends to.
function M.restDummies(doc, c, restWorld)
  local Rig = M.Rig
  local w = restWorld or Rig.solve(doc)
  local wa, wb = w[c.upper], w[c.lower]
  local a = Rig.bone(doc, c.upper)
  local mx, my = (wa.hx + wb.tx) / 2, (wa.hy + wb.ty) / 2
  local dx, dy = wb.hx - mx, wb.hy - my
  local l = sqrt(dx * dx + dy * dy)
  if l < 1e-6 then
    -- a straight chain: put the pole to the chain's left (screen)
    local ux, uy = wb.tx - wa.hx, wb.ty - wa.hy
    local ul = sqrt(ux * ux + uy * uy)
    if ul < 1e-9 then ux, uy, ul = 1, 0, 1 end
    dx, dy, l = uy / ul, -ux / ul, 1
  end
  local k = M.POLE_DIST * a.len / l
  return wb.tx, wb.ty, wb.hx + dx * k, wb.hy + dy * k
end

--- The rig root's offset at `frame` of `anim` (rig.lua ROOT): 0, 0 with
--- no frame (Rest) or no keys.
function M.offset(doc, frame, anim)
  if not frame then return 0, 0 end
  local Anim = M.Anim
  anim = anim or doc.anim
  local tx, ty = Anim.track(anim, M.Rig.ROOT, "x"), Anim.track(anim, M.Rig.ROOT, "y")
  return tx and #tx.keys > 0 and Anim.value(anim, tx, frame) or 0,
    ty and #ty.keys > 0 and Anim.value(anim, ty, frame) or 0
end

--- The dummies of `c` at `frame` of `anim` (default the active clip), in
--- the canvas: keyed values where there are keys, the rest position
--- elsewhere, both carried by the rig root. Returns tx, ty, px, py.
function M.dummies(doc, c, frame, restWorld, anim)
  local Anim = M.Anim
  anim = anim or doc.anim
  local rtx, rty, rpx, rpy = M.restDummies(doc, c, restWorld)
  local out = { tx = rtx, ty = rty, px = rpx, py = rpy }
  if frame then
    for _, ch in ipairs(M.CHANNELS) do
      local tr = Anim.track(anim, c.id, ch)
      if tr and #tr.keys > 0 then out[ch] = Anim.value(anim, tr, frame) end
    end
    local gx, gy = M.offset(doc, frame, anim)
    out.tx, out.px = out.tx + gx, out.px + gx
    out.ty, out.py = out.ty + gy, out.py + gy
  end
  return out.tx, out.ty, out.px, out.py
end

---------------------------------------
-- Solving --
---------------------------------------

local function parentAngle(doc, world, b)
  if b.parent == 0 then return 0 end
  return world[b.parent].angle
end

--- Lowest point (largest y) of a bone's skin in `world`.
local function lowestOf(b, w)
  return math.max(w.hy + b.rHead, w.ty + b.rTail)
end

--- Solves one controller into `pose` given the current `world`. `at`
--- ({ x, y }) replaces the target (the ground stop) without touching keys.
local function solveOne(doc, c, pose, world, restWorld, frame, at, anim)
  local Rig, Anim = M.Rig, M.Anim
  local a, b = Rig.bone(doc, c.upper), Rig.bone(doc, c.lower)
  local tip = c.tip ~= 0 and Rig.bone(doc, c.tip) or nil
  local wa = world[a.id]
  local tx, ty, px, py = M.dummies(doc, c, frame, restWorld, anim)
  if at then tx, ty = at.x, at.y end
  local pa = pose[a.id] or { rot = 0, x = 0, y = 0 }
  local pb = pose[b.id] or { rot = 0, x = 0, y = 0 }
  -- the bones' OWN length keys (squash and stretch), kept aside like
  -- ownRot below: the stretch multiplies them, a second pass must not
  -- read the factor the first one wrote
  if pa.ownS == nil then pa.ownS = pa.s or 1 end
  if pb.ownS == nil then pb.ownS = pb.s or 1 end
  local l1, l2 = a.len * pa.ownS, b.len * pb.ownS
  local dx, dy = tx - wa.hx, ty - wa.hy
  local d = sqrt(dx * dx + dy * dy)
  local s = 1
  if c.stretch and d > l1 + l2 then s = math.min(d / (l1 + l2), c.maxStretch) end
  -- the pole's side of the base -> target line picks the bend
  local cross = dx * (py - wa.hy) - dy * (px - wa.hx)
  local bend
  if abs(cross) < 1e-9 then
    local rw = restWorld[a.id]
    bend = Anim.bendOf(rw.angle, restWorld[b.id].angle)
  else
    bend = cross < 0 and 1 or -1
  end
  local a1, a2 = Anim.solve2(wa.hx, wa.hy, l1 * s, l2 * s, tx, ty, bend)
  pa.rot = a1 - parentAngle(doc, world, a) - a.angle
  pb.rot = a2 - a1 - b.angle
  pa.s, pb.s = pa.ownS * s, pb.ownS * s
  pose[a.id], pose[b.id] = pa, pb
  if tip and c.lockTip then
    -- the tip keeps its rest angle in the world (plus its own key)
    local pt = pose[tip.id] or { rot = 0, x = 0, y = 0 }
    -- its OWN key, kept aside: a second pass (the ground) must not read
    -- the rotation the first pass wrote
    if pt.ownRot == nil then pt.ownRot = pt.rot or 0 end
    local own = pt.ownRot
    pt.rot = restWorld[tip.id].angle + own - a2 - tip.angle
    pose[tip.id] = pt
  end
  return tx, ty, s
end

--- The pose of `frame` with every live controller applied, and its world.
--- frame nil = the rest pose (no IK). `anim`: a clip (default the active
--- one; the export walks them all).
function M.solve(doc, frame, anim)
  local Rig, Anim = M.Rig, M.Anim
  if not frame then
    return nil, Rig.solve(doc)
  end
  anim = anim or doc.anim
  local pose = Anim.sample(anim, frame)
  if #doc.ik == 0 then return pose, Rig.solve(doc, pose) end
  local restWorld = Rig.solve(doc)
  local world = Rig.solve(doc, pose)
  -- parents first: a controller further down sees the solved chain above
  local order = {}
  local index = {}
  for i, b in ipairs(doc.bones) do index[b.id] = i end
  for _, c in ipairs(doc.ik) do
    if c.on and index[c.upper] and index[c.lower] then order[#order + 1] = c end
  end
  table.sort(order, function(x, y) return index[x.upper] < index[y.upper] end)
  local ground = doc.ground
  for _, c in ipairs(order) do
    solveOne(doc, c, pose, world, restWorld, frame, nil, anim)
    world = Rig.solve(doc, pose)
    if ground and ground.on and ground.stop then
      -- lift the chain until its lowest skin point rests on the ground
      local endBone = c.tip ~= 0 and Rig.bone(doc, c.tip) or Rig.bone(doc, c.lower)
      local low = lowestOf(endBone, world[endBone.id])
      if low > ground.y + 1e-9 then
        -- solve again with the chain's ACTUAL end lifted by the overlap
        -- (the keyed target may be out of reach, lifting it would not help)
        local e = world[c.lower]
        solveOne(doc, c, pose, world, restWorld, frame, { x = e.tx, y = e.ty - (low - ground.y) }, anim)
        world = Rig.solve(doc, pose)
      end
    end
  end
  return pose, world
end

--- What the view draws for each live controller at `frame`: the dummies,
--- the chain's base and middle joint, its reach, and whether its end
--- touches the ground. { { c, tx, ty, px, py, bx, by, jx, jy, reach, onGround } }
function M.handles(doc, frame, world)
  local Rig = M.Rig
  local out = {}
  if #doc.ik == 0 then return out end
  local restWorld = Rig.solve(doc)
  world = world or select(2, M.solve(doc, frame))
  local keyed = frame and M.Anim.sample(doc.anim, frame) or {}
  local function ownLen(b)
    local p = keyed[b.id]
    return b.len * (p and p.s or 1)
  end
  for _, c in ipairs(doc.ik) do
    local a, b = Rig.bone(doc, c.upper), Rig.bone(doc, c.lower)
    if a and b then
      local tx, ty, px, py = M.dummies(doc, c, frame, restWorld)
      local wa, wb = world[a.id], world[b.id]
      local reach = (ownLen(a) + ownLen(b)) * (c.stretch and c.maxStretch or 1)
      local endBone = c.tip ~= 0 and Rig.bone(doc, c.tip) or b
      local onGround = false
      local g = doc.ground
      if g and g.on and endBone then
        onGround = lowestOf(endBone, world[endBone.id]) >= g.y - 0.5
      end
      out[#out + 1] = {
        c = c, tx = tx, ty = ty, px = px, py = py,
        bx = wa.hx, by = wa.hy, jx = wb.hx, jy = wb.hy, ex = wb.tx, ey = wb.ty,
        reach = reach, onGround = onGround,
      }
    end
  end
  return out
end

return M
