-- gltf.lua — glTF 2.0 (.glb binary or .gltf JSON text) to the common
-- skeleton (skel3d.lua). Pure Lua, dual-VM (LuaJIT has no string.unpack, so
-- float32 falls back to a manual IEEE-754 decoder).
--
-- Joints = skins[1].joints (reordered so parents come first); with no skin,
-- every mesh-less node of the default scene. A joint's parent is its nearest
-- ancestor that is also a joint. World matrices include the NON-joint
-- ancestors (Blender's "Armature" node with rotation and 0.01 scale). The
-- coordinates stay as they are (glTF is Y-up).
--
-- Each animation is a clip: translation / rotation / scale channels are
-- sampled (LINEAR with slerp for rotations, STEP, CUBICSPLINE as a Hermite
-- spline) at opts.fps (default 30) over the full node tree; nodes nobody
-- animates keep their default TRS. `weights` channels are ignored. A node
-- with a `matrix` that gets animated is treated as identity + the animation.
-- Sparse accessors are not supported. No end joints are invented.

local M = {}

M.Skel, M.M3d, M.Json = nil, nil, nil

function M.bind(Skel, M3d, Json)
  M.Skel, M.M3d, M.Json = Skel, M3d, Json
  return M
end

M.MAX_FRAMES = 2000
M.MAX_JOINTS = 512

local byte, sub, find, char = string.byte, string.sub, string.find, string.char
local floor, ceil, max, min = math.floor, math.ceil, math.max, math.min

---------------------------------------
-- Bytes
---------------------------------------

local B64 = {}
do
  local abc = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
  for i = 1, #abc do B64[byte(abc, i)] = i - 1 end
  B64[byte("-")], B64[byte("_")] = 62, 63 -- url-safe variant
end

local function b64decode(s)
  s = s:gsub("[^%w%+/%-_]", "")
  local out, n = {}, 0
  for i = 1, #s, 4 do
    local a, b, c, d = byte(s, i, i + 3)
    a, b, c, d = B64[a], B64[b], c and B64[c], d and B64[d]
    if not (a and b) then break end
    local v = a * 262144 + b * 4096 + (c or 0) * 64 + (d or 0)
    n = n + 1
    if d then out[n] = char(floor(v / 65536), floor(v / 256) % 256, v % 256)
    elseif c then out[n] = char(floor(v / 65536), floor(v / 256) % 256)
    else out[n] = char(floor(v / 65536)) end
  end
  return table.concat(out)
end

local function u32(s, p)
  local a, b, c, d = byte(s, p, p + 3)
  if not d then error("file truncated", 0) end
  return a + b * 256 + c * 65536 + d * 16777216
end

local unpack = string.unpack
local function f32(s, p)
  if unpack then return (unpack("<f", s, p)) end
  local b1, b2, b3, b4 = byte(s, p, p + 3)
  local e = (b4 % 128) * 2 + floor(b3 / 128)
  local m = (b3 % 128) * 65536 + b2 * 256 + b1
  local v
  if e == 0 then v = m * 2 ^ -149
  elseif e == 255 then v = m == 0 and math.huge or 0
  else v = (1 + m / 8388608) * 2 ^ (e - 127) end
  return b4 >= 128 and -v or v
end

-- component type -> size in bytes, reader, normalizer
local COMP = {
  [5120] = { 1, function(s, p) local v = byte(s, p); return v >= 128 and v - 256 or v end, 127 },
  [5121] = { 1, function(s, p) return byte(s, p) end, 255 },
  [5122] = { 2, function(s, p)
    local a, b = byte(s, p, p + 1); local v = a + b * 256; return v >= 32768 and v - 65536 or v
  end, 32767 },
  [5123] = { 2, function(s, p) local a, b = byte(s, p, p + 1); return a + b * 256 end, 65535 },
  [5125] = { 4, u32 },
  [5126] = { 4, f32 },
}
local NCOMP = { SCALAR = 1, VEC2 = 2, VEC3 = 3, VEC4 = 4, MAT2 = 4, MAT3 = 9, MAT4 = 16 }

---------------------------------------
-- Document, buffers, accessors
---------------------------------------

-- Splits a GLB into its JSON text and BIN chunk.
local function readGlb(bytes)
  if u32(bytes, 5) ~= 2 then error("unsupported glTF version", 0) end
  local p, json, bin = 13, nil, nil
  while p + 7 <= #bytes do
    local len, typ = u32(bytes, p), u32(bytes, p + 4)
    local data = sub(bytes, p + 8, p + 7 + len)
    if #data < len then error("file truncated", 0) end
    if typ == 0x4E4F534A and not json then json = data
    elseif typ == 0x004E4942 and not bin then bin = data end
    p = p + 8 + len + (4 - len % 4) % 4
  end
  if not json then error("GLB has no JSON chunk", 0) end
  return json, bin
end

-- A loader of buffers by index (0-based, as in the file), cached.
local function bufferLoader(doc, glbBin, opts)
  local cache = {}
  return function(bi)
    if cache[bi] then return cache[bi] end
    local b = doc.buffers and doc.buffers[bi + 1]
    if not b then error("buffer " .. bi .. " not found", 0) end
    local data
    local uri = b.uri
    if not uri then
      if bi == 0 then data = glbBin end
    elseif sub(uri, 1, 5) == "data:" then
      local _, e = find(uri, ";base64,", 1, true)
      if not e then error("unsupported data URI", 0) end
      data = b64decode(sub(uri, e + 1))
    elseif opts.readFile then
      data = opts.readFile((uri:gsub("%%(%x%x)", function(h) return char(tonumber(h, 16)) end)))
    end
    if not data then error("buffer " .. bi .. " not found", 0) end
    cache[bi] = data
    return data
  end
end

-- Accessor `ai` (0-based) as a flat array of numbers, plus its component
-- count per element and its element count.
local function readAccessor(doc, load, ai)
  local acc = doc.accessors and doc.accessors[ai + 1]
  if not acc then error("accessor " .. tostring(ai) .. " not found", 0) end
  local comp = COMP[acc.componentType]
  local nc = NCOMP[acc.type]
  if not comp or not nc then error("unsupported accessor type", 0) end
  if acc.sparse then error("sparse accessors are not supported", 0) end
  local count = acc.count or 0
  local out = {}
  if acc.bufferView == nil then
    for i = 1, count * nc do out[i] = 0 end
    return out, nc, count
  end
  local bv = doc.bufferViews and doc.bufferViews[acc.bufferView + 1]
  if not bv then error("bufferView not found", 0) end
  local data = load(bv.buffer or 0)
  local csize, read, norm = comp[1], comp[2], comp[3]
  local stride = bv.byteStride or csize * nc
  if stride < csize * nc then stride = csize * nc end
  local start = (bv.byteOffset or 0) + (acc.byteOffset or 0) + 1
  if count > 0 and start + (count - 1) * stride + csize * nc - 1 > #data then
    error("accessor out of range", 0)
  end
  local normalized = acc.normalized and norm
  local n = 0
  for e = 0, count - 1 do
    local p = start + e * stride
    for c = 0, nc - 1 do
      local v = read(data, p + c * csize)
      if normalized then v = max(v / normalized, -1) end
      n = n + 1
      out[n] = v
    end
  end
  return out, nc, count
end

---------------------------------------
-- Sampling
---------------------------------------

-- Index k (1-based) of the last key with time <= t (1 when before the first).
local function keyIndex(times, t)
  local lo, hi = 1, #times
  while lo < hi do
    local mid = floor((lo + hi + 1) / 2)
    if times[mid] <= t then lo = mid else hi = mid - 1 end
  end
  return lo
end

-- The value of sampler `s` at time t as a table of `nc` numbers.
local function evalSampler(s, t, isRot)
  local times, vals, nc = s.times, s.vals, s.nc
  local nk = #times
  local k = keyIndex(times, t)
  local cubic = s.interp == "CUBICSPLINE"
  local function key(i) -- value of key i
    local base = (cubic and (i - 1) * 3 + 1 or i - 1) * nc
    local v = {}
    for c = 1, nc do v[c] = vals[base + c] end
    return v
  end
  if t <= times[1] or k >= nk then
    local v = key(t <= times[1] and 1 or nk)
    return isRot and M.M3d.quatNormalize(v) or v
  end
  local t0, t1 = times[k], times[k + 1]
  local dt = t1 - t0
  local u = dt > 0 and (t - t0) / dt or 0
  local a = key(k)
  if s.interp == "STEP" then return a end
  local b = key(k + 1)
  if cubic then
    local o0 = ((k - 1) * 3 + 2) * nc -- out tangent of key k
    local i1 = (k * 3) * nc           -- in tangent of key k+1
    local u2, u3 = u * u, u * u * u
    local h00, h10, h01, h11 = 2 * u3 - 3 * u2 + 1, u3 - 2 * u2 + u, -2 * u3 + 3 * u2, u3 - u2
    local v = {}
    for c = 1, nc do
      v[c] = h00 * a[c] + h10 * dt * vals[o0 + c] + h01 * b[c] + h11 * dt * vals[i1 + c]
    end
    return isRot and M.M3d.quatNormalize(v) or v
  end
  if isRot then return M.M3d.quatSlerp(M.M3d.quatNormalize(a), M.M3d.quatNormalize(b), u) end
  local v = {}
  for c = 1, nc do v[c] = a[c] + (b[c] - a[c]) * u end
  return v
end

---------------------------------------
-- The parser
---------------------------------------

local function parse(bytes, opts)
  opts = opts or {}
  if type(bytes) ~= "string" then error("not a glTF file", 0) end
  local glbBin
  local text = bytes
  if sub(bytes, 1, 4) == "glTF" then
    text, glbBin = readGlb(bytes)
  elseif not find(bytes, "^%s*{") and not find(bytes, "^\239\187\191%s*{") then
    error("not a glTF file", 0)
  end
  local doc, err = M.Json.decode(text)
  if type(doc) ~= "table" then error("bad glTF JSON: " .. tostring(err), 0) end
  local ver = doc.asset and doc.asset.version
  if type(ver) ~= "string" or sub(ver, 1, 1) ~= "2" then error("unsupported glTF version", 0) end
  local nodes = doc.nodes
  if type(nodes) ~= "table" or #nodes == 0 then error("no nodes", 0) end
  local load = bufferLoader(doc, glbBin, opts)
  local nn = #nodes

  -- Parents, and default TRS per node (1-based node ids here)
  local parent, defT, defR, defS, staticM = {}, {}, {}, {}, {}
  for i = 1, nn do
    local nd = nodes[i]
    for _, c in ipairs(nd.children or {}) do
      if nodes[c + 1] and parent[c + 1] == nil then parent[c + 1] = i end
    end
    defT[i] = nd.translation or { 0, 0, 0 }
    defR[i] = nd.rotation or { 0, 0, 0, 1 }
    defS[i] = nd.scale or { 1, 1, 1 }
    if nd.matrix and #nd.matrix == 16 then
      staticM[i] = nd.matrix
    else
      staticM[i] = M.M3d.mat4FromTRS(defT[i], M.M3d.quatNormalize(defR[i]), defS[i])
    end
  end

  -- Depth of a node (guards against cycles)
  local function depth(i)
    local d = 0
    while parent[i] do
      i = parent[i]
      d = d + 1
      if d > nn then error("node cycle", 0) end
    end
    return d
  end

  -- Joint nodes
  local jnodes = {}
  local skin = doc.skins and doc.skins[1]
  if skin and skin.joints and #skin.joints > 0 then
    for i, n in ipairs(skin.joints) do
      if not nodes[n + 1] then error("skin joint not found", 0) end
      jnodes[i] = n + 1
    end
    -- Already parents-first? (nearest joint ancestor listed earlier)
    local at = {}
    for i, n in ipairs(jnodes) do at[n] = i end
    local ordered = true
    for i, n in ipairs(jnodes) do
      local p = parent[n]
      while p and not at[p] do p = parent[p] end
      if p and at[p] > i then ordered = false break end
    end
    if not ordered then
      local d = {}
      for i, n in ipairs(jnodes) do d[n] = { depth(n), i } end
      table.sort(jnodes, function(a, b)
        if d[a][1] ~= d[b][1] then return d[a][1] < d[b][1] end
        return d[a][2] < d[b][2]
      end)
    end
  else
    local roots = {}
    local scene = doc.scenes and doc.scenes[(doc.scene or 0) + 1]
    if scene and scene.nodes then
      for _, n in ipairs(scene.nodes) do roots[#roots + 1] = n + 1 end
    else
      for i = 1, nn do if not parent[i] then roots[#roots + 1] = i end end
    end
    local function walk(i, d)
      if d > nn then error("node cycle", 0) end
      local nd = nodes[i]
      if not nd then return end
      if nd.mesh == nil then jnodes[#jnodes + 1] = i end
      for _, c in ipairs(nd.children or {}) do walk(c + 1, d + 1) end
    end
    for _, r in ipairs(roots) do walk(r, 0) end
  end
  if #jnodes == 0 then error("no joints", 0) end
  if #jnodes > M.MAX_JOINTS then error("too many joints", 0) end

  local jindex = {}
  for j, n in ipairs(jnodes) do jindex[n] = j end
  local joints = {}
  for j, n in ipairs(jnodes) do
    local p = parent[n]
    while p and not jindex[p] do p = parent[p] end
    joints[j] = { name = M.Skel.cleanName(nodes[n].name, j), parent = p and jindex[p] or 0, isEnd = false }
  end

  -- Nodes to evaluate: the joints and all their ancestors, parents first
  local needed = {}
  for _, n in ipairs(jnodes) do
    local i = n
    while i and not needed[i] do needed[i] = true; i = parent[i] end
  end
  local topo = {}
  for i = 1, nn do if needed[i] then topo[#topo + 1] = i end end
  local dep = {}
  for _, i in ipairs(topo) do dep[i] = depth(i) end
  table.sort(topo, function(a, b)
    if dep[a] ~= dep[b] then return dep[a] < dep[b] end
    return a < b
  end)

  local mul, point = M.M3d.mat4Mul, M.M3d.mat4Point
  -- World positions of the joints given the local matrices `localM`
  local function positions(localM)
    local world = {}
    for _, i in ipairs(topo) do
      local p = parent[i]
      world[i] = p and mul(world[p], localM[i]) or localM[i]
    end
    local out = {}
    for j, n in ipairs(jnodes) do
      out[3 * j - 2], out[3 * j - 1], out[3 * j] = point(world[n], 0, 0, 0)
    end
    return out
  end

  local skel = { joints = joints, rest = positions(staticM), clips = {} }

  -- Animations
  local fps = tonumber(opts.fps) or 30
  if fps <= 0 then fps = 30 end
  for ai, anim in ipairs(doc.animations or {}) do
    local samplers = {}
    local function sampler(si)
      if samplers[si] then return samplers[si] end
      local sd = anim.samplers and anim.samplers[si + 1]
      if not sd then error("animation sampler not found", 0) end
      local times = readAccessor(doc, load, sd.input)
      local vals, nc = readAccessor(doc, load, sd.output)
      local s = { times = times, vals = vals, nc = nc, interp = sd.interpolation or "LINEAR" }
      local need = s.interp == "CUBICSPLINE" and 3 or 1
      if #times == 0 or #vals ~= #times * need * nc then error("bad animation sampler", 0) end
      samplers[si] = s
      return s
    end
    local chans, tmin, tmax = {}, math.huge, 0
    for _, ch in ipairs(anim.channels or {}) do
      local tg = ch.target
      local path = tg and tg.path
      local ni = tg and tg.node and tg.node + 1
      if ni and nodes[ni] and (path == "translation" or path == "rotation" or path == "scale") then
        local s = sampler(ch.sampler)
        if s.nc == (path == "rotation" and 4 or 3) then
          chans[#chans + 1] = { node = ni, path = path, s = s }
          tmin = min(tmin, s.times[1])
          tmax = max(tmax, s.times[#s.times])
        end
      end
    end
    if #chans > 0 then
      local count = floor((tmax - tmin) * fps + 0.5) + 1
      local k = M.Skel.stride(count)
      local pos, kept = {}, 0
      local animated = {}
      for _, c in ipairs(chans) do animated[c.node] = true end
      for f = 0, count - 1, k do
        local t = tmin + f / fps
        local curT, curR, curS = {}, {}, {}
        for _, c in ipairs(chans) do
          local v = evalSampler(c.s, t, c.path == "rotation")
          if c.path == "translation" then curT[c.node] = v
          elseif c.path == "rotation" then curR[c.node] = v
          else curS[c.node] = v end
        end
        local localM = {}
        for i in pairs(needed) do
          if animated[i] then
            local nd = nodes[i]
            localM[i] = M.M3d.mat4FromTRS(curT[i] or nd.translation, curR[i] or nd.rotation, curS[i] or nd.scale)
          else
            localM[i] = staticM[i]
          end
        end
        kept = kept + 1
        pos[kept] = positions(localM)
      end
      skel.clips[#skel.clips + 1] = { name = anim.name or ("clip" .. ai), frameTime = k / fps, count = kept, pos = pos }
    end
  end
  return skel
end

--- Parses a .glb / .gltf file's content: skel, or nil + a short message.
--- opts.readFile(uri) -> bytes | nil loads external buffers; opts.fps (30).
function M.parse(bytes, opts)
  local ok, res = pcall(parse, bytes, opts)
  if not ok then return nil, tostring(res) end
  return res
end

return M
