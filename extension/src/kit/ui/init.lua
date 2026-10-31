-- init.lua — the entry point of kit/ui: loads the sibling modules by
-- path (relative to THIS file, via debug.getinfo, the loadSibling pattern
-- of FastFX/src/engine/engine.lua) and returns them in one table:
--   `{ theme, text, layout, widgets, panel, dropdown, events, colorpicker }`
-- so a host does `local UI = dofile(root .. "/kit/ui/init.lua")`.
-- `widgets` already includes widgets_extra.lua's gradientBar/list
-- (widgets.lua folds it in itself).
--
-- It is NOT a loader for anything else and adds no behaviour of its own.

local function loadSibling(name)
  local info = debug.getinfo(1, "S")
  local src = info and info.source or ""
  if src:sub(1, 1) == "@" then src = src:sub(2) end
  local dir = src:match("^(.*)[/\\]") or "."
  local chunk, err = loadfile(dir .. "/" .. name .. ".lua")
  if not chunk then error("kit/ui: could not load " .. name .. ": " .. tostring(err)) end
  return chunk()
end

return {
  theme = loadSibling("theme"),
  text = loadSibling("text"),
  layout = loadSibling("layout"),
  widgets = loadSibling("widgets"),
  panel = loadSibling("panel"),
  dropdown = loadSibling("dropdown"),
  events = loadSibling("events"),
  colorpicker = loadSibling("colorpicker"),
}
