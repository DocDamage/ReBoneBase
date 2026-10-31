-- widgets.lua — the canvas widgets of kit/ui: slider, number, switch,
-- combo, button, label, separator (collapsible section header), tabs, textField.
--
-- Every widget is a table of pure functions over a `spec` (the caller's
-- declaration: kind, min, max, step, options...), a `rect`, a `value`
-- and a state `st = { hot, active, focused, disabled, editing, label }`:
--   `measure(spec, ctx) -> h`            row height it wants
--   `paint(spec, rect, value, st, ctx) -> ops`
--   `hit(spec, rect, x, y, ctx) -> part | nil`   ("track", "box", "toggle",
--                                                 "arrow", "body", index...)
--   `down/drag/up(spec, rect, x, y, value, ctx) -> action | nil`
--   `dblclick(spec, rect, x, y, value, ctx) -> action | nil`
--   `wheel(spec, rect, dy, value, ctx) -> newValue | nil`
--   `key(spec, key, value, ctx, mods) -> newValue | action | nil`
-- Actions: `{ type = "set", value = }`, `{ type = "openDropdown" }`,
-- `{ type = "openEntry" }`, `{ type = "press" }`, `{ type = "select",
-- index = }`. The LOOK and the geometry are Spaghetti's ui_panel.lua
-- (drawBox, the slider's label line + 8px track, the 55%-wide combo box);
-- all numbers come from `ctx.theme`, no size is hardcoded here.
--
-- Widgets do NOT own state, timers, focus or a mouse: panel.lua does.
-- Nothing here touches Aseprite; `paint` returns ops for a painter.

local M = {}

local floor = math.floor

---------------------------------------
-- Op helpers --
---------------------------------------

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

local function textW(ctx, str)
  if ctx.text then return ctx.text:width(str) end
  return #tostring(str) * ctx.theme.charW
end

--- Spaghetti's drawBox: fill, then a 1px frame.
local function box(ops, x, y, w, h, fill, st, th)
  local c = fill
  if st and st.hot and not st.disabled then c = th.boxHot end
  rect(ops, x, y, w, h, c)
  frame(ops, x, y, w, h, (st and st.focused) and th.focus or th.frame)
end

local function textColor(st, th, normal)
  if st and st.disabled then return th.disabled end
  return normal or th.text
end

local function inRect(x, y, rx, ry, rw, rh)
  return x >= rx and y >= ry and x < rx + rw and y < ry + rh
end

---------------------------------------
-- Numeric helpers --
---------------------------------------

local function decimalsOf(step)
  local d = 0
  local s = step or 1
  while d < 6 and s ~= floor(s) do
    s = s * 10
    d = d + 1
  end
  return d
end

local function clampStep(spec, v)
  local lo = spec.min or 0
  local hi = spec.max or 100
  local step = spec.step or 1
  if step > 0 then
    v = lo + floor((v - lo) / step + 0.5) * step
  end
  if v < lo then v = lo end
  if v > hi then v = hi end
  -- kill float dust: 3 * 0.1 is 0.30000000000000004, and an adapter
  -- comparing values would see a change on every drag
  if step >= 1 and step == floor(step) then
    v = floor(v + 0.5)
  elseif step > 0 then
    v = tonumber(string.format("%." .. decimalsOf(step) .. "f", v))
  end
  return v
end
M.clampStep = clampStep

--- The string a numeric value shows.
local function fmt(spec, v)
  v = tonumber(v) or spec.min or 0
  if spec.format then return spec.format(v) end
  local d = spec.decimals or decimalsOf(spec.step)
  if d == 0 then return tostring(floor(v + 0.5)) end
  return string.format("%." .. d .. "f", v)
end
M.format = fmt

--- Parses a typed value; nil when it is not a number.
function M.parseNumber(spec, str)
  local v = tonumber(str)
  if not v then return nil end
  return clampStep(spec, v)
end

---------------------------------------
-- slider --
---------------------------------------

local slider = {}

function slider.measure(spec, ctx) return ctx.theme.sliderH end

local function sliderTrack(spec, r, th)
  local ty = r.y + r.h - th.trackH - 3 * th.uiScale
  return r.x, ty, r.w, th.trackH
end

function slider.paint(spec, r, value, st, ctx)
  local th = ctx.theme
  local ops = {}
  local v = tonumber(value) or spec.min or 0
  local tc = textColor(st, th, th.text)
  if st.label and st.label ~= "" then text(ops, r.x, r.y, tc, st.label) end
  local vs = fmt(spec, v)
  local vw = textW(ctx, vs)
  spec._valueW = vw -- remembered for hit(): the value text's clickable zone
  text(ops, r.x + r.w - vw, r.y, tc, vs)
  local tx, ty, tw, tH = sliderTrack(spec, r, th)
  rect(ops, tx, ty, tw, tH, th.sliderTrack)
  frame(ops, tx, ty, tw, tH, st.focused and th.focus or th.frame)
  local lo, hi = spec.min or 0, spec.max or 100
  local t = 0
  if hi > lo then
    t = (v - lo) / (hi - lo)
    if t < 0 then t = 0 elseif t > 1 then t = 1 end
  end
  local fw = floor((tw - 2) * t)
  if fw > 0 then
    rect(ops, tx + 1, ty + 1, fw, tH - 2,
      st.disabled and th.disabled or th.sliderFill)
  end
  return ops
end

--- The value text's zone on the label line: its measured width (from the
--- last paint, else the widest value the spec can show) plus a pad.
local function sliderValueX(spec, r, ctx)
  local vw = spec._valueW
  if not vw then vw = textW(ctx, fmt(spec, spec.max or 100)) end
  return r.x + r.w - vw - ctx.theme.pad * ctx.theme.uiScale
