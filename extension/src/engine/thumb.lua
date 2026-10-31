-- thumb.lua — small square previews of a rig (the Poses and Library grids):
-- the skin at a pose, cropped to its ink and fitted into `size` x `size`.
-- A rig bigger than the square is averaged down (box filter), a smaller
-- one grows by whole pixels so it stays crisp. A rig with no skin yet (no
-- groups) shows its bones as lines. Pure Lua, dual-VM; buffers are
-- kit/buffer tables { w, h, d }.

local M = {}

local floor, max, min, abs = math.floor, math.max, math.min, math.abs

M.FILL = 200          -- fill alpha of the skin in a thumbnail (the view uses its own)
M.PAD = 2             -- empty pixels around the ink, at the thumbnail's size
M.BONE = { 170, 176, 196 }

local function blank(w, h)
  local d = {}
  for i = 1, w * h * 4 do d[i] = 0 end
  return { w = w, h = h, d = d }
end

--- The box of non-transparent pixels of `buf`: x0, y0, x1, y1 or nil.
function M.inkBox(buf)
  local x0, y0, x1, y1
  local w, d = buf.w, buf.d
  for y = 0, buf.h - 1 do
    for x = 0, w - 1 do
      if d[(y * w + x) * 4 + 4] > 0 then
        if not x0 then x0, y0, x1, y1 = x, y, x, y end
        if x < x0 then x0 = x end
        if x > x1 then x1 = x end
        if y < y0 then y0 = y end
        y1 = y
      end
    end
  end
  return x0, y0, x1, y1
end

--- `buf`'s ink fitted into a new size x size buffer, centred.
function M.fit(buf, size)
  local out = blank(size, size)
  local x0, y0, x1, y1 = M.inkBox(buf)
  if not x0 then return out end
  local bw, bh = x1 - x0 + 1, y1 - y0 + 1
  local room = max(1, size - 2 * M.PAD)
  local scale = min(room / bw, room / bh)
  local sd, od, sw = buf.d, out.d, buf.w
  if scale >= 1 then
    local k = floor(scale)
    local ox = floor((size - bw * k) / 2)
    local oy = floor((size - bh * k) / 2)
    for y = 0, bh * k - 1 do
      local sy = y0 + floor(y / k)
      for x = 0, bw * k - 1 do
        local si = (sy * sw + x0 + floor(x / k)) * 4 + 1
        local oi = ((oy + y) * size + ox + x) * 4 + 1
        od[oi], od[oi + 1], od[oi + 2], od[oi + 3] = sd[si], sd[si + 1], sd[si + 2], sd[si + 3]
      end
    end
    return out
  end
  -- smaller: each output pixel averages its footprint (alpha weighted)
  local tw, th = max(1, floor(bw * scale + 0.5)), max(1, floor(bh * scale + 0.5))
  local ox, oy = floor((size - tw) / 2), floor((size - th) / 2)
  for y = 0, th - 1 do
    local ya, yb = y0 + floor(y * bh / th), y0 + max(floor(y * bh / th), floor((y + 1) * bh / th) - 1)
    for x = 0, tw - 1 do
      local xa = x0 + floor(x * bw / tw)
      local xb = x0 + max(floor(x * bw / tw), floor((x + 1) * bw / tw) - 1)
      local r, g, b, a, n = 0, 0, 0, 0, 0
      for sy = ya, yb do
        for sx = xa, xb do
          local si = (sy * sw + sx) * 4 + 1
          local sa = sd[si + 3]
          r, g, b, a = r + sd[si] * sa, g + sd[si + 1] * sa, b + sd[si + 2] * sa, a + sa
          n = n + 1
        end
      end
      local oi = ((oy + y) * size + ox + x) * 4 + 1
      if a > 0 then
        od[oi], od[oi + 1], od[oi + 2] = floor(r / a + 0.5), floor(g / a + 0.5), floor(b / a + 0.5)
        -- thin parts stay visible: the alpha leans to the strongest ink
        od[oi + 3] = min(255, floor(a / n * 1.5 + 0.5))
      end
    end
  end
  return out
end

local function plot(buf, x, y, c)
  if x < 0 or y < 0 or x >= buf.w or y >= buf.h then return end
  local i = (y * buf.w + x) * 4 + 1
  buf.d[i], buf.d[i + 1], buf.d[i + 2], buf.d[i + 3] = c[1], c[2], c[3], 255
end

local function line(buf, ax, ay, bx, by, c)
  ax, ay, bx, by = floor(ax + 0.5), floor(ay + 0.5), floor(bx + 0.5), floor(by + 0.5)
  local dx, dy = abs(bx - ax), -abs(by - ay)
  local sx, sy = ax < bx and 1 or -1, ay < by and 1 or -1
  local err = dx + dy
  while true do
    plot(buf, ax, ay, c)
    if ax == bx and ay == by then break end
    local e2 = 2 * err
    if e2 >= dy then err = err + dy ax = ax + sx end
    if e2 <= dx then err = err + dx ay = ay + sy end
  end
end

--- The rig's skin (or its bones, when it has no skin) at `world` / `pose`
--- as drawn on its canvas, then fitted into a size x size buffer.
function M.render(Skin, doc, world, pose, size)
  local layers = Skin.render(doc, world, { outline = "pixel", pose = pose })
  if #layers > 0 then
    -- compose only the box the layers cover, not the whole canvas
    local x0, y0, x1, y1 = math.huge, math.huge, -1, -1
    for _, L in ipairs(layers) do
      x0, y0 = min(x0, L.x0), min(y0, L.y0)
      x1, y1 = max(x1, L.x0 + L.bw - 1), max(y1, L.y0 + L.bh - 1)
    end
    local moved = {}
    for i, L in ipairs(layers) do
      moved[i] = { part = L.part, x0 = L.x0 - x0, y0 = L.y0 - y0, bw = L.bw, bh = L.bh,
        mask = L.mask, line = L.line }
    end
    return M.fit(Skin.compose(moved, x1 - x0 + 1, y1 - y0 + 1, { fillAlpha = M.FILL }), size)
  end
  if #doc.bones == 0 then return blank(size, size) end
  local x0, y0, x1, y1 = math.huge, math.huge, -math.huge, -math.huge
  for _, b in ipairs(doc.bones) do
    local w = world[b.id]
    if w then
      x0, y0 = min(x0, w.hx, w.tx), min(y0, w.hy, w.ty)
      x1, y1 = max(x1, w.hx, w.tx), max(y1, w.hy, w.ty)
    end
  end
  if x1 < x0 then return blank(size, size) end
  -- lines drawn at most at the thumbnail's own size: a big rig does not
  -- need a canvas-sized buffer
  local room = max(1, size - 2 * M.PAD)
  local k = min(1, room / max(1, x1 - x0, y1 - y0))
  local buf = blank(floor((x1 - x0) * k) + 1, floor((y1 - y0) * k) + 1)
  for _, b in ipairs(doc.bones) do
    local w = world[b.id]
    if w then line(buf, (w.hx - x0) * k, (w.hy - y0) * k, (w.tx - x0) * k, (w.ty - y0) * k, M.BONE) end
  end
  return M.fit(buf, size)
end

return M
