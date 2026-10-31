-- layout.lua — rectangle arithmetic for kit/ui. A rect is `{ x, y, w, h }`
-- with NAMED fields, all whole numbers; functions return new tables and
-- never mutate their input.
--
-- It is NOT a constraint solver or a flexbox: fixed sizes and one
-- stretch slot are all a panel row needs.

local M = {}

local floor = math.floor

function M.rect(x, y, w, h)
  return { x = x, y = y, w = w, h = h }
end

--- Shrunk by `dx` on both sides and `dy` (default `dx`) top and bottom.
function M.inset(r, dx, dy)
  dy = dy or dx
  return { x = r.x + dx, y = r.y + dy, w = r.w - dx * 2, h = r.h - dy * 2 }
end

function M.contains(r, x, y)
  return x >= r.x and y >= r.y and x < r.x + r.w and y < r.y + r.h
end

--- Two rects side by side. `spec` is a number (left width) or
--- `{ fixed = w }` (left width) / `{ stretch = w }` (RIGHT width, left
--- stretches). `gap` sits between them.
function M.splitH(r, spec, gap)
  gap = gap or 0
  local leftW
  if type(spec) == "table" then
    if spec.stretch then leftW = r.w - spec.stretch - gap
    else leftW = spec.fixed or 0 end
  else
    leftW = spec
  end
  if leftW < 0 then leftW = 0 end
  if leftW > r.w then leftW = r.w end
  local rightW = r.w - leftW - gap
  if rightW < 0 then rightW = 0 end
  return { x = r.x, y = r.y, w = leftW, h = r.h },
    { x = r.x + leftW + gap, y = r.y, w = rightW, h = r.h }
end

--- Two rects stacked. Same `spec` vocabulary as splitH, for the TOP one.
function M.splitV(r, spec, gap)
  gap = gap or 0
  local topH
  if type(spec) == "table" then
    if spec.stretch then topH = r.h - spec.stretch - gap
    else topH = spec.fixed or 0 end
  else
    topH = spec
  end
  if topH < 0 then topH = 0 end
  if topH > r.h then topH = r.h end
  local botH = r.h - topH - gap
  if botH < 0 then botH = 0 end
  return { x = r.x, y = r.y, w = r.w, h = topH },
    { x = r.x, y = r.y + topH + gap, w = r.w, h = botH }
end

--- One rect per entry of `heights`, stacked from the top, full width.
--- They may run past the bottom of `r`: the caller clips or scrolls.
function M.rows(r, heights, gap)
  gap = gap or 0
  local out = {}
  local y = r.y
  for i = 1, #heights do
    out[i] = { x = r.x, y = y, w = r.w, h = heights[i] }
    y = y + heights[i] + gap
  end
  return out
end

--- `n` equal columns across `r`, `gap` between them, leftovers on the
--- last one (Spaghetti btnrow: `floor((innerW - (n-1)*4) / n)`).
function M.columns(r, n, gap)
  gap = gap or 0
  local out = {}
  if n < 1 then return out end
  local cw = floor((r.w - (n - 1) * gap) / n)
  local x = r.x
  for i = 1, n do
    local w = (i == n) and (r.x + r.w - x) or cw
    out[i] = { x = x, y = r.y, w = w, h = r.h }
    x = x + cw + gap
  end
  return out
end

return M
