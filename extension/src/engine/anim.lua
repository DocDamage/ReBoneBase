-- anim.lua — the animation of a Bonebase rig: keyframes per bone channel,
-- sampling with easing, and the analytic two-bone IK the pose tools use.
-- Pure Lua, dual-VM.
--
-- doc.anim = {
--   frames  = 12,         -- frames 1..frames
--   frameMs = 100,        -- duration of every frame
--   loop    = true,       -- the last key eases back into the first
--   ease    = "Ease",     -- ease of new keys
--   tracks  = { { bone = id, ch = channel, keys = { key, ... } }, ... },
-- }
-- A KEY is an array { frame, value, ease } sorted by frame, one per frame.
-- `ease` belongs to the segment that STARTS at that key (see EASES; "Step"
-- holds until the next key).
--
-- Channels. A track's `bone` is the id of what it animates (bones, parts
-- and IK controllers share one id space):
--   bone  rot   degrees over the rest angle
--         x, y  offset of the head (root / loose bones), in its frame
--         s     length factor (1 = rest; squash and stretch)
--   part  z     drawing order, replaces the part's z (always Step)
--         vis   1 shown, 0 hidden (always Step)
--   IK    tx ty px py (ik.lua)
--   the reference image, owner -1: rx ry rs rr ro rf (refimg.lua)
-- A channel without keys rests at NEUTRAL (0, s and vis at 1), so an
-- untouched bone stays in its rest pose.
--
-- Keys are always FK. The IK below is a posing aid: it returns the two
-- rotations that put a chain's end on a target, and the editor keys them
-- like any other rotation (DECISIONS: IK is a tool, not a live constraint).

local M = {}

local cos, sin, pi, sqrt, abs = math.cos, math.sin, math.pi, math.sqrt, math.abs
local RAD, DEG = pi / 180, 180 / pi

M.EASES = { "Linear", "Ease", "Ease in", "Ease out", "Back", "Bounce", "Step" }
M.CHANNELS = { "rot", "x", "y", "s" }
M.PART_CHANNELS = { "z", "vis" }
M.STEPPED = { z = true, vis = true, rf = true }   -- channels that never interpolate
M.NEUTRAL = { s = 1, vis = 1 }         -- the rest value when not 0
M.MIN_SCALE, M.MAX_SCALE = 0.1, 4
M.MAX_FRAMES = 240

--- The value channel `ch` has without keys.
function M.neutral(ch)
  return M.NEUTRAL[ch] or 0
end

local function angleOf(dx, dy)
  if math.atan2 then return math.atan2(dy, dx) * DEG end
  return math.atan(dy, dx) * DEG
end

local function wrap(a)
  a = a % 360
  if a > 180 then a = a - 360 end
  return a
end

--- `target` moved by whole turns to be as close as possible to `cur`, so
--- a key never spins the long way round.
function M.unwrap(cur, target)
  return cur + wrap(target - cur)
end

---------------------------------------
-- Tracks and keys --
---------------------------------------

