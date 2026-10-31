-- painter_buf.lua — rasterizes the SAME display list painter_ase.lua
-- sends to a Dialog canvas, but onto a kit/buffer.lua RGBA buffer. It is
-- the piece that makes PNG goldens of a canvas UI possible without
-- Aseprite: `M.paint(buf, ops, opts)` and then `Buffer.hash(buf)` /
-- `png.encode(buf)`.
--
-- Ops (see painter_ase.lua): rect / frame (optional 9th field alpha
-- 0..255), line (Bresenham, stroke width 1; sw > 1 stamps a sw x sw
-- square per pixel), text (embedded 5x7 bitmap font, ASCII 32..126, 1px
-- advance gap, integer `opts.scale`), image (nearest-neighbour copy),
-- drawimage (a host image: when `img` is a kit buffer (`img.w`/`img.d`)
-- the sub-rect is copied nearest-neighbour, otherwise a grey placeholder
-- rect stands in, so goldens of a window that shows an Aseprite `Image`
-- still run headless), save / restore / clip (a stack of intersected
-- rects).
--
-- The font is Spaghetti's self-made font_glyphs.lua (5 columns, 7 rows,
-- 2 more for descenders, advance 6). `M.measure(str, scale)` returns
-- `#str * 6 * scale, 8 * scale` -- what kit/test/stub.lua's
-- `measureText` answers too, so the same layout comes out of a test and
-- of this rasterizer.
--
-- It is NOT a pixel-exact copy of Aseprite's renderer: fonts, stroke
-- joins and text baselines differ. Goldens pin THIS painter's output;
-- how it looks in Aseprite is checked with eyes once.

local M = {}

local floor = math.floor

---------------------------------------
-- Font --
---------------------------------------

M.FONT_W = 5
M.FONT_H = 7      -- rows a baseline glyph uses
M.FONT_CELL = 9   -- rows including descenders
M.ADVANCE = 6
M.LINE_H = 8      -- what measure() reports as height

local GLYPHS = {}

local function G(ch, ...)
  GLYPHS[ch] = { ... }
end

G(" ", "00000")
G("!", "00100", "00100", "00100", "00100", "00100", "00000", "00100")
G('"', "01010", "01010", "01010")
G("#", "01010", "01010", "11111", "01010", "11111", "01010", "01010")
G("$", "00100", "01111", "10100", "01110", "00101", "11110", "00100")
G("%", "11001", "11010", "00010", "00100", "01000", "01011", "10011")
G("&", "01100", "10010", "10100", "01000", "10101", "10010", "01101")
G("'", "00100", "00100")
G("(", "00010", "00100", "01000", "01000", "01000", "00100", "00010")
G(")", "01000", "00100", "00010", "00010", "00010", "00100", "01000")
G("*", "00000", "00100", "10101", "01110", "10101", "00100", "00000")
G("+", "00000", "00100", "00100", "11111", "00100", "00100", "00000")
G(",", "00000", "00000", "00000", "00000", "00000", "00000", "00110",
  "00010", "00100")
