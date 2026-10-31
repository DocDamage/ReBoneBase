-- export_plan.lua — what an export puts in the sprite, computed without
-- Aseprite: how many frames, how long each one lasts, and per part a
-- layer with one cel per frame (a cropped RGBA buffer and its position).
-- src/export.lua turns the plan into layers and cels. Pure Lua, dual-VM.
--
-- plan = {
--   w, h, frames, frameMs,
--   durations = { [frame] = ms },   -- every frame's duration
--   tags = { { name, from, to, pingpong }, ... },
--   parts = { { id, name, color, z, cels = { [frame] = { x, y, buf, order } } }, ... },
-- }
-- Parts come bottom first (by rest z) and only parts with bones are
-- listed; a frame where a part has no ink, or is hidden (vis key), has no
-- cel. `order` is where the cel stacks in its frame (1 = bottom, among the
-- listed parts): the rest order unless a z key moved the part, and
-- export.lua turns the difference into the cel's z-index.
--
-- CLIPS go one after the other from frame 1, each with its own frame
-- times; with more than one clip (or a ping-pong one) each gets a tag. A rig without keys exports
-- ONE frame, its rest pose: twelve identical frames help nobody.

local M = {}

M.Rig, M.Skin, M.Anim, M.Ik = nil, nil, nil, nil

--- `Ik` (ik.lua) is optional: with it, live IK controllers are baked into
--- every frame exactly as the editor shows them.
function M.bind(Rig, Skin, Anim, Ik)
  M.Rig, M.Skin, M.Anim, M.Ik = Rig, Skin, Anim, Ik
  return M
end

--- opts: outline ("pixel" | "thick" | "none"), fillAlpha (0..255). The
--- sprite is the doc's bounds (rig.lua): cels are placed relative to it
--- and a cel wholly outside it is left out.
function M.plan(doc, opts)
  opts = opts or {}
  local Rig, Skin, Anim = M.Rig, M.Skin, M.Anim
  local clips = doc.clips or { doc.anim }
  local Ik = M.Ik
  local bd = Rig.bounds(doc)
  local plan = {
    w = bd.w, h = bd.h,
    frames = 0, frameMs = doc.anim.frameMs,
    durations = {}, tags = {},
    parts = {},
  }
  local used = {}
  for _, b in ipairs(doc.bones) do used[b.part] = true end
  local byId = {}
  for _, p in ipairs(Rig.partsByZ(doc)) do
    if used[p.id] then
      local e = { id = p.id, name = p.name, z = p.z,
        color = { p.color[1], p.color[2], p.color[3] }, cels = {} }
      plan.parts[#plan.parts + 1] = e
      byId[p.id] = e
    end
  end
  local index = {}
  for i, p in ipairs(doc.parts) do index[p.id] = i end
  local g = 0   -- the sprite frame
  local function frame(anim, f)
    g = g + 1
    plan.durations[g] = anim.durations and anim.durations[f] or anim.frameMs
    local pose, world
    if Ik then
      pose, world = Ik.solve(doc, f, anim)
    else
      pose = #anim.tracks > 0 and Anim.sample(anim, f) or nil
      world = Rig.solve(doc, pose)
    end
    -- this frame's stacking of the listed parts (ties by array order)
    local stack = {}
    for _, e in ipairs(plan.parts) do
      local p = Rig.part(doc, e.id)
      stack[#stack + 1] = { e = e, z = (Skin.partState(p, pose)), i = index[e.id] }
    end
    table.sort(stack, function(a, b)
      if a.z ~= b.z then return a.z < b.z end
      return a.i < b.i
    end)
    local order = {}
    for k, s in ipairs(stack) do order[s.e.id] = k end
    for _, L in ipairs(Skin.render(doc, world, { outline = opts.outline, pose = pose })) do
      local e = byId[L.part.id]
      if e then
        local buf = Skin.layerBuffer(L, { fillAlpha = opts.fillAlpha })
        local x, y = L.x0 - bd.x, L.y0 - bd.y
        if x < bd.w and y < bd.h and x + buf.w > 0 and y + buf.h > 0 then
          e.cels[g] = { x = x, y = y, order = order[e.id], buf = buf }
        end
      end
    end
  end
  for _, anim in ipairs(clips) do
    local from = g + 1
    -- a clip without keys is its rest pose: one frame
    local n = #anim.tracks == 0 and 1 or anim.frames
    for f = 1, n do frame(anim, f) end
    plan.tags[#plan.tags + 1] = { name = anim.name or "Animation", from = from, to = g,
      pingpong = anim.pingpong == true }
  end
  plan.frames = g
  if #clips == 1 and not plan.tags[1].pingpong then plan.tags = {} end
  return plan
end

return M
