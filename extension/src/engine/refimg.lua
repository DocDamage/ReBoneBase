-- refimg.lua — reference images: drawings or photos shown behind (or over)
-- the rig while animating, never exported. Pure Lua, dual-VM.
--
-- A clip (anim.lua) may carry one reference:
--
--   anim.ref = {
--     image  = key | nil,        -- the GLOBAL image, every frame
--     frames = { [f] = key },    -- frames with their own image instead
--     base   = { rx, ry, rs, rr, ro, rf },  -- the transform without keys
--     on     = true,             -- shown
--     front  = false,            -- drawn over the skin instead of under
--   }
--
-- The transform: rx / ry = where the image's centre sits on the canvas
-- (px), rs = scale, rr = rotation (deg, clockwise), ro = opacity (0..1),
-- rf = 1 when flipped left to right. Keys go in the clip's tracks with the
-- owner M.OWNER (-1, which no bone, part or IK controller uses), so every
-- clip has its own; without keys a channel is its `base` value (what the
-- Rest mode edits), like a bone's rest pose. `rf` never interpolates.
--
-- The images themselves live once per document: doc.images[key] =
-- { w, h, data }, `data` an RLE + base64 string of the RGBA pixels (no
-- inflate in pure Lua, so no PNG), the longest side at most MAX_SIDE.

local M = {}

M.Anim = nil

function M.bind(Anim)
  M.Anim = Anim
  return M
end

M.OWNER = -1
M.CHANNELS = { "rx", "ry", "rs", "rr", "ro", "rf" }
M.IS_CHANNEL = { rx = true, ry = true, rs = true, rr = true, ro = true, rf = true }
M.MAX_SIDE = 512
M.MIN_SCALE, M.MAX_SCALE = 0.02, 32

local floor, ceil, max, min = math.floor, math.ceil, math.max, math.min
local cos, sin, rad, abs = math.cos, math.sin, math.rad, math.abs
local char, byte = string.char, string.byte

---------------------------------------
-- The clip's reference
---------------------------------------

--- A new, empty reference for a canvas of w x h.
function M.new(w, h)
  return { image = nil, frames = {}, on = true, front = false,
    base = { rx = w / 2, ry = h / 2, rs = 1, rr = 0, ro = 0.5, rf = 0 } }
end

--- A channel value made valid.
function M.clampValue(ch, v)
  v = tonumber(v) or 0
  if v ~= v then v = 0 end
  if ch == "rs" then return max(M.MIN_SCALE, min(M.MAX_SCALE, v)) end
  if ch == "ro" then return max(0, min(1, v)) end
  if ch == "rf" then return v >= 0.5 and 1 or 0 end
  if ch == "rr" then return max(-3600, min(3600, v)) end
  return max(-100000, min(100000, v))
end

--- The transform at `frame` (nil = the base, Rest mode): a table with the
--- six channels.
function M.transform(anim, frame)
  local ref = anim.ref
  local out = {}
  for _, ch in ipairs(M.CHANNELS) do
    local v = ref and ref.base[ch] or 0
    if frame then
      local t = M.Anim.track(anim, M.OWNER, ch)
      if t and #t.keys > 0 then v = M.Anim.value(anim, t, frame) end
    end
    out[ch] = M.clampValue(ch, v)
  end
  return out
end

--- The image key shown at `frame` (nil = the base: the global one).
function M.imageAt(anim, frame)
  local ref = anim.ref
  if not ref then return nil end
  return (frame and ref.frames[frame]) or ref.image
end

--- True when the clip's reference has anything to show or keys.
function M.used(anim)
  local ref = anim.ref
  if ref and (ref.image or next(ref.frames)) then return true end
  for _, t in ipairs(anim.tracks) do
    if t.bone == M.OWNER then return true end
  end
  return false
end

---------------------------------------
-- Codec: RLE over RGBA pixels, then base64
---------------------------------------

local B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
local B64DEC = {}
for i = 1, 64 do B64DEC[byte(B64, i)] = i - 1 end