G("-", "00000", "00000", "00000", "11111", "00000", "00000", "00000")
G(".", "00000", "00000", "00000", "00000", "00000", "00110", "00110")
G("/", "00001", "00001", "00010", "00100", "01000", "10000", "10000")
G("0", "01110", "10001", "10011", "10101", "11001", "10001", "01110")
G("1", "00100", "01100", "00100", "00100", "00100", "00100", "01110")
G("2", "01110", "10001", "00001", "00010", "00100", "01000", "11111")
G("3", "11111", "00010", "00100", "00010", "00001", "10001", "01110")
G("4", "00010", "00110", "01010", "10010", "11111", "00010", "00010")
G("5", "11111", "10000", "11110", "00001", "00001", "10001", "01110")
G("6", "00110", "01000", "10000", "11110", "10001", "10001", "01110")
G("7", "11111", "00001", "00010", "00100", "01000", "01000", "01000")
G("8", "01110", "10001", "10001", "01110", "10001", "10001", "01110")
G("9", "01110", "10001", "10001", "01111", "00001", "00010", "01100")
G(":", "00000", "00110", "00110", "00000", "00110", "00110", "00000")
G(";", "00000", "00110", "00110", "00000", "00110", "00100", "01000")
G("<", "00010", "00100", "01000", "10000", "01000", "00100", "00010")
G("=", "00000", "00000", "11111", "00000", "11111", "00000", "00000")
G(">", "01000", "00100", "00010", "00001", "00010", "00100", "01000")
G("?", "01110", "10001", "00001", "00010", "00100", "00000", "00100")
G("@", "01110", "10001", "10111", "10101", "10111", "10000", "01110")
G("A", "01110", "10001", "10001", "11111", "10001", "10001", "10001")
G("B", "11110", "10001", "10001", "11110", "10001", "10001", "11110")
G("C", "01110", "10001", "10000", "10000", "10000", "10001", "01110")
G("D", "11100", "10010", "10001", "10001", "10001", "10010", "11100")
G("E", "11111", "10000", "10000", "11110", "10000", "10000", "11111")
G("F", "11111", "10000", "10000", "11110", "10000", "10000", "10000")
G("G", "01110", "10001", "10000", "10111", "10001", "10001", "01111")
G("H", "10001", "10001", "10001", "11111", "10001", "10001", "10001")
G("I", "01110", "00100", "00100", "00100", "00100", "00100", "01110")
G("J", "00111", "00010", "00010", "00010", "00010", "10010", "01100")
G("K", "10001", "10010", "10100", "11000", "10100", "10010", "10001")
G("L", "10000", "10000", "10000", "10000", "10000", "10000", "11111")
G("M", "10001", "11011", "10101", "10101", "10001", "10001", "10001")
G("N", "10001", "10001", "11001", "10101", "10011", "10001", "10001")
G("O", "01110", "10001", "10001", "10001", "10001", "10001", "01110")
G("P", "11110", "10001", "10001", "11110", "10000", "10000", "10000")
G("Q", "01110", "10001", "10001", "10001", "10101", "10010", "01101")
G("R", "11110", "10001", "10001", "11110", "10100", "10010", "10001")
G("S", "01111", "10000", "10000", "01110", "00001", "00001", "11110")
G("T", "11111", "00100", "00100", "00100", "00100", "00100", "00100")
G("U", "10001", "10001", "10001", "10001", "10001", "10001", "01110")
G("V", "10001", "10001", "10001", "10001", "10001", "01010", "00100")
G("W", "10001", "10001", "10001", "10101", "10101", "10101", "01010")
G("X", "10001", "10001", "01010", "00100", "01010", "10001", "10001")
G("Y", "10001", "10001", "01010", "00100", "00100", "00100", "00100")
G("Z", "11111", "00001", "00010", "00100", "01000", "10000", "11111")
G("[", "01110", "01000", "01000", "01000", "01000", "01000", "01110")
G("\\", "10000", "10000", "01000", "00100", "00010", "00001", "00001")
G("]", "01110", "00010", "00010", "00010", "00010", "00010", "01110")
G("^", "00100", "01010", "10001", "00000", "00000", "00000", "00000")
G("_", "00000", "00000", "00000", "00000", "00000", "00000", "11111")
G("`", "01000", "00100")
G("a", "00000", "00000", "01110", "00001", "01111", "10001", "01111")
G("b", "10000", "10000", "11110", "10001", "10001", "10001", "11110")
G("c", "00000", "00000", "01110", "10001", "10000", "10001", "01110")
G("d", "00001", "00001", "01111", "10001", "10001", "10001", "01111")
G("e", "00000", "00000", "01110", "10001", "11111", "10000", "01110")
G("f", "00110", "01001", "01000", "11100", "01000", "01000", "01000")
G("g", "00000", "00000", "01111", "10001", "10001", "10001", "01111",
  "00001", "01110")
G("h", "10000", "10000", "11110", "10001", "10001", "10001", "10001")
G("i", "00100", "00000", "01100", "00100", "00100", "00100", "01110")
G("j", "00010", "00000", "00110", "00010", "00010", "00010", "00010",
  "10010", "01100")
G("k", "10000", "10000", "10010", "10100", "11000", "10100", "10010")
G("l", "01100", "00100", "00100", "00100", "00100", "00100", "01110")
G("m", "00000", "00000", "11010", "10101", "10101", "10101", "10101")
G("n", "00000", "00000", "11110", "10001", "10001", "10001", "10001")
G("o", "00000", "00000", "01110", "10001", "10001", "10001", "01110")
G("p", "00000", "00000", "11110", "10001", "10001", "10001", "11110",
  "10000", "10000")
G("q", "00000", "00000", "01111", "10001", "10001", "10001", "01111",
  "00001", "00001")
G("r", "00000", "00000", "10110", "11001", "10000", "10000", "10000")
G("s", "00000", "00000", "01111", "10000", "01110", "00001", "11110")
G("t", "01000", "01000", "11100", "01000", "01000", "01001", "00110")
G("u", "00000", "00000", "10001", "10001", "10001", "10011", "01101")
G("v", "00000", "00000", "10001", "10001", "10001", "01010", "00100")
G("w", "00000", "00000", "10001", "10101", "10101", "10101", "01010")
G("x", "00000", "00000", "10001", "01010", "00100", "01010", "10001")
G("y", "00000", "00000", "10001", "10001", "10001", "10001", "01111",
  "00001", "01110")