function M.track(anim, bone, ch, create)
  for _, t in ipairs(anim.tracks) do
    if t.bone == bone and t.ch == ch then return t end
  end
  if not create then return nil end
  local t = { bone = bone, ch = ch, keys = {} }
  anim.tracks[#anim.tracks + 1] = t
  return t
end

--- Index of the key at `frame` in a track, or nil.
local function keyIndex(t, frame)
  for i, k in ipairs(t.keys) do
    if k[1] == frame then return i end
  end
  return nil
end

--- Sets (or replaces) a key; keeps the list sorted. A replaced key keeps
--- its ease unless one is given.
function M.setKey(anim, bone, ch, frame, value, ease)
  local t = M.track(anim, bone, ch, true)
  local i = keyIndex(t, frame)
  if i then
    t.keys[i][2] = value
    if ease then t.keys[i][3] = ease end
    return t.keys[i]
  end
  local k = { frame, value, ease or anim.ease or "Ease" }
  local at = #t.keys + 1
  for j, q in ipairs(t.keys) do
    if frame < q[1] then at = j break end
  end
  table.insert(t.keys, at, k)
  return k
end

--- Drops empty tracks.
local function compact(anim)
  local kept = {}
  for _, t in ipairs(anim.tracks) do
    if #t.keys > 0 then kept[#kept + 1] = t end
  end
  anim.tracks = kept
end

--- Removes the keys at `frame` of the given bones (all bones when `bones`
--- is nil). Returns how many went.
function M.removeKeys(anim, frame, bones)
  local n = 0
  for _, t in ipairs(anim.tracks) do
    if not bones or bones[t.bone] then
      local i = keyIndex(t, frame)
      if i then
        table.remove(t.keys, i)
        n = n + 1
      end
    end
  end
  compact(anim)
  return n
end

--- Moves the keys at `from` to `to` (replacing what was there) for the
--- given bones (all when nil). Returns how many moved.
function M.moveKeys(anim, from, to, bones)
  if from == to then return 0 end
  local n = 0
  for _, t in ipairs(anim.tracks) do
    if not bones or bones[t.bone] then
      local i = keyIndex(t, from)
      if i then
        local k = table.remove(t.keys, i)
        local j = keyIndex(t, to)
        if j then table.remove(t.keys, j) end
        k[1] = to
        local at = #t.keys + 1
        for q, kk in ipairs(t.keys) do
          if to < kk[1] then at = q break end
        end
        table.insert(t.keys, at, k)
        n = n + 1
      end
    end
  end
  return n
end

--- True when any of `bones` (all when nil) has a key at `frame`.
function M.hasKey(anim, frame, bones)
  for _, t in ipairs(anim.tracks) do
    if (not bones or bones[t.bone]) and keyIndex(t, frame) then return true end
  end
  return false
end

--- The keys at `frame` of `bones` (all when nil).
function M.keysAt(anim, frame, bones)
  local out = {}
  for _, t in ipairs(anim.tracks) do
    if not bones or bones[t.bone] then
      local i = keyIndex(t, frame)
      if i then out[#out + 1] = t.keys[i] end
    end
  end
  return out
end

--- Sorted list of frames that hold a key of `bones` (all when nil).
function M.keyFrames(anim, bones)
  local seen, out = {}, {}
  for _, t in ipairs(anim.tracks) do
    if not bones or bones[t.bone] then
      for _, k in ipairs(t.keys) do
        if not seen[k[1]] then
          seen[k[1]] = true
          out[#out + 1] = k[1]
        end
      end
    end
  end
  table.sort(out)
  return out
end

--- Drops tracks of bones that no longer exist and keys past the last
--- frame. `exists(id) -> bool`.
function M.prune(anim, exists)
  local kept = {}
  for _, t in ipairs(anim.tracks) do
    if exists(t.bone) then
      local keys = {}
      for _, k in ipairs(t.keys) do
        if k[1] >= 1 and k[1] <= anim.frames then keys[#keys + 1] = k end
      end
      t.keys = keys
      if #keys > 0 then kept[#kept + 1] = t end
    end
  end
  anim.tracks = kept
end

--- Moves every key AFTER frame `after` by `delta` frames (+1 makes room
--- for an inserted frame, -1 closes the gap of a deleted one). Keys that
--- would land on or before `after` are dropped (a deleted frame's own keys
--- go first, see the editor).
function M.shiftKeys(anim, after, delta)
  for _, t in ipairs(anim.tracks) do
    local keys = {}
    for _, k in ipairs(t.keys) do
      if k[1] > after then
        local f = k[1] + delta
        if f > after then
          k[1] = f
          keys[#keys + 1] = k
        end
      else
        keys[#keys + 1] = k
      end
    end
    t.keys = keys
  end
  compact(anim)
end

---------------------------------------
-- Key sets (the dope sheet) --
---------------------------------------
-- A KEY SET picks dope sheet cells: { [id] = { [frame] = true } }, every
-- channel of that bone / part / controller at that frame.

function M.setHas(set, id, f)
  local r = set[id]
  return r ~= nil and r[f] == true
end

function M.setAdd(set, id, f)
  local r = set[id]
  if not r then
    r = {}
    set[id] = r
  end
  r[f] = true
end

function M.setRemove(set, id, f)
  local r = set[id]
  if not r then return end
  r[f] = nil
  if next(r) == nil then set[id] = nil end
end

--- The cells of `set` that hold a key in `anim` (stale picks dropped),
--- as a new set, and how many there are.
function M.setLive(anim, set)
  local out, n = {}, 0
  for _, t in ipairs(anim.tracks) do
    local r = set[t.bone]
    if r then
      for _, k in ipairs(t.keys) do
        if r[k[1]] and not M.setHas(out, t.bone, k[1]) then
          M.setAdd(out, t.bone, k[1])
          n = n + 1
        end
      end
    end
  end
  return out, n
end

--- First and last frame in `set` (nil when empty) and its owners.
function M.setRange(set)
  local lo, hi
  local owners = {}
  for id, r in pairs(set) do
    for f in pairs(r) do
      owners[id] = true
      if not lo or f < lo then lo = f end
      if not hi or f > hi then hi = f end
    end
  end
  return lo, hi, owners
end

--- Moves the keys `set` picks to map(frame) (nil, or a frame outside the
--- clip, drops them), replacing other keys where they land; when two
--- picked keys land together the later one wins. Returns the set of where
--- they went. (The editor checks its moves first: it never drops keys.)
function M.remapKeys(anim, set, map)
  local out = {}
  for _, t in ipairs(anim.tracks) do
    local r = set[t.bone]
    if r then
      local moved, at = {}, {}
      for _, k in ipairs(t.keys) do
        if r[k[1]] then moved[#moved + 1] = k else at[k[1]] = k end
      end
      if #moved > 0 then
        for _, k in ipairs(moved) do
          local f = map(k[1])
          if f and f >= 1 and f <= anim.frames then
            k[1] = f
            at[f] = k
            M.setAdd(out, t.bone, f)
          end
        end
        local keys = {}
        for _, k in pairs(at) do keys[#keys + 1] = k end
        table.sort(keys, function(a, b) return a[1] < b[1] end)
        t.keys = keys
      end
    end
  end
  compact(anim)
  return out
end

--- The keys `set` picks, as a clipboard: frames relative to the first.
function M.copyKeys(anim, set)
  local lo = M.setRange(set)
  local items = {}
  for _, t in ipairs(anim.tracks) do
    local r = set[t.bone]
    if r then
      for _, k in ipairs(t.keys) do
        if r[k[1]] then items[#items + 1] = { t.bone, t.ch, k[1] - lo, k[2], k[3] } end
      end
    end
  end
  if #items == 0 then return nil end
  return { items = items }
end

--- Writes a clipboard from copyKeys with its first frame at `at` (keys
--- past the clip's end are left out; `exists(id)` filters owners).
--- Returns the set of pasted cells.
function M.pasteKeys(anim, clip, at, exists)
  local out = {}
  for _, it in ipairs(clip.items) do
    local f = at + it[3]
    if f >= 1 and f <= anim.frames and (not exists or exists(it[1])) then
      M.setKey(anim, it[1], it[2], f, it[4], it[5])
      M.setAdd(out, it[1], f)
    end
  end
  return out
end

--- Every key `set` picks (the key arrays themselves, to edit their ease).
function M.keysIn(anim, set)
  local out = {}
  for _, t in ipairs(anim.tracks) do
    local r = set[t.bone]
    if r then
      for _, k in ipairs(t.keys) do
        if r[k[1]] then out[#out + 1] = k end
      end
    end
  end
  return out
end

--- Removes the keys `set` picks. Returns how many went.
function M.deleteKeySet(anim, set)
  local n = 0
  for _, t in ipairs(anim.tracks) do
    local r = set[t.bone]
    if r then
      local keys = {}
      for _, k in ipairs(t.keys) do
        if r[k[1]] then n = n + 1 else keys[#keys + 1] = k end
      end
      t.keys = keys
    end
  end
  compact(anim)
  return n
end

local MIRROR_EASE = { ["Ease in"] = "Ease out", ["Ease out"] = "Ease in" }

--- Reverses the keys between frames `from` and `to` (of `bones`, all when
--- nil): a key at f goes to from + to - f, so the motion plays backwards.
--- Each segment keeps its ease, now on the key that starts it in the new
--- order (Ease in and Ease out swap: slow at the start becomes slow at
--- the end); the last key keeps the ease of whatever came after the range.
--- `pick` (a key set): only those keys move, and a key they land on that
--- was not picked is replaced (as when dragging keys). Returns how many
--- keys moved.
function M.reverseKeys(anim, from, to, bones, pick)
  local n = 0
  for _, t in ipairs(anim.tracks) do
    if not bones or bones[t.bone] then
      local inside, rest = {}, {}
      for _, k in ipairs(t.keys) do
        local f = k[1]
        if f >= from and f <= to and (not pick or M.setHas(pick, t.bone, f)) then
          inside[#inside + 1] = k
        else
          rest[#rest + 1] = k
        end
      end
      local landing = {}
      for _, k in ipairs(inside) do landing[from + to - k[1]] = true end
      local keys = {}
      for _, k in ipairs(rest) do
        if not landing[k[1]] then keys[#keys + 1] = k end
      end
      local m = #inside
      if m > 0 then
        local eases = {}
        for i = 1, m do eases[i] = inside[i][3] end
        for i = 1, m - 1 do
          inside[i + 1][3] = MIRROR_EASE[eases[i]] or eases[i]
        end
        inside[1][3] = eases[m]
        for _, k in ipairs(inside) do
          k[1] = from + to - k[1]
          keys[#keys + 1] = k
        end
        table.sort(keys, function(a, b) return a[1] < b[1] end)
        t.keys = keys
        n = n + m
      end
    end
  end
  return n
end

--- A pose value reflected across a vertical axis, for a bone whose twin
--- receives it: rotations turn the other way; an offset flips its x in
--- canvas space (a root) or its y in the parent's frame (any other bone,
--- whose mirrored parent has its perpendicular flipped).
function M.mirrorValue(ch, v, isRoot)
  if ch == "rot" then return -v end
  if ch == "x" then return isRoot and -v or v end
  if ch == "y" then return isRoot and v or -v end
  return v
end

---------------------------------------
-- Sampling --
---------------------------------------

local BACK = 1.70158

local function bounce(u)
  local n, d = 7.5625, 2.75
  if u < 1 / d then return n * u * u end
  if u < 2 / d then u = u - 1.5 / d return n * u * u + 0.75 end
  if u < 2.5 / d then u = u - 2.25 / d return n * u * u + 0.9375 end
  u = u - 2.625 / d
  return n * u * u + 0.984375
end

--- How far (0..1, Back overshoots) a segment with ease `name` is at u.
function M.ease(name, u)
  if name == "Step" then return 0 end
  if name == "Linear" then return u end
  if name == "Ease in" then return u * u * u end
  if name == "Ease out" then
    local v = 1 - u
    return 1 - v * v * v
  end
  if name == "Back" then
    -- out-back: past the next key, then settles on it
    local v = u - 1
    return 1 + (BACK + 1) * v * v * v + BACK * v * v
  end
  if name == "Bounce" then return bounce(u) end
  return (1 - cos(pi * u)) * 0.5
end

local easeOf = M.ease

local valueRaw

--- The value of a track at `frame` (a number; fractional frames work).
--- A length factor stays in [MIN_SCALE, MAX_SCALE] even when Back or
--- Bounce overshoot: a negative length would flip the bone.
function M.value(anim, t, frame)
  local v = valueRaw(anim, t, frame)
  if t.ch == "s" then v = math.max(M.MIN_SCALE, math.min(M.MAX_SCALE, v)) end
  return v
end

valueRaw = function(anim, t, frame)
  local keys = t.keys
  local n = #keys
  if n == 0 then return M.neutral(t.ch) end
  if n == 1 then return keys[1][2] end
  local stepped = M.STEPPED[t.ch]
  local first, last = keys[1], keys[n]
  local loop = anim.loop and anim.frames > 0
  if frame < first[1] or frame > last[1] then
    if not loop then
      return frame < first[1] and first[2] or last[2]
    end
    -- the gap across the loop seam: last key -> first key one loop later
    local a, b = last[1], first[1] + anim.frames
    local f = frame < first[1] and frame + anim.frames or frame
    if b <= a then return last[2] end
    local u = (f - a) / (b - a)
    -- a rotation goes round the seam the short way: 350 -> 0 is +10, the
    -- first key one turn on, not a whole turn back
    local to = first[2]
    if t.ch == "rot" then to = M.unwrap(last[2], to) end
    return last[2] + (to - last[2]) * easeOf(stepped and "Step" or last[3], u)
  end
  for i = 1, n - 1 do
    local a, b = keys[i], keys[i + 1]
    if frame <= b[1] then
      if frame == b[1] then return b[2] end
      local u = (frame - a[1]) / (b[1] - a[1])
      return a[2] + (b[2] - a[2]) * easeOf(stepped and "Step" or a[3], u)
    end
  end
  return last[2]
end

--- The pose (rig.lua) at `frame`. Part channels land under the part's id
--- (pose[partId].z / .vis, read by skin.lua render).
function M.sample(anim, frame)
  local pose = {}
  for _, t in ipairs(anim.tracks) do
    local p = pose[t.bone]
    if not p then
      p = { rot = 0, x = 0, y = 0 }
      pose[t.bone] = p
    end
    p[t.ch] = M.value(anim, t, frame)
  end
  return pose
end

---------------------------------------
-- Two-bone IK --
---------------------------------------

M.IK_EPS = 1e-4

--- The bend side of a chain as it is now: +1 when the lower bone turns
--- clockwise from the upper one, else -1.
function M.bendOf(upperAngle, lowerAngle)
  return wrap(lowerAngle - upperAngle) >= 0 and 1 or -1
end

--- World angles (degrees) of the upper and lower bone that put the chain's
--- end on (tx, ty), from the upper bone's head (px, py), lengths l1, l2,
--- bending to side `bend`. An unreachable target gets the closest pose
--- (straight towards it, or folded).
function M.solve2(px, py, l1, l2, tx, ty, bend)
  local dx, dy = tx - px, ty - py
  local d = sqrt(dx * dx + dy * dy)
  local lo, hi = abs(l1 - l2) + M.IK_EPS, l1 + l2 - M.IK_EPS
  if d < lo then d = lo elseif d > hi then d = hi end
  local theta = (dx == 0 and dy == 0) and 0 or angleOf(dx, dy)
  local c = (l1 * l1 + d * d - l2 * l2) / (2 * l1 * d)
  if c > 1 then c = 1 elseif c < -1 then c = -1 end
  local alpha = math.acos(c) * DEG
  local a1 = theta - (bend or 1) * alpha
  local ex, ey = px + l1 * cos(a1 * RAD), py + l1 * sin(a1 * RAD)
  local gx, gy = px + d * cos(theta * RAD), py + d * sin(theta * RAD)
  local a2 = angleOf(gx - ex, gy - ey)
  return a1, a2
end

return M
