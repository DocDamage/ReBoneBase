-- dopesheet.lua — the dope sheet under the timeline strip: one row per
-- bone (indented under its parent), per IK controller and per part with
-- z / vis keys, one cell per frame and a mark on every key. A pure model
-- like view.lua: it paints into the view's display list, reads its
-- regions (`regions.sheet`, `regions.names`, and the strip's cells via
-- `V:cell` / `V:frameAt`) and the view routes the input of its region
-- here. Edits go through the editor (editor.lua, "Dope sheet" section),
-- one undo step each.
--
-- Mouse: click a key to pick it (Shift adds it or takes it out); drag
-- picked keys to move them; Alt+drag scales their timing around the first
-- one (around the last when the first is the one grabbed); drag from an
-- empty cell to pick with a box (Shift keeps the pick); click an empty
-- cell to clear the pick and go to that frame; click a name to select
-- that bone. The wheel scrolls the rows.

local M = {}
local floor, max, min = math.floor, math.max, math.min

M.ROW_H = 12       -- logical px
M.NAME_W = 104     -- name column; the Play button sits on top of it
M.DEFAULT_H = 64   -- rows area by default, logical px
M.INDENT = 6       -- per level under the root, logical px
M.MAX_INDENT = 6   -- levels
M.KEY_MARK = 2     -- half size of a key mark, logical px (as the strip)

