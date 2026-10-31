-- panel.lua — a scrolling column of widget rows driven by specs and an
-- adapter: the generic half of Spaghetti's ui_panel.lua, with the
-- State/node half replaced by six callbacks.
--
-- `M.new(specs, adapter, ctx)`:
--   `specs`   array of `{ kind = "slider", label = , min = , ... }`; a spec
--             with `row = "same"` shares the previous row (equal widths);
--   `adapter` `{ get(spec) -> value, set(spec, value), visible(spec),
--             enabled(spec), options(spec) -> items, label(spec),
--             selected(spec) -> index, action(spec, action) }`, all but
--             get/set optional -- FastFX plugs bind.lua in here and the
--             kit never learns what a "composition" is; `selected` feeds
--             `st.selected` to `list`/`gradientBar` paint (e.g. the
--             gradient stop being edited, separate from the widget's
--             own `value`); `action` receives any action this file does
--             not know (stopDown/barDown/stopDrag/stopDouble/activate...
--             from gradientBar/list) so a consumer can mutate its own
--             stop list or open a file without the kit knowing what
--             either of those are;
--   `ctx`     `{ theme, text }` (kit/ui/theme.lua, kit/ui/text.lua).
-- Then `P:layout(rect)`, `P:paint() -> ops`, `P:mouseDown(x, y, button)`,
-- `P:mouseMove(x, y)`, `P:mouseUp(x, y, button)`, `P:wheel(x, y, dy)`,
-- `P:key(key, mods)` -- every input returns true when it consumed the
-- event (the host repaints then). Buttons arrive as "left"/"right"/
-- "middle". Actions widgets return (set / openDropdown / openEntry /
-- openPicker / press / select / anything else) are applied here: set ->
-- adapter.set, press -> spec.onPress or adapter.set(spec, true), select
-- -> adapter.set(spec, index), openDropdown -> `P.dropdown`, openPicker
-- -> `P.picker` (kit/ui/colorpicker.lua, for the `color` widget; every
-- change it reports applies live through adapter.set; an optional
-- `adapter.palette(spec) -> { {r,g,b}, ... }` gives it a swatch grid,
-- re-read on every open), openEntry -> an
-- inline textField over the row, anything else -> `adapter.action`. Tab /
-- Shift+Tab move focus; Left/Right nudge a focused slider (Shift x5);
-- Enter (or a click on the value text) opens its entry, which starts
-- EMPTY with the current value as placeholder: type to replace, Enter on
-- an empty entry keeps the value, Esc cancels, Tab commits and opens the
-- next slider/number's entry.
--
-- Sections: a `separator` spec with text (and `collapsible ~= false`) is
-- a section header; a click on it returns toggleSection and the panel
-- drops every row after it up to the next header (or the end) from the
-- layout while folded (no paint, no hit, no Tab stop); a double click
-- (toggleAll) puts every header in the state that one just took. The
-- folded flag lives in `adapter.collapsed(spec) -> bool` /
-- `adapter.setCollapsed(spec, bool)` when the adapter has them (FastFX
-- persists them per separator id), else in `spec._collapsed`.
--
-- Wheel convention: the wheel over the panel ALWAYS scrolls it (even
-- over a slider/number/list); with the RIGHT button held (`rightHeld`,
-- tracked from mouseDown/mouseUp "right") the wheel adjusts the widget
-- under the pointer instead. An open dropdown/picker always gets the
-- wheel. A panel with nothing to scroll returns false so the host can
-- pass the wheel on.
--
-- It is NOT a window: no Dialog, no canvas, no timers; the host owns
-- those and forwards events (see the canvas-window template).

local M = {}

local floor = math.floor

local function loadSibling(name)
  local info = debug.getinfo(1, "S")
  local src = info and info.source or ""
  if src:sub(1, 1) == "@" then src = src:sub(2) end
  local dir = src:match("^(.*)[/\\]") or "."
  local chunk, err = loadfile(dir .. "/" .. name .. ".lua")
  if not chunk then error("kit/ui/panel: could not load " .. name .. ": " .. tostring(err)) end
  return chunk()
end

local Widgets = loadSibling("widgets")
local Dropdown = loadSibling("dropdown")
local Colorpicker = loadSibling("colorpicker")
local Events = loadSibling("events")
local Layout = loadSibling("layout")

M.Widgets = Widgets

local P = {}
P.__index = P

---------------------------------------
-- Construction --
---------------------------------------

local function defaultTrue() return true end

