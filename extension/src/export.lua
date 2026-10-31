-- export.lua — the Aseprite bridge that writes a rig into a sprite: the
-- plan from engine/export_plan.lua becomes a `Bonebase` layer group (one
-- locked layer per part, bottom of the stack, so the user draws above
-- it), frames with their duration, and the rig itself saved in the
-- sprite's properties so reopening Bonebase on that sprite brings it back.
--
-- Re-exporting is idempotent: the group is found by its property (not
-- its name) and KEPT where it is; only the part layers inside it (marked
-- with properties("bonebase").part) are deleted and built again at its
-- bottom. The user's layers and cels are never touched, not even a layer
-- the user put inside the group; frames are only added, never removed,
-- and the exported ones get the rig's frame times. Clips go one after the
-- other, each under a tag of its name (with its direction); our tags are
-- marked in their properties and rebuilt on every export, the user's
-- tags stay.
-- Everything happens in ONE transaction: one Ctrl+Z in Aseprite undoes it.

local M = {}

M.KEY = "bonebase"   -- properties("bonebase") on the sprite and the layers
M.GROUP_NAME = "Bonebase"

M.Buffer, M.Plan, M.Rig, M.Serial = nil, nil, nil, nil

function M.bind(deps)
  for k, v in pairs(deps) do M[k] = v end
  return M
end

local function props(obj)
  return obj.properties(M.KEY)
end

--- The Bonebase group anywhere in the layer tree, or nil.
function M.findGroup(layers)
  for _, l in ipairs(layers) do
    if l.isGroup then
      local ok, p = pcall(props, l)
      if ok and p and p.group == true then return l end
      local inner = M.findGroup(l.layers)
      if inner then return inner end
    end
  end
  return nil
end

--- The rig saved in a sprite, decoded, or nil (+ why when it is broken).
function M.loadRig(sprite)
  if not sprite then return nil end
  local ok, p = pcall(props, sprite)
  if not ok or not p or type(p.rig) ~= "string" then return nil end
  return M.Rig.decode(M.Serial, p.rig)
end

function M.saveRig(sprite, doc)
  props(sprite).rig = M.Rig.encode(M.Serial, doc)
end

--- Every layer of the tree, depth first, top-level order kept.
local function allLayers(layers, out)
  out = out or {}
  for _, l in ipairs(layers) do
    out[#out + 1] = l
    if l.isGroup then allLayers(l.layers, out) end
  end
  return out
end

--- Writes `doc` into `sprite` (nil = a new RGB sprite of the rig's size).
--- opts: outline, fillAlpha, lock (default true). Returns ok, message,
--- sprite.
function M.run(doc, sprite, opts)
  opts = opts or {}
  local plan = M.Plan.plan(doc, opts)
  if #plan.parts == 0 then return false, "Nothing to export: add bones first" end
  local created = false
  if not sprite then
    -- never inside a transaction (AseKit rules/api-aseprite.md)
    sprite = Sprite(plan.w, plan.h, ColorMode.RGB)
    created = true
  end
  if sprite.colorMode ~= ColorMode.RGB then
    return false, "Bonebase needs an RGB sprite (Sprite > Color Mode > RGB Color)"
  end
  local before = app.layer
  local beforeId = before and before.id
  local layerCount = 0
  app.transaction("Bonebase export", function()
    while #sprite.frames < plan.frames do sprite:newEmptyFrame() end
    for f = 1, plan.frames do
      sprite.frames[f].duration = (plan.durations and plan.durations[f] or plan.frameMs) / 1000
    end
    local oldTags = {}
    for _, tag in ipairs(sprite.tags) do
      local okP, p = pcall(props, tag)
      if okP and p and p.clip == true then oldTags[#oldTags + 1] = tag end
    end
    for _, tag in ipairs(oldTags) do sprite:deleteTag(tag) end
    for _, c in ipairs(plan.tags or {}) do
      local tag = sprite:newTag(c.from, c.to)
      tag.name = c.name
      tag.aniDir = c.pingpong and AniDir.PING_PONG or AniDir.FORWARD
      pcall(function() props(tag).clip = true end)
    end
    local g = M.findGroup(sprite.layers)
    if g then
      -- only our part layers go; whatever the user put inside stays
      local doomed = {}
      for _, l in ipairs(g.layers) do
        local okP, p = pcall(props, l)
        if okP and p and p.part ~= nil then doomed[#doomed + 1] = l end
      end
      for _, l in ipairs(doomed) do sprite:deleteLayer(l) end
    else
      g = sprite:newGroup()
      g.name = M.GROUP_NAME
      props(g).group = true
      g.stackIndex = 1
    end
    for i, part in ipairs(plan.parts) do
      local layer = sprite:newLayer()
      layer.name = part.name
      layer.parent = g
      layer.stackIndex = i
      layer.color = Color{ r = part.color[1], g = part.color[2], b = part.color[3], a = 255 }
      props(layer).part = part.id
      for f = 1, plan.frames do
        local cel = part.cels[f]
        if cel then
          local c = sprite:newCel(layer, f, M.Buffer.toImage(cel.buf), Point(cel.x, cel.y))
          -- a z key restacks the part in this frame: the layers sit at 1..N
          -- in the group and Aseprite draws a cel at stackIndex + zIndex
          -- (cel.zIndex exists since Aseprite 1.3; older ones keep the rest
          -- order)
          local dz = (cel.order or i) - i
          if dz ~= 0 then pcall(function() c.zIndex = dz end) end
        end
      end
      if opts.lock ~= false then layer.isEditable = false end
      layerCount = layerCount + 1
    end
    M.saveRig(sprite, doc)
  end)
  -- back to the layer the user was on, wherever it is in the tree, or
  -- else the top-level one highest in the stack that is not ours
  local group = M.findGroup(sprite.layers)
  local ours = {}
  if group then
    ours[group.id] = true
    for _, l in ipairs(group.layers) do
      local okP, p = pcall(props, l)
      if okP and p and p.part ~= nil then ours[l.id] = true end
    end
  end
  local target
  if beforeId and not ours[beforeId] then
    for _, l in ipairs(allLayers(sprite.layers)) do
      if l.id == beforeId then target = l break end
    end
  end
  if not target then
    for i = #sprite.layers, 1, -1 do
      local l = sprite.layers[i]
      if not ours[l.id] then target = l break end
    end
  end
  if target then pcall(function() app.layer = target end) end
  local nTags = plan.tags and #plan.tags or 0
  local msg = string.format("Exported %d %s, %d %s%s%s", plan.frames,
    plan.frames == 1 and "frame" or "frames", layerCount,
    layerCount == 1 and "layer" or "layers",
    nTags > 0 and string.format(", %d %s", nTags, nTags == 1 and "tag" or "tags") or "",
    created and " to a new sprite" or "")
  return true, msg, sprite
end

return M
