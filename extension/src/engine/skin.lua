-- skin.lua — the skin of a Bonebase rig: the shape that wraps the bones,
-- in the spirit of Blender's Skin modifier. Pure Lua, dual-VM.
--
-- The skin is a SIGNED DISTANCE FIELD, not a polygon. Per bone, the
-- uneven capsule of Inigo Quilez (a disc of radius rHead at the head, a
-- disc of rTail at the tail and the two tangent lines between them); the
-- bulges of that side subtracted on top; the bones of one PART merged
-- with a smooth minimum of radius `part.blend`. A pixel is ink when the
-- field at its CENTRE is <= 0.
--
-- Why a field and not the stroke expansion of vec_raster (the first plan):
--   * the tangent hull of two discs is exact here, with no kink where a
--     thin end meets a thick one;
--   * bulges are a subtraction and the smooth union at armpits and hips
--     is one formula, where polygons would need booleans and offsets;
--   * there is no self-intersection to defend against, whatever the
--     user does to radii and bulges.
--
-- A BULGE list (bone.bulgeL / bone.bulgeR) is { { t, d }, ... } sorted by
-- t in (0, 1): `d` px added to the radius on that side at `t` along the
-- bone. Between handles the profile is a cosine ease through (0, 0), the
-- handles and (1, 0), so every handle is the crest of its bump and the
-- ends stay exactly at rHead / rTail (joints keep matching).
--
-- LEFT and RIGHT are as seen walking from head to tail on screen (y
-- grows downwards): a bone pointing right has its left side above it.
--
-- A LAYER (one per part, what the export turns into an Aseprite layer):
--   { part, x0, y0, bw, bh, mask, line }
-- `mask` and `line` are dense 0/1 arrays over the bounding box,
-- (x, y) -> (y - y0) * bw + (x - x0) + 1. `line` is the outline: ink
-- pixels with an empty 4-neighbour, made pixel-perfect (no L corners).

local M = {}

local sqrt, abs, cos, pi = math.sqrt, math.abs, math.cos, math.pi
local floor, ceil, min, max = math.floor, math.ceil, math.min, math.max

M.BULGE_T_MIN = 0.02
M.BULGE_T_MAX = 0.98

---------------------------------------
-- Bulges --
---------------------------------------

local function ease(t0, d0, t1, d1, t)
  if t1 <= t0 then return d1 end
  local u = (t - t0) / (t1 - t0)
  return d0 + (d1 - d0) * (1 - cos(pi * u)) * 0.5
end

