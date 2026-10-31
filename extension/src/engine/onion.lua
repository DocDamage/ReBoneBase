-- onion.lua — the onion skin of Pose mode: which frames around the current
-- one show, and how. Pure Lua, dual-VM.
--
-- Settings (`sanitize` fills and clamps them; the view keeps them in the
-- plugin preferences, so they last between sessions):
--   before, after  how many frames back and forward (0..MAX)
--   step           every how many frames (1..MAX): 2 shows f-2, f-4...
--   keys           the nearest KEYED frames of the clip instead (step
--                  does not apply)
--   wrap           a looping clip goes on from its other end
--   style          "Tint" silhouettes in one colour, "Outline" only their
--                  edge, "Skin" the groups' own colours, "Bones" the
--                  skeleton
--   colorBefore, colorAfter   { r, g, b } of the frames back / forward
--   opacity        % of the nearest frame; each further one keeps
--   fade           % less than the one before it
--   front          drawn over the rig instead of under it
--
-- `frames(anim, cur, o)` -> { { f, dir (-1 back, 1 forward), k (1 =
-- nearest) } }; `render(env, doc, list, o, cache?)` -> canvas-sized buffer
-- or nil, env = { Skin, Rig, Anim, Ik? }. `cache` (newCache) keeps what
-- each frame covers between calls: playback and a new opacity, colour or
-- frame count only composite again. The caller drops it when the rig or
-- the style changes.

local M = {}

M.MAX = 8
M.STYLES = { "Tint", "Outline", "Skin", "Bones" }

M.DEFAULTS = {
  before = 1, after = 1, step = 1, keys = false, wrap = true, style = "Tint",
  colorBefore = { 255, 110, 110 }, colorAfter = { 110, 190, 255 },
  opacity = 43, fade = 50, front = false,
}

local floor, max, min, abs = math.floor, math.max, math.min, math.abs

local function int(v, lo, hi, def)
  v = tonumber(v)
  if not v or v ~= v then return def end
  return max(lo, min(hi, floor(v + 0.5)))
end

local function color(v, def)
  if type(v) ~= "table" then return { def[1], def[2], def[3] } end
  local out = {}
  for i = 1, 3 do out[i] = int(v[i] or v[({ "r", "g", "b" })[i]], 0, 255, def[i]) end
  return out
end

--- A full, valid settings table from whatever `t` holds (nil included).
function M.sanitize(t)
  t = type(t) == "table" and t or {}
  local D = M.DEFAULTS
  local style = D.style
  for _, s in ipairs(M.STYLES) do
    if t.style == s then style = s end
  end
  return {
    before = int(t.before, 0, M.MAX, D.before),
    after = int(t.after, 0, M.MAX, D.after),
    step = int(t.step, 1, M.MAX, D.step),
    keys = t.keys == true,
    wrap = t.wrap ~= false,
    style = style,
    colorBefore = color(t.colorBefore, D.colorBefore),
    colorAfter = color(t.colorAfter, D.colorAfter),
    opacity = int(t.opacity, 5, 100, D.opacity),
    fade = int(t.fade, 0, 90, D.fade),
    front = t.front == true,
  }
end

--- A string that changes whenever a setting does (render caches by it).
function M.key(o)
  local cb, ca = o.colorBefore, o.colorAfter
  return table.concat({ o.before, o.after, o.step, tostring(o.keys), tostring(o.wrap), o.style,
    cb[1], cb[2], cb[3], ca[1], ca[2], ca[3], o.opacity, o.fade }, ",")
end

