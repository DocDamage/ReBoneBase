-- bvh.lua — BVH motion capture files to the common skeleton (skel3d.lua).
-- Pure Lua, dual-VM.
--
-- HIERARCHY: ROOT / JOINT name { OFFSET x y z / CHANNELS n ... / children /
-- End Site { OFFSET } }; several ROOTs are fine. MOTION: Frames, Frame Time,
-- then one row of numbers per frame (one number per channel, in file order).
--
-- A joint's local rotation is the product of its rotation channels in the
-- ORDER LISTED: `Zrotation Xrotation Yrotation` is R = Rz * Rx * Ry, applied
-- to a child vector as R * v (degrees). A root uses offset + its position
-- channels; a non-root joint WITH position channels uses the channel values
-- instead of the offset (exporters that write 6 channels everywhere). End
-- Sites become joints named `<parent>_end` with isEnd = true. The result is
-- one clip named "Take"; rest = every rotation zero (the offsets, T-pose).

local M = {}

M.Skel, M.M3d = nil, nil

function M.bind(Skel, M3d)
  M.Skel, M.M3d = Skel, M3d
  return M
end

M.MAX_FRAMES = 2000
M.MAX_JOINTS = 512

local find, sub, gmatch = string.find, string.sub, string.gmatch
local lower = string.lower

local ROT = { xrotation = "x", yrotation = "y", zrotation = "z" }
local POS = { xposition = 1, yposition = 2, zposition = 3 }