--- The bulge of one side at t in 0..1 (0 outside the handles' reach).
function M.bulgeAt(list, t)
  local n = #list
  if n == 0 then return 0 end
  if t <= 0 or t >= 1 then return 0 end
  local pt, pd = 0, 0
  for i = 1, n do
    local q = list[i]
    if t <= q[1] then return ease(pt, pd, q[1], q[2], t) end
    pt, pd = q[1], q[2]
  end
  return ease(pt, pd, 1, 0, t)
end

local function sideList(bone, side)
  if side == "left" then return bone.bulgeL end
  return bone.bulgeR
end

local function clampT(t)
  if t < M.BULGE_T_MIN then return M.BULGE_T_MIN end
  if t > M.BULGE_T_MAX then return M.BULGE_T_MAX end
  return t
end

--- Adds a bulge handle, keeping the list sorted. Returns its index.
function M.addBulge(bone, side, t, d)
  local list = sideList(bone, side)
  t = clampT(t)
  local i = #list + 1
  for k = 1, #list do
    if t < list[k][1] then i = k break end
  end
  table.insert(list, i, { t, d })
  return i
end

--- Moves handle `i` to (t, d); returns its new index (the list re-sorts).
function M.setBulge(bone, side, i, t, d)
  local list = sideList(bone, side)
  if not list[i] then return nil end
  table.remove(list, i)
  return M.addBulge(bone, side, t, d)
end

function M.removeBulge(bone, side, i)
  local list = sideList(bone, side)
  if not list[i] then return false end
  table.remove(list, i)
  return true
end

---------------------------------------
-- Distance field --
---------------------------------------

local function maxBulge(bone)
  local m = 0
  for _, q in ipairs(bone.bulgeL) do if q[2] > m then m = q[2] end end
  for _, q in ipairs(bone.bulgeR) do if q[2] > m then m = q[2] end end
  return m
end

--- Signed distance from (px, py) to one bone's skin; `w` is its world
--- entry from Rig.solve. The length is the WORLD one: a length key
--- (squash and stretch, foreshortening) makes the capsule shorter or
--- longer with the same radii, it never inflates it.
function M.boneDistance(bone, w, px, py)
  local h = sqrt((w.tx - w.hx) ^ 2 + (w.ty - w.hy) ^ 2)
  if not (h > 1e-9) then
    -- a degenerate bone (bad file, NaN) is a disc: never divide by it
    local dx, dy = px - w.hx, py - w.hy
    return sqrt(dx * dx + dy * dy) - max(bone.rHead, bone.rTail)
  end
  local c, s = (w.tx - w.hx) / h, (w.ty - w.hy) / h
  local qx, qy = px - w.hx, py - w.hy
  local along = qx * c + qy * s
  local side = -qx * s + qy * c -- > 0: right side
  local ax = abs(side)
  local r1, r2 = bone.rHead, bone.rTail
  local b = (r1 - r2) / h
  local d
  if b * b >= 1 then
    -- one end's disc swallows the other: no tangent lines exist
    local d1 = sqrt(along * along + side * side) - r1
    local d2 = sqrt((along - h) * (along - h) + side * side) - r2
    d = min(d1, d2)
  else
    local a = sqrt(1 - b * b)
    local k = -b * ax + a * along
    if k < 0 then
      d = sqrt(ax * ax + along * along) - r1
    elseif k > a * h then
      d = sqrt(ax * ax + (along - h) * (along - h)) - r2
    else
      d = ax * a + along * b - r1
    end
  end
  local list = side < 0 and bone.bulgeL or bone.bulgeR
  if #list > 0 then d = d - M.bulgeAt(list, along / h) end
  return d
end

--- Distance from the axis to the drawn edge on `side` at t (0..1), on
--- the straight part of the capsule: (linear radius + bulge) / a, with
--- a = sqrt(1 - b^2) and b = (rHead - rTail) / len (the tangent lines of
--- a tapered capsule lean, so the edge sits further out than the linear
--- radius). Returns the offset, `a` and the linear radius: what the
--- editor and the view use so a bulge handle sits ON the edge.
function M.edgeOffset(bone, t, side)
  local lin = bone.rHead + (bone.rTail - bone.rHead) * t
  local list = side == "left" and bone.bulgeL or bone.bulgeR
  local b = bone.len > 1e-9 and (bone.rHead - bone.rTail) / bone.len or 0
  local a = b * b < 1 and sqrt(1 - b * b) or 1
  return (lin + M.bulgeAt(list, t)) / a, a, lin
end

--- Polynomial smooth minimum (k = blend radius in px; 0 = plain min).
function M.smin(a, b, k)
  if k <= 0 then return min(a, b) end
  local hh = max(k - abs(a - b), 0) / k
  return min(a, b) - hh * hh * k * 0.25
end

--- The bones of a part, in rig order.
local function bonesOf(doc, partId)
  local out = {}
  for _, b in ipairs(doc.bones) do
    if b.part == partId then out[#out + 1] = b end
  end
  return out
end

--- The field of a whole part at one point.
function M.partDistance(bones, world, blend, px, py)
  local d = math.huge
  for i = 1, #bones do
    local b = bones[i]
    local e = M.boneDistance(b, world[b.id], px, py)
    if d == math.huge then d = e else d = M.smin(d, e, blend) end
  end
  return d
end

---------------------------------------
-- Raster --
---------------------------------------

--- Removes L corners from an outline in place, scanning row by row: a
--- line pixel with line neighbours on two orthogonal sides, nothing on
--- the two opposite sides and an empty diagonal between the two is a
--- corner the path can cut diagonally.
local function pixelPerfect(line, bw, bh)
  local function at(x, y)
    if x < 0 or y < 0 or x >= bw or y >= bh then return 0 end
    return line[y * bw + x + 1]
  end
  -- (dx1, dy1), (dx2, dy2): the two orthogonal neighbours of the corner
  local PAIRS = { { 0, -1, 1, 0 }, { 1, 0, 0, 1 }, { 0, 1, -1, 0 }, { -1, 0, 0, -1 } }
  for y = 0, bh - 1 do
    for x = 0, bw - 1 do
      local i = y * bw + x + 1
      if line[i] == 1 then
        for _, p in ipairs(PAIRS) do
          local ax, ay, bx, by = p[1], p[2], p[3], p[4]
          if at(x + ax, y + ay) == 1 and at(x + bx, y + by) == 1
            and at(x - ax, y - ay) == 0 and at(x - bx, y - by) == 0
            and at(x + ax + bx, y + ay + by) == 0 then
            line[i] = 0
            break
          end
        end
      end
    end
  end
end

--- One part rasterized into a layer, or nil when it has no ink.
--- opts: w, h (canvas), outline ("pixel" | "thick" | "none").
function M.rasterPart(doc, world, part, opts)
  local W, H = floor(opts.w), floor(opts.h)
  local bones = bonesOf(doc, part.id)
  if #bones == 0 then return nil end
  local blend = part.blend or 0
  local x0, y0, x1, y1 = math.huge, math.huge, -math.huge, -math.huge
  for _, b in ipairs(bones) do
    local w = world[b.id]
    local m = max(b.rHead, b.rTail) + maxBulge(b) + blend + 1
    x0 = min(x0, w.hx - m, w.tx - m)
    y0 = min(y0, w.hy - m, w.ty - m)
    x1 = max(x1, w.hx + m, w.tx + m)
    y1 = max(y1, w.hy + m, w.ty + m)
  end
  x0, y0 = max(0, floor(x0)), max(0, floor(y0))
  x1, y1 = min(W - 1, ceil(x1)), min(H - 1, ceil(y1))
  if x1 < x0 or y1 < y0 then return nil end
  local bw, bh = x1 - x0 + 1, y1 - y0 + 1
  local mask, any = {}, false
  for y = 0, bh - 1 do
    local py = y0 + y + 0.5
    for x = 0, bw - 1 do
      local ink = M.partDistance(bones, world, blend, x0 + x + 0.5, py) <= 0
      mask[y * bw + x + 1] = ink and 1 or 0
      if ink then any = true end
    end
  end
  if not any then return nil end
  local mode = opts.outline or "pixel"
  local line = {}
  local function inside(x, y)
    if x < 0 or y < 0 or x >= bw or y >= bh then
      return false
    end
    return mask[y * bw + x + 1] == 1
  end
  for y = 0, bh - 1 do
    for x = 0, bw - 1 do
      local i = y * bw + x + 1
      local edge = false
      if mode ~= "none" and mask[i] == 1 then
        edge = not (inside(x - 1, y) and inside(x + 1, y)
          and inside(x, y - 1) and inside(x, y + 1))
        if not edge and mode == "thick" then
          edge = not (inside(x - 1, y - 1) and inside(x + 1, y - 1)
            and inside(x - 1, y + 1) and inside(x + 1, y + 1))
        end
      end
      line[i] = edge and 1 or 0
    end
  end
  if mode == "pixel" then pixelPerfect(line, bw, bh) end
  return { part = part, x0 = x0, y0 = y0, bw = bw, bh = bh, mask = mask, line = line }
end

--- The drawing order and visibility of part `p` under `pose` (anim.lua
--- puts the part channels z / vis under the part's id).
function M.partState(p, pose)
  local q = pose and pose[p.id]
  local z = q and q.z or p.z
  return z, not (q and q.vis ~= nil and q.vis < 0.5)
end

--- Every visible part with ink, as layers in stacking order (lowest z
--- first). opts.pose: the pose `world` came from, for animated z / vis.
function M.render(doc, world, opts)
  opts = opts or {}
  local o = {
    w = opts.w or doc.canvas.w, h = opts.h or doc.canvas.h,
    outline = opts.outline,
  }
  local order = {}
  for i, p in ipairs(doc.parts) do
    local z, vis = M.partState(p, opts.pose)
    if vis then order[#order + 1] = { p = p, i = i, z = z } end
  end
  table.sort(order, function(a, b)
    if a.z ~= b.z then return a.z < b.z end
    return a.i < b.i
  end)
  local layers = {}
  for _, e in ipairs(order) do
    local layer = M.rasterPart(doc, world, e.p, o)
    if layer then layers[#layers + 1] = layer end
  end
  return layers
end

---------------------------------------
-- Preview --
---------------------------------------

local function over(d, i, r, g, b, a)
  if a <= 0 then return end
  local da = d[i + 3]
  if da == 0 or a >= 255 then
    d[i], d[i + 1], d[i + 2], d[i + 3] = r, g, b, a
    return
  end
  local sa, ba = a / 255, da / 255
  local oa = sa + ba * (1 - sa)
  local function mix(s, t) return floor((s * sa + t * ba * (1 - sa)) / oa + 0.5) end
  d[i] = mix(r, d[i])
  d[i + 1] = mix(g, d[i + 1])
  d[i + 2] = mix(b, d[i + 2])
  d[i + 3] = floor(oa * 255 + 0.5)
end

--- Layers composited into a kit/buffer-shaped table { w, h, d }: the
--- fill in the part colour at `fillAlpha`, the outline darkened and
--- opaque. opts: fillAlpha (default 110), lineShade (0..1, default 0.55).
function M.compose(layers, w, h, opts)
  opts = opts or {}
  local fa = opts.fillAlpha or 110
  local shade = opts.lineShade or 0.55
  local d = {}
  for i = 1, w * h * 4 do d[i] = 0 end
  for _, L in ipairs(layers) do
    local c = L.part.color
    local lr, lg, lb = floor(c[1] * shade), floor(c[2] * shade), floor(c[3] * shade)
    for y = 0, L.bh - 1 do
      for x = 0, L.bw - 1 do
        local k = y * L.bw + x + 1
        if L.mask[k] == 1 then
          local i = ((L.y0 + y) * w + (L.x0 + x)) * 4 + 1
          if L.line[k] == 1 then
            over(d, i, lr, lg, lb, 255)
          else
            over(d, i, c[1], c[2], c[3], fa)
          end
        end
      end
    end
  end
  return { w = w, h = h, d = d }
end

--- One layer alone as a buffer cropped to its box (what becomes an
--- Aseprite cel at (L.x0, L.y0)): fill in the part colour at `fillAlpha`,
--- outline darkened and opaque. Same look as `compose`.
function M.layerBuffer(L, opts)
  opts = opts or {}
  local fa = opts.fillAlpha or 110
  local shade = opts.lineShade or 0.55
  local c = L.part.color
  local lr, lg, lb = floor(c[1] * shade), floor(c[2] * shade), floor(c[3] * shade)
  local d = {}
  for k = 1, L.bw * L.bh do
    local i = (k - 1) * 4 + 1
    if L.mask[k] == 1 then
      if L.line[k] == 1 then
        d[i], d[i + 1], d[i + 2], d[i + 3] = lr, lg, lb, 255
      else
        d[i], d[i + 1], d[i + 2], d[i + 3] = c[1], c[2], c[3], fa
      end
    else
      d[i], d[i + 1], d[i + 2], d[i + 3] = 0, 0, 0, 0
    end
  end
  return { w = L.bw, h = L.bh, d = d }
end

--- How many ink pixels a layer has (tests and the status bar).
function M.count(layer, which)
  local arr = which == "line" and layer.line or layer.mask
  local n = 0
  for i = 1, #arr do n = n + arr[i] end
  return n
end

return M
