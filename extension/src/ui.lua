-- ui.lua — the Aseprite bridge of the Bonebase window: one resizeable Dialog with a
-- SINGLE `dlg:canvas` (paint, mouse down/move/up, wheel, key). Everything
-- else lives in src/view.lua (pure window model) and src/editor.lua (pure
-- rig editor), painted through kit/painter_ase.lua.
--
-- One editor (document + undo) per sprite, kept in State.session.editors
-- by `sprite.id` (API objects have no stable identity, ids do): closing
-- and reopening the window keeps each sprite's rig, and a sprite that
-- was exported to brings its rig back from its properties (export.lua).

local M = {}

M.Painter, M.UI, M.View, M.Buffer = nil, nil, nil, nil
M.Rig, M.Skin, M.Anim, M.Editor, M.Templates, M.Export, M.Ik = nil, nil, nil, nil, nil, nil, nil

function M.bind(deps)
  for k, v in pairs(deps) do M[k] = v end
  return M
end

local function alive(dlg)
  if not dlg then return false end
  local ok, b = pcall(function() return dlg.bounds end)
  return ok and b ~= nil
end

--- Every other window of the extension: a child of the Bonebase window
--- (`parent`), so Aseprite keeps it on top of it. Without one, a click
--- outside sends a modal window BEHIND the Bonebase window, which then
--- takes no clicks either until Esc (Spaghetti DECISIONS 2026-08-31,
--- VeSprite). Falls back to a loose window when the API refuses `parent`
--- or the main window is gone; nil when no window can be made.
function M.newDialog(opts)
  opts = type(opts) == "table" and opts or {}
  if alive(M.mainDlg) then
    local withParent = { parent = M.mainDlg }
    for k, v in pairs(opts) do withParent[k] = v end
    local ok, dlg = pcall(Dialog, withParent)
    if ok and dlg then return dlg end
  end
  local ok, dlg = pcall(Dialog, opts)
  return ok and dlg or nil
end