--- The frames to show around `cur` of `anim`, nearest first per side. A
--- frame shows once, on the side where it is nearer (short looping clips
--- reach the same frame both ways).
function M.frames(anim, cur, o)
  local n = anim.frames
  -- a ping-pong clip turns back at its ends instead of going round
  local wrap = o.wrap and anim.loop and not anim.pingpong and n > 1
  local out, seen = {}, { [cur] = true }
  local function add(f, dir, k)
    if f and not seen[f] then
      seen[f] = true
      out[#out + 1] = { f = f, dir = dir, k = k }
    end
  end
  if o.keys then
    -- the keyed frames in the order a side meets them
    local kf = M.poseKeys(anim)
    local back, fwd = {}, {}
    for i = #kf, 1, -1 do if kf[i] < cur then back[#back + 1] = kf[i] end end
    for i = 1, #kf do if kf[i] > cur then fwd[#fwd + 1] = kf[i] end end
    if wrap then
      for i = #kf, 1, -1 do if kf[i] > cur then back[#back + 1] = kf[i] end end
      for i = 1, #kf do if kf[i] < cur then fwd[#fwd + 1] = kf[i] end end
    end
    for k = 1, max(o.before, o.after) do
      if k <= o.before then add(back[k], -1, k) end
      if k <= o.after then add(fwd[k], 1, k) end
    end
    return out
  end
  local function at(f)
    if wrap then return (f - 1) % n + 1 end
    if f >= 1 and f <= n then return f end
    return nil
  end
  for k = 1, max(o.before, o.after) do
    -- round a loop once at most
    if wrap and k * o.step >= n then break end
    if k <= o.before then add(at(cur - k * o.step), -1, k) end
    if k <= o.after then add(at(cur + k * o.step), 1, k) end
  end
  return out
end

--- The sorted frames that key the pose: every track but the reference
--- image's (refimg.lua OWNER, -1), which moves no bone.
function M.poseKeys(anim)
  local seen, out = {}, {}
  for _, t in ipairs(anim.tracks) do
    if t.bone ~= -1 then
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

--- Alpha (0..255) of the k-th frame of a side.
function M.alpha(o, k)
  local a = o.opacity * 255 / 100 * (1 - o.fade / 100) ^ (k - 1)
  return max(1, floor(a + 0.5))
end

--- Clips a segment to [0, w) x [0, h) (Liang-Barsky): the part inside,
--- or nil. A bone far off the canvas costs nothing to draw.
local function clip(w, h, x0, y0, x1, y1)
  for _, v in ipairs({ x0, y0, x1, y1 }) do
    if v ~= v or v == math.huge or v == -math.huge then return nil end
  end
  local t0, t1 = 0, 1
  local dx, dy = x1 - x0, y1 - y0
  local p = { -dx, dx, -dy, dy }
  local q = { x0, w - 1 - x0, y0, h - 1 - y0 }
  for i = 1, 4 do
    if p[i] == 0 then
      if q[i] < 0 then return nil end
    else
      local r = q[i] / p[i]
      if p[i] < 0 then
        if r > t1 then return nil end
        if r > t0 then t0 = r end
      else
        if r < t0 then return nil end
        if r < t1 then t1 = r end
      end
    end
  end
  return x0 + t0 * dx, y0 + t0 * dy, x0 + t1 * dx, y0 + t1 * dy
end

--- Marks the pixels of a segment in `m` (canvas w x h, 1-based flat).
local function line(m, w, h, x0, y0, x1, y1)
  x0, y0, x1, y1 = clip(w, h, x0, y0, x1, y1)
  if not x0 then return end
  x0, y0, x1, y1 = floor(x0), floor(y0), floor(x1), floor(y1)
  local dx, dy = abs(x1 - x0), -abs(y1 - y0)
  local sx, sy = x0 < x1 and 1 or -1, y0 < y1 and 1 or -1
  local err = dx + dy
  for _ = 1, dx - dy + 1 do
    if x0 >= 0 and y0 >= 0 and x0 < w and y0 < h then m[y0 * w + x0 + 1] = true end
    if x0 == x1 and y0 == y1 then break end
    local e2 = 2 * err
    if e2 >= dy then err = err + dy x0 = x0 + sx end
    if e2 <= dx then err = err + dx y0 = y0 + sy end
  end
end

--- The pixels a frame covers, as a set of canvas indices (1-based).
local function frameMask(env, doc, world, pose, style, w, h)
  local m = {}
  if style == "Bones" then
    for _, b in ipairs(doc.bones) do
      local wb = world[b.id]
      if wb then line(m, w, h, wb.hx, wb.hy, wb.tx, wb.ty) end
    end
    return m
  end
  for _, L in ipairs(env.Skin.render(doc, world, { outline = "none", pose = pose })) do
    for y = 0, L.bh - 1 do
      for x = 0, L.bw - 1 do
        if L.mask[y * L.bw + x + 1] == 1 then m[(L.y0 + y) * w + (L.x0 + x) + 1] = true end
      end
    end
  end
  if style == "Outline" then
    -- only the pixels next to one the silhouette leaves out
    local edge = {}
    for i in pairs(m) do
      local p = i - 1
      local x, y = p % w, floor(p / w)
      if x == 0 or y == 0 or x == w - 1 or y == h - 1 or not m[i - 1] or not m[i + 1]
        or not m[i - w] or not m[i + w] then
        edge[i] = true
      end
    end
    return edge
  end
  return m
end

local function over(d, i, r, g, b, a)
  local da = d[i + 3]
  if da == 0 then
    d[i], d[i + 1], d[i + 2], d[i + 3] = r, g, b, a
    return
  end
  local sa, ba = a / 255, da / 255
  local oa = sa + ba * (1 - sa)
  d[i] = floor((r * sa + d[i] * ba * (1 - sa)) / oa + 0.5)
  d[i + 1] = floor((g * sa + d[i + 1] * ba * (1 - sa)) / oa + 0.5)
  d[i + 2] = floor((b * sa + d[i + 2] * ba * (1 - sa)) / oa + 0.5)
  d[i + 3] = floor(oa * 255 + 0.5)
end

--- What frame `f` covers in `style`: { idx } (canvas indices, 1-based),
--- plus r, g, b, a lists for the skin.
local function frameData(env, doc, f, style, w, h)
  local pose, world
  if env.Ik then
    pose, world = env.Ik.solve(doc, f)
  else
    pose = env.Anim.sample(doc.anim, f)
    world = env.Rig.solve(doc, pose)
  end
  local out = { idx = {} }
  if style == "Skin" then
    local layers = env.Skin.render(doc, world, { outline = "pixel", pose = pose })
    local s = env.Skin.compose(layers, w, h, { fillAlpha = 255 }).d
    out.r, out.g, out.b, out.a = {}, {}, {}, {}
    local n = 0
    for i = 1, w * h * 4, 4 do
      if s[i + 3] > 0 then
        n = n + 1
        out.idx[n] = (i - 1) / 4 + 1
        out.r[n], out.g[n], out.b[n], out.a[n] = s[i], s[i + 1], s[i + 2], s[i + 3]
      end
    end
    return out
  end
  local n = 0
  for p in pairs(frameMask(env, doc, world, pose, style, w, h)) do
    n = n + 1
    out.idx[n] = p
  end
  return out
end

--- An empty per-frame cache for render.
function M.newCache(tag)
  return { tag = tag, frames = {} }
end

--- The onion skin of `list` (frames()) over `doc`'s canvas: the nearest
--- frames win (a back frame over a forward one as near). Nil when nothing
--- shows.
function M.render(env, doc, list, o, cache)
  local w, h = doc.canvas.w, doc.canvas.h
  local skin = o.style == "Skin"
  local order = {}
  for i, e in ipairs(list) do order[i] = e end
  -- flat colours: nearest first, a pixel taken stays; the skin blends,
  -- so it goes furthest first
  table.sort(order, function(a, b)
    if a.k ~= b.k then
      if skin then return a.k > b.k end
      return a.k < b.k
    end
    if skin then return a.dir > b.dir end
    return a.dir < b.dir
  end)
  local d
  for _, e in ipairs(order) do
    local fd = cache and cache.frames[e.f]
    if not fd then
      fd = frameData(env, doc, e.f, o.style, w, h)
      if cache then cache.frames[e.f] = fd end
    end
    local a = M.alpha(o, e.k)
    if not d then
      d = {}
      for i = 1, w * h * 4 do d[i] = 0 end
    end
    local idx = fd.idx
    if skin then
      for j = 1, #idx do
        over(d, (idx[j] - 1) * 4 + 1, fd.r[j], fd.g[j], fd.b[j], floor(fd.a[j] * a / 255 + 0.5))
      end
    else
      local c = e.dir < 0 and o.colorBefore or o.colorAfter
      for j = 1, #idx do
        local i = (idx[j] - 1) * 4 + 1
        -- flat colours do not pile up: the nearer frame keeps the pixel
        if a > d[i + 3] then d[i], d[i + 1], d[i + 2], d[i + 3] = c[1], c[2], c[3], a end
      end
    end
  end
  if not d then return nil end
  return { w = w, h = h, d = d }
end

function M.bind(Anim)
  M.Anim = Anim
  return M
end

return M