function M.new(specs, adapter, ctx)
  assert(type(adapter) == "table" and adapter.get and adapter.set,
    "panel.new: adapter needs get(spec) and set(spec, v)")
  assert(ctx and ctx.theme, "panel.new: ctx.theme required")
  local self = setmetatable({
    specs = specs or {},
    adapter = adapter,
    ctx = ctx,
    rect = { x = 0, y = 0, w = 200, h = 200 },
    scroll = 0,
    contentH = 0,
    rows = {},        -- { y, h, cells = { { spec, widget, rect } } }
    hover = nil,      -- cell under the pointer
    capture = nil,    -- cell receiving drags
    focus = nil,      -- cell with keyboard focus
    dropdown = nil,   -- open Dropdown + its cell
    picker = nil,     -- open Colorpicker + its cell
    entry = nil,      -- { cell, edit, rect }
    rightHeld = false, -- right button down: wheel adjusts instead of scrolling
    events = Events.new(),
    now = function() return os.clock() end,
    dirty = true,
  }, P)
  return self
end

---------------------------------------
-- Adapter access --
---------------------------------------

function P:visible(spec)
  local f = self.adapter.visible
  if f then return f(spec) ~= false end
  return spec.hidden ~= true
end

function P:enabled(spec)
  local f = self.adapter.enabled
  if f then return f(spec) ~= false end
  return spec.disabled ~= true
end

--- True for a separator with text: it starts a new section (and ends
--- the previous one) even when it is not foldable itself.
local function isTitled(spec)
  if spec.kind ~= "separator" then return false end
  local str = spec.text or spec.label
  return str ~= nil and str ~= ""
end

--- True for a separator that heads a FOLDABLE section.
function P:isHeader(spec)
  return spec.kind == "separator" and Widgets.separator.isHeader(spec)
end

function P:collapsed(spec)
  if not self:isHeader(spec) then return false end
  local f = self.adapter.collapsed
  if f then return f(spec) == true end
  return spec._collapsed == true
end

function P:setCollapsed(spec, v)
  v = (v == true)
  if self:collapsed(spec) == v then return false end
  local f = self.adapter.setCollapsed
  if f then f(spec, v) else spec._collapsed = v end
  self.dirty = true
  return true
end