--- `text` split after each ". ", "? " or "! " (what is left included).
function M.sentences(text)
  text = tostring(text or "")
  local out, pos = {}, 1
  while true do
    local a, b = text:find("[%.%?!]%s+", pos)
    if not a then break end
    out[#out + 1] = text:sub(pos, a)
    pos = b + 1
  end
  local rest = text:sub(pos):match("^%s*(.-)%s*$")
  if rest ~= "" then out[#out + 1] = rest end
  return out
end

--- A yes / no question in a child window (app.alert cannot take a
--- parent). True for `yes`. Each sentence of `text` goes on its own line.
function M.ask(text, yes, no)
  local dlg = M.newDialog{ title = "Bonebase" }
  if not dlg then return false end
  for _, line in ipairs(M.sentences(text)) do
    dlg:label{ text = line }
    dlg:newrow()
  end
  dlg:button{ id = "ok", text = yes or "Continue", focus = true }
  dlg:button{ id = "cancel", text = no or "Cancel" }
  dlg:show{ wait = true }
  return dlg.data.ok == true
end

--- MouseButton.LEFT/RIGHT/MIDDLE -> "left"/"right"/"middle".
local function buttonName(b)
  if b == MouseButton.RIGHT then return "right" end
  if b == MouseButton.MIDDLE then return "middle" end
  return "left"
end

local function modsOf(ev)
  return { shift = ev.shiftKey == true, ctrl = ev.ctrlKey == true, alt = ev.altKey == true }
end

--- Aseprite key event -> the kit's key names: printable characters as
--- themselves, "KeyA" -> "a", "Digit1" -> "1", "ArrowLeft" -> "Left",
--- the rest by code ("Escape", "Delete", "Enter"...).
local function keyName(ev)
  local code = tostring(ev.code or "")
  local k = ev.key
  if type(k) == "string" and #k == 1 and k:byte() >= 33 and k:byte() <= 126 then
    return k
  end
  if code:match("^Key%a$") then return code:sub(4):lower() end
  if code:match("^Digit%d$") then return code:sub(6) end
  if code:match("^Arrow%a+$") then return code:sub(6) end
  if code == "Space" or k == " " then return "Space" end
  return code
end

--- The editor of the active sprite: the session's, else the rig saved
--- in the sprite, else an empty one of the sprite's size (64 x 64 with no
--- sprite open). Returns it and the session key (sprite id, 0 = none).
local function editorFor(s)
  s.editors = s.editors or {}
  local spr = app.sprite
  local key = spr and spr.id or 0
  local E = s.editors[key]
  if not E then
    local doc = spr and M.Export.loadRig(spr) or nil
    if not doc then
      doc = M.Rig.new(spr and spr.width or 64, spr and spr.height or 64)
    end
    E = M.Editor.new(doc)
    E.spriteId = spr and spr.id or nil
    s.editors[key] = E
  end
  -- the sprite was resized in Aseprite: its bounds in the canvas follow
  if spr then
    local bd = M.Rig.bounds(E.doc)
    if bd.w ~= spr.width or bd.h ~= spr.height then
      M.Rig.setSpriteSize(E.doc, spr.width, spr.height)
      E:changed()
    end
  end
  return E, key
end

--- The open sprite with id `id`, or nil.
local function spriteById(id)
  if not id then return nil end
  for _, spr in ipairs(app.sprites) do
    if spr.id == id then return spr end
  end
  return nil
end

function M.open(State)
  local s = State.session
  if not s then
    s = {}
    State.session = s
  end

  local E, key = editorFor(s)
  if s.dlg and alive(s.dlg) then
    if s.key == key then
      pcall(function() s.dlg:repaint() end)
      return s.dlg
    end
    -- another sprite is active now: reopen on its rig
    M.close(State)
  end
  s.E, s.key = E, key
  if not State.seededOnce then
    State.seededOnce = true
    pcall(M.seedFactory, State)
  end

  local dlg = Dialog{
    title = "Bonebase " .. tostring(State.VERSION or ""),
    resizeable = true,
    onclose = function()
      M.stopTimer(s)
      if M.mainDlg == s.dlg then M.mainDlg = nil end
      s.dlg = nil
      s.V = nil
    end,
  }
  -- `-b --script` has no GUI thread: `Dialog{}` returns nil there.
  if not dlg then return nil end
  s.dlg = dlg
  M.mainDlg = dlg

  local function repaint()
    pcall(function() dlg:repaint() end)
  end

  -- an error inside a handler becomes a warning on the status line instead
  -- of Aseprite's script-error dialog on every mouse move
  local function guard(fn)
    local ok, res = pcall(fn)
    if ok then return res end
    s.eventError = tostring(res)
    if s.V then
      s.V:setNotice("Error: " .. s.eventError, true)
      s.V.capture = nil
    end
    return true
  end

  -- playback: one Timer tick per frame, restarted when the frame time
  -- changes (the clip's, or a frame with its own duration)
  local onPlay
  onPlay = function(playing, frameMs)
    M.stopTimer(s)
    if not playing then return end
    s.playMs = tonumber(frameMs) or 100
    s.timer = Timer{
      interval = math.max(0.01, s.playMs / 1000),
      ontick = function()
        if not alive(dlg) then M.stopTimer(s) return end
        if s.V and s.V:tick() then repaint() end
        local ms = s.V and s.V.playing and s.V:frameMs()
        if ms and ms ~= s.playMs then onPlay(true, ms) end
      end,
    }
    s.timer:start()
  end

  -- Export: into the editor's own sprite, or a new one when it has none
  -- (then the editor moves to that sprite's key)
  local function onExport(opts)
    local target = spriteById(s.E.spriteId)
    local ok, msg, spr = M.Export.run(s.E.doc, target, opts)
    if ok and spr and s.E.spriteId ~= spr.id then
      s.editors[s.key] = nil
      s.E.spriteId = spr.id
      s.key = spr.id
      s.editors[spr.id] = s.E
    end
    pcall(function() app.refresh() end)
    return ok, msg
  end

  -- the View tab's Background: the editor's sprite at a frame, its
  -- visible layers (or only the one named `only`) composed without the
  -- Bonebase group, at the sprite's size (RGB only; blend modes ignored,
  -- it is a reference, not a render)
  local function reference(frame, only)
    local spr = spriteById(s.E.spriteId)
    if not spr or spr.colorMode ~= ColorMode.RGB then return nil end
    local f = math.max(1, math.min(frame, #spr.frames))
    local img = Image(spr.width, spr.height, ColorMode.RGB)
    -- drawing in the sprite refreshes the reference (once per sprite)
    if s.refWatched ~= spr.id then
      s.refWatched = spr.id
      pcall(function()
        spr.events:on("change", function()
          if s.V then
            s.V.refToken = s.V.refToken + 1
            repaint()
          end
        end)
      end)
    end
    local group = M.Export.findGroup(spr.layers)
    local skip = group and group.id
    -- `picked`: inside the chosen layer (a group); the chosen one shows
    -- even when hidden, everything else only when visible
    local function walk(layers, picked)
      for _, l in ipairs(layers) do
        if l.id ~= skip then
          local hit = only ~= nil and l.name == only
          local on = only == nil or picked or hit
          local shown = hit or l.isVisible
          if l.isGroup then
            if not on then walk(l.layers, false)   -- the chosen one may be deeper
            elseif shown then walk(l.layers, only ~= nil) end
          elseif on and shown then
            pcall(function()
              local cel = l:cel(f)
              if cel then
                local op = math.floor((l.opacity or 255) * (cel.opacity or 255) / 255)
                img:drawImage(cel.image, cel.position, op)
              end
            end)
          end
        end
      end
    end
    walk(spr.layers, false)
    return M.Buffer.fromImage(img)
  end

  -- the sprite's layers by name, top of the stack first (groups too: a
  -- group stands for everything in it), without the Bonebase group
  local function layers()
    local spr = spriteById(s.E.spriteId)
    if not spr then return {} end
    local group = M.Export.findGroup(spr.layers)
    local out, seen = {}, {}
    local function walk(list)
      for i = #list, 1, -1 do
        local l = list[i]
        if not (group and l.id == group.id) then
          if not seen[l.name] then out[#out + 1], seen[l.name] = l.name, true end
          if l.isGroup then walk(l.layers) end
        end
      end
    end
    walk(spr.layers)
    return out
  end

  -- destructive actions ask with a native yes / no (ui-style.md), in a
  -- child window that cannot fall behind this one
  local function confirm(text, onYes)
    if M.ask(text, "Continue", "Cancel") then
      onYes()
      repaint()
    end
  end

  dlg:canvas{
    id = "canvas",
    width = M.View.DEF_W,
    height = M.View.DEF_H,
    hexpand = true,
    vexpand = true,
    onpaint = function(ev)
      local gc = ev.context
      s.paintCount = (s.paintCount or 0) + 1
      if not s.V then
        -- gc:measureText only exists inside onpaint
        local text = M.UI.text.new(function(str)
          local sz = gc:measureText(str)
          return sz.width, sz.height
        end)
        s.V = M.View.new{
          UI = M.UI, Rig = M.Rig, Skin = M.Skin, Anim = M.Anim, Editor = M.Editor, Ik = M.Ik,
          Dope = M.Dope, Promo = M.Promo, openUrl = M.openUrl,
          Ref = M.Ref, loadRef = M.loadRef,
          Import = M.Import, loadMotion = M.loadMotion,
          Lib = M.Lib, Poses = M.Poses, Thumb = M.Thumb, library = M.library(),
          Onion = M.Onion, onion = M.prefTable(State, "onion"),
          onOnion = function(t) M.setPrefTable(State, "onion", t) end,
          Templates = M.Templates, editor = s.E, text = text, version = State.VERSION,
          onPlay = onPlay, confirm = confirm, onExport = onExport, reference = reference,
          layers = layers,
          leftW = M.pref(State, "leftW"),
          onSplit = function(w) M.setPref(State, "leftW", w) end,
          sheetH = M.pref(State, "sheetH"),
          onSheet = function(h) M.setPref(State, "sheetH", h) end,
        }
      end
      local w = math.floor(tonumber(gc.width) or M.View.DEF_W)
      local h = math.floor(tonumber(gc.height) or M.View.DEF_H)
      s.V:layout(w, h)
      local ok, ops = pcall(s.V.paint, s.V)
      if ok and ops then
        s.paintError = nil
        local pok, perr = pcall(M.Painter.paint, gc, ops, { toImage = M.Buffer.toImage })
        if not pok then s.paintError = tostring(perr) end
      else
        s.paintError = tostring(ops)
        gc.color = Color{ r = 255, g = 80, b = 80 }
        gc:fillText("paint error: " .. tostring(ops), 4, 4)
      end
    end,
    onmousedown = function(ev)
      if s.V and guard(function()
        return s.V:mouseDown(ev.x, ev.y, buttonName(ev.button), modsOf(ev))
      end) then repaint() end
    end,
    onmousemove = function(ev)
      if s.V and guard(function() return s.V:mouseMove(ev.x, ev.y, modsOf(ev)) end) then
        repaint()
      end
    end,
    onmouseup = function(ev)
      if s.V and guard(function()
        return s.V:mouseUp(ev.x, ev.y, buttonName(ev.button), modsOf(ev))
      end) then repaint() end
    end,
    onwheel = function(ev)
      local dy = tonumber(ev.deltaY) or 0
      if s.V and guard(function() return s.V:wheel(ev.x, ev.y, dy, modsOf(ev)) end) then
        repaint()
      end
    end,
    onkeydown = function(ev)
      if s.V and guard(function() return s.V:key(keyName(ev), modsOf(ev)) end) then
        repaint()
        ev:stopPropagation()
      end
    end,
  }

  dlg:show{ wait = false }
  return dlg
end

---------------------------------------
-- Reference images: where they come from
---------------------------------------

--- How a sprite looks at `frame`, as an RGB image (any colour mode).
function M.spriteImage(spr, frame)
  local img = Image(ImageSpec{ width = spr.width, height = spr.height, colorMode = ColorMode.RGB })
  img:drawSprite(spr, frame or 1)
  return img
end

-- a picture bigger than this is scaled down before it becomes a Lua table
-- (a 8000 x 8000 PNG would take gigabytes); the store keeps 512 px anyway
M.REF_LOAD_SIDE = 1024
M.REF_MAX_PIXELS = 8192 * 8192

--- The image as a buffer, scaled down first when it is big.
local function bufferOfImage(img)
  local w, h = img.width, img.height
  local side = math.max(w, h)
  if side > M.REF_LOAD_SIDE then
    local k = M.REF_LOAD_SIDE / side
    img = Image(img)
    img:resize(math.max(1, math.floor(w * k + 0.5)), math.max(1, math.floor(h * k + 0.5)))
  end
  return M.Buffer.fromImage(img)
end

local function isOpen(spr)
  for _, s in ipairs(app.sprites) do
    if s == spr then return true end
  end
  return false
end

--- Runs `fn(sprite)` on a sprite Aseprite opens just for it, then gives
--- the active sprite back. It only closes a sprite that was NOT open
--- before: a file the user already has open is theirs, never closed here.
local function withTempSprite(open, fn)
  local before = app.sprite
  local had = {}
  for _, s in ipairs(app.sprites) do had[#had + 1] = s end
  local ok, spr = pcall(open)
  if not ok or not spr then return nil end
  local mine = true
  for _, s in ipairs(had) do
    if s == spr then mine = false end
  end
  local okFn, out, why = pcall(fn, spr)
  if mine and isOpen(spr) then pcall(function() spr:close() end) end
  if before and isOpen(before) then pcall(function() app.sprite = before end) end
  if not okFn then return nil end
  return out, why
end

--- A sprite at `frame` as a buffer, or nil + why when it is too big.
local function spriteBuffer(spr, frame)
  if spr.width * spr.height > M.REF_MAX_PIXELS then
    return nil, string.format("The image is too big (%d x %d px)", spr.width, spr.height)
  end
  return bufferOfImage(M.spriteImage(spr, frame))
end

--- A file (PNG, JPG, GIF, .aseprite...) as a buffer: its first frame. A
--- file that is already open is read from its sprite.
function M.refFromFile(path)
  if type(path) ~= "string" or path == "" or not app.fs.isFile(path) then
    return nil, "Choose an image file"
  end
  for _, s in ipairs(app.sprites) do
    if s.filename ~= "" and app.fs.normalizePath(s.filename) == app.fs.normalizePath(path) then
      return spriteBuffer(s, 1)
    end
  end
  local buf, why = withTempSprite(function() return Sprite{ fromFile = path } end,
    function(spr) return spriteBuffer(spr, 1) end)
  if not buf then return nil, why or ("Could not read " .. app.fs.fileName(path)) end
  return buf
end

local function hasInk(buf)
  for i = 4, #buf.d, 4 do
    if buf.d[i] ~= 0 then return true end
  end
  return false
end

--- The clipboard's image as a buffer (app.clipboard where Aseprite has
--- it, else a sprite made from the clipboard, without its dialog, and
--- closed again). An empty result means there was no image.
function M.refFromClipboard()
  local ok, img = pcall(function() return app.clipboard and app.clipboard.image end)
  if ok and img and img.colorMode == ColorMode.RGB and img.width > 0 then
    return bufferOfImage(img)
  end
  local before = app.sprite
  local buf = withTempSprite(function()
    app.command.NewFile{ ui = false, fromClipboard = true }
    if app.sprite == before then return nil end
    return app.sprite
  end, function(spr) return spriteBuffer(spr, 1) end)
  if not buf or not hasInk(buf) then return nil, "The clipboard has no image" end
  return buf
end

--- An open sprite at one of its frames, as a buffer.
function M.refFromSprite(spr, frame)
  if not spr then return nil, "No sprite" end
  frame = math.max(1, math.min(#spr.frames, math.floor(tonumber(frame) or 1)))
  return spriteBuffer(spr, frame)
end

local function spriteName(spr)
  local name = spr.filename ~= "" and app.fs.fileName(spr.filename) or "Sprite"
  return string.format("%s (%d x %d)", name, spr.width, spr.height)
end

--- The View tab's buttons: asks with a small native dialog where needed
--- (a file, or an open sprite and its frame).
function M.loadRef(kind)
  if kind == "clipboard" then return M.refFromClipboard() end
  if kind == "file" then
    local dlg = M.newDialog{ title = "Reference image" }
    if not dlg then return nil end
    dlg:file{ id = "file", label = "Image", open = true,
      filetypes = { "png", "jpg", "jpeg", "gif", "bmp", "webp", "aseprite", "ase" } }
    dlg:button{ id = "ok", text = "Load", focus = true }
    dlg:button{ id = "cancel", text = "Cancel" }
    dlg:show{ wait = true }
    if not dlg.data.ok then return nil end
    return M.refFromFile(dlg.data.file)
  end
  if kind == "sprite" then
    local list, names = {}, {}
    for _, spr in ipairs(app.sprites) do
      list[#list + 1] = spr
      names[#names + 1] = #list .. ". " .. spriteName(spr)
    end
    if #list == 0 then return nil, "Open a sprite in Aseprite first" end
    local dlg = M.newDialog{ title = "Reference from a sprite" }
    if not dlg then return nil end
    dlg:combobox{ id = "sprite", label = "Sprite", option = names[1], options = names }
    dlg:number{ id = "frame", label = "Frame", text = "1", decimals = 0 }
    dlg:button{ id = "ok", text = "Use", focus = true }
    dlg:button{ id = "cancel", text = "Cancel" }
    dlg:show{ wait = true }
    if not dlg.data.ok then return nil end
    local pick
    for i, n in ipairs(names) do if n == dlg.data.sprite then pick = list[i] end end
    return M.refFromSprite(pick, dlg.data.frame)
  end
  return nil
end

M.MOTION_MAX_BYTES = 64 * 1024 * 1024

local function readBytes(path)
  local f = io.open(path, "rb")
  if not f then return nil end
  local data = f:read("*a")
  f:close()
  return data
end

--- A 3D animation file (Import 3D): asks for it with a native dialog and
--- returns its bytes, its name and a reader for the files next to it (a
--- .gltf's buffers), or nil and why.
function M.loadMotion()
  local dlg = M.newDialog{ title = "Import 3D animation" }
  if not dlg then return nil end
  dlg:file{ id = "file", label = "File", open = true, filetypes = { "glb", "gltf", "bvh" } }
  dlg:label{ id = "hint", text = "FBX (Mixamo): open it in Blender and export glTF (.glb)" }
  dlg:button{ id = "ok", text = "Open", focus = true }
  dlg:button{ id = "cancel", text = "Cancel" }
  dlg:show{ wait = true }
  if not dlg.data.ok then return nil end
  local path = dlg.data.file
  if type(path) ~= "string" or path == "" or not app.fs.isFile(path) then
    return nil, "Choose a .glb, .gltf or .bvh file"
  end
  if (app.fs.fileSize(path) or 0) > M.MOTION_MAX_BYTES then
    return nil, "The file is too big (64 MB at most)"
  end
  local bytes = readBytes(path)
  if not bytes then return nil, "Could not read " .. app.fs.fileName(path) end
  local dir = app.fs.filePath(path)
  local function readFile(uri)
    -- only files beside the .gltf (or below it), never a path out of it
    if type(uri) ~= "string" or uri:find("%.%.") or uri:match("^%a+:") or uri:match("^[/\\]") then
      return nil
    end
    return readBytes(app.fs.joinPath(dir, uri))
  end
  return bytes, app.fs.fileName(path), readFile
end

---------------------------------------
-- The rig library (library.lua): one .bbrig per rig (the same text a
-- sprite keeps in its properties) and a .png thumbnail beside it, in
-- Aseprite's user folder so a reinstall of the extension keeps them.
---------------------------------------

M.LIB_EXT = ".bbrig"

function M.libraryDir()
  local base = app.fs.userConfigPath
  if type(base) ~= "string" or base == "" then return nil end
  return app.fs.joinPath(base, "bonebase", "rigs")
end

local function writeBytes(path, data)
  local f, err = io.open(path, "wb")
  if not f then return false, err end
  local ok, werr = f:write(data)
  f:close()
  if not ok then return false, werr end
  return true
end

local function thumbPath(rigPath)
  return rigPath:sub(1, #rigPath - #M.LIB_EXT) .. ".png"
end

--- The host side of the Library tab: list, thumb, save, load, text,
--- remove, openFolder (library.lua documents them).
function M.library()
  local L = {}
  function L.list()
    local dir = M.libraryDir()
    local out = {}
    if not dir or not app.fs.isDirectory(dir) then return out end
    for _, f in ipairs(app.fs.listFiles(dir)) do
      if f:sub(-#M.LIB_EXT):lower() == M.LIB_EXT then
        out[#out + 1] = { name = f:sub(1, #f - #M.LIB_EXT), path = app.fs.joinPath(dir, f) }
      end
    end
    table.sort(out, function(a, b) return a.name:lower() < b.name:lower() end)
    return out
  end
  function L.thumb(e)
    local p = thumbPath(e.path)
    if not app.fs.isFile(p) then return nil end
    local ok, img = pcall(Image, { fromFile = p })
    if not ok or not img then return nil end
    if img.colorMode ~= ColorMode.RGB then
      local rgb = Image(img.width, img.height, ColorMode.RGB)
      rgb:drawImage(img, Point(0, 0))
      img = rgb
    end
    return M.Buffer.fromImage(img)
  end
  function L.save(name, doc, thumb)
    local dir = M.libraryDir()
    if not dir then return false, "no user folder" end
    if not app.fs.isDirectory(dir) then app.fs.makeAllDirectories(dir) end
    local path = app.fs.joinPath(dir, name .. M.LIB_EXT)
    local text = M.Rig.encode(M.Serial, doc)
    -- written aside first: a failed write never loses the rig already there
    local tmp = path .. ".tmp"
    local ok, err = writeBytes(tmp, text)
    if not ok or (app.fs.fileSize(tmp) or -1) ~= #text then
      pcall(os.remove, tmp)
      return false, err or "the file came out short"
    end
    pcall(os.remove, path)
    pcall(os.rename, tmp, path)
    -- trust the disk, not os.rename's return (Aseprite's gives none)
    if not app.fs.isFile(path) then return false, "could not replace " .. app.fs.fileName(path) end
    -- a name that only differs in case is this entry now (Windows already
    -- replaced it): gone only once the new file is there
    for _, e in ipairs(L.list()) do
      if e.name:lower() == name:lower() and e.name ~= name then L.remove(e) end
    end
    local png = thumbPath(path)
    pcall(os.remove, png)
    if thumb and thumb.w > 0 then writeBytes(png, M.Png.encode(thumb)) end
    return true, text
  end
  --- The doc, nil or the reason, and the file's text.
  function L.load(e)
    local text = readBytes(e.path)
    if not text then return nil, "could not read the file" end
    local doc, why = M.Rig.decode(M.Serial, text)
    return doc, why, text
  end
  --- The text of entry `name` (nil when there is none).
  function L.text(name)
    for _, e in ipairs(L.list()) do
      if e.name:lower() == name:lower() then return readBytes(e.path) end
    end
    return nil
  end
  function L.remove(e)
    pcall(os.remove, e.path)
    pcall(os.remove, thumbPath(e.path))
    -- trust the disk, not os.remove's return
    if app.fs.isFile(e.path) then return false, "the file is still there" end
    return true
  end
  function L.openFolder()
    local dir = M.libraryDir()
    if not dir then return false, "no user folder" end
    if not app.fs.isDirectory(dir) then app.fs.makeAllDirectories(dir) end
    local url = "file:///" .. dir:gsub("\\", "/")
    if pcall(function() app.command.Launch{ website = url } end) then return true end
    if pcall(function() app.command.Launch{ path = url } end) then return true end
    return false, dir
  end
  return L
end

--- Copies the factory rigs (the extension's rigs/ folder) into the user's
--- library once (Lib.toSeed): the names seeded go in the preferences, so
--- one the user deletes stays deleted. Copied files that fail to write
--- are tried again next time.
function M.seedFactory(State)
  if not State.root or not State.plugin then return end
  local src = app.fs.joinPath(State.root, "rigs")
  local dir = M.libraryDir()
  if not dir or not app.fs.isDirectory(src) then return end
  local factory = {}
  for _, f in ipairs(app.fs.listFiles(src)) do
    if f:sub(-#M.LIB_EXT):lower() == M.LIB_EXT then factory[#factory + 1] = f:sub(1, #f - #M.LIB_EXT) end
  end
  local copy, seeded = M.Lib.toSeed(factory, M.library().list(), M.prefTable(State, "seeded"))
  if #copy > 0 and not app.fs.isDirectory(dir) then app.fs.makeAllDirectories(dir) end
  for _, name in ipairs(copy) do
    local from = app.fs.joinPath(src, name .. M.LIB_EXT)
    local to = app.fs.joinPath(dir, name .. M.LIB_EXT)
    local text = readBytes(from)
    if not (text and writeBytes(to, text) and app.fs.isFile(to)) then
      seeded[name] = nil
    else
      local png = readBytes(thumbPath(from))
      if png then writeBytes(thumbPath(to), png) end
    end
  end
  M.setPrefTable(State, "seeded", seeded)
end

--- Opens a web page in the browser (Launch takes `website` in newer
--- Aseprite versions and `path` in older ones). True when one worked.
function M.openUrl(url)
  if pcall(function() app.command.Launch{ website = url } end) then return true end
  return pcall(function() app.command.Launch{ path = url } end) == true
end

--- plugin.preferences, when the host gave us the plugin (never in tests).
function M.pref(State, key)
  local p = State.plugin and State.plugin.preferences
  return p and p[key] or nil
end

--- A table kept in the preferences as text (kit/serial.lua): nil when
--- missing or unreadable.
function M.prefTable(State, key)
  local s = M.pref(State, key)
  if type(s) ~= "string" or not M.Serial then return nil end
  local ok, t = pcall(M.Serial.decode, s)
  return ok and type(t) == "table" and t or nil
end

function M.setPrefTable(State, key, t)
  if M.Serial then M.setPref(State, key, M.Serial.encode(t)) end
end

function M.setPref(State, key, v)
  local p = State.plugin and State.plugin.preferences
  if p then p[key] = v end
end

function M.stopTimer(s)
  if s.timer then
    local t = s.timer
    s.timer = nil
    pcall(function() t:stop() end)
  end
end

function M.close(State)
  local s = State.session
  if s then M.stopTimer(s) end
  if s and s.dlg then
    local d = s.dlg
    if M.mainDlg == d then M.mainDlg = nil end
    s.dlg = nil
    pcall(function() d:close() end)
  end
end

return M
