-- buffer.lua — a flat RGBA buffer, `{ w, h, d }`, and the only bridge to
-- Aseprite's `Image`.
--
-- `d` is a flat array of bytes, 4 per pixel (r,g,b,a), 1-indexed, row
-- major: pixel (x,y) starts at `(y*w + x)*4 + 1`. Every function here but
-- `toImage`/`fromImage` is pure Lua, no host, and runs the same under
-- Lua 5.4 and LuaJIT.
--
-- `toImage`/`fromImage` resolve `Image`, `ImageSpec` and `ColorMode` as
-- GLOBALS at call time (not at load time), so this module still `dofile`s
-- headless: a test that never calls them never needs Aseprite.
--
-- `hash` is the golden-hash formula from Spaghetti's tests
-- (scripts/tests/test_*.lua): FNV-ish accumulator, `HASH_MUL = 1000003`,
-- `HASH_MOD = 2^32 - 5`, folded over every byte of `d` in order. Same
-- inputs, same VM or not, same number -- what a golden test pins.

local M = {}

local floor = math.floor
local char = string.char

local HASH_MUL, HASH_MOD = 1000003, 4294967291

---------------------------------------
-- Construction --
---------------------------------------

--- A new buffer, filled with (r,g,b,a) or transparent black.
function M.new(w, h, r, g, b, a)
  w, h = floor(w), floor(h)
  local buf = { w = w, h = h, d = {} }
  M.fill(buf, r or 0, g or 0, b or 0, a or 0)
  return buf
end

--- Every pixel to (r,g,b,a).
function M.fill(buf, r, g, b, a)
  local d = buf.d
  local n = buf.w * buf.h
  local i = 1
  for _ = 1, n do
    d[i], d[i + 1], d[i + 2], d[i + 3] = r, g, b, a
    i = i + 4
  end
  return buf
end

--- A deep copy: a new `d` table, same size.
function M.copy(buf)
  local out = { w = buf.w, h = buf.h, d = {} }
  local src, dst = buf.d, out.d
  for i = 1, buf.w * buf.h * 4 do dst[i] = src[i] end
  return out
end

---------------------------------------
-- Pixels --
---------------------------------------

--- r, g, b, a at (x, y), 0-based, or nil outside the buffer.
function M.get(buf, x, y)
  if x < 0 or y < 0 or x >= buf.w or y >= buf.h then return nil end
  local i = (y * buf.w + x) * 4 + 1
  local d = buf.d
  return d[i], d[i + 1], d[i + 2], d[i + 3]
end

--- Sets the pixel at (x, y), 0-based. No-op outside the buffer.
function M.set(buf, x, y, r, g, b, a)
  if x < 0 or y < 0 or x >= buf.w or y >= buf.h then return end
  local i = (y * buf.w + x) * 4 + 1
  local d = buf.d
  d[i], d[i + 1], d[i + 2], d[i + 3] = r, g, b, a
end

---------------------------------------
-- Golden hash --
---------------------------------------

--- Deterministic hash of `buf.d`, the one every golden test in Spaghetti
--- uses: seeded like FNV-1a's offset basis, folded with HASH_MUL, reduced
--- mod HASH_MOD on every step so it never overflows a Lua number.
function M.hash(buf)
  local d = buf.d
  local n = buf.w * buf.h * 4
  local h = 2166136261 % HASH_MOD
  for i = 1, n do h = (h * HASH_MUL + d[i]) % HASH_MOD end
  return h
end

--- The same formula over a byte string (an `Image.bytes` in Aseprite, the
--- stub's packed pixels headless): what the preset-hash smoke and
--- test_presets_hash.lua compare across the two worlds.
function M.hashString(s)
  local n = #s
  local h = 2166136261 % HASH_MOD
  local byte = string.byte
  for i = 1, n, 64 do
    local j = i + 63
    if j > n then j = n end
    local t = { byte(s, i, j) }
    for k = 1, #t do h = (h * HASH_MUL + t[k]) % HASH_MOD end
  end
  return h
end

---------------------------------------
-- Aseprite bridge --
---------------------------------------

--- Flat RGBA buffer -> RGB `Image`. `Image`/`ImageSpec`/`ColorMode` are
--- read as globals here, not above, so this file loads headless.
function M.toImage(buf)
  local w, h = buf.w, buf.h
  local img = Image(ImageSpec{ width = w, height = h,
    colorMode = ColorMode.RGB })
  local d = buf.d
  local parts = {}
  local k = 0
  for i = 1, w * h * 4, 4 do
    k = k + 1
    parts[k] = char(d[i], d[i + 1], d[i + 2], d[i + 3])
  end
  img.bytes = table.concat(parts)
  return img
end

--- RGB `Image` -> flat RGBA buffer.
function M.fromImage(img)
  local w, h = img.width, img.height
  local buf = { w = w, h = h, d = {} }
  local d = buf.d
  local bytes = img.bytes
  local n = w * h * 4
  local pos = 1
  while pos <= n do
    local last = pos + 511
    if last > n then last = n end
    local vals = { bytes:byte(pos, last) }
    for k = 1, #vals do d[pos + k - 1] = vals[k] end
    pos = last + 1
  end
  return buf
end

return M