local function rectOp(ops, x, y, w, h, c, a)
  ops[#ops + 1] = { "rect", floor(x), floor(y), floor(w), floor(h), c[1], c[2], c[3], a }
end

--- The rows: the rig root when it has keys, bones in rig order, then the
--- IK controllers, then the parts with keys.
--- { kind = "root" | "bone" | "ik" | "part" | "ref", id, name, depth, bone? }
function M.rows(V)
  local doc = V.E.doc
  local rows, depth = {}, {}
  for _, t in ipairs(doc.anim.tracks) do
    if t.bone == V.Rig.ROOT then
      rows[1] = { kind = "root", id = V.Rig.ROOT, name = "Root", depth = 0 }
      break
    end
  end
  for _, b in ipairs(doc.bones) do
    local d = b.parent ~= 0 and (depth[b.parent] or 0) + 1 or 0
    depth[b.id] = d
    rows[#rows + 1] = { kind = "bone", id = b.id, name = b.name, depth = d, bone = b.id }
  end
  for _, c in ipairs(doc.ik) do
    local lo = V.Rig.bone(doc, c.lower)
    rows[#rows + 1] = { kind = "ik", id = c.id, name = "IK " .. (lo and lo.name or "?"),
      depth = 0, bone = c.lower }
  end
  local animated = {}
  for _, t in ipairs(doc.anim.tracks) do animated[t.bone] = true end
  if animated[-1] then
    rows[#rows + 1] = { kind = "ref", id = -1, name = "Reference", depth = 0 }
  end
  for _, p in ipairs(doc.parts) do
    if animated[p.id] then
      rows[#rows + 1] = { kind = "part", id = p.id, name = "Group " .. p.name, depth = 0 }
    end
  end
  return rows
end

local function rowH(V)
  return M.ROW_H * V.theme.uiScale
end

--- The scroll clamped to the rows there are.
function M.clampScroll(V, rows)
  local r = V.regions.sheet
  local total = #(rows or M.rows(V)) * rowH(V)
  local most = max(0, total - (r and r.h or 0))
  V.sheetScroll = max(0, min(most, V.sheetScroll or 0))
  return V.sheetScroll
end

--- Row index (may be past the last row) under screen y.
local function rowIndex(V, y)
  local r = V.regions.sheet
  return floor((y - r.y + (V.sheetScroll or 0)) / rowH(V)) + 1
end

--- What is under (x, y): { index, row, f, zone = "name" | "cell" } or nil.
function M.hit(V, x, y, rows)
  local r = V.regions.sheet
  if not r or x < r.x or y < r.y or x >= r.x + r.w or y >= r.y + r.h then return nil end
  rows = rows or M.rows(V)
  local i = rowIndex(V, y)
  local names = V.regions.names
  local zone = x < names.x + names.w and "name" or "cell"
  return { index = i, row = rows[i], f = V:frameAt(x), zone = zone }
end

--- Where a picked key at frame f is drawn while a drag is live.
local function ghostFrame(V, d, f)
  local n = V.E.doc.anim.frames
  if d.kind == "move" then
    -- held inside the clip as one block, like editor.moveSelKeys
    return f + max(1 - d.lo, min(n - d.hi, d.f - d.f0))
  end
  if d.kind == "scale" and d.f0 ~= d.pivot then
    local k = (d.f - d.pivot) / (d.f0 - d.pivot)
    if k <= 0 then return f end
    return max(1, min(n, floor(d.pivot + (f - d.pivot) * k + 0.5)))
  end
  return f
end

function M.paint(V, ops)
  local r = V.regions.sheet
  if not r or r.h <= 0 or r.w <= 0 then return end
  local th = V.theme
  local s = th.uiScale
  local E = V.E
  local anim = E.doc.anim
  local rows = M.rows(V)
  local scroll = M.clampScroll(V, rows)
  local rh = rowH(V)
  local names = V.regions.names
  local strip = V.regions.strip
  ops[#ops + 1] = { "save" }
  ops[#ops + 1] = { "clip", r.x, r.y, r.w, r.h }
  rectOp(ops, r.x, r.y, r.w, r.h, th.box)
  rectOp(ops, names.x, r.y, names.w, r.h, th.panel)
  -- keys per row
  local keys = {}
  for _, t in ipairs(anim.tracks) do
    local m = keys[t.bone]
    if not m then
      m = {}
      keys[t.bone] = m
    end
    for _, k in ipairs(t.keys) do m[k[1]] = true end
  end
  local pose = E.mode == "Pose"
  if pose then
    local x, cw = V:cell(E.frame)
    rectOp(ops, x, r.y, cw, r.h, th.accent, 36)
  end
  local d = V.sheetDrag
  local k = M.KEY_MARK * s
  local first = max(1, floor(scroll / rh) + 1)
  local last = min(#rows, floor((scroll + r.h) / rh) + 1)
  for i = first, last do
    local row = rows[i]
    local y = r.y + (i - 1) * rh - scroll
    local sel = (row.bone and row.kind == "bone" and E:isSelected(row.bone))
      or (row.kind == "part" and V.partSel == row.id)
    if sel then rectOp(ops, r.x, y, r.w, rh, th.rowHot) end
    local ind = min(row.depth, M.MAX_INDENT) * M.INDENT * s
    local tc = sel and th.text or th.textDim
    ops[#ops + 1] = { "text", names.x + th.pad + ind, y + floor((rh - th.fontH) / 2),
      tc[1], tc[2], tc[3], V.text:ellipsize(row.name, max(0, names.w - 2 * th.pad - ind)) }
    local c = row.kind == "ik" and th.ikTarget or row.kind == "part" and th.ikPole
      or row.kind == "ref" and th.ikOff or row.kind == "root" and th.rootHandle or th.keyMark
    local m = keys[row.id]
    if m then
      for f in pairs(m) do
        local picked = V.Anim.setHas(E.keySel, row.id, f)
        local gf = (picked and d and (d.kind == "move" or d.kind == "scale")) and ghostFrame(V, d, f) or f
        local x, cw = V:cell(gf)
        local mx, my = x + floor(cw / 2), y + floor(rh / 2)
        if gf ~= f then
          -- the key's place before the drag, dimmed
          local ox, ocw = V:cell(f)
          rectOp(ops, ox + floor(ocw / 2) - k, my - k, 2 * k + 1, 2 * k + 1, c, 80)
        end
        rectOp(ops, mx - k - s, my - k - s, 2 * k + 1 + 2 * s, 2 * k + 1 + 2 * s, th.overlayBg)
        rectOp(ops, mx - k, my - k, 2 * k + 1, 2 * k + 1, picked and th.accent or c)
      end
    end
  end
  if #rows == 0 then
    ops[#ops + 1] = { "text", strip.x + th.pad, r.y + th.pad, th.textDim[1], th.textDim[2],
      th.textDim[3], "No bones yet" }
  end
  if d and d.kind == "box" and d.moved then
    local fa, fb = min(d.f0, d.f1), max(d.f0, d.f1)
    local ra, rb = min(d.r0, d.r1), max(d.r0, d.r1)
    local x0 = V:cell(fa)
    local x1, cw1 = V:cell(fb)
    local y0 = r.y + (ra - 1) * rh - scroll
    local y1 = r.y + rb * rh - scroll
    ops[#ops + 1] = { "frame", x0, y0, x1 + cw1 - x0, y1 - y0, th.focus[1], th.focus[2], th.focus[3] }
  end
  if not pose then
    -- Rest mode: dimmed like the strip, still live (a click enters Pose)
    rectOp(ops, r.x, r.y, r.w, r.h, th.bg, 110)
  end
  ops[#ops + 1] = { "restore" }
end

function M.down(V, x, y, button, mods)
  mods = mods or {}
  local E = V.E
  local rows = M.rows(V)
  local h = M.hit(V, x, y, rows)
  if not h or button ~= "left" then return false end
  if V.playing then V:togglePlay() end
  if E.mode ~= "Pose" then V:setMode("Pose") end
  if h.zone == "name" then
    local row = h.row
    if not row then return false end
    if row.kind == "part" then
      V.partSel = row.id
      return true
    end
    if row.kind == "ref" then return E:setTool("Ref") end
    if row.kind == "root" then return true end
    if mods.shift and row.kind == "bone" then return E:toggleSelect(row.bone) end
    E.sel = row.bone
    V:followSelection()
    return true
  end
  local row = h.row
  local anim = E.doc.anim
  if row and V.Anim.hasKey(anim, h.f, { [row.id] = true }) then
    if mods.shift then
      E:pickKey(row.id, h.f, "toggle")
      V.sheetDrag = nil
      return true
    end
    if not V.Anim.setHas(E.keySel, row.id, h.f) then E:pickKey(row.id, h.f, "only") end
    local d = { kind = mods.alt and "scale" or "move", f0 = h.f, f = h.f }
    local lo, hi = V.Anim.setRange(E.keySel)
    d.lo, d.hi = lo, hi
    if d.kind == "scale" then d.pivot = (h.f == lo and hi ~= lo) and hi or lo end
    V.sheetDrag = d
    E:setFrame(h.f)
    return true
  end
  V.sheetDrag = { kind = "box", r0 = h.index, r1 = h.index, f0 = h.f, f1 = h.f,
    add = mods.shift == true, moved = false }
  return true
end

function M.move(V, x, y)
  local d = V.sheetDrag
  if not d then return false end
  local f = V:frameAt(x)
  if d.kind == "box" then
    local i = max(1, rowIndex(V, y))
    if i == d.r1 and f == d.f1 then return false end
    d.r1, d.f1 = i, f
    d.moved = true
    return true
  end
  if f == d.f then return false end
  d.f = f
  return true
end

function M.up(V, x, y)
  local d = V.sheetDrag
  V.sheetDrag = nil
  if not d then return false end
  local E = V.E
  if d.kind == "box" then
    if not d.moved then
      if not d.add then E:clearKeySel() end
      E:setFrame(d.f0)
      return true
    end
    local rows = M.rows(V)
    local ids = {}
    for i = max(1, min(d.r0, d.r1)), min(#rows, max(d.r0, d.r1)) do ids[#ids + 1] = rows[i].id end
    return E:pickKeyBox(ids, d.f0, d.f1, d.add)
  end
  if d.f == d.f0 then return true end
  if d.kind == "move" then
    if E:moveSelKeys(d.f - d.f0) then E:setFrame(d.f) end
    return true
  end
  if d.kind == "scale" and d.f0 ~= d.pivot then
    local ok, why = E:scaleSelKeys((d.f - d.pivot) / (d.f0 - d.pivot), d.pivot)
    if not ok and why then
      V:setNotice(why == "overlap" and "Keys of one row would land on the same frame"
        or "The keys would leave the clip", true)
    end
  end
  return true
end

function M.wheel(V, dy)
  V.sheetScroll = (V.sheetScroll or 0) + (dy > 0 and 3 or -3) * rowH(V)
  M.clampScroll(V)
  return true
end

return M
