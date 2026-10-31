-- widgets_extra.lua — gradientBar and list, kept out of widgets.lua so
-- that file stays under ~900 lines. Same contract as widgets.lua's
-- header (measure/paint/hit/down/drag/dblclick/wheel/key -> actions);
-- `M.extend(W)` mutates an existing widgets module table (adds the
-- widgets, extends `kinds`/`focusable`) so widgets.lua can fold this in
-- and panel.lua/init.lua keep seeing ONE registry.
--
-- gradientBar's `hit` needs the current value (stop positions) but the
-- widget contract's `hit(spec, rect, x, y, ctx)` carries no value: paint/
-- down/drag/dblclick stash it on `spec._lastValue` first, so hit() can
-- read it back. Similarly `spec._dragIndex` (which stop drag() follows)
-- and `spec._scroll` (list's scroll offset) and `spec._items` (list's
-- resolved options, set by panel.lua from adapter.options) live on the
-- spec table between calls -- the one bit of state widgets are allowed
-- to keep, since panel.lua never passes a persistent `st` into these
-- callbacks (see widgets.lua's header: down/drag/up take no `st`).

local M = {}

local floor = math.floor

local function rect(ops, x, y, w, h, c, a)
  ops[#ops + 1] = { "rect", x, y, w, h, c[1], c[2], c[3], a }
end

local function frame(ops, x, y, w, h, c, a)
  ops[#ops + 1] = { "frame", x, y, w, h, c[1], c[2], c[3], a }
end

local function text(ops, x, y, c, str)
  ops[#ops + 1] = { "text", x, y, c[1], c[2], c[3], tostring(str) }
end

local function fontH(ctx)
  if ctx.text then return ctx.text:height() end
  return ctx.theme.fontH
end

local function inRect(x, y, rx, ry, rw, rh)
  return x >= rx and y >= ry and x < rx + rw and y < ry + rh
end

local function checker(ops, x, y, w, h)
  local cell = 4
  for yy = 0, h - 1, cell do
    for xx = 0, w - 1, cell do
      local even = ((floor(xx / cell) + floor(yy / cell)) % 2) == 0
      local c = even and { 90, 90, 96 } or { 66, 66, 72 }
      local ww = w - xx < cell and w - xx or cell
      local hh = h - yy < cell and h - yy or cell
      rect(ops, x + xx, y + yy, ww, hh, c)
    end
  end
end

---------------------------------------
-- gradientBar --
---------------------------------------

local gradientBar = {}

function gradientBar.measure(spec, ctx)
  local th = ctx.theme
  return th.gradientBarH + th.gap + th.gradientMarkH
end

--- Stops normalised: `x` filled in (uniform spacing) when missing.
local function stopsOf(value)
  local stops = value or {}
  local n = #stops
  if n == 0 then return {} end
  local out = {}
  for i = 1, n do
    local s = stops[i]
    local x = s.x
    if x == nil then x = (n == 1) and 0 or (i - 1) / (n - 1) end
    out[i] = { r = s.r or 0, g = s.g or 0, b = s.b or 0, a = s.a or 255, x = x }
  end
  return out
end
gradientBar.stopsOf = stopsOf

--- Linear interpolation of the stop list at `t` (0..1).
local function colorAt(stops, t)
  local n = #stops
  if n == 0 then return 0, 0, 0, 255 end
  if n == 1 or t <= stops[1].x then
    local s = stops[1]
    return s.r, s.g, s.b, s.a
  end
  if t >= stops[n].x then
    local s = stops[n]
    return s.r, s.g, s.b, s.a
  end
  for i = 1, n - 1 do
    local a, b = stops[i], stops[i + 1]
    if t >= a.x and t <= b.x then
      local span = b.x - a.x
      local k = (span > 0) and ((t - a.x) / span) or 0
      return
        floor(a.r + (b.r - a.r) * k + 0.5),
        floor(a.g + (b.g - a.g) * k + 0.5),
        floor(a.b + (b.b - a.b) * k + 0.5),
        floor(a.a + (b.a - a.a) * k + 0.5)
    end
  end
  local s = stops[n]
  return s.r, s.g, s.b, s.a
end
gradientBar.colorAt = colorAt

local function barRect(spec, r, th)
  return r.x, r.y, r.w, th.gradientBarH
end
gradientBar.barRect = barRect

local function marksRect(spec, r, th)
  return r.x, r.y + th.gradientBarH + th.gap, r.w, th.gradientMarkH
end
gradientBar.marksRect = marksRect

--- Paints the bar column by column. Two optional hooks on the spec let
--- a caller show something other than the plain stop interpolation while
--- the markers still edit `value`: `spec.baseColor(t) -> r, g, b` gives
--- each column's colour (e.g. an emitter's RGB gradient) and
--- `spec.overlayAlpha(t) -> a01` multiplies each column's alpha (an
--- opacity envelope over that gradient; a checker is drawn underneath).
function gradientBar.paint(spec, r, value, st, ctx)
  local th = ctx.theme
  local ops = {}
  spec._lastValue = value
  local bx, by, bw, bh = barRect(spec, r, th)
  local stops = stopsOf(value)
  local baseColor, overlayAlpha = spec.baseColor, spec.overlayAlpha
  local hasAlpha = overlayAlpha ~= nil
  for i = 1, #stops do if stops[i].a < 255 then hasAlpha = true end end
  if hasAlpha then checker(ops, bx, by, bw, bh) end
  if #stops == 0 and not baseColor then
    rect(ops, bx, by, bw, bh, th.box)
  else
    for col = 0, bw - 1 do
      local t = (bw > 1) and (col / (bw - 1)) or 0
      local cr, cg, cb, ca = colorAt(stops, t)
      if baseColor then
        local r2, g2, b2 = baseColor(t)
        cr, cg, cb = floor((r2 or 0) + 0.5), floor((g2 or 0) + 0.5), floor((b2 or 0) + 0.5)
      end
      if overlayAlpha then
        local k = tonumber(overlayAlpha(t)) or 1
        if k < 0 then k = 0 elseif k > 1 then k = 1 end
        ca = floor(ca * k + 0.5)
      end
      if ca > 0 then rect(ops, bx + col, by, 1, bh, { cr, cg, cb }, ca) end
    end
  end
  frame(ops, bx, by, bw, bh, th.frame)
  local mx, my, mw, mh = marksRect(spec, r, th)
  for i = 1, #stops do
    local s = stops[i]
    local px = mx + floor(s.x * (mw - 1) + 0.5)
    local hi = (st.selected == i)
    for row = 0, mh - 1 do
      local half = floor((mh - row) / 2)
      if half > 0 then
        local c = hi and th.accent or { s.r, s.g, s.b }
        rect(ops, px - half, my + row, half * 2 + 1, 1, c)
      end
    end
    local fh2 = floor(mh / 2)
    frame(ops, px - fh2, my, mh, mh, hi and th.focus or th.frame)
  end
  return ops
end

--- `{"stop", index}` on a marker (+-3px), `{"bar", t}` over the bar,
--- "body" elsewhere inside the row, nil outside. Reads the stop
--- positions from `spec._lastValue` (see the file header).
function gradientBar.hit(spec, r, x, y, ctx)
  if not inRect(x, y, r.x, r.y, r.w, r.h) then return nil end
  local th = ctx.theme
  local bx, by, bw, bh = barRect(spec, r, th)
  local mx, my, mw, mh = marksRect(spec, r, th)
  local stops = stopsOf(spec._lastValue)
  if y >= my - 2 and y < my + mh + 2 then
    for i = 1, #stops do
      local px = mx + floor(stops[i].x * (mw - 1) + 0.5)
      if x >= px - 3 and x <= px + 3 then return { "stop", i } end
    end
  end
  if y >= by and y < by + bh then
    local t = (bw > 1) and ((x - bx) / (bw - 1)) or 0
    if t < 0 then t = 0 elseif t > 1 then t = 1 end
    return { "bar", t }
  end
  return "body"
end

function gradientBar.down(spec, r, x, y, value, ctx)
  spec._lastValue = value
  local part = gradientBar.hit(spec, r, x, y, ctx)
  if type(part) == "table" and part[1] == "stop" then
    spec._dragIndex = part[2]
    return { type = "stopDown", index = part[2] }
  elseif type(part) == "table" and part[1] == "bar" then
    spec._dragIndex = nil
    return { type = "barDown", t = part[2] }
  end
  spec._dragIndex = nil
  return nil
end

--- Clamped between the neighbouring stops with a 0.001 slack, so two
--- stops never cross or land exactly on top of each other.
function gradientBar.drag(spec, r, x, y, value, ctx)
  spec._lastValue = value
  local idx = spec._dragIndex
  if not idx then return nil end
  local th = ctx.theme
  local bx, by, bw, bh = barRect(spec, r, th)
  local t = (bw > 1) and ((x - bx) / (bw - 1)) or 0
  local stops = stopsOf(value)
  local lo, hi = 0, 1
  if stops[idx - 1] then lo = stops[idx - 1].x + 0.001 end
  if stops[idx + 1] then hi = stops[idx + 1].x - 0.001 end
  if t < lo then t = lo end
  if t > hi then t = hi end
  return { type = "stopDrag", index = idx, t = t }
end

function gradientBar.dblclick(spec, r, x, y, value, ctx)
  spec._lastValue = value
  local part = gradientBar.hit(spec, r, x, y, ctx)
  if type(part) == "table" and part[1] == "stop" then
    return { type = "stopDouble", index = part[2] }
  end
  return nil
end

M.gradientBar = gradientBar

---------------------------------------
-- list --
---------------------------------------

local list = {}

--- Rows shown; independent of the actual rect height (measure() always
--- returns exactly `rowsOf(spec) * dropRowH`, so they match in practice).
local function rowsOf(spec) return spec.rows or 6 end
list.rowsOf = rowsOf

--- `spec._items` (set by panel.lua from `adapter.options`) or `spec.items`.
local function itemsOf(spec)
  local it = spec._items or spec.items
  if type(it) == "function" then it = it(spec) end
  if type(it) ~= "table" then return {} end
  return it
end
list.itemsOf = itemsOf

local function itemText(item)
  if type(item) == "table" then return item.label or "" end
  return tostring(item)
end

local function maxScrollOf(spec)
  local m = #itemsOf(spec) - rowsOf(spec)
  if m < 0 then m = 0 end
  return m
end

local function clampScroll(spec)
  local sc = spec._scroll or 0
  local m = maxScrollOf(spec)
  if sc > m then sc = m end
  if sc < 0 then sc = 0 end
  spec._scroll = sc
  return sc
end
list.clampScroll = clampScroll

function list.measure(spec, ctx) return rowsOf(spec) * ctx.theme.dropRowH end

function list.paint(spec, r, value, st, ctx)
  local th = ctx.theme
  local ops = {}
  rect(ops, r.x, r.y, r.w, r.h, th.box)
  frame(ops, r.x, r.y, r.w, r.h, st.focused and th.focus or th.frame)
  local items = itemsOf(spec)
  local rowH = th.dropRowH
  local scroll = clampScroll(spec)
  local rows = rowsOf(spec)
  local sel = st.selected or value
  local fh = fontH(ctx)
  local s = th.uiScale
  for row = 1, rows do
    local i = row + scroll
    local item = items[i]
    if item == nil then break end
    local ry = r.y + (row - 1) * rowH
    if i == sel then
      rect(ops, r.x + 1, ry, r.w - 2, rowH, th.boxHot)
    end
    local tx = r.x + 3 * s
    if spec.thumb then
      local buf = spec.thumb(item)
      if buf then
        ops[#ops + 1] = { "image", tx, ry, rowH, rowH, buf }
        tx = tx + rowH + 3 * s
      end
    end
    local ty = ry + floor((rowH - fh) / 2)
    local str = itemText(item)
    local c = (type(item) == "table" and item.dim) and th.textDim or th.textValue
    if ctx.text then str = ctx.text:ellipsize(str, r.x + r.w - tx - 2 * s) end
    text(ops, tx, ty, c, str)
  end
  return ops
end

--- The absolute item index under (x, y), "body" inside the row but past
--- the last item, or nil outside.
function list.hit(spec, r, x, y, ctx)
  if not inRect(x, y, r.x, r.y, r.w, r.h) then return nil end
  local rowH = ctx.theme.dropRowH
  local scroll = clampScroll(spec)
  local row = floor((y - r.y) / rowH) + 1
  local i = row + scroll
  if itemsOf(spec)[i] then return i end
  return "body"
end

function list.down(spec, r, x, y, value, ctx)
  local i = list.hit(spec, r, x, y, ctx)
  if type(i) == "number" and i ~= value then
    return { type = "set", value = i }
  end
  return nil
end

function list.dblclick(spec, r, x, y, value, ctx)
  local i = list.hit(spec, r, x, y, ctx)
  if type(i) == "number" then
    return { type = "activate", index = i }
  end
  return nil
end

-- A list is a scroll container, not a value: the panel hands it the wheel
-- WITHOUT the right button (see panel.lua), and only falls back to panel
-- scrolling when the list cannot move further in that direction.
list.scrollable = true

--- True when the list can scroll in direction `dy` (>0 down).
function list.canScroll(spec, dy)
  local sc = spec._scroll or 0
  if dy > 0 then return sc < maxScrollOf(spec) end
  return sc > 0
end

function list.wheel(spec, r, dy, value, ctx)
  local sc = spec._scroll or 0
  spec._scroll = sc + ((dy > 0) and 1 or -1)
  clampScroll(spec)
  return nil
end

function list.key(spec, key, value, ctx, mods)
  local n = #itemsOf(spec)
  local v = tonumber(value) or 0
  if key == "Down" then
    if n == 0 then return nil end
    if v < n then v = v + 1 elseif v == 0 then v = 1 end
  elseif key == "Up" then
    if n == 0 then return nil end
    if v > 1 then v = v - 1 elseif v == 0 then v = 1 end
  elseif key == "Enter" then
    if value then return { type = "activate", index = value } end
    return nil
  else
    return nil
  end
  local rows = rowsOf(spec)
  local sc = spec._scroll or 0
  if v <= sc then sc = v - 1 end
  if v > sc + rows then sc = v - rows end
  spec._scroll = sc
  clampScroll(spec)
  return v
end

M.list = list

---------------------------------------
-- extend --
---------------------------------------

--- Folds gradientBar/list into an existing widgets module `W` (mutates
--- and returns it).
function M.extend(W)
  W.gradientBar = gradientBar
  W.list = list
  W.kinds[#W.kinds + 1] = "gradientBar"
  W.kinds[#W.kinds + 1] = "list"
  W.focusable.list = true
  return W
end

return M
