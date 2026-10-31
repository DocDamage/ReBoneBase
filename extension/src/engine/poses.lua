-- poses.lua — the pose library of a rig (doc.poses, rig.lua documents the
-- entries): a pose is what a frame shows, kept so it can be keyed again on
-- any frame of any clip. Pure Lua, dual-VM.
--
-- What a pose holds, per thing it moves:
--   bones  rot, x, y, s as keyed; a bone a live IK chain turns also keeps
--          the rotation the solver gave it (`fk`), which is what gets
--          keyed when that chain is off where the pose is applied (with it
--          on, the dummies and the keyed values give the same picture).
--          `rot` is always what to key under a live chain, `fk` without
--          one: a hand of a chain that is off but keeps its angle stores
--          the key that angle needs in `rot`.
--          The length factor is the keyed one (the IK stretch comes back
--          from the dummies)
--   ik     the dummies of every controller, RELATIVE to the rig root (as
--          their keys are), so a pose moves with the root it is keyed on;
--          a chain that is off gives them where its bones end
--   parts  z and vis of the groups that animate them
-- The rig root itself is not part of a pose: where the character stands
-- belongs to the clip.
--
-- `anim(doc, pose)` builds a one-frame animation that shows the pose, for
-- the thumbnails (ik.lua solve on it like on any clip).

local M = {}

M.Rig, M.Anim, M.Ik = nil, nil, nil

function M.bind(Rig, Anim, Ik)
  M.Rig, M.Anim, M.Ik = Rig, Anim, Ik
  return M
end

