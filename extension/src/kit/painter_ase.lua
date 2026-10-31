-- painter_ase.lua — paints a display list of `ops` into a Dialog canvas's
-- graphics context (`gc`), the way VeSprite's `ui.lua` (lines 30-75) does.
--
-- The pattern this exists for: a "pure" window keeps a model and a
-- `paint(w, h) -> ops` function with no Aseprite in it, and this module is
-- the one thing downstream of that which DOES touch Aseprite -- `Color`,
-- `Rectangle`, and (through `kit/buffer.toImage`) `Image`. All three are
-- resolved as globals at CALL time, not at load time, so this file loads
-- headless.
--
-- `M.paint(gc, ops, opts)`:
--   * `gc` is a Dialog canvas's paint-event `context` (or `kit/test/stub`'s
--     fake one);
--   * `ops` is an array of tables, each `{ kind, ... }`;
--   * `opts.toImage(buf)` overrides the image conversion (defaults to
--     `kit/buffer.toImage`); `opts.imageCache` overrides the weak cache
--     table (defaults to a private one, so unrelated windows do not
--     collide).
--
-- Op formats (index 1 is the kind, exactly as VeSprite's `ui.lua` emits
-- them):
--   `{ "rect",  x, y, w, h, r, g, b [, a] }`      -- filled rectangle
--   `{ "frame", x, y, w, h, r, g, b [, a] }`       -- 1px stroked rectangle
--                                                     (a = 0..255, default 255)
--   `{ "line",  r, g, b, strokeWidth, x1, y1, x2, y2, ... }`
--                                                  -- polyline, >=1 segment
--   `{ "text",  x, y, r, g, b, str }`
--   `{ "image", x, y, w, h, buf }`                -- buf: kit/buffer table,
--                                                     drawn at (x,y,w,h),
--                                                     skipped when w/h <= 0
--                                                     or the buffer is empty
--   `{ "drawimage", img, sx, sy, sw, sh, dx, dy, dw, dh }`
--                                                  -- img: an OPAQUE host image
--                                                     (an Aseprite `Image` the
--                                                     consumer already has,
--                                                     e.g. FastFX's rendered
--                                                     frame); straight
--                                                     gc:drawImage, no cache
--   `{ "save" }`, `{ "restore" }`, `{ "clip", x, y, w, h }`
--                                                  -- gc:save() / gc:restore();
--                                                     clip = beginPath + rect +
--                                                     clip(), so pair it with
--                                                     save/restore. (Verify
--                                                     against real Aseprite:
--                                                     GraphicsContext has
--                                                     save/restore/clip/rect
--                                                     since 1.3; not yet in
--                                                     rules/api-aseprite.md
--                                                     from a shipped script.)

local M = {}

--- A weak-keyed cache: `buf -> Image`, so an unchanged buffer table never
--- gets converted twice, and a buffer nobody holds any more is collected.
local defaultCache = setmetatable({}, { __mode = "k" })

local function imageOf(buf, toImage, cache)
  local img = cache[buf]
  if img then return img end
  img = toImage(buf)
  cache[buf] = img
  return img
end

--- `opts.toImage(buf) -> Image` is required only if `ops` contains an
--- "image" op; pass `Buffer.toImage` (kit/buffer.lua) or your own.
function M.paint(gc, ops, opts)
  opts = opts or {}
  local toImage = opts.toImage
  local cache = opts.imageCache or defaultCache

  for i = 1, #ops do
    local o = ops[i]
    local k = o[1]
    if k == "rect" then
      gc.color = Color{ r = o[6], g = o[7], b = o[8], a = o[9] or 255 }
      gc:fillRect(Rectangle(o[2], o[3], o[4], o[5]))
    elseif k == "frame" then
      gc.color = Color{ r = o[6], g = o[7], b = o[8], a = o[9] or 255 }
      gc.strokeWidth = 1
      gc:strokeRect(Rectangle(o[2], o[3], o[4], o[5]))
    elseif k == "line" then
      gc.color = Color{ r = o[2], g = o[3], b = o[4] }
      gc.strokeWidth = o[5]
      gc:beginPath()
      gc:moveTo(o[6], o[7])
      for j = 8, #o - 1, 2 do gc:lineTo(o[j], o[j + 1]) end
      gc:stroke()
    elseif k == "text" then
      gc.color = Color{ r = o[4], g = o[5], b = o[6] }
      gc:fillText(o[7], o[2], o[3])
    elseif k == "image" then
      local buf = o[6]
      if o[4] > 0 and o[5] > 0 and buf and buf.w > 0 and buf.h > 0 then
        local img = imageOf(buf, toImage, cache)
        gc:drawImage(img, 0, 0, buf.w, buf.h, o[2], o[3], o[4], o[5])
      end
    elseif k == "drawimage" then
      if o[2] and o[5] > 0 and o[6] > 0 and o[9] > 0 and o[10] > 0 then
        gc:drawImage(o[2], o[3], o[4], o[5], o[6], o[7], o[8], o[9], o[10])
      end
    elseif k == "save" then
      gc:save()
    elseif k == "restore" then
      gc:restore()
    elseif k == "clip" then
      gc:beginPath()
      gc:rect(Rectangle(o[2], o[3], o[4], o[5]))
      gc:clip()
    end
  end
end

return M