end

function slider.hit(spec, r, x, y, ctx)
  if not inRect(x, y, r.x, r.y, r.w, r.h) then return nil end
  local tx, ty, tw, tH = sliderTrack(spec, r, ctx.theme)
  if y >= ty - 2 and y < ty + tH + 2 then return "track" end
  if x >= sliderValueX(spec, r, ctx) then return "value" end
  return "body"
end

--- Spaghetti's sliderApply: x across the track's inner width -> value.
local function sliderApply(spec, r, x, ctx)
  local tx, _, tw = sliderTrack(spec, r, ctx.theme)
  local bx, bw = tx + 1, tw - 2
  local t = (x - bx) / bw
  if t < 0 then t = 0 elseif t > 1 then t = 1 end
  local lo, hi = spec.min or 0, spec.max or 100
  return { type = "set", value = clampStep(spec, lo + t * (hi - lo)) }
end

function slider.down(spec, r, x, y, value, ctx)
  if slider.hit(spec, r, x, y, ctx) == "value" then
    return { type = "openEntry" }
  end
  return sliderApply(spec, r, x, ctx)
end

function slider.drag(spec, r, x, y, value, ctx)
  return sliderApply(spec, r, x, ctx)
end

function slider.dblclick(spec, r, x, y, value, ctx)
  return { type = "openEntry" }
end

function slider.wheel(spec, r, dy, value, ctx)
  local step = spec.step or 1
  local v = tonumber(value) or spec.min or 0
  return clampStep(spec, v + ((dy > 0) and -step or step))
end

function slider.key(spec, key, value, ctx, mods)
  local step = spec.step or 1
  if mods and mods.shift then step = step * 5 end
  local v = tonumber(value) or spec.min or 0
  if key == "Left" or key == "Down" then return clampStep(spec, v - step) end
  if key == "Right" or key == "Up" then return clampStep(spec, v + step) end
  if key == "Enter" then return { type = "openEntry" } end
  return nil
end

--- Where an inline entry sits over a slider: the value corner, box-high.
function slider.entryRect(spec, r, ctx)
  local th = ctx.theme
  local w = floor(r.w * 0.45)
  return { x = r.x + r.w - w, y = r.y, w = w, h = th.boxH }
end

M.slider = slider

---------------------------------------
-- number (typed value, wheel nudges) --
---------------------------------------

local number = {}

function number.measure(spec, ctx) return ctx.theme.rowH end

local function numberBox(spec, r, th)
  local s = th.uiScale
  local bw = r.w - 70 * s
  if bw < 40 * s then bw = 40 * s end
  if bw > 74 * s then bw = 74 * s end
  if bw > r.w then bw = r.w end
  local by = r.y + floor((r.h - th.boxH) / 2)
  return r.x + r.w - bw, by, bw, th.boxH
end

