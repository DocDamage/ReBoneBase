-- text.lua — text measurement with an INJECTED measurer, cached per
-- string.
--
-- Widgets need `width(str)` to lay out values and tabs, but the only
-- thing that knows how wide a string is in Aseprite is `gc:measureText`,
-- which exists only inside `onpaint`. So the measurer is a plain
-- function `measure(str) -> w, h` the host supplies: in Aseprite, a
-- wrapper around `gc:measureText(str)` (`.width`, `.height`); headless,
-- `painter_buf.measure` (the 5x7 font). Same widgets, same layout code,
-- two rulers.
--
-- It is NOT a text renderer: it emits no ops and knows no glyphs.

local M = {}

local Text = {}
Text.__index = Text

--- `measure(str) -> w, h`. `fallbackCharW`/`fallbackH` are used when no
--- measurer is given (a theme's `charW`/`fontH`), so tests can run with
--- `M.new()` alone.
function M.new(measure, fallbackCharW, fallbackH)
  local cw, fh = fallbackCharW or 6, fallbackH or 8
  if not measure then
    measure = function(s) return #s * cw, fh end
  end
  return setmetatable({ measure = measure, cache = {}, count = 0,
    _h = nil }, Text)
end

--- Pixel width of `str` (cached). The cache is dropped past 4096 entries
--- so a live value that changes every frame cannot grow it forever.
function Text:width(str)
  str = tostring(str or "")
  local w = self.cache[str]
  if w then return w end
  local h
  w, h = self.measure(str)
  if self.count > 4096 then self.cache = {} self.count = 0 end
  self.cache[str] = w
  self.count = self.count + 1
  if not self._h and h then self._h = h end
  return w
end

--- Line height of the font (measured once, from "Xg").
function Text:height()
  if not self._h then
    local _, h = self.measure("Xg")
    self._h = h or 8
  end
  return self._h
end

--- `str` cut to fit `maxW`, with ".." appended when something was cut
--- (Spaghetti ui_browse.lua:210). Falls back to the first character when
--- even that plus the dots does not fit.
function Text:ellipsize(str, maxW)
  str = tostring(str or "")
  if self:width(str) <= maxW then return str end
  local n = #str
  while n > 1 do
    n = n - 1
    local cut = str:sub(1, n) .. ".."
    if self:width(cut) <= maxW then return cut end
  end
  return str:sub(1, 1)
end

--- Empties the cache (call when the font or uiScale changes).
function Text:clear()
  self.cache = {}
  self.count = 0
  self._h = nil
end

return M
