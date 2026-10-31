-- serial.lua — a deterministic Lua serializer and its sandboxed reader.
--
-- Pure Lua. Part of AseKit's shared `kit/` library: any document (a
-- VeSprite drawing, a Spaghetti graph, ...) is written as a Lua table
-- literal (`return { ... }`), which is what a save file and a plugin's
-- preferences both hold. Two rules that make a save reproducible byte for
-- byte:
--
--   * arrays (1..n without holes) are written by index, everything else
--     by SORTED key -- `pairs` order is the one thing Lua 5.4 and LuaJIT
--     do not agree on;
--   * numbers that are integers print as integers, the rest with %.17g,
--     so a value survives a round trip unchanged.
--
-- Reading goes through `load` with an EMPTY environment: a document is
-- data and must not be able to call anything.

local M = {}

local floor = math.floor

local function isIdent(k)
  return type(k) == "string" and k:match("^[%a_][%w_]*$") ~= nil
end

local function num(v)
  if v ~= v then return "0" end -- NaN never enters a file
  if v == math.huge then return "1e308" end
  if v == -math.huge then return "-1e308" end
  if v == floor(v) and math.abs(v) < 2^53 then
    return string.format("%d", v)
  end
  return string.format("%.17g", v)
end

local function ser(v, out, depth)
  local t = type(v)
  if t == "number" then
    out[#out + 1] = num(v)
  elseif t == "string" then
    out[#out + 1] = string.format("%q", v)
  elseif t == "boolean" then
    out[#out + 1] = v and "true" or "false"
  elseif t == "table" then
    if depth > 64 then out[#out + 1] = "nil" return end
    local n = #v
    local isArray = n > 0
    if isArray then
      -- a table is an array only when it has NO other keys
      for k in pairs(v) do
        if type(k) ~= "number" or k < 1 or k > n or k ~= floor(k) then
          isArray = false
          break
        end
      end
    end
    out[#out + 1] = "{"
    if isArray then
      for i = 1, n do
        if i > 1 then out[#out + 1] = "," end
        ser(v[i], out, depth + 1)
      end
    else
      local keys = {}
      for k in pairs(v) do
        local kt = type(k)
        if kt == "string" or kt == "number" then keys[#keys + 1] = k end
      end
      table.sort(keys, function(a, b)
        local ta, tb = type(a), type(b)
        if ta ~= tb then return ta < tb end
        return a < b
      end)
      for i = 1, #keys do
        local k = keys[i]
        if i > 1 then out[#out + 1] = "," end
        if isIdent(k) then
          out[#out + 1] = k .. "="
        elseif type(k) == "number" then
          out[#out + 1] = "[" .. num(k) .. "]="
        else
          out[#out + 1] = "[" .. string.format("%q", k) .. "]="
        end
        ser(v[k], out, depth + 1)
      end
    end
    out[#out + 1] = "}"
  else
    out[#out + 1] = "nil"
  end
end

--- A value as a Lua expression.
function M.encode(v)
  local out = {}
  ser(v, out, 0)
  return table.concat(out)
end

--- A whole file: `return <expr>`.
function M.encodeFile(v)
  return "return " .. M.encode(v) .. "\n"
end

--- The value back, or nil + why. Accepts both a bare expression and a
--- `return ...` file. Sandboxed: no globals at all.
function M.decode(text)
  if type(text) ~= "string" then return nil, "not a string" end
  local src = text
  if not src:match("^%s*return") then src = "return " .. src end
  local chunk, err
  if setfenv then
    -- LuaJIT / 5.1
    chunk, err = loadstring(src, "=asekit")
    if chunk then setfenv(chunk, {}) end
  else
    chunk, err = load(src, "=asekit", "t", {})
  end
  if not chunk then return nil, tostring(err) end
  local ok, v = pcall(chunk)
  if not ok then return nil, tostring(v) end
  return v
end

return M