-- Hierarchy tokens -> joints { name, parent, isEnd, off, chans }. `chans`
-- lists { "r", axis } or { "p", 1..3 } in file order. Returns joints, nch.
local function parseHierarchy(head)
  local toks, n = {}, 0
  for t in gmatch((head:gsub("[{}]", " %0 ")), "%S+") do
    n = n + 1
    toks[n] = t
  end
  if lower(toks[1] or "") ~= "hierarchy" then error("not a BVH file", 0) end
  local joints, stack, i, nch = {}, {}, 2, 0
  local function num()
    local v = tonumber(toks[i])
    if not v then error("bad number in hierarchy", 0) end
    i = i + 1
    return v
  end
  while i <= n do
    local t = lower(toks[i])
    i = i + 1
    if t == "root" or t == "joint" or t == "end" then
      local name
      if t == "end" then
        if lower(toks[i] or "") ~= "site" then error("bad End Site", 0) end
        i = i + 1
        name = (stack[#stack] and joints[stack[#stack]].name or "joint") .. "_end"
      else
        name = toks[i]
        i = i + 1
      end
      if toks[i] ~= "{" then error("expected '{'", 0) end
      i = i + 1
      if t == "root" then stack = {} end
      if #joints >= M.MAX_JOINTS then error("too many joints", 0) end
      local idx = #joints + 1
      joints[idx] = { name = M.Skel.cleanName(name, idx), parent = stack[#stack] or 0,
        isEnd = t == "end", off = { 0, 0, 0 }, chans = {} }
      stack[#stack + 1] = idx
    elseif t == "offset" then
      local j = joints[stack[#stack] or 0]
      if not j then error("OFFSET outside a joint", 0) end
      j.off = { num(), num(), num() }
    elseif t == "channels" then
      local j = joints[stack[#stack] or 0]
      if not j then error("CHANNELS outside a joint", 0) end
      local c = num()
      for _ = 1, c do
        local ch = lower(toks[i] or "")
        i = i + 1
        if ROT[ch] then
          j.chans[#j.chans + 1] = { "r", ROT[ch] }
        elseif POS[ch] then
          j.chans[#j.chans + 1] = { "p", POS[ch] }
        else
          error("unknown channel", 0)
        end
      end
      nch = nch + c
    elseif t == "}" then
      if #stack == 0 then error("unbalanced braces", 0) end
      stack[#stack] = nil
    else
      error("unexpected token in hierarchy", 0)
    end
  end
  if #stack ~= 0 then error("unbalanced braces", 0) end
  if #joints == 0 then error("no joints", 0) end
  return joints, nch
end

-- Local translation and rotation of joint j for one row of channel values
-- (`row`, first channel at `base`+1); returns x, y, z, quat.
local function localPose(j, row, base)
  local q = nil
  local ox, oy, oz = j.off[1], j.off[2], j.off[3]
  local px, py, pz
  for k, ch in ipairs(j.chans) do
    local v = row[base + k]
    if ch[1] == "r" then
      local r = M.M3d.quatAxis(ch[2], v)
      q = q and M.M3d.quatMul(q, r) or r
    elseif ch[2] == 1 then px = v
    elseif ch[2] == 2 then py = v
    else pz = v end
  end
  if j.parent == 0 then -- root: offset + channels
    ox, oy, oz = ox + (px or 0), oy + (py or 0), oz + (pz or 0)
  else -- non-root: channels replace the offset
    ox, oy, oz = px or ox, py or oy, pz or oz
  end
  return ox, oy, oz, q
end

-- World positions (flat) of every joint for one row of values (nil = rest).
local function worldPositions(joints, row)
  local wq, wx, wy, wz, out = {}, {}, {}, {}, {}
  local base = 0
  local rotate = M.M3d.quatRotate
  for i, j in ipairs(joints) do
    local x, y, z, q
    if row then
      x, y, z, q = localPose(j, row, base)
      base = base + #j.chans
    else
      x, y, z = j.off[1], j.off[2], j.off[3]
    end
    local p = j.parent
    if p == 0 then
      wx[i], wy[i], wz[i], wq[i] = x, y, z, q
    else
      local pq = wq[p]
      local rx, ry, rz = x, y, z
      if pq then rx, ry, rz = rotate(pq, x, y, z) end
      wx[i], wy[i], wz[i] = wx[p] + rx, wy[p] + ry, wz[p] + rz
      wq[i] = (pq and q) and M.M3d.quatMul(pq, q) or pq or q
    end
    out[3 * i - 2], out[3 * i - 1], out[3 * i] = wx[i], wy[i], wz[i]
  end
  return out
end

local function parse(text)
  if type(text) ~= "string" then error("not a BVH file", 0) end
  local mp = find(text, "MOTION", 1, true)
  if not mp then
    if not find(text, "^%s*HIERARCHY") then error("not a BVH file", 0) end
    error("no MOTION section", 0)
  end
  local joints, nch = parseHierarchy(sub(text, 1, mp - 1))
  local rest = sub(text, mp + 6)
  local _, fe, nf = find(rest, "^%s*Frames:%s*(%d+)")
  if not fe then error("bad Frames line", 0) end
  local _, te, ft = find(rest, "^%s*Frame%s+Time:%s*([%d%.eE+-]+)", fe + 1)
  if not te then error("bad Frame Time line", 0) end
  nf, ft = tonumber(nf), tonumber(ft)
  if not ft or ft <= 0 then error("bad Frame Time", 0) end
  if nf < 1 then error("no frames", 0) end
  if nch == 0 then error("no channels", 0) end

  local skel = { joints = {}, rest = worldPositions(joints, nil), clips = {} }
  for i, j in ipairs(joints) do
    skel.joints[i] = { name = j.name, parent = j.parent, isEnd = j.isEnd }
  end

  -- Only the frames that survive decimation are evaluated
  local k = M.Skel.stride(nf)
  local pos, kept = {}, 0
  local row, rn, frame = {}, 0, 0
  for w in gmatch(sub(rest, te + 1), "%S+") do
    local v = tonumber(w)
    if not v then error("bad number in motion data", 0) end
    rn = rn + 1
    row[rn] = v
    if rn == nch then
      frame = frame + 1
      if (frame - 1) % k == 0 then
        kept = kept + 1
        pos[kept] = worldPositions(joints, row)
      end
      rn = 0
      if frame == nf then break end
    end
  end
  if frame < nf or rn ~= 0 then error("motion data truncated", 0) end
  skel.clips[1] = { name = "Take", frameTime = ft * k, count = kept, pos = pos }
  return skel
end

--- Parses BVH text: skel, or nil + a short message.
function M.parse(text)
  local ok, res = pcall(parse, text)
  if not ok then return nil, tostring(res) end
  return res
end

return M
