-- stub.lua — a headless stand-in for Aseprite's API, for Lua 5.4 and
-- LuaJIT alike.
--
-- Installed by `dofile("kit/test/stub.lua")`: it DEFINES GLOBALS (`app`,
-- `Image`, `ImageSpec`, `ColorMode`, `Color`, `Rectangle`, `Dialog`,
-- `Point`, `Sprite`, `MouseButton`), the way the real script environment
-- does, and RETURNS a table of test utilities. Based on FastFX's
-- `scripts/tests/test_render_ab.lua:17-37` (the `app`/`Image`/`ImageSpec`/
-- `ColorMode` stub) and EasyInterpolation's `scripts/tests/lib/
-- fake_dialog.lua` (the `Dialog` double).
--
-- It is a test double, not an emulator: it records what it is asked to do
-- and answers the minimum a headless test needs, not a pixel-perfect
-- Aseprite.

local stub = {}
local floor = math.floor

-- Arithmetic, not bitwise operators: `&`/`|`/`<<` are Lua 5.3+ only and
-- this stub must load on LuaJIT too (kit stays dual-VM).
local function packRGBA(r, g, b, a)
  return r + g * 256 + b * 65536 + a * 16777216
end
local function unpackRGBA(p)
  local r = p % 256
  local g = floor(p / 256) % 256
  local b = floor(p / 65536) % 256
  local a = floor(p / 16777216) % 256
  return r, g, b, a
end

---------------------------------------
-- app.* --
---------------------------------------

app = app or {
  pixelColor = {
    rgba = function(r, g, b, a)
      return packRGBA(r % 256, g % 256, b % 256, (a or 255) % 256)
    end,
    rgbaR = function(p) return (select(1, unpackRGBA(p))) end,
    rgbaG = function(p) return (select(2, unpackRGBA(p))) end,
    rgbaB = function(p) return (select(3, unpackRGBA(p))) end,
    rgbaA = function(p) return (select(4, unpackRGBA(p))) end,
  },
  fs = {
    joinPath = function(...)
      local parts = { ... }
      return table.concat(parts, "/")
    end,
    filePath = function(p)
      return p:match("^(.*)[/\\][^/\\]*$") or "."
    end,
    fileName = function(p)
      return p:match("[^/\\]+$") or p
    end,
    isFile = function(p)
      local f = io.open(p, "rb")
      if f then f:close() return true end
      return false
    end,
  },
  alert = function(t) return 1 end,
  refresh = function() end,
  transaction = function(_, fn) return fn() end,
  sprite = nil,
  layer = nil,
  frame = nil,
  fgColor = { red = 0, green = 0, blue = 0, alpha = 255 },
  bgColor = { red = 255, green = 255, blue = 255, alpha = 255 },
}

---------------------------------------
-- Colour modes and Image --
---------------------------------------

ColorMode = ColorMode or { RGB = 2, GRAY = 1, INDEXED = 4 }

function ImageSpec(t) return t end

local ImgMethods = {}
function ImgMethods:putPixel(x, y, v)
  if x >= 0 and y >= 0 and x < self.width and y < self.height then
    self.px[y * self.width + x + 1] = v
  end
end
function ImgMethods:getPixel(x, y)
  if x < 0 or y < 0 or x >= self.width or y >= self.height then
    return 0
  end
  return self.px[y * self.width + x + 1] or 0
end
function ImgMethods:clear(v)
  local n = self.width * self.height
  for i = 1, n do self.px[i] = v or 0 end
end
function ImgMethods:drawSprite(spr, frame) end -- nothing to composite headless

local ImgMt = {
  __index = function(self, k)
    if k == "bytes" then
      local px = rawget(self, "px")
      local n = self.width * self.height
      local parts = {}
      for i = 1, n do
        local r, g, b, a = unpackRGBA(px[i] or 0)
        parts[i] = string.char(r, g, b, a)
      end
      return table.concat(parts)
    end
    return ImgMethods[k]
  end,
  __newindex = function(self, k, v)
    if k == "bytes" then
      assert(#v == self.rowStride * self.height, "bytes length mismatch")
      local px = rawget(self, "px")
      local i = 0
      for pos = 1, #v, 4 do
        i = i + 1
        local r, g, b, a = v:byte(pos, pos + 3)
        px[i] = packRGBA(r, g, b, a)
      end
    else
      rawset(self, k, v)
    end
  end,
}

--- `Image(ImageSpec{...})` or `Image(w, h)`.
function Image(a, b)
  local spec = a
  if type(a) == "number" then
    spec = { width = a, height = b, colorMode = ColorMode.RGB }
  end
  local img = {
    width = spec.width,
    height = spec.height,
    colorMode = spec.colorMode or ColorMode.RGB,
    rowStride = spec.width * 4,
    px = {},
  }
  return setmetatable(img, ImgMt)
end

---------------------------------------
-- Color, Rectangle, Point --
---------------------------------------

function Color(t)
  t = t or {}
  return { red = t.r or 0, green = t.g or 0, blue = t.b or 0,
    alpha = t.a or 255 }
end

function Rectangle(x, y, w, h)
  return { x = x, y = y, width = w, height = h }
end

function Point(x, y)
  return { x = x, y = y }
end

MouseButton = MouseButton or { LEFT = 1, RIGHT = 2, MIDDLE = 3 }

---------------------------------------
-- Dialog (see EasyInterpolation's fake_dialog.lua) --
---------------------------------------

local function newDialog(spec)
  local dlg = {
    data = {}, widgets = {}, byId = {}, tabs = {}, currentTab = nil,
    tabsClosed = false, selectedTab = nil, shown = false, closed = false,
    bounds = { x = 0, y = 0, width = 640, height = 480 },
  }

  local function record(kind, wspec)
    wspec = wspec or {}
    local w = {
      kind = kind, id = wspec.id, text = wspec.text, label = wspec.label,
      tab = dlg.currentTab, enabled = (wspec.enabled ~= false),
      selected = wspec.selected, option = wspec.option,
      options = wspec.options, onclick = wspec.onclick,
      onchange = wspec.onchange, onrelease = wspec.onrelease,
      spec = wspec,
    }
    dlg.widgets[#dlg.widgets + 1] = w
    if wspec.id then
      dlg.byId[wspec.id] = w
      if wspec.selected ~= nil then dlg.data[wspec.id] = wspec.selected end
      if wspec.option ~= nil then dlg.data[wspec.id] = wspec.option end
      if wspec.value ~= nil then
        w.value = wspec.value
        dlg.data[wspec.id] = wspec.value
      end
      if kind == "number" and wspec.text ~= nil then
        dlg.data[wspec.id] = tonumber(wspec.text)
      end
      if kind == "entry" and wspec.text ~= nil then
        dlg.data[wspec.id] = wspec.text
      end
      if kind == "color" and wspec.color ~= nil then
        dlg.data[wspec.id] = wspec.color
      end
      if kind == "file" and wspec.filename ~= nil then
        dlg.data[wspec.id] = wspec.filename
      end
    end
    return w
  end

  function dlg:tab(t) dlg.currentTab = t.id
    dlg.tabs[#dlg.tabs + 1] = { id = t.id, text = t.text }
    return dlg
  end
  function dlg:endtabs(t)
    dlg.tabsClosed = true
    dlg.selectedTab = t and t.selected or nil
    dlg.currentTab = nil
    return dlg
  end
  function dlg:label(t) record("label", t) return dlg end
  function dlg:separator(t) record("separator", t) return dlg end
  function dlg:newrow(t) record("newrow", t) return dlg end
  function dlg:check(t) record("check", t) return dlg end
  function dlg:combobox(t) record("combobox", t) return dlg end
  function dlg:button(t) record("button", t) return dlg end
  function dlg:canvas(t) record("canvas", t) return dlg end
  function dlg:number(t) record("number", t) return dlg end
  function dlg:slider(t) record("slider", t) return dlg end
  function dlg:entry(t) record("entry", t) return dlg end
  function dlg:color(t) record("color", t) return dlg end
  function dlg:file(t) record("file", t) return dlg end

  function dlg:modify(t)
    local w = t and t.id and dlg.byId[t.id]
    if not w then return dlg end
    if t.options ~= nil then w.options = t.options end
    if t.enabled ~= nil then w.enabled = t.enabled end
    if t.option ~= nil then w.option = t.option dlg.data[t.id] = t.option end
    if t.selected ~= nil then w.selected = t.selected dlg.data[t.id] = t.selected end
    if t.value ~= nil then w.value = t.value dlg.data[t.id] = t.value end
    if t.text ~= nil then
      w.text = t.text
      if w.kind == "number" then dlg.data[t.id] = tonumber(t.text)
      else dlg.data[t.id] = t.text end
    end
    return dlg
  end

  function dlg:show(t) dlg.shown = true dlg.showSpec = t return dlg end
  function dlg:close() dlg.closed = true return dlg end
  function dlg:repaint() return dlg end

  --- Click a button, or set a value and fire its handler.
  function dlg:fire(id, value)
    local w = dlg.byId[id]
    if not w then error("no widget with id " .. tostring(id)) end
    if value ~= nil then dlg.data[id] = value end
    if w.onclick then w.onclick() end
    if w.onchange then w.onchange() end
    return dlg
  end

  return dlg
end

function Dialog(spec)
  return newDialog(spec or {})
end

---------------------------------------
-- Test utilities --
---------------------------------------

--- A fake `gc` (Dialog canvas paint-context): records every call so a
--- test can assert what was drawn. `gc.calls` is the array, in order,
--- each `{ "fillRect", x, y, w, h }` etc.
--- `scale` (default 1) sizes the deterministic `measureText`: width is
--- `6 * #str * scale`, height `8 * scale` -- the 5x7 font of
--- kit/painter_buf.lua plus its 1px advance, so a headless layout and a
--- painter_buf golden agree.
function stub.gc(w, h, scale)
  local gc = { width = w or 100, height = h or 100, calls = {},
    color = nil, strokeWidth = 1, scale = scale or 1 }
  local function log(name, ...)
    local call = { name, ... }
    gc.calls[#gc.calls + 1] = call
  end
  function gc:fillRect(r) log("fillRect", r.x, r.y, r.width, r.height, self.color) end
  function gc:strokeRect(r) log("strokeRect", r.x, r.y, r.width, r.height, self.color) end
  function gc:fillText(text, x, y) log("fillText", text, x, y, self.color) end
  function gc:drawImage(img, sx, sy, sw, sh, dx, dy, dw, dh)
    log("drawImage", img, sx, sy, sw, sh, dx, dy, dw, dh)
  end
  function gc:beginPath() log("beginPath") end
  function gc:moveTo(x, y) log("moveTo", x, y) end
  function gc:lineTo(x, y) log("lineTo", x, y) end
  function gc:stroke() log("stroke", self.color, self.strokeWidth) end
  function gc:measureText(text)
    return { width = #tostring(text) * 6 * self.scale, height = 8 * self.scale }
  end
  function gc:save() log("save") end
  function gc:restore() log("restore") end
  function gc:rect(r) log("rect", r.x, r.y, r.width, r.height) end
  function gc:clip() log("clip") end
  return gc
end

stub.Dialog = Dialog

--- A fake plugin table, for `kit/host.lua` tests: `newCommand` just
--- records what it was given, like Aseprite's real one.
function stub.plugin()
  return {
    preferences = {},
    commands = {},
    newCommand = function(self, t)
      self.commands[#self.commands + 1] = t
    end,
  }
end

return stub