G("z", "00000", "00000", "11111", "00010", "00100", "01000", "11111")
G("{", "00110", "00100", "00100", "01000", "00100", "00100", "00110")
G("|", "00100", "00100", "00100", "00100", "00100", "00100", "00100")
G("}", "01100", "00100", "00100", "00010", "00100", "00100", "01100")
G("~", "00000", "00000", "01000", "10101", "00010", "00000", "00000")

M.glyphs = GLYPHS

--- Width and height of `str` at integer `scale` (default 1).
function M.measure(str, scale)
  scale = scale or 1
  return #tostring(str) * M.ADVANCE * scale, M.LINE_H * scale
end

--- A `measure(str) -> w, h` closure for kit/ui/text.lua.
function M.measurer(scale)
  return function(str) return M.measure(str, scale) end
end

---------------------------------------
-- Pixel writer with clip + alpha --
---------------------------------------

local function newState(buf, scale)
  return {
    buf = buf, d = buf.d, w = buf.w, h = buf.h,
    clip = { x = 0, y = 0, w = buf.w, h = buf.h },
    stack = {},
    scale = scale or 1,
  }
end

local function put(S, x, y, r, g, b, a)
  local c = S.clip
  if x < c.x or y < c.y or x >= c.x + c.w or y >= c.y + c.h then return end
  if x < 0 or y < 0 or x >= S.w or y >= S.h then return end
  local i = (y * S.w + x) * 4 + 1
  local d = S.d
  if a == nil or a >= 255 then
    d[i], d[i + 1], d[i + 2], d[i + 3] = r, g, b, 255
    return
  end
  if a <= 0 then return end
  local t = a / 255
  local u = 1 - t
  d[i] = floor(r * t + d[i] * u + 0.5)
  d[i + 1] = floor(g * t + d[i + 1] * u + 0.5)
  d[i + 2] = floor(b * t + d[i + 2] * u + 0.5)
  local da = d[i + 3]
  d[i + 3] = floor(a + da * u + 0.5)
end

local function fillRect(S, x, y, w, h, r, g, b, a)
  x, y, w, h = floor(x), floor(y), floor(w), floor(h)
  if w <= 0 or h <= 0 then return end
  local c = S.clip
  local x0 = x > c.x and x or c.x
  local y0 = y > c.y and y or c.y
  local x1 = (x + w < c.x + c.w) and (x + w) or (c.x + c.w)
  local y1 = (y + h < c.y + c.h) and (y + h) or (c.y + c.h)
  if x0 < 0 then x0 = 0 end
  if y0 < 0 then y0 = 0 end
  if x1 > S.w then x1 = S.w end
  if y1 > S.h then y1 = S.h end
  for py = y0, y1 - 1 do
    for px = x0, x1 - 1 do put(S, px, py, r, g, b, a) end
  end
end

local function strokeRect(S, x, y, w, h, r, g, b, a)
  x, y, w, h = floor(x), floor(y), floor(w), floor(h)
  if w <= 0 or h <= 0 then return end
  fillRect(S, x, y, w, 1, r, g, b, a)
  if h > 1 then fillRect(S, x, y + h - 1, w, 1, r, g, b, a) end
  if h > 2 then
    fillRect(S, x, y + 1, 1, h - 2, r, g, b, a)
    if w > 1 then fillRect(S, x + w - 1, y + 1, 1, h - 2, r, g, b, a) end
  end
end

local function line(S, x1, y1, x2, y2, sw, r, g, b, a)
  x1, y1, x2, y2 = floor(x1), floor(y1), floor(x2), floor(y2)
  sw = floor(sw or 1)
  if sw < 1 then sw = 1 end
  local half = floor((sw - 1) / 2)
  local dx = x2 > x1 and x2 - x1 or x1 - x2
  local dy = y2 > y1 and y2 - y1 or y1 - y2
  local sx = x1 < x2 and 1 or -1
  local sy = y1 < y2 and 1 or -1
  local err = dx - dy
  local x, y = x1, y1
  local guard = dx + dy + 2
  while guard > 0 do
    guard = guard - 1
    if sw == 1 then
      put(S, x, y, r, g, b, a)
    else
      fillRect(S, x - half, y - half, sw, sw, r, g, b, a)
    end
    if x == x2 and y == y2 then break end
    local e2 = err * 2
    if e2 > -dy then
      err = err - dy
      x = x + sx
    end
    if e2 < dx then
      err = err + dx
      y = y + sy
    end
  end
end

