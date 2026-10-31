-- skel3d.lua — what bvh.lua and gltf.lua share: the limits and the decimation
-- of long clips, plus a validator for the common output. Pure Lua, dual-VM.
--
--   skel = {
--     joints = { { name, parent, isEnd }, ... },  -- parents BEFORE children, 0 = root
--     rest   = { x1,y1,z1, x2,y2,z2, ... },       -- world positions, joint j at 3j-2..3j
--     clips  = { { name, frameTime, count, pos = { [f] = { x1,y1,z1, ... } } }, ... },
--   }

local M = {}

M.MAX_FRAMES = 2000
M.MAX_JOINTS = 512

--- The step k that keeps a clip of `count` frames within MAX_FRAMES (every
--- k-th frame stays, starting with the first).
function M.stride(count)
  return math.max(1, math.ceil(count / M.MAX_FRAMES))
end

--- Thins a clip in place to at most MAX_FRAMES; frameTime grows by the step.
function M.decimate(clip)
  local k = M.stride(clip.count)
  if k == 1 then return clip end
  local pos, n = {}, 0
  for f = 1, clip.count, k do
    n = n + 1
    pos[n] = clip.pos[f]
  end
  clip.pos, clip.count, clip.frameTime = pos, n, clip.frameTime * k
  return clip
end

--- A joint name that is never empty.
function M.cleanName(name, i)
  if type(name) ~= "string" or name == "" then return "joint" .. i end
  return name
end

--- true when `skel` has the common shape, else nil + the reason.
function M.validate(skel)
  if type(skel) ~= "table" or type(skel.joints) ~= "table" then return nil, "no joints" end
  local nj = #skel.joints
  if nj == 0 then return nil, "no joints" end
  if nj > M.MAX_JOINTS then return nil, "too many joints" end
  for i, j in ipairs(skel.joints) do
    if type(j.name) ~= "string" or j.name == "" then return nil, "joint " .. i .. " has no name" end
    if type(j.parent) ~= "number" or j.parent < 0 or j.parent >= i then
      return nil, "joint " .. i .. " has a bad parent"
    end
  end
  if #skel.rest ~= nj * 3 then return nil, "rest size mismatch" end
  for ci, c in ipairs(skel.clips) do
    if c.count ~= #c.pos then return nil, "clip " .. ci .. " frame count mismatch" end
    for f = 1, c.count do
      if #c.pos[f] ~= nj * 3 then return nil, "clip " .. ci .. " frame " .. f .. " size mismatch" end
    end
  end
  return true
end

return M
