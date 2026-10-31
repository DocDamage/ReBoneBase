-- events.lua — a small pointer-gesture machine: raw down/move/up in,
-- click / doubleClick / dragStart / drag / dragEnd out.
--
-- `now` is injected (seconds, any monotonic clock) so tests never sleep.
-- A double click is a second press within DBL_S of the first, inside
-- DBL_PX, same button; a drag starts once the pointer has moved DRAG_PX
-- from the press. `E.captured` is whatever the caller stored on `down`
-- (a widget, a row index...) and rides on every drag/up event, so the
-- thing pressed keeps receiving moves after the pointer leaves it.
--
-- It is NOT a dispatcher: it knows no widgets or rects. The panel feeds
-- it and reads the returned event; FastFX's preview does the same.

local M = {}

M.DBL_S = 0.3
M.DBL_PX = 4
M.DRAG_PX = 3

local Ev = {}
Ev.__index = Ev

function M.new()
  return setmetatable({
    pressed = false, button = nil, x0 = 0, y0 = 0, x = 0, y = 0,
    dragging = false, captured = nil,
    lastClickT = nil, lastClickX = 0, lastClickY = 0, lastClickB = nil,
  }, Ev)
end

local function dist(ax, ay, bx, by)
  local dx, dy = ax - bx, ay - by
  if dx < 0 then dx = -dx end
  if dy < 0 then dy = -dy end
  return dx > dy and dx or dy
end

--- Returns `{ type = "down" | "doubleClick", x, y, button }`. A
--- doubleClick is reported ON THE SECOND PRESS (Aseprite delivers that
--- press as `ondblclick`, so a host may call `E:down` from there too).
--- `capture` is stored as `E.captured`.
function Ev:down(x, y, button, now, capture)
  now = now or 0
  self.pressed, self.button = true, button
  self.x0, self.y0, self.x, self.y = x, y, x, y
  self.dragging = false
  self.captured = capture
  local dbl = self.lastClickT and (now - self.lastClickT) < M.DBL_S
    and self.lastClickB == button
    and dist(x, y, self.lastClickX, self.lastClickY) <= M.DBL_PX
  if dbl then
    self.lastClickT = nil
    return { type = "doubleClick", x = x, y = y, button = button,
      captured = capture }
  end
  self.lastClickT, self.lastClickX, self.lastClickY, self.lastClickB =
    now, x, y, button
  return { type = "down", x = x, y = y, button = button, captured = capture }
end

--- Returns `{ type = "move" }` when nothing is pressed, `"dragStart"`
--- the first time the pointer is DRAG_PX or more from the press, `"drag"`
--- after (both carry `dx, dy` from the press and `captured`), or `"hold"`
--- while pressed inside the threshold.
function Ev:move(x, y)
  self.x, self.y = x, y
  if not self.pressed then return { type = "move", x = x, y = y } end
  local dx, dy = x - self.x0, y - self.y0
  local ev = { x = x, y = y, dx = dx, dy = dy, button = self.button,
    captured = self.captured }
  if self.dragging then
    ev.type = "drag"
  elseif dist(x, y, self.x0, self.y0) >= M.DRAG_PX then
    self.dragging = true
    -- a drag cannot also be half of a double click
    self.lastClickT = nil
    ev.type = "dragStart"
  else
    ev.type = "hold"
  end
  return ev
end

--- Returns `{ type = "click" }` (press and release without a drag) or
--- `{ type = "dragEnd" }`, both with `captured`; `"up"` if nothing was
--- pressed. Clears the capture.
function Ev:up(x, y, button)
  local cap = self.captured
  local ev = { x = x, y = y, button = button or self.button, captured = cap }
  if not self.pressed then
    ev.type = "up"
    return ev
  end
  ev.type = self.dragging and "dragEnd" or "click"
  self.pressed, self.dragging, self.captured = false, false, nil
  return ev
end

--- Drops any gesture in flight (window lost, escape).
function Ev:cancel()
  self.pressed, self.dragging, self.captured = false, false, nil
end

return M
