-- dropdown.lua — the model of an open combo list: where it sits, which
-- options are on screen, which one is under the pointer, how far it has
-- scrolled. Geometry follows Spaghetti/src/dropdown.lua (16px rows, 2px
-- border, ceiling = the bounds rect, scroll when it does not fit) and
-- its panel's "open upward when there is no room below" rule.
--
-- `M.open(items, anchorRect, boundsRect, ctx, current)` returns a
-- dropdown `D` with `items`, `index` (the highlighted option), `rect`,
-- `scroll`, `rowH`; `D:paint() -> ops` (the panel emits these LAST so
-- the list sits on top), `D:indexAt(x, y)`, `D:wheel(dy)`,
-- `D:hover(x, y)`.
--
-- It is NOT a widget and applies no value: the panel reads `indexAt` on
-- the click that closes it and calls its adapter.

local M = {}

local floor = math.floor

local D = {}
D.__index = D

--- `anchor` is the combo box; the list takes its width. `bounds` is the
--- area the list may cover (the panel's rect, or the whole canvas).
--- `current` (an index) is scrolled into view and highlighted.
function M.open(items, anchor, bounds, ctx, current)
  local th = ctx.theme
  local rowH = th.dropRowH
  local n = #items
  local pad = 2
  local gap = 2
  local below = (bounds.y + bounds.h - gap) - (anchor.y + anchor.h)
  local above = anchor.y - bounds.y - gap
  local avail = below
  local up = false
  local full = n * rowH + pad
  if full > below and above > below then
    up = true
    avail = above
  end
  local maxRows = floor((avail - pad) / rowH)
  if maxRows < 1 then maxRows = 1 end
  local rows = n < maxRows and n or maxRows
  local h = rows * rowH + pad
  local y = up and (anchor.y - h) or (anchor.y + anchor.h)
  local lowest = bounds.y + bounds.h - h - gap
  if y > lowest then y = lowest end
  if y < bounds.y then y = bounds.y end
  local x = anchor.x
  if x + anchor.w > bounds.x + bounds.w then x = bounds.x + bounds.w - anchor.w end
  if x < bounds.x then x = bounds.x end

  local self = setmetatable({
    items = items, index = current, rect = { x = x, y = y, w = anchor.w, h = h },
    scroll = 0, rows = rows, rowH = rowH, pad = pad, ctx = ctx,
    maxScroll = n - rows,
  }, D)
  if self.maxScroll < 0 then self.maxScroll = 0 end
  -- bring the current option into view (Spaghetti's `want`)
  if current then
    if current > rows then self.scroll = current - rows end
    self:clamp()
  end
  return self
end

function D:clamp()
  if self.scroll > self.maxScroll then self.scroll = self.maxScroll end
  if self.scroll < 0 then self.scroll = 0 end
end

--- The option index under (x, y), or nil.
function D:indexAt(x, y)
  local r = self.rect
  if x < r.x or x >= r.x + r.w or y < r.y or y >= r.y + r.h then return nil end
  local row = floor((y - r.y - 1) / self.rowH) + 1
  if row < 1 or row > self.rows then return nil end
  local i = row + self.scroll
  if i > #self.items then return nil end
  return i
end

--- Highlights whatever is under (x, y). Returns true when it changed.
function D:hover(x, y)
  local i = self:indexAt(x, y)
  if i ~= self.index then
    self.index = i
    return true
  end
  return false
end

--- Wheel down (dy > 0) walks down the list, 3 options a notch.
--- Returns true when something moved.
function D:wheel(dy)
  local was = self.scroll
  self.scroll = self.scroll + ((dy > 0) and 3 or -3)
  self:clamp()
  return self.scroll ~= was
end

--- Keyboard: Up/Down move the highlight, keeping it on screen.
function D:step(delta)
  local i = (self.index or 0) + delta
  if i < 1 then i = 1 end
  if i > #self.items then i = #self.items end
  self.index = i
  if i <= self.scroll then self.scroll = i - 1 end
  if i > self.scroll + self.rows then self.scroll = i - self.rows end
  self:clamp()
  return i
end

function D:paint()
  local th = self.ctx.theme
  local r = self.rect
  local ops = {}
  ops[#ops + 1] = { "rect", r.x, r.y, r.w, r.h, th.dropdownBg[1], th.dropdownBg[2], th.dropdownBg[3] }
  ops[#ops + 1] = { "frame", r.x, r.y, r.w, r.h, th.frame[1], th.frame[2], th.frame[3] }
  local fh = self.ctx.text and self.ctx.text:height() or th.fontH
  local ty = floor((self.rowH - fh) / 2)
  local s = th.uiScale
  for row = 1, self.rows do
    local i = row + self.scroll
    local opt = self.items[i]
    if opt ~= nil then
      local oy = r.y + 1 + (row - 1) * self.rowH
      if i == self.index then
        ops[#ops + 1] = { "rect", r.x + 1, oy, r.w - 2, self.rowH,
          th.boxHot[1], th.boxHot[2], th.boxHot[3] }
      end
      local str = tostring(opt)
      if self.ctx.text then
        str = self.ctx.text:ellipsize(str, r.w - 10 * s)
      end
      ops[#ops + 1] = { "text", r.x + 5 * s, oy + ty,
        th.textValue[1], th.textValue[2], th.textValue[3], str }
    end
  end
  -- a list that can scroll has to look like it can
  if self.maxScroll > 0 then
    local track = r.h - self.pad
    local thumbH = floor(track * self.rows / #self.items + 0.5)
    if thumbH < 4 then thumbH = 4 end
    if thumbH > track then thumbH = track end
    local ty2 = r.y + 1 + floor((track - thumbH) * self.scroll / self.maxScroll + 0.5)
    ops[#ops + 1] = { "rect", r.x + r.w - 3, ty2, 2, thumbH,
      th.textDim[1], th.textDim[2], th.textDim[3] }
  end
  return ops
end

return M