local function b64encode(s)
  local out = {}
  local n = #s
  for i = 1, n, 3 do
    local a, b, c = byte(s, i, i + 2)
    local v = a * 65536 + (b or 0) * 256 + (c or 0)
    local c1 = floor(v / 262144) % 64
    local c2 = floor(v / 4096) % 64
    local c3 = floor(v / 64) % 64
    local c4 = v % 64
    out[#out + 1] = B64:sub(c1 + 1, c1 + 1) .. B64:sub(c2 + 1, c2 + 1)
      .. (b and B64:sub(c3 + 1, c3 + 1) or "=") .. (c and B64:sub(c4 + 1, c4 + 1) or "=")
  end
  return table.concat(out)
end

local function b64decode(s)
  local out = {}
  local n = #s
  if n % 4 ~= 0 then return nil end
  for i = 1, n, 4 do
    local a, b, c, d = byte(s, i, i + 3)
    local va, vb = B64DEC[a], B64DEC[b]
    if not va or not vb then return nil end
    -- padding only at the very end, and "x=" never followed by a digit
    if (c == 61 or d == 61) and i + 3 ~= n then return nil end
    if c == 61 and d ~= 61 then return nil end
    local vc = c ~= 61 and B64DEC[c] or 0
    local vd = d ~= 61 and B64DEC[d] or 0
    if not vc or not vd then return nil end
    local v = va * 262144 + vb * 4096 + vc * 64 + vd
    local x, y, z = floor(v / 65536) % 256, floor(v / 256) % 256, v % 256
    if c == 61 then
      out[#out + 1] = char(x)
    elseif d == 61 then
      out[#out + 1] = char(x, y)
    else
      out[#out + 1] = char(x, y, z)
    end
  end
  return table.concat(out)
end

M.b64encode, M.b64decode = b64encode, b64decode

--- A kit/buffer-shaped image as a string: runs of up to 255 equal pixels
--- (fully transparent pixels count as one colour), base64.
function M.encode(buf)
  local d = buf.d
  local n = buf.w * buf.h
  local out = {}
  local i = 1
  local function px(k)
    local j = (k - 1) * 4 + 1
    if d[j + 3] == 0 then return 0, 0, 0, 0 end
    return d[j], d[j + 1], d[j + 2], d[j + 3]
  end
  while i <= n do
    local r, g, b, a = px(i)
    local run = 1
    while i + run <= n and run < 255 do
      local r2, g2, b2, a2 = px(i + run)
      if r2 ~= r or g2 ~= g or b2 ~= b or a2 ~= a then break end
      run = run + 1
    end
    out[#out + 1] = char(run, r, g, b, a)
    i = i + run
  end
  return b64encode(table.concat(out))
end

--- The image back as a buffer, or nil when `data` does not hold exactly w x h.
function M.decode(data, w, h)
  local raw = type(data) == "string" and b64decode(data)
  if not raw or #raw % 5 ~= 0 then return nil end
  local d = {}
  local k = 0
  local total = w * h * 4
  for i = 1, #raw, 5 do
    local run, r, g, b, a = byte(raw, i, i + 4)
    if run == 0 or k + run * 4 > total then return nil end
    for _ = 1, run do
      d[k + 1], d[k + 2], d[k + 3], d[k + 4] = r, g, b, a
      k = k + 4
    end
  end
  if k ~= total then return nil end
  return { w = w, h = h, d = d }
end

---------------------------------------
-- Import
---------------------------------------

--- `buf` with its longest side at most `side` (a box filter, alpha
--- weighted); the same table when it already fits.
function M.fit(buf, side)
  side = side or M.MAX_SIDE
  local w, h = buf.w, buf.h
  local longest = max(w, h)
  if longest <= side then return buf end
  local f = side / longest
  local ow, oh = max(1, floor(w * f + 0.5)), max(1, floor(h * f + 0.5))
  local d, s = {}, buf.d
  for oy = 0, oh - 1 do
    local y0, y1 = floor(oy * h / oh), max(floor(oy * h / oh) + 1, floor((oy + 1) * h / oh))
    for ox = 0, ow - 1 do
      local x0, x1 = floor(ox * w / ow), max(floor(ox * w / ow) + 1, floor((ox + 1) * w / ow))
      local r, g, b, a, n = 0, 0, 0, 0, 0
      for y = y0, y1 - 1 do
        for x = x0, x1 - 1 do
          local i = (y * w + x) * 4 + 1
          local al = s[i + 3]
          r, g, b, a = r + s[i] * al, g + s[i + 1] * al, b + s[i + 2] * al, a + al
          n = n + 1
        end
      end
      local o = (oy * ow + ox) * 4 + 1
      if a > 0 then
        d[o], d[o + 1], d[o + 2] = floor(r / a + 0.5), floor(g / a + 0.5), floor(b / a + 0.5)
      else
        d[o], d[o + 1], d[o + 2] = 0, 0, 0
      end
      d[o + 3] = floor(a / n + 0.5)
    end
  end
  return { w = ow, h = oh, d = d }
end

--- A short, stable name for an image's content: two hashes over EVERY
--- byte (two drawings one stroke apart must not share a name).
local function keyOf(w, h, data)
  local h1, h2 = 5381, 0
  local n = #data
  for i = 1, n, 256 do
    local bytes = { byte(data, i, min(n, i + 255)) }
    for k = 1, #bytes do
      local c = bytes[k]
      h1 = (h1 * 33 + c) % 4294967296
      h2 = (h2 * 65599 + c + 1) % 2147483647
    end
  end
  return string.format("i%dx%d_%08x%08x_%d", w, h, h1, h2, n)
end

--- Puts an image in the document's store (fitted to MAX_SIDE, once per
--- content). Returns its key.
function M.store(doc, buf)
  buf = M.fit(buf)
  local data = M.encode(buf)
  local key = keyOf(buf.w, buf.h, data)
  doc.images = doc.images or {}
  -- the same name with other pixels (should never happen): a new name
  local base, n = key, 1
  while doc.images[key] and doc.images[key].data ~= data do
    n = n + 1
    key = base .. "_" .. n
  end
  if not doc.images[key] then doc.images[key] = { w = buf.w, h = buf.h, data = data } end
  return key
end

-- decoded images, by their data string (undo copies share the strings);
-- a few at a time, each can be megabytes of Lua table
local decodedByData, decodedN = {}, 0

--- The stored image as a buffer (decoded once), or nil.
function M.bufferOf(doc, key)
  local e = key and doc.images and doc.images[key]
  if not e then return nil end
  local b = decodedByData[e.data]
  if not b then
    b = M.decode(e.data, e.w, e.h)
    if not b then return nil end
    decodedN = decodedN + 1
    if decodedN > 8 then decodedByData, decodedN = {}, 1 end
    decodedByData[e.data] = b
  end
  return b
end

--- The keys any clip of the document uses (the rest are not saved).
function M.usedKeys(doc)
  local used = {}
  for _, anim in ipairs(doc.clips or { doc.anim }) do
    local ref = anim.ref
    if ref then
      if ref.image then used[ref.image] = true end
      for _, k in pairs(ref.frames) do used[k] = true end
    end
  end
  return used
end

---------------------------------------
-- Drawing
---------------------------------------

-- by the source buffer itself (weak: a buffer that goes away takes its
-- results with it; an address reused by another buffer is another key)
local transformed = setmetatable({}, { __mode = "k" })

--- `buf` flipped, turned by `deg` and faded to `opacity`, nearest pixel;
--- the result is as big as the turned image's box. Cached.
function M.transformBuffer(buf, deg, flip, opacity)
  deg = floor((deg or 0) * 2 + 0.5) / 2
  opacity = floor((opacity or 1) * 50 + 0.5) / 50
  local key = deg .. "|" .. (flip and 1 or 0) .. "|" .. opacity
  local mine = transformed[buf]
  if not mine then
    mine = { n = 0 }
    transformed[buf] = mine
  end
  local hit = mine[key]
  if hit then return hit end
  local w, h, s = buf.w, buf.h, buf.d
  local a = rad(deg % 360)
  local c, sn = cos(a), sin(a)
  local ow = max(1, ceil(abs(w * c) + abs(h * sn) - 1e-9))
  local oh = max(1, ceil(abs(w * sn) + abs(h * c) - 1e-9))
  local d = {}
  local cx, cy = w / 2, h / 2
  local ocx, ocy = ow / 2, oh / 2
  for oy = 0, oh - 1 do
    for ox = 0, ow - 1 do
      -- back to the source: undo the turn around the centres
      local dx, dy = ox + 0.5 - ocx, oy + 0.5 - ocy
      local sx = floor(dx * c + dy * sn + cx)
      local sy = floor(-dx * sn + dy * c + cy)
      local o = (oy * ow + ox) * 4 + 1
      if sx >= 0 and sy >= 0 and sx < w and sy < h then
        if flip then sx = w - 1 - sx end
        local i = (sy * w + sx) * 4 + 1
        d[o], d[o + 1], d[o + 2] = s[i], s[i + 1], s[i + 2]
        d[o + 3] = floor(s[i + 3] * opacity + 0.5)
      else
        d[o], d[o + 1], d[o + 2], d[o + 3] = 0, 0, 0, 0
      end
    end
  end
  local out = { w = ow, h = oh, d = d }
  mine.n = mine.n + 1
  if mine.n > 16 then
    -- a drag through many angles: keep only the latest ones
    mine = { n = 1 }
    transformed[buf] = mine
  end
  mine[key] = out
  return out
end

--- What to draw for the clip at `frame` (nil = Rest): the transformed
--- buffer and its box on the canvas { buf, x, y, w, h }, or nil.
function M.draw(doc, anim, frame)
  local ref = anim.ref
  if not ref or not ref.on then return nil end
  local src = M.bufferOf(doc, M.imageAt(anim, frame))
  if not src then return nil end
  local tf = M.transform(anim, frame)
  if tf.ro <= 0 then return nil end
  local buf = M.transformBuffer(src, tf.rr, tf.rf == 1, tf.ro)
  local w, h = buf.w * tf.rs, buf.h * tf.rs
  return { buf = buf, x = tf.rx - w / 2, y = tf.ry - h / 2, w = w, h = h, tf = tf }
end

--- The scale that fits an image of w x h inside a canvas of cw x ch.
function M.fitScale(w, h, cw, ch)
  return min(cw / w, ch / h)
end

return M