function number.paint(spec, r, value, st, ctx)
  local th = ctx.theme
  local ops = {}
  local bx, by, bw, bh = numberBox(spec, r, th)
  local ty = by + floor((bh - fontH(ctx)) / 2)
  if st.label and st.label ~= "" then
    text(ops, r.x, ty, textColor(st, th, th.text), st.label)
  end
  box(ops, bx, by, bw, bh, th.box, st, th)
  text(ops, bx + 4 * th.uiScale, ty, textColor(st, th, th.textValue),
    fmt(spec, value))
  return ops
end

function number.hit(spec, r, x, y, ctx)
  if not inRect(x, y, r.x, r.y, r.w, r.h) then return nil end
  local bx, by, bw, bh = numberBox(spec, r, ctx.theme)
  if inRect(x, y, bx, by, bw, bh) then return "box" end
  return "body"
end

function number.down(spec, r, x, y, value, ctx)
  if number.hit(spec, r, x, y, ctx) == "box" then
    return { type = "openEntry" }
  end
  return nil
end

number.wheel = slider.wheel
number.key = slider.key

function number.entryRect(spec, r, ctx)
  local bx, by, bw, bh = numberBox(spec, r, ctx.theme)
  return { x = bx, y = by, w = bw, h = bh }
end

M.number = number

---------------------------------------
-- switch --
---------------------------------------

local switch = {}

function switch.measure(spec, ctx) return ctx.theme.rowH end

local function switchPill(r, th)
  return r.x, r.y + floor((r.h - th.switchH) / 2), th.switchW, th.switchH
end

function switch.paint(spec, r, value, st, ctx)
  local th = ctx.theme
  local ops = {}
  local px, py, pw, ph = switchPill(r, th)
  local on = value == true
  local fill = on and th.switchOn or th.switchOff
  if st.disabled then fill = th.disabled end
  rect(ops, px, py, pw, ph, fill)
  frame(ops, px, py, pw, ph, st.focused and th.focus or th.frame)
  local kw = ph - 4
  local kx = on and (px + pw - 2 - kw) or (px + 2)
  rect(ops, kx, py + 2, kw, kw, on and th.accentText or th.textDim)
  if st.label and st.label ~= "" then
    text(ops, px + pw + th.gap + 2 * th.uiScale,
      r.y + floor((r.h - fontH(ctx)) / 2), textColor(st, th, th.text),
      st.label)
  end
  return ops
end

function switch.hit(spec, r, x, y, ctx)
  if not inRect(x, y, r.x, r.y, r.w, r.h) then return nil end
  local px, py, pw, ph = switchPill(r, ctx.theme)
  if inRect(x, y, px, py, pw, ph) then return "toggle" end
  return "body"
end

function switch.down(spec, r, x, y, value, ctx)
  return { type = "set", value = not (value == true) }
end

function switch.key(spec, key, value, ctx, mods)
  if key == "Enter" or key == "Space" or key == " " then
    return not (value == true)
  end
  return nil
end

M.switch = switch

---------------------------------------
-- combo --
---------------------------------------

local combo = {}

function combo.measure(spec, ctx) return ctx.theme.rowH end

local function comboBox(spec, r, th)
  local bw = floor(r.w * 0.55)
  local by = r.y + floor((r.h - th.boxH) / 2)
  return r.x + r.w - bw, by, bw, th.boxH
end
combo.box = comboBox

function combo.paint(spec, r, value, st, ctx)
  local th = ctx.theme
  local ops = {}
  local bx, by, bw, bh = comboBox(spec, r, th)
  local ty = by + floor((bh - fontH(ctx)) / 2)
  if st.label and st.label ~= "" then
    text(ops, r.x, ty, textColor(st, th, th.text), st.label)
  end
  box(ops, bx, by, bw, bh, th.box, st, th)
  local s = th.uiScale
  local arrowW = 10 * s
  local shown = tostring(value == nil and "" or value)
  if ctx.text then shown = ctx.text:ellipsize(shown, bw - 4 * s - arrowW - 2 * s) end
  text(ops, bx + 4 * s, ty, textColor(st, th, th.textValue), shown)
  text(ops, bx + bw - arrowW, ty, textColor(st, th, th.textDim), "v")
  return ops
end

function combo.hit(spec, r, x, y, ctx)
  if not inRect(x, y, r.x, r.y, r.w, r.h) then return nil end
  local bx, by, bw, bh = comboBox(spec, r, ctx.theme)
  if inRect(x, y, bx, by, bw, bh) then
    if x >= bx + bw - 12 * ctx.theme.uiScale then return "arrow" end
    return "box"
  end
  return "body"
