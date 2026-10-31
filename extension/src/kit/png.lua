-- png.lua — a pure-Lua PNG encoder, no zlib, no native deps.
--
-- `M.encode(buf)` with `buf = { w, h, d }` (flat RGBA, 4 bytes per pixel,
-- 1-indexed, row major — the same layout as `buffer.lua`) returns a full
-- PNG file as a binary string: 8-byte signature, IHDR (8-bit RGBA,
-- color type 6), one IDAT holding a zlib stream, IEND.
--
-- The zlib stream never actually compresses: deflate's "stored" block
-- type (BTYPE 00) just wraps the raw bytes, split into blocks of at most
-- 65535 bytes, with a 2-byte zlib header (0x78 0x01) and a trailing
-- adler32. That keeps this file small and deterministic (no Huffman
-- tables to get right) at the cost of file size — fine for headless
-- previews, not for shipping assets.
--
-- Runs on Lua 5.4 and LuaJIT alike: everything below is arithmetic, no
-- bitwise operators, no `string.pack`.

local M = {}

local floor = math.floor
local char = string.char

---------------------------------------
-- crc32 --
---------------------------------------

--- XOR of two integers in [0, 2^32), done by repeated halving since this
--- file avoids bitwise operators (Lua 5.3+ only) to stay LuaJIT-compatible.
local function xor32(a, b)
  local out, mul = 0, 1
  for _ = 1, 32 do
    local abit, bbit = a % 2, b % 2
    if abit ~= bbit then out = out + mul end
    a, b = floor(a / 2), floor(b / 2)
    mul = mul * 2
  end
  return out
end

-- The standard CRC-32 (zlib/PNG polynomial 0xEDB88320) lookup table, one
-- entry per byte value.
local crcTable = {}
for n = 0, 255 do
  local c = n
  for _ = 1, 8 do
    if c % 2 == 1 then
      c = xor32(floor(c / 2), 0xEDB88320)
    else
      c = floor(c / 2)
    end
  end
  crcTable[n] = c
end

--- CRC-32 (zlib/PNG polynomial 0xEDB88320) of a string.
function M.crc32(s)
  local crc = 0xFFFFFFFF
  for i = 1, #s do
    local b = s:byte(i)
    crc = xor32(floor(crc / 256), crcTable[xor32(crc % 256, b)])
  end
  return xor32(crc, 0xFFFFFFFF)
end

---------------------------------------
-- adler32 --
---------------------------------------

local ADLER_MOD = 65521

local function adler32(s)
  local a, b = 1, 0
  for i = 1, #s do
    a = (a + s:byte(i)) % ADLER_MOD
    b = (b + a) % ADLER_MOD
  end
  return b * 65536 + a
end

---------------------------------------
-- big/little-endian helpers --
---------------------------------------

local function u32be(v)
  v = v % 4294967296
  return char(
    floor(v / 16777216) % 256,
    floor(v / 65536) % 256,
    floor(v / 256) % 256,
    v % 256
  )
end

local function u16le(v)
  return char(v % 256, floor(v / 256) % 256)
end

---------------------------------------
-- PNG chunks --
---------------------------------------

local function chunk(kind, data)
  data = data or ""
  return u32be(#data) .. kind .. data .. u32be(M.crc32(kind .. data))
end

---------------------------------------
-- zlib stored-block deflate --
---------------------------------------

--- Wraps `raw` (uncompressed bytes) in a minimal zlib stream: header
--- 0x78 0x01, one or more deflate "stored" blocks (BTYPE 00, so each is
--- byte-aligned already: the 3-bit block header lands alone in its own
--- byte since no bits are pending), trailing adler32 of `raw`.
local function zlibStored(raw)
  local parts = { char(0x78, 0x01) }
  local n = #raw
  local pos = 1
  if n == 0 then
    parts[#parts + 1] = char(0x01) .. u16le(0) .. u16le(0xFFFF)
  end
  while pos <= n do
    local last = math.min(pos + 65534, n)
    local len = last - pos + 1
    local isFinal = last == n
    parts[#parts + 1] = char(isFinal and 1 or 0)
    parts[#parts + 1] = u16le(len)
    parts[#parts + 1] = u16le((65535 - len))
    parts[#parts + 1] = raw:sub(pos, last)
    pos = last + 1
  end
  parts[#parts + 1] = u32be(adler32(raw))
  return table.concat(parts)
end

---------------------------------------
-- Encoder --
---------------------------------------

--- `buf = { w, h, d }`, flat RGBA, 1-indexed, row major (`buffer.lua`'s
--- layout). Returns the PNG file as a binary string.
function M.encode(buf)
  local w, h, d = buf.w, buf.h, buf.d
  assert(w and h and d, "png.encode: buf needs w, h, d")

  -- filtered scanlines: filter type 0 (None) + w*4 raw bytes, per row.
  local rows = {}
  for y = 0, h - 1 do
    local parts = { char(0) }
    local base = y * w * 4
    local rowBytes = {}
    for x = 0, w - 1 do
      local i = base + x * 4 + 1
      rowBytes[#rowBytes + 1] = char(d[i], d[i + 1], d[i + 2], d[i + 3])
    end
    parts[#parts + 1] = table.concat(rowBytes)
    rows[#rows + 1] = table.concat(parts)
  end
  local raw = table.concat(rows)

  local ihdr = u32be(w) .. u32be(h) .. char(8, 6, 0, 0, 0)
  local idat = zlibStored(raw)

  local sig = char(137, 80, 78, 71, 13, 10, 26, 10)
  return sig .. chunk("IHDR", ihdr) .. chunk("IDAT", idat) .. chunk("IEND", "")
end

return M
