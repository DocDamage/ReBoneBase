-- library.lua — the Poses and Library tabs of the Bonebase window (pure, no
-- Aseprite API), and the grid of square previews both use.
--
-- POSES (doc.poses, engine/poses.lua): the rig's own pose library. A pose
-- is what a frame shows; "Add frame" keeps the current one, "Apply" (or a
-- double click) keys it on the current frame, mirrored or only on the
-- selected bones if asked. The poses travel with the rig: in the sprite and
-- in the rig library.
--
-- LIBRARY: rigs saved on disk with their clips and poses, for any sprite.
-- The host gives `library = { list() -> { { name, ... } }, thumb(e) ->
-- buffer | nil, save(name, doc, thumb) -> ok, why | text written,
-- load(e) -> doc | nil, why, text, text(name) -> the entry's file text |
-- nil, remove(e) -> ok, why, openFolder?() -> ok, why }` (ui.lua keeps the
-- files in Aseprite's user folder); without it the tab says so.
-- A rig remembers the entry it was saved to or loaded from (doc.libName)
-- and the stamp of that file: "Save rig" under the panel writes it back,
-- asking only when the file on disk is no longer the one it knew.
--
-- The GRID widget (kind "grid"): square tiles `spec.cell` logical px wide
-- with a name below, `spec.rows` rows shown, as many columns as fit; the
-- wheel scrolls rows. `spec.items()` -> { { label, ... } }, `spec.thumb(item,
-- px)` -> buffer | nil (px = the tile's size on screen). Its value is the
-- selected index; a double click or Enter is an "activate" action.
--
-- Each spec of these tabs carries its own get / set / enabled / visible /
-- onAction: the view's adapter asks the spec first.

local M = {}

local floor, max, min, ceil = math.floor, math.max, math.min, math.ceil

M.CELL = 64           -- tile size, logical px
M.THUMB = 128         -- size of the thumbnail saved with a library rig, px
M.EMPTY_POSES = "No poses yet: Add frame keeps this one"
M.EMPTY_LIB = "No rigs saved yet"
M.DEFAULT_NAME = "My rig"

---------------------------------------
-- Grid widget --
---------------------------------------

local Grid = {}
M.Grid = Grid

local function itemsOf(spec)
  local it = spec.items
  if type(it) == "function" then it = it(spec) end
  return type(it) == "table" and it or {}
end

local function fontH(ctx)
  if ctx.text then return ctx.text:height() end
  return ctx.theme.fontH
end

local function metrics(spec, ctx)
  local th = ctx.theme
  local s = th.uiScale or 1
  local tile = (spec.cell or M.CELL) * s
  local labelH = fontH(ctx) + 4 * s
  return tile, labelH, th.gap
end

function Grid.measure(spec, ctx)
  local tile, labelH, gap = metrics(spec, ctx)
  return (spec.rows or 2) * (tile + labelH + gap) + gap
end

local function cols(spec, r, ctx)
  local tile, _, gap = metrics(spec, ctx)
  return max(1, floor((r.w - gap) / (tile + gap)))
end

local function maxScroll(spec, n, c)
  return max(0, ceil(n / c) - (spec.rows or 2))
end

local function clamp(spec, n, c)
  local sc = min(max(0, spec._scroll or 0), maxScroll(spec, n, c))
  spec._scroll = sc
  return sc
end

--- The tile rect of item `i` (absolute index), or nil when scrolled out.
local function tileRect(spec, r, i, ctx)
  local tile, labelH, gap = metrics(spec, ctx)
  local c = cols(spec, r, ctx)
  local row = floor((i - 1) / c) - (spec._scroll or 0)
  if row < 0 or row >= (spec.rows or 2) then return nil end
  local used = c * (tile + gap) - gap
  local x0 = r.x + floor((r.w - used) / 2)
  local col = (i - 1) % c
  return x0 + col * (tile + gap), r.y + gap + row * (tile + labelH + gap), tile, labelH
end

function Grid.paint(spec, r, value, st, ctx)
  local th = ctx.theme
  local ops = {}
  ops[#ops + 1] = { "rect", r.x, r.y, r.w, r.h, th.box[1], th.box[2], th.box[3] }
  local fc = st.focused and th.focus or th.frame
  ops[#ops + 1] = { "frame", r.x, r.y, r.w, r.h, fc[1], fc[2], fc[3] }
  local items = itemsOf(spec)
  local c = cols(spec, r, ctx)
  spec._cols = c
  spec._n = #items
  local sc = clamp(spec, #items, c)
  local fh = fontH(ctx)
  if #items == 0 then
    local str = spec.empty or ""
    if ctx.text then str = ctx.text:ellipsize(str, r.w - 8) end
    local tw = ctx.text and ctx.text:width(str) or #str * 6
    ops[#ops + 1] = { "text", r.x + floor((r.w - tw) / 2), r.y + floor((r.h - fh) / 2),
      th.textDim[1], th.textDim[2], th.textDim[3], str }
    return ops
  end
  local first = sc * c + 1
  local last = min(#items, first + c * (spec.rows or 2) - 1)
  for i = first, last do
    local x, y, tile, labelH = tileRect(spec, r, i, ctx)
    if x then
      local item = items[i]
      local sel = i == value
      local bg = sel and th.boxHot or th.panel
      ops[#ops + 1] = { "rect", x, y, tile, tile, bg[1], bg[2], bg[3] }
      local buf = spec.thumb and spec.thumb(item, tile)
      if buf and buf.w > 0 and buf.h > 0 then
        ops[#ops + 1] = { "image", x, y, tile, tile, buf }
      end
      local fr = sel and th.accent or th.frame
      ops[#ops + 1] = { "frame", x, y, tile, tile, fr[1], fr[2], fr[3] }
      if sel then ops[#ops + 1] = { "frame", x + 1, y + 1, tile - 2, tile - 2, fr[1], fr[2], fr[3] } end
      local str = tostring(item.label or "")
      if ctx.text then str = ctx.text:ellipsize(str, tile + 2) end
      local tw = ctx.text and ctx.text:width(str) or #str * 6
      local tc = sel and th.textValue or th.textDim
      ops[#ops + 1] = { "text", x + floor((tile - tw) / 2), y + tile + floor((labelH - fh) / 2),
        tc[1], tc[2], tc[3], str }
    end
  end
  -- a thin bar on the right when there are more rows than shown
  local ms = maxScroll(spec, #items, c)
  if ms > 0 then
    local s = th.uiScale or 1
    local track = r.h - 4 * s
    local total = ceil(#items / c)
    local h = max(4 * s, floor(track * (spec.rows or 2) / total + 0.5))
    local y = r.y + 2 * s + floor((track - h) * sc / ms + 0.5)
    ops[#ops + 1] = { "rect", r.x + r.w - 3 * s, y, 2 * s, h, th.accent[1], th.accent[2], th.accent[3] }
  end
  return ops
end

--- The item index under (x, y), or nil.
function Grid.hit(spec, r, x, y, ctx)
  local items = itemsOf(spec)
  for i = (spec._scroll or 0) * cols(spec, r, ctx) + 1, #items do
    local tx, ty, tile, labelH = tileRect(spec, r, i, ctx)
    if not tx then break end
    if x >= tx and x < tx + tile and y >= ty and y < ty + tile + labelH then return i end
  end
  return nil
end

function Grid.down(spec, r, x, y, value, ctx)
  local i = Grid.hit(spec, r, x, y, ctx)
  if i and i ~= value then return { type = "set", value = i } end
  return nil
end

function Grid.dblclick(spec, r, x, y, value, ctx)
  local i = Grid.hit(spec, r, x, y, ctx)
  if i then return { type = "activate", index = i } end
  return nil
end

Grid.scrollable = true

function Grid.canScroll(spec, dy)
  local sc = spec._scroll or 0
  if dy > 0 then return sc < maxScroll(spec, spec._n or 0, spec._cols or 1) end
  return sc > 0
end

function Grid.wheel(spec, r, dy, value, ctx)
  spec._scroll = (spec._scroll or 0) + ((dy > 0) and 1 or -1)
  clamp(spec, #itemsOf(spec), cols(spec, r, ctx))
  return nil
end

function Grid.key(spec, key, value, ctx, mods)
  local n = #itemsOf(spec)
  if n == 0 then return nil end
  local c = spec._cols or 1
  local v = tonumber(value) or 0
  if key == "Enter" then
    if value then return { type = "activate", index = value } end
    return nil
  end
  local step = ({ Left = -1, Right = 1, Up = -c, Down = c })[key]
  if not step then return nil end
  v = v == 0 and 1 or min(n, max(1, v + step))
  -- keep it in view
  local row = floor((v - 1) / c)
  local sc = spec._scroll or 0
  if row < sc then sc = row end
  if row >= sc + (spec.rows or 2) then sc = row - (spec.rows or 2) + 1 end
  spec._scroll = sc
  return v
end

--- Folds the grid into kit/ui's widget registry (once).
function M.extend(W)
  if W.grid then return W end
  W.grid = Grid
  W.kinds[#W.kinds + 1] = "grid"
  W.focusable.grid = true
  return W
end

---------------------------------------
-- Thumbnails --
---------------------------------------

--- A cache that keeps only what the last paints asked for: `get(key,
--- make)` returns the cached buffer or makes it; `sweep()` (once per
--- paint) forgets what was not asked since the previous sweep.
function M.cache()
  local live, used = {}, {}
  return {
    get = function(key, make)
      local b = used[key] or live[key]
      if b == nil then b = make() or false end
      used[key] = b
      return b or nil
    end,
    sweep = function()
      live, used = used, {}
    end,
  }
end

--- What a thumbnail of `doc` depends on besides the pose: its bones,
--- groups and controllers (not the keys).
function M.signature(doc)
  local out = { doc.canvas.w, doc.canvas.h }
  for _, b in ipairs(doc.bones) do
    out[#out + 1] = table.concat({ b.id, b.parent, tostring(b.connected), b.x, b.y, b.len, b.angle,
      b.rHead, b.rTail, b.part }, ",")
    for si, side in ipairs({ b.bulgeL, b.bulgeR }) do
      out[#out + 1] = si == 1 and "L" or "R"
      for _, k in ipairs(side) do out[#out + 1] = k[1] .. ":" .. k[2] end
    end
  end
  for _, p in ipairs(doc.parts) do
    out[#out + 1] = table.concat({ p.id, p.color[1], p.color[2], p.color[3], p.z, p.blend }, ",")
  end
  for _, c in ipairs(doc.ik) do
    out[#out + 1] = table.concat({ c.id, tostring(c.on), tostring(c.stretch), c.maxStretch,
      tostring(c.lockTip) }, ",")
  end
  local g = doc.ground
  out[#out + 1] = tostring(g.on) .. tostring(g.stop) .. g.y
  return table.concat(out, ";")
end

local function fmt(v) return string.format("%.4f", v) end

--- What a pose's thumbnail depends on: every value it keys.
function M.poseKey(pose)
  local out = {}
  for _, e in ipairs(pose.bones) do
    out[#out + 1] = table.concat({ e.id, fmt(e.rot), fmt(e.x), fmt(e.y), fmt(e.s), e.fk and fmt(e.fk) or "",
      e.fks and fmt(e.fks) or "" }, ",")
  end
  for _, e in ipairs(pose.ik) do
    out[#out + 1] = table.concat({ "i", e.id, fmt(e.tx), fmt(e.ty), fmt(e.px), fmt(e.py) }, ",")
  end
  for _, e in ipairs(pose.parts) do
    out[#out + 1] = table.concat({ "p", e.id, e.z or "", e.vis or "" }, ",")
  end
  return table.concat(out, ";")
end

--- The view's pose thumbnail of `pose` at `px` (cached by rig and pose).
function M.poseThumb(view, pose, px)
  local E = view.E
  if view._sigRev ~= E.rev then
    view._sig, view._sigRev = M.signature(E.doc), E.rev
  end
  -- the pose's values as a string, worked out once per `bones` table
  -- (Update and undo give new tables; the old ones are never changed)
  view.poseKeys = view.poseKeys or setmetatable({}, { __mode = "k" })
  local pk = view.poseKeys[pose.bones]
  if not pk then
    pk = M.poseKey(pose)
    view.poseKeys[pose.bones] = pk
  end
  local key = view._sig .. "|" .. pk .. "|" .. px
  -- while a drag edits the rig every move would redraw them all: the last
  -- ones stay until the button is let go
  view.lastThumbs = view.lastThumbs or {}
  local last = view.lastThumbs[pose.id]
  if E.drag and last and last.px == px then return last.buf end
  local buf = view.thumbs.get(key, function()
    local p, world = view.Poses.solve(E.doc, pose)
    return view.Thumb.render(view.Skin, E.doc, world, p, px)
  end)
  view.lastThumbs[pose.id] = { px = px, buf = buf }
  return buf
end

--- The rig as the viewport shows it now, for the library.
function M.rigThumb(view)
  local E = view.E
  return view.Thumb.render(view.Skin, E.doc, E:world(), E:pose(), M.THUMB)
end

---------------------------------------
-- Poses tab --
---------------------------------------

function M.selectedPose(view)
  local P = view.Poses
  return view.poseSel and P and P.find(view.E.doc, view.poseSel) or nil
end

--- The selected pose's place in the library, or nil.
function M.poseIndex(view)
  if not view.poseSel then return nil end
  local _, i = view.Poses.find(view.E.doc, view.poseSel)
  return i
end

--- Keys pose `id` on the current frame (Pose mode) and says so.
function M.applyPose(view, id, mirrored)
  local E = view.E
  local pose = view.Poses.find(E.doc, id)
  if not pose then return false end
  view:setMode("Pose")
  local n, d = E:applyPose(id, { mirrored = mirrored, selected = view.poseOnlySel })
  if n == 0 and (d or 0) == 0 then
    view:setNotice(view.poseOnlySel and "None of the selected bones is in this pose" or
      "Nothing in this pose fits the rig", true)
  else
    view:setNotice(string.format("%s%s on frame %d (%d %s)", pose.name, mirrored and " mirrored" or "",
      E.frame, n, n == 1 and "bone" or "bones"))
  end
  return true
end

function M.poseSpecs(view)
  local E = view.E
  local P = view.Poses
  local function sel() return M.selectedPose(view) end
  local function hasSel() return sel() ~= nil end
  return {
    { id = "poseSep", kind = "separator", text = "Poses of this rig" },
    { id = "poseGrid", kind = "grid", rows = 3, cell = M.CELL, empty = M.EMPTY_POSES,
      items = function()
        local out = {}
        for i, p in ipairs(E.doc.poses) do out[i] = { label = p.name, pose = p } end
        return out
      end,
      thumb = function(item, px) return M.poseThumb(view, item.pose, px) end,
      get = function() return M.poseIndex(view) end,
      set = function(_, i)
        local p = E.doc.poses[i]
        view.poseSel = p and p.id or nil
      end,
      onAction = function(_, a)
        local p = a.type == "activate" and E.doc.poses[a.index]
        if p then
          view.poseSel = p.id
          M.applyPose(view, p.id, false)
        end
      end },
    { id = "poseName", kind = "textField", label = "Name", maxLen = view.Rig.MAX_NAME,
      visible = hasSel,
      get = function() local p = sel() return p and p.name or "" end,
      set = function(_, v) local p = sel() if p then E:renamePose(p.id, v) end end },
    { id = "poseAdd", kind = "button", text = "Add frame",
      enabled = function() return #E.doc.bones > 0 and #E.doc.poses < view.Rig.MAX_POSES end,
      onPress = function()
        -- what gets kept is what Pose mode shows at this frame
        view:setMode("Pose")
        local p = E:addPose()
        if p then
          view.poseSel = p.id
          view:setNotice(string.format("Frame %d kept as %s", E.frame, p.name))
        end
      end },
    { id = "poseUpdate", kind = "button", text = "Update", row = "same", enabled = hasSel,
      onPress = function()
        local p = sel()
        view:setMode("Pose")
        if p and E:updatePose(p.id) then view:setNotice(p.name .. " now holds frame " .. E.frame) end
      end },
    { id = "poseApply", kind = "button", text = "Apply to frame", enabled = hasSel,
      onPress = function() local p = sel() if p then M.applyPose(view, p.id, false) end end },
    { id = "poseMirror", kind = "button", text = "Apply mirrored", row = "same", enabled = hasSel,
      onPress = function() local p = sel() if p then M.applyPose(view, p.id, true) end end },
    { id = "poseOnlySel", kind = "switch", label = "Only selected bones",
      get = function() return view.poseOnlySel == true end,
      set = function(_, v) view.poseOnlySel = v == true end },
    { id = "poseUp", kind = "button", text = "Move left", enabled = function()
        local i = M.poseIndex(view)
        return i ~= nil and i > 1
      end, onPress = function() if view.poseSel then E:movePose(view.poseSel, -1) end end },
    { id = "poseDown", kind = "button", text = "Move right", row = "same", enabled = function()
        local i = M.poseIndex(view)
        return i ~= nil and i < #E.doc.poses
      end, onPress = function() if view.poseSel then E:movePose(view.poseSel, 1) end end },
    { id = "poseDelete", kind = "button", text = "Delete pose", enabled = hasSel,
      onPress = function()
        local p = sel()
        if p and E:removePose(p.id) then
          view.poseSel = nil
          view:setNotice(p.name .. " deleted (Ctrl+Z brings it back)")
        end
      end },
    { id = "poseHint", kind = "label", dim = true,
      text = "Double-click a pose to key it on this frame" },
  }
end

---------------------------------------
-- Library tab --
---------------------------------------

--- Reads the library list again, with every thumbnail. The files are
--- read HERE, from a click or a tab change, never while painting: reading
--- an image from disk inside the canvas' paint while a clip plays (its
--- Timer firing) hung Aseprite (1.9.0).
function M.refresh(view)
  local lib = view.library
  view.libItems = {}
  view.libThumbs = {}
  if not lib then return end
  local ok, list = pcall(lib.list)
  if ok and type(list) == "table" then view.libItems = list end
  for _, e in ipairs(view.libItems) do
    local tok, buf = pcall(lib.thumb, e)
    view.libThumbs[e.name] = tok and buf or false
  end
  local still
  for _, e in ipairs(view.libItems) do
    if e.name == view.libSel then still = true end
  end
  if not still then view.libSel = nil end
end

local function libEntry(view)
  for i, e in ipairs(view.libItems or {}) do
    if e.name == view.libSel then return e, i end
  end
  return nil
end
M.libEntry = libEntry

--- A file name from what the user typed: no path characters, trimmed.
function M.cleanName(name)
  name = tostring(name or ""):gsub('[\\/:*?"<>|%c]', ""):gsub("^[%s%.]+", ""):gsub("[%s%.]+$", "")
  name = name:sub(1, 48)
  -- names Windows keeps for devices (CON, NUL, COM1...) cannot be files
  local up = name:upper():match("^([^%.]*)")
  if up == "CON" or up == "PRN" or up == "AUX" or up == "NUL" or up:match("^COM%d$") or up:match("^LPT%d$") then
    name = name .. "_"
  end
  return name
end

--- A short fingerprint of an entry's file text: tells the file this rig
--- saved or loaded from one written later (by another sprite, or by
--- another rig of this undo history saved under the same name).
function M.stamp(text)
  if type(text) ~= "string" then return nil end
  local h, n = 0, #text
  for i = 1, n, 256 do
    local b = { text:byte(i, math.min(n, i + 255)) }
    for j = 1, #b do h = (h * 31 + b[j]) % 4294967296 end
  end
  return n .. ":" .. string.format("%08x", h)
end

--- The factory rigs (shipped in the extension's rigs/) to copy into the
--- user's library: each name of `factory` that was never seeded and has
--- no library entry yet (case aside). `seeded` is the set of names seeded
--- before, kept in the preferences, so a factory rig the user deleted
--- never comes back. Returns the names to copy and the new seeded set
--- (a name the user already had counts as seeded: theirs wins).
function M.toSeed(factory, entries, seeded)
  local have, done, copy = {}, {}, {}
  for _, e in ipairs(entries or {}) do have[e.name:lower()] = true end
  for name in pairs(seeded or {}) do done[name] = true end
  for _, name in ipairs(factory or {}) do
    if not done[name] then
      if not have[name:lower()] then copy[#copy + 1] = name end
      done[name] = true
    end
  end
  return copy, done
end

--- The stamp of entry `name` on disk now (nil: there is none).
local function stampNow(view, name)
  local lib = view.library
  if not lib.text then return nil end
  local ok, text = pcall(lib.text, name)
  return ok and M.stamp(text) or nil
end

--- Writes the rig to entry `name` and ties it to that entry.
local function write(view, name)
  local E, doc = view.E, view.E.doc
  -- the file names its own entry too
  local oldName, oldStamp, oldChanged = doc.libName, doc.libStamp, doc.libChanged
  E:linkLibrary(name, oldStamp)
  local ok, res = view.library.save(name, doc, M.rigThumb(view))
  if not ok then
    E:linkLibrary(oldName, oldStamp)
    doc.libChanged = oldChanged
    view:setNotice("Could not save " .. name .. ": " .. tostring(res or "?"), true)
    return true
  end
  E:linkLibrary(name, M.stamp(res) or stampNow(view, name))
  M.refresh(view)
  view.libSel = name
  view:setNotice(name .. " saved in the rig library")
  return true
end

--- Saves the current rig as `view.libName`: asks before replacing an entry
--- that is not this rig's own.
function M.save(view)
  local E = view.E
  local name = M.cleanName(view.libName or E.doc.libName or M.DEFAULT_NAME)
  if name == "" then view:setNotice("Give the rig a name first", true) return true end
  if #E.doc.bones == 0 then view:setNotice("Nothing to save: the rig is empty", true) return true end
  if not view.libItems then M.refresh(view) end
  -- this rig's own entry, as it saved or loaded it (nobody wrote it since)
  local own = (E.doc.libName or ""):lower() == name:lower() and stampNow(view, name) == E.doc.libStamp
  for _, e in ipairs(view.libItems) do
    if e.name:lower() == name:lower() and not own and view.confirm then
      view.confirm("Replace " .. e.name .. " in the rig library?", function() return write(view, e.name) end)
      return true
    end
  end
  return write(view, name)
end

--- The always-there Save: the rig goes back to its library entry, or, when
--- it has none yet, the Library tab opens to give it a name. It asks first
--- when the entry is not the file this rig saved or loaded: written since
--- (another sprite, another rig saved under that name, a link the sprite
--- brought from someone else's library) or deleted.
function M.saveLinked(view)
  local E = view.E
  if #E.doc.bones == 0 then view:setNotice("Nothing to save: the rig is empty", true) return true end
  local name = E.doc.libName and M.cleanName(E.doc.libName) or ""
  if name == "" then
    view:setTab("Library")
    view:setNotice("Name the rig and press Save to library")
    return true
  end
  local now = stampNow(view, name)
  if now ~= E.doc.libStamp and view.confirm then
    local text = now and name .. " changed in the rig library since this rig was saved or loaded. Save over it?"
      or name .. " is no longer in the rig library. Save it there again?"
    view.confirm(text, function() return write(view, name) end)
    return true
  end
  return write(view, name)
end

--- Replaces the current rig with library entry `e` (one undo step), on
--- the sprite as it is: bottom centre on bottom centre.
function M.load(view, e)
  local lib, E = view.library, view.E
  e = e or libEntry(view)
  if not e then return false end
  local function apply()
    local doc, why, text = lib.load(e)
    if not doc then
      view:setNotice("Could not load " .. e.name .. ": " .. tostring(why or "?"), true)
      return true
    end
    view.Rig.fitInto(doc, E.doc)
    if view.playing then view:togglePlay() end
    view:setMode("Rest")
    E:loadDoc(doc)
    -- the entry it came from, whatever name the file says
    E:linkLibrary(e.name, M.stamp(text) or stampNow(view, e.name))
    view.libName = e.name
    view.partSel, view.poseSel = nil, nil
    view:fit()
    view:setNotice(e.name .. " loaded (Ctrl+Z brings the previous rig back)")
    return true
  end
  if #E.doc.bones > 0 and view.confirm then
    view.confirm("Replace the current rig with " .. e.name .. "?", apply)
    return true
  end
  return apply()
end

function M.remove(view)
  local lib = view.library
  local e = libEntry(view)
  if not e then return false end
  local function go()
    local ok, why = lib.remove(e)
    if not ok then view:setNotice("Could not delete " .. e.name .. ": " .. tostring(why or "?"), true) end
    -- this rig is no longer kept anywhere
    if ok and (view.E.doc.libName or ""):lower() == e.name:lower() then view.E:linkLibrary(nil) end
    M.refresh(view)
    if ok then view:setNotice(e.name .. " deleted from the rig library") end
    return true
  end
  if view.confirm then
    view.confirm("Delete " .. e.name .. " from the rig library? This cannot be undone.", go)
    return true
  end
  return go()
end

function M.librarySpecs(view)
  if not view.libItems then M.refresh(view) end
  local function has() return view.library ~= nil end
  local function hasSel() return libEntry(view) ~= nil end
  if not view.library then
    return {
      { id = "libSep", kind = "separator", text = "Rig library" },
      { id = "libNone", kind = "label", dim = true, text = "The rig library needs Aseprite" },
    }
  end
  return {
    { id = "libSep", kind = "separator", text = "Rig library" },
    { id = "libGrid", kind = "grid", rows = 3, cell = M.CELL, empty = M.EMPTY_LIB,
      items = function()
        local out = {}
        for i, e in ipairs(view.libItems or {}) do out[i] = { label = e.name, entry = e } end
        return out
      end,
      -- painting only scales what refresh read (no file access here)
      thumb = function(item, px)
        local key = item.entry.name .. "|" .. px
        local t = view.libThumbs[key]
        if t == nil then
          local buf = view.libThumbs[item.entry.name]
          t = buf and view.Thumb.fit(buf, px) or false
          view.libThumbs[key] = t
        end
        return t or nil
      end,
      get = function() local _, i = libEntry(view) return i end,
      set = function(_, i)
        local e = view.libItems[i]
        view.libSel = e and e.name or nil
        if e then view.libName = e.name end
      end,
      onAction = function(_, a)
        local e = a.type == "activate" and view.libItems[a.index]
        if e then
          view.libSel = e.name
          M.load(view, e)
        end
      end },
    { id = "libLoad", kind = "button", text = "Load", enabled = hasSel,
      onPress = function() M.load(view) end },
    { id = "libDelete", kind = "button", text = "Delete", row = "same", enabled = hasSel,
      onPress = function() M.remove(view) end },
    { id = "libSaveSep", kind = "separator", text = "Save this rig" },
    { id = "libLinked", kind = "label", dim = true, text = "", labelOf = function()
        local name = view.E.doc.libName
        if not name then return "Not in the library yet" end
        return "Kept as " .. name .. (view.E:libDirty() and ", with unsaved changes" or "")
      end },
    { id = "libName", kind = "textField", label = "Name", maxLen = 48,
      get = function() return view.libName or view.E.doc.libName or M.DEFAULT_NAME end,
      set = function(_, v) view.libName = tostring(v or "") end },
    { id = "libSave", kind = "button", text = "Save to library", enabled = function()
        return has() and #view.E.doc.bones > 0
      end, onPress = function() M.save(view) end },
    { id = "libHint", kind = "label", dim = true,
      text = "Saved with its clips and poses, for any sprite" },
    { id = "libHint2", kind = "label", dim = true, text = "Then Save rig, below, keeps it up to date" },
    { id = "libRefresh", kind = "button", text = "Refresh",
      onPress = function() M.refresh(view) end },
    { id = "libFolder", kind = "button", text = "Open folder", row = "same",
      visible = function() return view.library.openFolder ~= nil end,
      onPress = function()
        local ok, why = view.library.openFolder()
        if not ok then view:setNotice("Could not open the folder: " .. tostring(why or "?"), true) end
      end },
  }
end

return M