local function drawText(S, x, y, r, g, b, str)
  x, y = floor(x), floor(y)
  local s = S.scale
  str = tostring(str)
  for ci = 1, #str do
    local rows = GLYPHS[str:sub(ci, ci)]
    if rows then
      local gx = x + (ci - 1) * M.ADVANCE * s
      for ry = 1, #rows do
        local row = rows[ry]
        for rx = 1, #row do
          if row:sub(rx, rx) == "1" then
            if s == 1 then
              put(S, gx + rx - 1, y + ry - 1, r, g, b)
            else
              fillRect(S, gx + (rx - 1) * s, y + (ry - 1) * s, s, s, r, g, b)
            end
          end
        end
      end
    end
  end
end

local function drawImage(S, x, y, w, h, src)
  x, y, w, h = floor(x), floor(y), floor(w), floor(h)
  if w <= 0 or h <= 0 or not src or src.w <= 0 or src.h <= 0 then return end
  local sd = src.d
  for py = 0, h - 1 do
    local sy = floor(py * src.h / h)
    for px = 0, w - 1 do
      local sx = floor(px * src.w / w)
      local i = (sy * src.w + sx) * 4 + 1
      put(S, x + px, y + py, sd[i], sd[i + 1], sd[i + 2], sd[i + 3])
    end
  end
end

--- `drawimage` with a kit buffer `src`: copies `sx,sy,sw,sh` of it to
--- `dx,dy,dw,dh` nearest-neighbour. A non-buffer (opaque host image) is
--- drawn as a mid-grey rect of the destination size.
local function drawImageSub(S, src, sx, sy, sw, sh, dx, dy, dw, dh)
  dx, dy, dw, dh = floor(dx), floor(dy), floor(dw), floor(dh)
  if dw <= 0 or dh <= 0 then return end
  if type(src) ~= "table" or not src.w or not src.d then
    fillRect(S, dx, dy, dw, dh, 128, 128, 128, 255)
    return
  end
  sx, sy, sw, sh = floor(sx), floor(sy), floor(sw), floor(sh)
  if sw <= 0 or sh <= 0 then return end
  local sd = src.d
  for py = 0, dh - 1 do
    local yy = sy + floor(py * sh / dh)
    if yy >= 0 and yy < src.h then
      for px = 0, dw - 1 do
        local xx = sx + floor(px * sw / dw)
        if xx >= 0 and xx < src.w then
          local i = (yy * src.w + xx) * 4 + 1
          put(S, dx + px, dy + py, sd[i], sd[i + 1], sd[i + 2], sd[i + 3])
        end
      end
    end
  end
end

local function intersect(a, x, y, w, h)
  local x0 = x > a.x and x or a.x
  local y0 = y > a.y and y or a.y
  local ax1, ay1 = a.x + a.w, a.y + a.h
  local x1 = (x + w < ax1) and (x + w) or ax1
  local y1 = (y + h < ay1) and (y + h) or ay1
  if x1 < x0 then x1 = x0 end
  if y1 < y0 then y1 = y0 end
  return { x = x0, y = y0, w = x1 - x0, h = y1 - y0 }
end

---------------------------------------
-- Entry --
---------------------------------------

--- Rasterizes `ops` onto `buf` in place. `opts.scale` is the integer
--- text scale (default 1). Returns `buf`.
function M.paint(buf, ops, opts)
  opts = opts or {}
  local S = newState(buf, opts.scale)
  for i = 1, #ops do
    local o = ops[i]
    local k = o[1]
    if k == "rect" then
      fillRect(S, o[2], o[3], o[4], o[5], o[6], o[7], o[8], o[9])
    elseif k == "frame" then
      strokeRect(S, o[2], o[3], o[4], o[5], o[6], o[7], o[8], o[9])
    elseif k == "line" then
      for j = 6, #o - 3, 2 do
        line(S, o[j], o[j + 1], o[j + 2], o[j + 3], o[5], o[2], o[3], o[4])
      end
    elseif k == "text" then
      drawText(S, o[2], o[3], o[4], o[5], o[6], o[7])
    elseif k == "image" then
      drawImage(S, o[2], o[3], o[4], o[5], o[6])
    elseif k == "drawimage" then
      drawImageSub(S, o[2], o[3], o[4], o[5], o[6], o[7], o[8], o[9], o[10])
    elseif k == "save" then
      S.stack[#S.stack + 1] = S.clip
    elseif k == "restore" then
      local n = #S.stack
      if n > 0 then
        S.clip = S.stack[n]
        S.stack[n] = nil
      end
    elseif k == "clip" then
      S.clip = intersect(S.clip, floor(o[2]), floor(o[3]), floor(o[4]), floor(o[5]))
    end
  end
  return buf
end

return M