--- Every section header spec, in order.
function P:headers()
  local out = {}
  for _, spec in ipairs(self.specs) do
    if self:isHeader(spec) then out[#out + 1] = spec end
  end
  return out
end

function P:labelOf(spec)
  local f = self.adapter.label
  if f then
    local l = f(spec)
    if l ~= nil then return l end
  end
  return spec.label
end

function P:optionsOf(spec)
  local f = self.adapter.options
  if f then
    local o = f(spec)
    if type(o) == "function" then o = o(spec) end
    if type(o) == "table" then return o end
  end
  local o = spec.options or spec.items or {}
  -- an adapter-less host may leave a function here; call it with the spec
  if type(o) == "function" then o = o(spec) end
  if type(o) ~= "table" then return {} end
  return o
end

---------------------------------------
-- Layout --
---------------------------------------

function P:invalidateLayout()
  self.dirty = true
end

--- Lays every visible spec out in `rect` (absolute canvas coords), one
--- row per spec unless `row = "same"`. Rows are positioned with the
--- current scroll applied, so their rects are what paint and hit use.
function P:layout(rect)
  if rect then self.rect = rect end
  local r = self.rect
  local th = self.ctx.theme
  local pad, gap = th.pad, th.gap
  local innerX, innerW = r.x + pad, r.w - pad * 2
  local rows = {}
  local y = pad
  local folded = false -- inside a collapsed section
  for _, spec in ipairs(self.specs) do
    local widget = Widgets.get(spec.kind)
    if isTitled(spec) then folded = self:collapsed(spec) end
    local shown = widget and self:visible(spec) and (not folded or isTitled(spec))
    if shown then
      if spec.kind == "list" then spec._items = self:optionsOf(spec) end
      local h = widget.measure(spec, self.ctx)
      local row = rows[#rows]
      if spec.row == "same" and row then
        row.cells[#row.cells + 1] = { spec = spec, widget = widget }
        if h > row.h then row.h = h end
      else
        row = { y = y, h = h, cells = { { spec = spec, widget = widget } } }
        rows[#rows + 1] = row
        y = y + h
      end
    end
  end
  -- second pass: y of a row may have shifted when a "same" cell was
  -- taller than the first; recompute from the top
  y = pad
  for _, row in ipairs(rows) do
    row.y = y
    y = y + row.h
  end
  self.contentH = y + pad
  local maxScroll = self.contentH - r.h
  if maxScroll < 0 then maxScroll = 0 end
  if self.scroll > maxScroll then self.scroll = maxScroll end
  if self.scroll < 0 then self.scroll = 0 end
  local scroll = self.scroll
  local body = { x = innerX, y = 0, w = innerW, h = 0 }
  for _, row in ipairs(rows) do
    body.y = r.y + row.y - scroll
    body.h = row.h
    local cols = Layout.columns(body, #row.cells, self.ctx.theme.colGap or gap)
    for i, cell in ipairs(row.cells) do
      cell.rect = cols[i]
      cell.row = row
    end
  end
  self.rows = rows
  self.dirty = false
  -- a cell held onto across a relayout may have vanished
  local function still(cell)
    if not cell then return nil end
    for _, row in ipairs(rows) do
      for _, c in ipairs(row.cells) do
        if c.spec == cell.spec then return c end
      end
    end
    return nil
  end
  self.hover = still(self.hover)
  self.focus = still(self.focus)
  self.capture = still(self.capture)
  if self.entry then
    local c = still(self.entry.cell)
    if c then
      self.entry.cell = c
      self.entry.rect = self:entryRect(c)
    else
      self.entry = nil
    end
  end
  return rows
end

function P:ensureLayout()
  if self.dirty then self:layout() end
end

--- Clamped scroll; relayout follows. Returns true when it moved.
function P:setScroll(v)
  local maxScroll = self.contentH - self.rect.h
  if maxScroll < 0 then maxScroll = 0 end
  if v > maxScroll then v = maxScroll end
  if v < 0 then v = 0 end
  v = floor(v)
  if v == self.scroll then return false end
  self.scroll = v
  self:layout()
  return true
end

--- The row (layout table) whose band contains canvas `y`, or nil.
function P:rowAt(y)
  self:ensureLayout()
  for _, row in ipairs(self.rows) do
    local ry = self.rect.y + row.y - self.scroll
    if y >= ry and y < ry + row.h then return row end
  end
  return nil
end

--- The cell under (x, y), or nil. Cells outside the panel rect are
--- not hittable (they are clipped away).
function P:cellAt(x, y)
  if not Layout.contains(self.rect, x, y) then return nil end
  local row = self:rowAt(y)
  if not row then return nil end
  for _, cell in ipairs(row.cells) do
    if Layout.contains(cell.rect, x, y) then return cell end
  end
  return nil
end

function P:entryRect(cell)
  local w = cell.widget
  if w.entryRect then
    local hasLabel = self:labelOf(cell.spec)
    return w.entryRect(cell.spec, cell.rect, self.ctx,
      hasLabel ~= nil and hasLabel ~= "")
  end
  return cell.rect
end

---------------------------------------
-- Paint --
---------------------------------------

function P:paint()
  self:ensureLayout()
  local th = self.ctx.theme
  local r = self.rect
  local ops = {}
  ops[#ops + 1] = { "save" }
  ops[#ops + 1] = { "clip", r.x, r.y, r.w, r.h }
  ops[#ops + 1] = { "rect", r.x, r.y, r.w, r.h, th.panel[1], th.panel[2], th.panel[3] }
  for _, row in ipairs(self.rows) do
    local ry = r.y + row.y - self.scroll
    if ry + row.h > r.y and ry < r.y + r.h then
      for _, cell in ipairs(row.cells) do
        local spec = cell.spec
        local st = {
          hot = (self.hover == cell),
          active = (self.capture == cell),
          focused = (self.focus == cell),
          disabled = not self:enabled(spec),
          label = self:labelOf(spec),
          selected = self.adapter.selected and self.adapter.selected(spec) or nil,
          collapsed = self:collapsed(spec),
        }
        local value = self.adapter.get(spec)
        local wops = cell.widget.paint(spec, cell.rect, value, st, self.ctx)
        for i = 1, #wops do ops[#ops + 1] = wops[i] end
      end
    end
  end
  if self.entry then
    local e = self.entry
    local st = { focused = true, editing = e.edit }
    local wops = Widgets.textField.paint(e.spec, e.rect, e.edit.text, st, self.ctx)
    for i = 1, #wops do ops[#ops + 1] = wops[i] end
  end
  ops[#ops + 1] = { "restore" }
  if self.dropdown then
    local dops = self.dropdown.model:paint()
    for i = 1, #dops do ops[#ops + 1] = dops[i] end
  end
  if self.picker then
    local pops = self.picker.model:paint()
    for i = 1, #pops do ops[#ops + 1] = pops[i] end
  end
  return ops
end

---------------------------------------
-- Actions --
---------------------------------------

function P:openEntry(cell)
  local spec = cell.spec
  local value = self.adapter.get(spec)
  local shown, placeholder
  if spec.kind == "slider" or spec.kind == "number" then
    -- numeric entries open empty so typing replaces; the current value
    -- shows as placeholder and an empty commit keeps it
    placeholder = Widgets.format(spec, value)
    shown = ""
  else
    shown = value
  end
  self.entry = {
    cell = cell,
    spec = { kind = "textField", maxLen = spec.maxLen, placeholder = placeholder },
    edit = Widgets.textField.begin(shown),
    rect = self:entryRect(cell),
  }
  self.focus = cell
end

function P:commitEntry()
  local e = self.entry
  if not e then return false end
  self.entry = nil
  local spec = e.cell.spec
  local v
  if spec.kind == "slider" or spec.kind == "number" then
    if e.edit.text == "" then return true end -- empty: keep the value
    v = Widgets.parseNumber(spec, e.edit.text)
    if v == nil then return true end -- not a number: keep the old value
  else
    v = e.edit.text
  end
  if v ~= self.adapter.get(spec) then self.adapter.set(spec, v) end
  return true
end

function P:cancelEntry()
  if not self.entry then return false end
  self.entry = nil
  return true
end

function P:openDropdown(cell, items)
  local spec = cell.spec
  -- `spec.options` may be a function (resolved by the adapter): never
  -- trust an items list handed over by the widget unless it is a table.
  if type(items) ~= "table" then items = self:optionsOf(spec) end
  if #items == 0 then return end
  local anchor = cell.rect
  if cell.widget.anchorRect then
    anchor = cell.widget.anchorRect(spec, cell.rect, self.ctx)
  end
  local value = self.adapter.get(spec)
  local current
  for i = 1, #items do
    if items[i] == value then current = i break end
  end
  local bounds = self.bounds or self.rect
  self.dropdown = {
    cell = cell,
    model = Dropdown.open(items, anchor, bounds, self.ctx, current),
  }
end

function P:closeDropdown()
  if not self.dropdown then return false end
  self.dropdown = nil
  return true
end

--- Opens a Colorpicker anchored on `cell` (the `color` widget's box).
function P:openPicker(cell)
  local spec = cell.spec
  local anchor = cell.rect
  if cell.widget.anchorRect then
    anchor = cell.widget.anchorRect(spec, cell.rect, self.ctx)
  end
  local value = self.adapter.get(spec)
  local bounds = self.bounds or self.rect
  -- `adapter.palette(spec)` (optional) -> swatch grid; read on every open,
  -- never cached, so the host can follow the active sprite
  local palette = self.adapter.palette and self.adapter.palette(spec) or nil
  self.picker = { cell = cell,
    model = Colorpicker.open(value, anchor, bounds, self.ctx, { palette = palette }) }
end

function P:closePicker()
  if not self.picker then return false end
  self.picker = nil
  return true
end

--- Applies a Colorpicker action ("set" every live change, "close" on Esc)
--- against the cell it was opened for.
function P:applyPickerAction(action)
  if not action or not self.picker then return end
  local spec = self.picker.cell.spec
  if action.type == "set" then
    self.adapter.set(spec, action.value)
  elseif action.type == "close" then
    self:closePicker()
  end
end

function P:apply(action, cell)
  if not action then return false end
  local spec = cell.spec
  local t = action.type
  if t == "set" then
    if action.value ~= self.adapter.get(spec) then
      self.adapter.set(spec, action.value)
    end
  elseif t == "openDropdown" then
    self:openDropdown(cell, action.items)
  elseif t == "openEntry" then
    self:openEntry(cell)
  elseif t == "openPicker" then
    self:openPicker(cell)
  elseif t == "press" then
    if spec.onPress then spec.onPress(spec) else self.adapter.set(spec, true) end
  elseif t == "select" then
    self.adapter.set(spec, action.index)
  elseif t == "toggleSection" then
    self:setCollapsed(spec, not self:collapsed(spec))
    self:layout() -- clamps the scroll to the new contentH
  elseif t == "toggleAll" then
    -- the first press of the double click already toggled this header:
    -- every section follows the state it has now
    local want = self:collapsed(spec)
    for _, h in ipairs(self:headers()) do self:setCollapsed(h, want) end
    self:layout()
  elseif self.adapter.action then
    -- unknown to the kit (stopDown/barDown/stopDrag/stopDouble/activate...):
    -- the consumer's adapter decides what it means
    self.adapter.action(spec, action)
  end
  return true
end

---------------------------------------
-- Mouse --
---------------------------------------

function P:mouseDown(x, y, button)
  self:ensureLayout()
  -- right-held state for the wheel; a left press also clears it so a
  -- right mouseUp lost outside the canvas cannot leave it stuck
  if button == "right" then
    self.rightHeld = true
  elseif button == "left" then
    self.rightHeld = false
  end
  -- an open picker eats the click: work its controls or dismiss
  if self.picker then
    local pk = self.picker
    if Layout.contains(pk.model.rect, x, y) then
      if button == "left" then
        self:applyPickerAction(pk.model:mouseDown(x, y))
      end
      return true
    end
    self:closePicker()
    if not Layout.contains(self.rect, x, y) then return false end
    return true
  end
  -- an open list eats the click: pick or dismiss
  if self.dropdown then
    local d = self.dropdown
    local i = d.model:indexAt(x, y)
    self:closeDropdown()
    if i then
      local item = d.model.items[i]
      if item ~= self.adapter.get(d.cell.spec) then
        self.adapter.set(d.cell.spec, item)
      end
      return true
    end
    if not Layout.contains(self.rect, x, y) then return false end
    -- fallthrough: the click that closed the list is not a second click
    return true
  end
  -- an entry commits on any click outside its box
  if self.entry then
    if Layout.contains(self.entry.rect, x, y) then return true end
    self:commitEntry()
  end
  if not Layout.contains(self.rect, x, y) then return false end
  local cell = self:cellAt(x, y)
  local ev = self.events:down(x, y, button, self.now(), cell)
  if button ~= "left" then return true end -- swallow other buttons
  if not cell or not self:enabled(cell.spec) then return true end
  local w = cell.widget
  if Widgets.focusable[cell.spec.kind] then self.focus = cell end
  local value = self.adapter.get(cell.spec)
  local action
  if ev.type == "doubleClick" and w.dblclick then
    action = w.dblclick(cell.spec, cell.rect, x, y, value, self.ctx)
  elseif w.down then
    action = w.down(cell.spec, cell.rect, x, y, value, self.ctx)
  end
  -- a click that opened an entry is not the start of a drag
  local opensEntry = type(action) == "table" and action.type == "openEntry"
  if (w.drag or w.up) and not opensEntry then self.capture = cell end
  self:apply(action, cell)
  return true
end

function P:mouseMove(x, y)
  self:ensureLayout()
  local ev = self.events:move(x, y)
  if self.picker then
    self:applyPickerAction(self.picker.model:mouseDrag(x, y))
    return true
  end
  if self.dropdown then
    self.dropdown.model:hover(x, y)
    return true
  end
  if self.capture then
    local cell = self.capture
    if cell.widget.drag and (ev.type == "drag" or ev.type == "dragStart"
      or ev.type == "hold") then
      local value = self.adapter.get(cell.spec)
      self:apply(cell.widget.drag(cell.spec, cell.rect, x, y, value, self.ctx), cell)
    end
    return true
  end
  local cell = self:cellAt(x, y)
  local changed = (cell ~= self.hover)
  self.hover = cell
  return changed or Layout.contains(self.rect, x, y)
end

function P:mouseUp(x, y, button)
  if button == "right" then self.rightHeld = false end
  if self.picker then
    self.picker.model:mouseUp(x, y)
    return true
  end
  local ev = self.events:up(x, y, button)
  local cell = self.capture
  self.capture = nil
  if cell then
    if cell.widget.up then
      local value = self.adapter.get(cell.spec)
      self:apply(cell.widget.up(cell.spec, cell.rect, x, y, value, self.ctx), cell)
    end
    return true
  end
  return Layout.contains(self.rect, x, y)
end

--- `dy > 0` is a wheel DOWN (Spaghetti's convention). The panel scrolls;
--- only with the right button held does a widget with its own wheel
--- (slider/number/list) take the event and move its value. Returns
--- false when the panel has nothing to scroll (the host passes it on).
function P:wheel(x, y, dy)
  self:ensureLayout()
  if self.picker then
    self:applyPickerAction(self.picker.model:wheel(x, y, dy))
    return true
  end
  if self.dropdown then
    self.dropdown.model:wheel(dy)
    return true
  end
  if not Layout.contains(self.rect, x, y) then return false end
  local cell = self:cellAt(x, y)
  -- scroll containers (lists) take the wheel on their own while they can
  -- still move; values (sliders, numbers) only with the right button held
  if cell and cell.widget.scrollable and self:enabled(cell.spec)
     and (not cell.widget.canScroll or cell.widget.canScroll(cell.spec, dy)) then
    cell.widget.wheel(cell.spec, cell.rect, dy, self.adapter.get(cell.spec), self.ctx)
    return true
  end
  if self.rightHeld then
    if cell and cell.widget.wheel and self:enabled(cell.spec) then
      local value = self.adapter.get(cell.spec)
      local nv = cell.widget.wheel(cell.spec, cell.rect, dy, value, self.ctx)
      if nv ~= nil and nv ~= value then self.adapter.set(cell.spec, nv) end
      return true
    end
  end
  if self.contentH <= self.rect.h then return false end
  local step = self.ctx.theme.scrollStep
  self:setScroll(self.scroll + ((dy > 0) and step or -step))
  return true
end

---------------------------------------
-- Keyboard --
---------------------------------------

--- Every focusable, enabled cell in reading order.
function P:focusOrder()
  local out = {}
  for _, row in ipairs(self.rows) do
    for _, cell in ipairs(row.cells) do
      if Widgets.focusable[cell.spec.kind] and self:enabled(cell.spec) then
        out[#out + 1] = cell
      end
    end
  end
  return out
end

function P:moveFocus(delta)
  local order = self:focusOrder()
  if #order == 0 then self.focus = nil return false end
  local at = 0
  for i, c in ipairs(order) do
    if c == self.focus then at = i break end
  end
  local i = at + delta
  if at == 0 then i = (delta > 0) and 1 or #order end
  if i < 1 then i = #order end
  if i > #order then i = 1 end
  self.focus = order[i]
  self:scrollTo(self.focus)
  return true
end

--- Scrolls just enough to show `cell`.
function P:scrollTo(cell)
  local r = self.rect
  local top = cell.row.y
  local bot = top + cell.row.h
  if top - self.scroll < 0 then
    self:setScroll(top - self.ctx.theme.pad)
  elseif bot - self.scroll > r.h then
    self:setScroll(bot - r.h + self.ctx.theme.pad)
  end
end

--- `mods` = `{ shift = bool, ctrl = bool, alt = bool }` (optional).
function P:key(key, mods)
  self:ensureLayout()
  mods = mods or {}
  if key == "Escape" then self.rightHeld = false end
  if self.picker then
    self:applyPickerAction(self.picker.model:key(key))
    return true
  end
  if self.entry then
    local e = self.entry
    if key == "Tab" then
      -- commit, hop to the next row and reopen if it takes an entry
      self:commitEntry()
      self:moveFocus(mods.shift and -1 or 1)
      local f = self.focus
      if f and (f.spec.kind == "slider" or f.spec.kind == "number") then
        self:openEntry(f)
      end
      return true
    end
    local _, action = Widgets.textField.key(e.spec, key, e.edit, self.ctx, mods)
    if action and action.type == "commit" then
      self:commitEntry()
    elseif action and action.type == "cancel" then
      self:cancelEntry()
    end
    return true
  end
  if self.dropdown then
    local d = self.dropdown
    if key == "Escape" then
      self:closeDropdown()
    elseif key == "Down" then
      d.model:step(1)
    elseif key == "Up" then
      d.model:step(-1)
    elseif key == "Enter" then
      local i = d.model.index
      self:closeDropdown()
      if i then self.adapter.set(d.cell.spec, d.model.items[i]) end
    else
      return false
    end
    return true
  end
  if key == "Tab" then
    return self:moveFocus(mods.shift and -1 or 1)
  end
  local cell = self.focus
  if not cell or not self:enabled(cell.spec) then return false end
  local w = cell.widget
  if cell.spec.kind == "textField" then
    -- a focused text field is edited through its inline entry only:
    -- textField.key works on an edit state, not on the committed value
    if key == "Enter" then
      self:openEntry(cell)
      return true
    end
    return false
  end
  if not w.key then return false end
  local value = self.adapter.get(cell.spec)
  local ret = w.key(cell.spec, key, value, self.ctx, mods)
  if ret == nil then return false end
  if type(ret) == "table" and ret.type then
    self:apply(ret, cell)
  elseif ret ~= value then
    self.adapter.set(cell.spec, ret)
  end
  return true
end

return M
