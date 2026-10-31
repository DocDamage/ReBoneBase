-- json.lua — a JSON decoder for the glTF parser. Pure Lua, dual-VM (no utf8
-- library, no string.pack). Strings, numbers and whitespace are taken with
-- string.find, never char by char, so a few MB decode quickly.
--
-- Objects become tables, arrays become tables 1..n, `null` becomes nil (an
-- object key with null is absent, an array slot with null is a hole).

local M = {}

M.MAX_DEPTH = 200

local byte, sub, find, char = string.byte, string.sub, string.find, string.char
local floor, concat = math.floor, table.concat

local ESC = { ['"'] = '"', ["\\"] = "\\", ["/"] = "/", b = "\b", f = "\f", n = "\n", r = "\r", t = "\t" }

local function fail(msg, i) error(("%s at %d"):format(msg, i), 0) end

--- UTF-8 bytes of a code point.
local function utf8char(cp)
  if cp < 0x80 then return char(cp) end
  if cp < 0x800 then return char(0xC0 + floor(cp / 64), 0x80 + cp % 64) end
  if cp < 0x10000 then
    return char(0xE0 + floor(cp / 4096), 0x80 + floor(cp / 64) % 64, 0x80 + cp % 64)
  end
  return char(0xF0 + floor(cp / 262144), 0x80 + floor(cp / 4096) % 64,
    0x80 + floor(cp / 64) % 64, 0x80 + cp % 64)
end

-- The string whose opening quote is at i; returns value, next index.
local function parseString(s, i)
  local start = i + 1
  local j = find(s, '["\\]', start)
  if not j then fail("unterminated string", i) end
  if byte(s, j) == 34 then return sub(s, start, j - 1), j + 1 end -- no escapes
  local parts, n = {}, 0
  while true do
    if not j then fail("unterminated string", i) end
    n = n + 1
    parts[n] = sub(s, start, j - 1)
    if byte(s, j) == 34 then return concat(parts), j + 1 end
    local e = sub(s, j + 1, j + 1)
    if e == "u" then
      local hex = s:match("^%x%x%x%x", j + 2)
      if not hex then fail("bad \\u escape", j) end
      local cp = tonumber(hex, 16)
      j = j + 6
      if cp >= 0xD800 and cp < 0xDC00 then -- high surrogate: needs its low half
        local lo = s:match("^\\u(%x%x%x%x)", j)
        local lcp = lo and tonumber(lo, 16)
        if lcp and lcp >= 0xDC00 and lcp < 0xE000 then
          cp = 0x10000 + (cp - 0xD800) * 1024 + (lcp - 0xDC00)
          j = j + 6
        else
          cp = 0xFFFD
        end
      elseif cp >= 0xDC00 and cp < 0xE000 then
        cp = 0xFFFD
      end
      n = n + 1
      parts[n] = utf8char(cp)
    else
      local r = ESC[e]
      if not r then fail("bad escape", j) end
      n = n + 1
      parts[n] = r
      j = j + 2
    end
    start = j
    j = find(s, '["\\]', start)
  end
end

local parseValue

local function skip(s, i) return find(s, "[^ \t\r\n]", i) or #s + 1 end

local function parseNumber(s, i)
  local _, e = find(s, "^-?%d+", i)
  if not e then fail("bad number", i) end
  local _, e2 = find(s, "^%.%d+", e + 1)
  e = e2 or e
  local _, e3 = find(s, "^[eE][+-]?%d+", e + 1)
  e = e3 or e
  return tonumber(sub(s, i, e)), e + 1
end

function parseValue(s, i, depth)
  i = skip(s, i)
  local c = byte(s, i)
  if c == 34 then return parseString(s, i) end
  if c == 123 or c == 91 then -- { [
    if depth >= M.MAX_DEPTH then fail("nesting too deep", i) end
    local isObj = c == 123
    local t = {}
    i = skip(s, i + 1)
    local close = isObj and 125 or 93
    if byte(s, i) == close then return t, i + 1 end
    local n = 0
    while true do
      if isObj then
        i = skip(s, i)
        if byte(s, i) ~= 34 then fail("expected a key", i) end
        local key
        key, i = parseString(s, i)
        i = skip(s, i)
        if byte(s, i) ~= 58 then fail("expected ':'", i) end
        local v
        v, i = parseValue(s, i + 1, depth + 1)
        t[key] = v
      else
        local v
        v, i = parseValue(s, i, depth + 1)
        n = n + 1
        t[n] = v
      end
      i = skip(s, i)
      local d = byte(s, i)
      if d == close then return t, i + 1 end
      if d ~= 44 then fail("expected ',' or closing bracket", i) end
      i = i + 1
    end
  end
  if c == 45 or (c and c >= 48 and c <= 57) then return parseNumber(s, i) end
  if sub(s, i, i + 3) == "true" then return true, i + 4 end
  if sub(s, i, i + 4) == "false" then return false, i + 5 end
  if sub(s, i, i + 3) == "null" then return nil, i + 4 end
  fail(c and "unexpected character" or "unexpected end", i)
end

--- Decodes a JSON text: value (nil for `null`), or nil + a short message.
function M.decode(str)
  if type(str) ~= "string" then return nil, "not a string" end
  local ok, v, i = pcall(function()
    local s = str
    if sub(s, 1, 3) == "\239\187\191" then s = sub(s, 4) end -- BOM
    local val, nxt = parseValue(s, 1, 0)
    if skip(s, nxt) <= #s then fail("trailing characters", skip(s, nxt)) end
    return val, nxt
  end)
  if not ok then return nil, tostring(v) end
  return v
end

return M