end

function combo.down(spec, r, x, y, value, ctx)
  local part = combo.hit(spec, r, x, y, ctx)
  if part == "box" or part == "arrow" then
    return { type = "openDropdown" }
  end
  return nil
end

function combo.key(spec, key, value, ctx, mods)
  if key == "Enter" or key == "Space" or key == " " then
    return { type = "openDropdown" }
  end
  return nil
end

--- Anchor for the dropdown: the box itself.
function combo.anchorRect(spec, r, ctx)
  local bx, by, bw, bh = comboBox(spec, r, ctx.theme)
  return { x = bx, y = by, w = bw, h = bh }
end

M.combo = combo

---------------------------------------
-- button --
---------------------------------------

local button = {}

function button.measure(spec, ctx) return ctx.theme.rowH end

function button.paint(spec, r, value, st, ctx)
  local th = ctx.theme
  local ops = {}
  local by = r.y + floor((r.h - th.boxH) / 2)
  local fill = th.button
  if st.active then fill = th.accent end
  box(ops, r.x, by, r.w, th.boxH, fill, st, th)
  local label = st.label or spec.text or spec.label or ""
  local c = st.active and th.accentText or textColor(st, th, th.textValue)
  text(ops, r.x + 5 * th.uiScale, by + floor((th.boxH - fontH(ctx)) / 2),
    c, label)
  return ops
end

function button.hit(spec, r, x, y, ctx)
  if inRect(x, y, r.x, r.y, r.w, r.h) then return "body" end
  return nil
end

function button.down(spec, r, x, y, value, ctx)
  return nil -- armed; the press fires on release inside
end

function button.up(spec, r, x, y, value, ctx)
  if inRect(x, y, r.x, r.y, r.w, r.h) then return { type = "press" } end
  return nil
end

function button.key(spec, key, value, ctx, mods)
  if key == "Enter" or key == "Space" or key == " " then
    return { type = "press" }
  end
  return nil
end

M.button = button

---------------------------------------
-- label --
---------------------------------------

local label = {}

function label.measure(spec, ctx) return ctx.theme.labelH end

function label.paint(spec, r, value, st, ctx)
  local th = ctx.theme
  local ops = {}
  local str = st.label or spec.text or (value ~= nil and tostring(value)) or ""
  local c = spec.dim and th.textDim or th.text
  if spec.warning then c = th.warning end
  text(ops, r.x, r.y, textColor(st, th, c), str)
  return ops
end

function label.hit() return nil end

M.label = label

---------------------------------------
-- separator --
---------------------------------------

local separator = {}