--- The pose `doc` shows at `frame` of its active clip, named `name`
--- (no id yet: add gives it one).
function M.capture(doc, frame, name)
  local Anim, Ik, Rig = M.Anim, M.Ik, M.Rig
  local anim = doc.anim
  local raw = Anim.sample(anim, frame)
  local solved, world = Ik.solve(doc, frame)
  local driven = M.driven(doc)
  local pose = { name = tostring(name or "pose"):sub(1, Rig.MAX_NAME), bones = {}, ik = {}, parts = {} }
  -- the hand or foot of a chain that is off but keeps its angle when on:
  -- the key that angle needs (its world angle over the rest one)
  local lockOff = {}
  for _, c in ipairs(doc.ik) do
    if not c.on and c.lockTip and c.tip ~= 0 then lockOff[c.tip] = true end
  end
  local restWorld = next(lockOff) and Rig.solve(doc)
  for _, b in ipairs(doc.bones) do
    local p, q = raw[b.id], solved[b.id]
    local e = { id = b.id, rot = p and p.rot or 0, x = p and p.x or 0, y = p and p.y or 0,
      s = p and p.s or 1 }
    if driven[b.id] and q and q.rot ~= e.rot then e.fk = q.rot end
    -- a stretched chain: the length the solver gave, for where it is off
    if driven[b.id] and q and q.s and math.abs(q.s - e.s) > 1e-9 then e.fks = q.s end
    if lockOff[b.id] and world[b.id] then
      e.fk = e.rot
      e.rot = Rig.wrap(world[b.id].angle - restWorld[b.id].angle)
    end
    pose.bones[#pose.bones + 1] = e
  end
  local gx, gy = Ik.offset(doc, frame)
  for _, c in ipairs(doc.ik) do
    -- a chain that is off: its dummies where its FK shape puts them, so
    -- the pose still holds where that chain is on
    local tx, ty, px, py
    if c.on then
      tx, ty, px, py = Ik.dummies(doc, c, frame)
    else
      tx, ty, px, py = Ik.restDummies(doc, c, world)
    end
    pose.ik[#pose.ik + 1] = { id = c.id, tx = tx - gx, ty = ty - gy, px = px - gx, py = py - gy }
  end
  for _, part in ipairs(doc.parts) do
    local z, vis = Anim.track(anim, part.id, "z"), Anim.track(anim, part.id, "vis")
    if (z and #z.keys > 0) or (vis and #vis.keys > 0) then
      local p = raw[part.id] or {}
      pose.parts[#pose.parts + 1] = { id = part.id, z = z and #z.keys > 0 and p.z or nil,
        vis = vis and #vis.keys > 0 and p.vis or nil }
    end
  end
  return pose
end

--- The bones live IK chains turn: their upper and lower bones, and the
--- hand or foot of a chain that keeps its angle. A set of ids.
function M.driven(doc)
  local out = {}
  for _, c in ipairs(doc.ik) do
    if c.on then
      out[c.upper], out[c.lower] = true, true
      if c.lockTip and c.tip ~= 0 then out[c.tip] = true end
    end
  end
  return out
end

--- The rotation to key for pose entry `e` on `doc` as it is: the keyed one
--- where a live chain turns the bone, else what the pose showed.
function M.rotOf(e, driven)
  if driven[e.id] then return e.rot end
  return e.fk or e.rot
end

--- The same for the length factor (`fks`: a stretched chain's).
function M.sOf(e, driven)
  if driven[e.id] then return e.s end
  return e.fks or e.s
end

--- Dummies for the controllers of `doc` the pose knows nothing about (a
--- chain added after it was kept): where the pose's FK shape ends, relative
--- to the rig root. { { id, tx, ty, px, py, tip?, tipRot? } }: `tipRot`
--- is the key a hand that keeps its angle needs to look as in the pose.
function M.missingDummies(doc, pose)
  local Rig, Ik = M.Rig, M.Ik
  local known = {}
  for _, e in ipairs(pose.ik) do known[e.id] = true end
  local list = {}
  for _, c in ipairs(doc.ik) do
    if not known[c.id] then list[#list + 1] = c end
  end
  if #list == 0 then return {} end
  local fk = {}
  for _, e in ipairs(pose.bones) do
    fk[e.id] = { rot = e.fk or e.rot, x = e.x, y = e.y, s = e.fks or e.s }
  end
  local world = Rig.solve(doc, fk)
  local restWorld = Rig.solve(doc)
  local out = {}
  for _, c in ipairs(list) do
    if world[c.upper] and world[c.lower] then
      local tx, ty, px, py = Ik.restDummies(doc, c, world)
      local e = { id = c.id, tx = tx, ty = ty, px = px, py = py }
      if c.lockTip and c.tip ~= 0 and world[c.tip] then
        e.tip, e.tipRot = c.tip, Rig.wrap(world[c.tip].angle - restWorld[c.tip].angle)
      end
      out[#out + 1] = e
    end
  end
  return out
end

--- boneId -> key for the hands of `missing` (missingDummies) that a live
--- chain locks.
function M.tipRots(missing, driven)
  local out = {}
  for _, e in ipairs(missing) do
    if e.tip and driven[e.tip] then out[e.tip] = e.tipRot end
  end
  return out
end

function M.find(doc, id)
  for i, p in ipairs(doc.poses) do
    if p.id == id then return p, i end
  end
  return nil
end

--- Appends `pose` with a new id. Nil when the library is full.
function M.add(doc, pose)
  if #doc.poses >= M.Rig.MAX_POSES then return nil end
  pose.id = doc.nextPose
  doc.nextPose = doc.nextPose + 1
  doc.poses[#doc.poses + 1] = pose
  return pose
end

function M.remove(doc, id)
  local _, i = M.find(doc, id)
  if not i then return false end
  table.remove(doc.poses, i)
  return true
end

--- A free name like "pose 3" (or `base 2`, `base 3`...).
function M.freeName(doc, base)
  local used = {}
  for _, p in ipairs(doc.poses) do used[p.name] = true end
  if base and base ~= "" and not used[base] then return base end
  base = (base and base ~= "") and base or "pose"
  local n = base == "pose" and #doc.poses + 1 or 2
  while used[base .. " " .. n] do n = n + 1 end
  return base .. " " .. n
end

--- A one-frame animation (frame 1) keyed with the pose: what `Ik.solve(doc,
--- 1, anim)` turns into the pose and world the pose shows.
function M.anim(doc, pose)
  local Anim, Rig = M.Anim, M.Rig
  local a = Rig.newAnim("pose")
  a.frames, a.loop = 1, false
  local driven = M.driven(doc)
  local missing = M.missingDummies(doc, pose)
  local tipRot = M.tipRots(missing, driven)
  for _, e in ipairs(pose.bones) do
    local b = Rig.bone(doc, e.id)
    if b then
      Anim.setKey(a, e.id, "rot", 1, tipRot[e.id] or M.rotOf(e, driven))
      if b.parent == 0 or not b.connected then
        Anim.setKey(a, e.id, "x", 1, e.x)
        Anim.setKey(a, e.id, "y", 1, e.y)
      end
      local sv = M.sOf(e, driven)
      if sv ~= 1 then Anim.setKey(a, e.id, "s", 1, sv) end
    end
  end
  local dummies = {}
  for _, e in ipairs(pose.ik) do dummies[#dummies + 1] = e end
  for _, e in ipairs(missing) do dummies[#dummies + 1] = e end
  for _, e in ipairs(dummies) do
    if M.Ik.find(doc, e.id) then
      for _, ch in ipairs({ "tx", "ty", "px", "py" }) do Anim.setKey(a, e.id, ch, 1, e[ch]) end
    end
  end
  for _, e in ipairs(pose.parts) do
    if Rig.part(doc, e.id) then
      if e.z then Anim.setKey(a, e.id, "z", 1, e.z) end
      if e.vis then Anim.setKey(a, e.id, "vis", 1, e.vis) end
    end
  end
  return a
end

--- The pose and world of `pose` on `doc` (ik.lua solve over `anim`).
function M.solve(doc, pose)
  return M.Ik.solve(doc, 1, M.anim(doc, pose))
end

return M
