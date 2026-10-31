-- colorpicker.lua — the model of an open colour picker: swatch + R/G/B/A
-- sliders (kit/ui/widgets.lua's `slider`, same look and gestures) + an
-- editable hex field, floating over the panel like dropdown.lua's list.
-- Geometry follows dropdown.lua's "open below, else above, clamp to
-- bounds" rule; the LOOK (drawBox, slider track) is Spaghetti's
-- ui_panel.lua, same as every other kit/ui widget.
--
-- `M.open(value, anchorRect, boundsRect, ctx, opts)` returns a picker `P`
-- with `P.rect`; `opts.palette = { {r,g,b}, ... }` (up to 256, positional
-- or named) adds a swatch grid under the hex field: `theme.swatch` px
-- cells, as many per row as the picker is wide, `theme.pickerPaletteRows`
-- rows visible and the wheel scrolls the rest; a click on a swatch emits
-- `set` with that RGB and the CURRENT alpha, hover highlights, the swatch
-- equal to the current RGB is marked. Without a palette the layout is
-- byte-identical to before. `P:paint() -> ops` (the panel emits these LAST, on top);
-- `P:mouseDown(x, y) -> action|nil`, `P:mouseDrag(x, y) -> action|nil`,
-- `P:mouseUp(x, y)`, `P:wheel(x, y, dy) -> action|nil`,
-- `P:key(key) -> action|nil`. Actions: `{ type = "set", value = {r,g,b,a} }`
-- (EVERY change, live) or `{ type = "close" }` (Esc). Panel.lua applies
-- "set" through `adapter.set` and closes on "close" or on a click outside
-- `P.rect` (which it checks itself, like the dropdown).
--
-- It is NOT a widget in the widgets.lua sense (no `spec`/`kind`): panel
-- opens it from the `color` widget's `openPicker` action, one at a time,
-- the same way it opens a Dropdown.

local M = {}

local floor = math.floor

local function loadSibling(name)
  local info = debug.getinfo(1, "S")
  local src = info and info.source or ""
  if src:sub(1, 1) == "@" then src = src:sub(2) end
  local dir = src:match("^(.*)[/\\]") or "."
  local chunk, err = loadfile(dir .. "/" .. name .. ".lua")
  if not chunk then error("kit/ui/colorpicker: could not load " .. name .. ": " .. tostring(err)) end
  return chunk()
end

local Widgets = loadSibling("widgets")
local Layout = loadSibling("layout")

local Picker = {}
Picker.__index = Picker

local function clamp255(v)
  v = floor((tonumber(v) or 0) + 0.5)
  if v < 0 then v = 0 end
  if v > 255 then v = 255 end
  return v
end

function Picker:toHex(v)
  return string.format("%02X%02X%02X%02X", v.r, v.g, v.b, v.a)
end

--- "RRGGBB" or "RRGGBBAA" (an optional leading "#" is ignored); nil when
--- it is not one of those two shapes.
function Picker:parseHex(s)
  s = tostring(s or ""):gsub("^#", "")
  if #s ~= 6 and #s ~= 8 then return nil end
  if not s:match("^%x+$") then return nil end
  local r = tonumber(s:sub(1, 2), 16)
  local g = tonumber(s:sub(3, 4), 16)
  local b = tonumber(s:sub(5, 6), 16)
  local a = 255
  if #s == 8 then a = tonumber(s:sub(7, 8), 16) end
  if not (r and g and b and a) then return nil end
  return { r = r, g = g, b = b, a = a }
end

function Picker:cloneValue()
  return { r = self.value.r, g = self.value.g, b = self.value.b, a = self.value.a }
end

function Picker:syncHex()
  self.hexEdit.text = self:toHex(self.value)
  self.hexEdit.cursor = #self.hexEdit.text
end

--- `{ {r,g,b}, ... }` normalised (positional or named entries, alpha
--- dropped, at most 256); `{}` for anything that is not a list.
local function normalisePalette(list)
  local out = {}
  if type(list) ~= "table" then return out end
  for i = 1, #list do
    local c = list[i]
    if type(c) == "table" then
      out[#out + 1] = { clamp255(c.r or c[1] or 0), clamp255(c.g or c[2] or 0), clamp255(c.b or c[3] or 0) }
      if #out >= 256 then break end
    end
  end
  return out
end

--- `anchor` is the color widget's box; `bounds` the area the picker may
--- cover (the panel's rect, or the whole canvas).
function M.open(value, anchor, bounds, ctx, opts)
  local th = ctx.theme
  value = value or {}
  opts = opts or {}
  local pad, gap = th.pad, th.gap
  local swatchH = th.pickerSwatchH
  local rowH = th.sliderH
  local hexH = th.rowH
  local w = anchor.w
  if w < th.pickerW then w = th.pickerW end
  local h = pad * 2 + swatchH + gap + rowH * 4 + gap + hexH

  -- optional palette grid under the hex field
  local palette = normalisePalette(opts.palette)
  local cell = th.swatch or 10
  local maxRows = th.pickerPaletteRows or 4
  local perRow, rowsTotal, visRows = 1, 0, 0
  if #palette > 0 then
    perRow = floor((w - pad * 2) / cell)
    if perRow < 1 then perRow = 1 end
    rowsTotal = math.ceil(#palette / perRow)
    visRows = rowsTotal < maxRows and rowsTotal or maxRows
    h = h + gap + visRows * cell
  end

  local gapOut = 2
  local below = (bounds.y + bounds.h - gapOut) - (anchor.y + anchor.h)
  local above = anchor.y - bounds.y - gapOut
  local y
  if h <= below or below >= above then
    y = anchor.y + anchor.h
  else
    y = anchor.y - h
  end
  local lowest = bounds.y + bounds.h - h
  if y > lowest then y = lowest end
  if y < bounds.y then y = bounds.y end
  local x = anchor.x
  if x + w > bounds.x + bounds.w then x = bounds.x + bounds.w - w end
  if x < bounds.x then x = bounds.x end

  local rows = {}
  local ry = y + pad
  rows[#rows + 1] = { kind = "swatch", rect = { x = x + pad, y = ry, w = w - pad * 2, h = swatchH } }
  ry = ry + swatchH + gap
  local channels = { "r", "g", "b", "a" }
  local labels = { "R", "G", "B", "A" }
  for i = 1, 4 do
    rows[#rows + 1] = { kind = "slider", channel = channels[i], label = labels[i],
      rect = { x = x + pad, y = ry, w = w - pad * 2, h = rowH } }
    ry = ry + rowH
  end
  ry = ry + gap
  rows[#rows + 1] = { kind = "hex", rect = { x = x + pad, y = ry, w = w - pad * 2, h = hexH } }
  ry = ry + hexH
  if #palette > 0 then
    ry = ry + gap
    rows[#rows + 1] = { kind = "palette",
      rect = { x = x + pad, y = ry, w = perRow * cell, h = visRows * cell } }
  end

  local self = setmetatable({
    value = { r = clamp255(value.r or 255), g = clamp255(value.g or 255),
      b = clamp255(value.b or 255), a = clamp255(value.a == nil and 255 or value.a) },
    rect = { x = x, y = y, w = w, h = h },
    rows = rows, ctx = ctx, focus = nil, dragChannel = nil,
    palette = palette, palPerRow = perRow, palRows = rowsTotal, palVisRows = visRows,
    palCell = cell, palScroll = 0, palHover = nil,
  }, Picker)
  self.hexEdit = Widgets.textField.begin(self:toHex(self.value))
  return self
end

local function paintSwatch(ops, r, v, th)
  if v.a < 255 then
    local cell = 4
    for yy = 0, r.h - 1, cell do
      for xx = 0, r.w - 1, cell do
        local even = ((floor(xx / cell) + floor(yy / cell)) % 2) == 0
        local c = even and { 90, 90, 96 } or { 66, 66, 72 }
        local ww = r.w - xx < cell and r.w - xx or cell
        local hh = r.h - yy < cell and r.h - yy or cell
        ops[#ops + 1] = { "rect", r.x + xx, r.y + yy, ww, hh, c[1], c[2], c[3] }
      end
    end
  end
  ops[#ops + 1] = { "rect", r.x, r.y, r.w, r.h, v.r, v.g, v.b, v.a }
  ops[#ops + 1] = { "frame", r.x, r.y, r.w, r.h, th.frame[1], th.frame[2], th.frame[3] }
end

function Picker:paint()
  local th = self.ctx.theme
  local r = self.rect
  local ops = {}
  ops[#ops + 1] = { "rect", r.x, r.y, r.w, r.h, th.dropdownBg[1], th.dropdownBg[2], th.dropdownBg[3] }
  ops[#ops + 1] = { "frame", r.x, r.y, r.w, r.h, th.frame[1], th.frame[2], th.frame[3] }
  for _, row in ipairs(self.rows) do
    if row.kind == "swatch" then
      paintSwatch(ops, row.rect, self.value, th)
    elseif row.kind == "slider" then
      local spec = { kind = "slider", min = 0, max = 255, step = 1 }
      local st = { label = row.label, focused = (self.focus == row.channel) }
      local wops = Widgets.slider.paint(spec, row.rect, self.value[row.channel], st, self.ctx)
      for i = 1, #wops do ops[#ops + 1] = wops[i] end
    elseif row.kind == "hex" then
      local spec = { kind = "textField" }
      local editing = (self.focus == "hex") and self.hexEdit or nil
      local shown = editing and editing.text or self:toHex(self.value)
      local st = { focused = (self.focus == "hex"), editing = editing, label = "Hex" }
      local wops = Widgets.textField.paint(spec, row.rect, shown, st, self.ctx)
      for i = 1, #wops do ops[#ops + 1] = wops[i] end
    elseif row.kind == "palette" then
      self:paintPalette(ops, row.rect, th)
    end
  end
  return ops
end

function Picker:paletteRow()
  for _, row in ipairs(self.rows) do
    if row.kind == "palette" then return row end
  end
  return nil
end

--- Index (1-based) of the palette swatch under (x, y), nil when none.
function Picker:paletteIndexAt(x, y)
  local row = self:paletteRow()
  if not row or not Layout.contains(row.rect, x, y) then return nil end
  local cell = self.palCell
  local col = floor((x - row.rect.x) / cell)
  local r = floor((y - row.rect.y) / cell) + self.palScroll
  local i = r * self.palPerRow + col + 1
  if i < 1 or i > #self.palette then return nil end
  return i
end

--- Index of the first swatch equal to the current RGB (alpha ignored).
function Picker:paletteCurrent()
  local v = self.value
  for i = 1, #self.palette do
    local c = self.palette[i]
    if c[1] == v.r and c[2] == v.g and c[3] == v.b then return i end
  end
  return nil
end

function Picker:paintPalette(ops, r, th)
  local cell, perRow = self.palCell, self.palPerRow
  local first = self.palScroll * perRow
  local current = self:paletteCurrent()
  for i = first + 1, #self.palette do
    local k = i - first - 1
    local cy = floor(k / perRow)
    if cy >= self.palVisRows then break end
    local cx = k % perRow
    local c = self.palette[i]
    local px, py = r.x + cx * cell, r.y + cy * cell
    ops[#ops + 1] = { "rect", px, py, cell, cell, c[1], c[2], c[3] }
    if i == current then
      ops[#ops + 1] = { "frame", px, py, cell, cell, th.accent[1], th.accent[2], th.accent[3] }
      if cell > 4 then
        ops[#ops + 1] = { "frame", px + 1, py + 1, cell - 2, cell - 2,
          th.accentText[1], th.accentText[2], th.accentText[3] }
      end
    elseif i == self.palHover then
      ops[#ops + 1] = { "frame", px, py, cell, cell, th.text[1], th.text[2], th.text[3] }
    end
  end
  ops[#ops + 1] = { "frame", r.x, r.y, r.w, r.h, th.frame[1], th.frame[2], th.frame[3] }
  if self.palRows > self.palVisRows then
    -- scroll thumb along the right edge, proportional to the visible window
    local thumbH = floor(r.h * self.palVisRows / self.palRows + 0.5)
    if thumbH < 3 then thumbH = 3 end
    local maxScroll = self.palRows - self.palVisRows
    local ty = r.y + floor((r.h - thumbH) * self.palScroll / maxScroll + 0.5)
    ops[#ops + 1] = { "rect", r.x + r.w - 2, ty, 2, thumbH, th.accent[1], th.accent[2], th.accent[3] }
  end
end

function Picker:sliderRow(channel)
  for _, row in ipairs(self.rows) do
    if row.channel == channel then return row end
  end
  return nil
end

function Picker:mouseDown(x, y)
  for _, row in ipairs(self.rows) do
    if row.kind == "slider" and Layout.contains(row.rect, x, y) then
      self.focus = row.channel
      local spec = { kind = "slider", min = 0, max = 255, step = 1 }
      local act = Widgets.slider.down(spec, row.rect, x, y, self.value[row.channel], self.ctx)
      if act and act.type == "set" then
        self.value[row.channel] = act.value
        self.dragChannel = row.channel
        self:syncHex()
        return { type = "set", value = self:cloneValue() }
      end
      return nil
    elseif row.kind == "hex" and Layout.contains(row.rect, x, y) then
      self.focus = "hex"
      self.hexEdit = Widgets.textField.begin(self:toHex(self.value))
      return nil
    elseif row.kind == "palette" and Layout.contains(row.rect, x, y) then
      local i = self:paletteIndexAt(x, y)
      if not i then return nil end
      local c = self.palette[i]
      self.focus = nil
      self.value.r, self.value.g, self.value.b = c[1], c[2], c[3]
      self:syncHex()
      return { type = "set", value = self:cloneValue() }
    end
  end
  return nil
end

function Picker:mouseDrag(x, y)
  if not self.dragChannel then
    -- plain motion (panel.lua forwards every move here): hovered swatch
    if #self.palette > 0 then self.palHover = self:paletteIndexAt(x, y) end
    return nil
  end
  local row = self:sliderRow(self.dragChannel)
  if not row then return nil end
  local spec = { kind = "slider", min = 0, max = 255, step = 1 }
  local act = Widgets.slider.drag(spec, row.rect, x, y, self.value[self.dragChannel], self.ctx)
  if act and act.type == "set" then
    self.value[self.dragChannel] = act.value
    self:syncHex()
    return { type = "set", value = self:cloneValue() }
  end
  return nil
end

function Picker:mouseUp()
  self.dragChannel = nil
  return nil
end

function Picker:wheel(x, y, dy)
  local pal = self:paletteRow()
  if pal and Layout.contains(pal.rect, x, y) then
    local maxScroll = self.palRows - self.palVisRows
    if maxScroll > 0 then
      local step = dy > 0 and 1 or (dy < 0 and -1 or 0)
      local ns = self.palScroll + step
      if ns < 0 then ns = 0 elseif ns > maxScroll then ns = maxScroll end
      self.palScroll = ns
      self.palHover = self:paletteIndexAt(x, y)
    end
    return nil
  end
  for _, row in ipairs(self.rows) do
    if row.channel and Layout.contains(row.rect, x, y) then
      local spec = { kind = "slider", min = 0, max = 255, step = 1 }
      local nv = Widgets.slider.wheel(spec, row.rect, dy, self.value[row.channel], self.ctx)
      if nv ~= nil and nv ~= self.value[row.channel] then
        self.value[row.channel] = nv
        self:syncHex()
        return { type = "set", value = self:cloneValue() }
      end
      return nil
    end
  end
  return nil
end

function Picker:key(key)
  if key == "Escape" then return { type = "close" } end
  if self.focus == "hex" and self.hexEdit then
    local ed, action = Widgets.textField.key({ kind = "textField", maxLen = 8 }, key, self.hexEdit, self.ctx, {})
    self.hexEdit = ed
    if action and action.type == "cancel" then
      self.focus = nil
      return nil
    end
    local v = self:parseHex(ed.text)
    if v then
      self.value = v
      return { type = "set", value = self:cloneValue() }
    end
    return nil
  end
  return nil
end

return M