--- A separator with text is a SECTION HEADER: unless `spec.collapsible ==
--- false` it paints a small triangle (rects, not a glyph: painter_buf's
--- 5x7 font and Aseprite's must agree), the text and the line, and a
--- click anywhere on the row returns `{ type = "toggleSection" }`; a
--- double click returns `{ type = "toggleAll" }`. `st.collapsed` (from
--- panel.lua) picks the triangle: pointing down = open, right = folded.
--- Without text it is only a line and never hittable.
local function isHeader(spec, st)
  local str = (st and st.label) or spec.text or spec.label
  return (str ~= nil and str ~= "") and spec.collapsible ~= false
end

separator.isHeader = isHeader

--- Width the triangle takes before the text (theme.sepArrowW).
local function sepArrowW(ctx)
  return ctx.theme.sepArrowW * ctx.theme.uiScale
end

--- A labelled separator (section header) is a label row plus `sepH` of
--- breathing room above it, so the header does not sit glued to the
--- previous row's widget; the text is bottom-aligned in that box.
function separator.measure(spec, ctx)
  local th = ctx.theme
  local str = spec.text or spec.label
  if str and str ~= "" then return th.labelH + th.sepH end
  return th.sepH
end

--- Triangle made of 1px rects, 7x4 px: pointing right (folded) or down
--- (open); `s` is the ui scale.
local function triangle(ops, x, y, collapsed, c, s)
  for i = 0, 3 do
    local len = (7 - 2 * i) * s
    if collapsed then
      rect(ops, x + i * s, y + i * s, s, len, c)   -- columns 7,5,3,1 high
    else
      rect(ops, x + i * s, y + i * s, len, s, c)   -- rows 7,5,3,1 wide
    end
  end
end

separator.triangle = triangle

function separator.paint(spec, r, value, st, ctx)
  local th = ctx.theme
  local ops = {}
  local str = st.label or spec.text or spec.label
  if str and str ~= "" then
    local ty = r.y + math.max(0, r.h - th.labelH)
    local tx = r.x
    local ly = ty + floor(fontH(ctx) / 2)
    if isHeader(spec, st) then
      local aw = sepArrowW(ctx)
      local c = (st.hot and not st.disabled) and th.text or th.textDim
      -- the 7x7 triangle sits on the text's middle line; rects start at
      -- ly - 3 so both orientations are centred on it
      triangle(ops, r.x, ly - 3 * th.uiScale, st.collapsed, c, th.uiScale)
      tx = r.x + aw
    end
    text(ops, tx, ty, th.textDim, str)
    local tw = (tx - r.x) + textW(ctx, str) + th.gap
    if r.w - tw > 0 then
      rect(ops, r.x + tw, ly, r.w - tw, 1, th.separator)
    end
  else
    rect(ops, r.x, r.y + floor(r.h / 2), r.w, 1, th.separator)
  end
  return ops
end

function separator.hit(spec, r, x, y)
  if not isHeader(spec) then return nil end
  if x >= r.x and x < r.x + r.w and y >= r.y and y < r.y + r.h then return "header" end
  return nil
end

function separator.down(spec, r, x, y)
  if separator.hit(spec, r, x, y) then return { type = "toggleSection" } end
  return nil
end

function separator.dblclick(spec, r, x, y)
  if separator.hit(spec, r, x, y) then return { type = "toggleAll" } end
  return nil
end

M.separator = separator

---------------------------------------
-- tabs --
---------------------------------------

local tabs = {}

function tabs.measure(spec, ctx) return ctx.theme.tabH end

--- Spaghetti's tabWidth: text + 18 at 1x.
local function tabWidth(ctx, str)
  return textW(ctx, str) + ctx.theme.tabPad * 2
end
tabs.tabWidth = tabWidth

--- Sum of every tab's width.
function tabs.totalWidth(spec, ctx)
  local items = spec.items or {}
  local w = 0
  for i = 1, #items do w = w + tabWidth(ctx, tostring(items[i])) end
  return w
end

--- Width of the "<" / ">" indicator zones painted at the bar's edges when
--- the tabs do not fit.
local function arrowW(ctx)
  return textW(ctx, "<") + ctx.theme.tabPad
end
tabs.arrowW = arrowW

--- True when the tabs are wider than the rect (the bar scrolls).
function tabs.overflow(spec, r, ctx)
  return tabs.totalWidth(spec, ctx) > r.w
end

--- Clamps `spec._scroll` (px hidden on the left) to the bar; 0 when the
--- tabs fit. Returns the clamped value.
function tabs.clampScroll(spec, r, ctx)
  local max = math.max(0, tabs.totalWidth(spec, ctx) - r.w)
  local sc = math.max(0, math.min(spec._scroll or 0, max))
  spec._scroll = sc
  return sc
end

--- Left edge (bar-relative, unscrolled) and width of tab `i`.
local function tabSpan(spec, i, ctx)
  local items = spec.items or {}
  local x = 0
  for k = 1, #items do
    local tw = tabWidth(ctx, tostring(items[k]))
    if k == i then return x, tw end
    x = x + tw
  end
  return nil
end

--- Scrolls so tab `i` is fully visible between the indicator zones.
--- Returns true when the scroll changed.
function tabs.scrollTo(spec, r, i, ctx)
  local before = tabs.clampScroll(spec, r, ctx)
  if not tabs.overflow(spec, r, ctx) then return false end
  local x, tw = tabSpan(spec, i, ctx)
  if not x then return false end
  local aw = arrowW(ctx)
  local sc = before
  if x - sc < aw then sc = x - aw end
  if x + tw - sc > r.w - aw then sc = x + tw - (r.w - aw) end
  spec._scroll = sc
  return tabs.clampScroll(spec, r, ctx) ~= before
end

--- "prev" / "next" when (x, y) is on a visible indicator zone, else nil.
function tabs.hitIndicator(spec, r, x, y, ctx)
  if not inRect(x, y, r.x, r.y, r.w, r.h) then return nil end
  if not tabs.overflow(spec, r, ctx) then return nil end
  local sc = tabs.clampScroll(spec, r, ctx)
  local aw = arrowW(ctx)
  local max = tabs.totalWidth(spec, ctx) - r.w
  if sc > 0 and x < r.x + aw then return "prev" end
  if sc < max and x >= r.x + r.w - aw then return "next" end
  return nil
end

--- Scrolls one tab towards `dir` ("prev" / "next"): the first tab cut on
--- that side becomes fully visible.
function tabs.scrollStep(spec, r, dir, ctx)
  local sc = tabs.clampScroll(spec, r, ctx)
  local items = spec.items or {}
  local aw = arrowW(ctx)
  for i = 1, #items do
    local x, tw = tabSpan(spec, i, ctx)
    if dir == "prev" and x - sc < aw and x + tw - sc > aw then
      return tabs.scrollTo(spec, r, i, ctx)
    elseif dir == "next" and x - sc < r.w - aw and x + tw - sc > r.w - aw then
      return tabs.scrollTo(spec, r, i, ctx)
    end
  end
  return false
end

--- `value` is the selected index (1-based); `spec.items` the labels.
--- When the tabs do not fit they are clipped to the rect, shifted by
--- `spec._scroll`, and "<" / ">" zones mark the hidden sides.
function tabs.paint(spec, r, value, st, ctx)
  local th = ctx.theme
  local ops = {}
  local items = spec.items or {}
  local over = tabs.overflow(spec, r, ctx)
  local sc = tabs.clampScroll(spec, r, ctx)
  rect(ops, r.x, r.y, r.w, r.h, th.tabBg)
  rect(ops, r.x, r.y + r.h - 1, r.w, 1, th.frame)
  if over then
    ops[#ops + 1] = { "save" }
    ops[#ops + 1] = { "clip", r.x, r.y, r.w, r.h }
  end
  local tx = r.x - sc
  local ty = r.y + floor((r.h - fontH(ctx)) / 2)
  for i = 1, #items do
    local str = tostring(items[i])
    local tw = tabWidth(ctx, str)
    local active = (value == i)
    if active then rect(ops, tx, r.y, tw, r.h, th.tabActive) end
    frame(ops, tx, r.y, tw, r.h, th.frame)
    text(ops, tx + th.tabPad, ty, active and th.text or th.textDim, str)
    tx = tx + tw
  end
  if over then
    ops[#ops + 1] = { "restore" }
    local aw = arrowW(ctx)
    local max = tabs.totalWidth(spec, ctx) - r.w
    local ax = floor(th.tabPad / 2)
    if sc > 0 then
      rect(ops, r.x, r.y, aw, r.h - 1, th.tabBg)
      rect(ops, r.x + aw - 1, r.y, 1, r.h - 1, th.frame)
      text(ops, r.x + ax, ty, th.text, "<")
    end
    if sc < max then
      rect(ops, r.x + r.w - aw, r.y, aw, r.h - 1, th.tabBg)
      rect(ops, r.x + r.w - aw, r.y, 1, r.h - 1, th.frame)
      text(ops, r.x + r.w - aw + ax, ty, th.text, ">")
    end
  end
  return ops
end

--- The index of the tab under x, or nil (the strip past the last tab and
--- the indicator zones are part of the bar but select nothing). Honours
--- `spec._scroll`.
function tabs.hit(spec, r, x, y, ctx)
  if not inRect(x, y, r.x, r.y, r.w, r.h) then return nil end
  if tabs.hitIndicator(spec, r, x, y, ctx) then return nil end
  local items = spec.items or {}
  local tx = r.x - tabs.clampScroll(spec, r, ctx)
  for i = 1, #items do
    local tw = tabWidth(ctx, tostring(items[i]))
    if x >= tx and x < tx + tw then return i end
    tx = tx + tw
  end
  return nil
end

--- Click: a tab -> `{ type = "select", index }` (scrolled into view); an
--- indicator -> scrolls one tab and returns `{ type = "scroll" }`.
function tabs.down(spec, r, x, y, value, ctx)
  local dir = tabs.hitIndicator(spec, r, x, y, ctx)
  if dir then
    tabs.scrollStep(spec, r, dir, ctx)
    return { type = "scroll" }
  end
  local i = tabs.hit(spec, r, x, y, ctx)
  if i and i ~= value then
    tabs.scrollTo(spec, r, i, ctx)
    return { type = "select", index = i }
  end
  return nil
end

function tabs.key(spec, key, value, ctx, mods)
  local n = #(spec.items or {})
  local v = tonumber(value) or 1
  if key == "Left" and v > 1 then return { type = "select", index = v - 1 } end
  if key == "Right" and v < n then return { type = "select", index = v + 1 } end
  return nil
end

--- Wheel over the bar: dy > 0 selects the next tab, dy < 0 the previous
--- (clamped), scrolling the bar so the new tab is visible. Returns
--- `{ type = "select", index }` or nil at the ends.
function tabs.wheel(spec, r, dy, value, ctx)
  local n = #(spec.items or {})
  local v = tonumber(value) or 1
  local i = v + (dy > 0 and 1 or (dy < 0 and -1 or 0))
  i = math.max(1, math.min(n, i))
  if i == v then return nil end
  tabs.scrollTo(spec, r, i, ctx)
  return { type = "select", index = i }
end

M.tabs = tabs

---------------------------------------
-- textField (inline editing, no selection) --
---------------------------------------

local textField = {}

function textField.measure(spec, ctx) return ctx.theme.rowH end

--- A fresh edit state for `value`; the cursor starts at the end.
function textField.begin(value)
  local s = tostring(value == nil and "" or value)
  return { text = s, cursor = #s, original = s }
end

local function fieldBox(spec, r, th, hasLabel)
  local by = r.y + floor((r.h - th.boxH) / 2)
  if hasLabel then
    local bw = floor(r.w * 0.55)
    return r.x + r.w - bw, by, bw, th.boxH
  end
  return r.x, by, r.w, th.boxH
end

--- `value` is the committed string; while `st.editing` (an edit state
--- from `begin`) is set, its text and cursor are drawn instead.
function textField.paint(spec, r, value, st, ctx)
  local th = ctx.theme
  local ops = {}
  local hasLabel = st.label and st.label ~= ""
  local bx, by, bw, bh = fieldBox(spec, r, th, hasLabel)
  local ty = by + floor((bh - fontH(ctx)) / 2)
  if hasLabel then text(ops, r.x, ty, textColor(st, th, th.text), st.label) end
  local ed = st.editing
  local shown = ed and ed.text or tostring(value == nil and "" or value)
  local s = th.uiScale
  box(ops, bx, by, bw, bh, th.box, st, th)
  if ed then frame(ops, bx, by, bw, bh, th.focus) end
  local pad = 4 * s
  if ctx.text and not ed then shown = ctx.text:ellipsize(shown, bw - pad * 2) end
  if shown == "" and spec.placeholder then
    -- also while editing: an empty entry shows the value it will keep
    text(ops, bx + pad, ty, th.textDim, spec.placeholder)
  else
    text(ops, bx + pad, ty, textColor(st, th, th.textValue), shown)
  end
  if ed then
    local cx = bx + pad + textW(ctx, ed.text:sub(1, ed.cursor))
    rect(ops, cx, by + 2, 1, bh - 4, th.cursor)
  end
  return ops
end

function textField.hit(spec, r, x, y, ctx)
  if not inRect(x, y, r.x, r.y, r.w, r.h) then return nil end
  return "box"
end

function textField.down(spec, r, x, y, value, ctx)
  return { type = "openEntry" }
end

function textField.entryRect(spec, r, ctx, hasLabel)
  local bx, by, bw, bh = fieldBox(spec, r, ctx.theme, hasLabel)
  return { x = bx, y = by, w = bw, h = bh }
end

--- Applies one key to an edit state. Returns the (same, mutated) edit and
--- an action: `{ type = "commit", value = text }`, `{ type = "cancel" }`
--- or nil. Printable keys are single-character strings; the rest are
--- named ("Backspace", "Delete", "Left", "Right", "Home", "End",
--- "Enter", "Escape").
function textField.key(spec, key, ed, ctx, mods)
  if type(ed) ~= "table" then return ed, nil end -- not an edit state
  local t, c = ed.text, ed.cursor
  if key == "Enter" then
    return ed, { type = "commit", value = t }
  elseif key == "Escape" then
    ed.text, ed.cursor = ed.original, #ed.original
    return ed, { type = "cancel" }
  elseif key == "Backspace" then
    if c > 0 then
      ed.text = t:sub(1, c - 1) .. t:sub(c + 1)
      ed.cursor = c - 1
    end
  elseif key == "Delete" then
    if c < #t then ed.text = t:sub(1, c) .. t:sub(c + 2) end
  elseif key == "Left" then
    if c > 0 then ed.cursor = c - 1 end
  elseif key == "Right" then
    if c < #t then ed.cursor = c + 1 end
  elseif key == "Home" then
    ed.cursor = 0
  elseif key == "End" then
    ed.cursor = #t
  elseif key == "Space" then
    ed.text = t:sub(1, c) .. " " .. t:sub(c + 1)
    ed.cursor = c + 1
  elseif type(key) == "string" and #key == 1 and key:byte() >= 32
    and key:byte() <= 126 then
    if not spec.maxLen or #t < spec.maxLen then
      ed.text = t:sub(1, c) .. key .. t:sub(c + 1)
      ed.cursor = c + 1
    end
  end
  return ed, nil
end

M.textField = textField

---------------------------------------
-- color (swatch + hex, opens kit/ui/colorpicker.lua) --
---------------------------------------

local color = {}

function color.measure(spec, ctx) return ctx.theme.rowH end

local function colorBox(spec, r, th)
  local bw = floor(r.w * 0.55)
  local by = r.y + floor((r.h - th.boxH) / 2)
  return r.x + r.w - bw, by, bw, th.boxH
end
color.box = colorBox

--- A small checkerboard, used under any alpha < 255 swatch.
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
color.checker = checker

function color.paint(spec, r, value, st, ctx)
  local th = ctx.theme
  local ops = {}
  local bx, by, bw, bh = colorBox(spec, r, th)
  local ty = by + floor((bh - fontH(ctx)) / 2)
  if st.label and st.label ~= "" then
    text(ops, r.x, ty, textColor(st, th, th.text), st.label)
  end
  box(ops, bx, by, bw, bh, th.box, st, th)
  local v = value or { r = 255, g = 255, b = 255, a = 255 }
  local s = th.uiScale
  local sw = bh - 4 * s
  local sx, sy = bx + 2 * s, by + 2 * s
  if (v.a or 255) < 255 then checker(ops, sx, sy, sw, sw) end
  rect(ops, sx, sy, sw, sw, { v.r or 0, v.g or 0, v.b or 0 }, v.a)
  frame(ops, sx, sy, sw, sw, th.frame)
  local hex = string.format("%02X%02X%02X", v.r or 0, v.g or 0, v.b or 0)
  if (v.a or 255) < 255 then hex = hex .. string.format("%02X", v.a) end
  text(ops, sx + sw + 4 * s, ty, textColor(st, th, th.textValue), hex)
  return ops
end

function color.hit(spec, r, x, y, ctx)
  if not inRect(x, y, r.x, r.y, r.w, r.h) then return nil end
  local bx, by, bw, bh = colorBox(spec, r, ctx.theme)
  if inRect(x, y, bx, by, bw, bh) then return "box" end
  return "body"
end

function color.down(spec, r, x, y, value, ctx)
  if color.hit(spec, r, x, y, ctx) == "box" then
    return { type = "openPicker" }
  end
  return nil
end

function color.key(spec, key, value, ctx, mods)
  if key == "Enter" or key == "Space" or key == " " then
    return { type = "openPicker" }
  end
  return nil
end

--- Anchor for the picker: the swatch+hex box itself.
function color.anchorRect(spec, r, ctx)
  local bx, by, bw, bh = colorBox(spec, r, ctx.theme)
  return { x = bx, y = by, w = bw, h = bh }
end

M.color = color

---------------------------------------
-- Registry --
---------------------------------------

M.kinds = { "slider", "number", "switch", "combo", "button", "label",
  "separator", "tabs", "textField", "color" }

--- The widget for a kind, or nil.
function M.get(kind)
  local w = M[kind]
  if type(w) == "table" and w.measure then return w end
  return nil
end

--- Widgets that take keyboard focus (Tab stops).
M.focusable = { slider = true, number = true, switch = true, combo = true,
  button = true, tabs = true, textField = true, color = true }

---------------------------------------
-- widgets_extra.lua: gradientBar, list --
---------------------------------------
-- Kept in a sibling file so this one does not grow past ~900 lines; same
-- contract, folded into this registry so panel.lua and init.lua see one
-- module.

do
  local info = debug.getinfo(1, "S")
  local src = info and info.source or ""
  if src:sub(1, 1) == "@" then src = src:sub(2) end
  local dir = src:match("^(.*)[/\\]") or "."
  local chunk = loadfile(dir .. "/widgets_extra.lua")
  if chunk then chunk().extend(M) end
end

return M
