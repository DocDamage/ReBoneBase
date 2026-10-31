-- theme.lua — the colours and metrics every kit/ui widget draws with.
--
-- This is the LOOK of Spaghetti's sidebar (Spaghetti/src/ui_panel.lua:
-- TAB_H 18, PAD 6, ROW_H per kind, the 38/38/44 panel and 235/220/120
-- accent) lifted out as plain numbers, so that no widget hardcodes a
-- size or a colour. `M.default()` is the base at 1x; `M.scaled(theme,
-- uiScale)` returns a copy with every metric multiplied (integers).
--
-- It is NOT a style sheet: no inheritance, no per-widget overrides,
-- no host colours. A consumer that wants a different look builds its
-- own table with the same keys.

local M = {}

local floor = math.floor

--- Every colour is `{ r, g, b }` (0..255); every metric is a whole number
--- of logical pixels at 1x.
function M.default()
  return {
    -- colours (Spaghetti ui_panel.lua paint / drawBox / paintTabBar)
    bg          = { 30, 30, 36 },     -- window behind everything
    panel       = { 38, 38, 44 },     -- panel body
    row         = { 38, 38, 44 },     -- a row at rest (same as panel)
    rowHot      = { 46, 46, 54 },     -- hovered row wash
    box         = { 30, 30, 36 },     -- value boxes (slider track, combo)
    boxHot      = { 70, 70, 82 },     -- hovered box (drawBox hot)
    button      = { 52, 52, 58 },     -- buttons (drawBox fill 52)
    frame       = { 22, 22, 26 },     -- 1px borders
    text        = { 225, 225, 230 },  -- labels
    textDim     = { 160, 160, 170 },  -- inactive tab text, hints
    textValue   = { 235, 235, 240 },  -- values inside boxes
    accent      = { 235, 220, 120 },  -- slider fill, switch on, focus
    accentText  = { 22, 22, 26 },     -- text over accent
    disabled    = { 110, 110, 120 },  -- disabled text
    tabBg       = { 30, 30, 36 },
    tabActive   = { 58, 58, 66 },
    sliderTrack = { 30, 30, 36 },
    sliderFill  = { 235, 220, 120 },
    switchOn    = { 235, 220, 120 },
    switchOff   = { 60, 60, 70 },
    dropdownBg  = { 36, 36, 42 },
    separator   = { 62, 62, 72 },
    warning     = { 255, 120, 80 },
    focus       = { 235, 220, 120 },
    cursor      = { 235, 235, 240 },

    -- metrics (logical px at 1x)
    rowH     = 18,   -- combo / number / switch / button row
    sliderH  = 24,   -- label line + track (ROW_H.slider)
    labelH   = 14,   -- ROW_H.label
    sepH     = 7,    -- ROW_H.sep
    sepArrowW = 10,  -- section header: triangle (7px) + gap before the text
    pad      = 6,    -- PAD
    gap      = 4,    -- generic spacing
    colGap   = 14,   -- between side-by-side cells of one row (a right-aligned
                     -- value must not touch the next cell's label)
    tabH     = 18,   -- TAB_H
    tabPad   = 9,    -- text inset inside a tab (tabWidth = text + 18)
    fontH    = 8,    -- line height of the UI font
    charW    = 6,    -- fallback advance when no measurer is around
    boxH     = 16,   -- height of value boxes inside an 18px row
    trackH   = 8,    -- slider track height
    switchW  = 22,   -- switch pill width
    switchH  = 12,
    labelW   = 0,    -- 0 = proportional (see panel.lua LABEL_FRAC)
    dropRowH = 16,   -- Spaghetti dropdown.lua ROW
    scrollStep = 24, -- wheel step over the panel body
    pickerW  = 140,  -- colorpicker minimum width
    pickerSwatchH = 28, -- colorpicker's big swatch
    swatch   = 10,   -- colorpicker palette grid: one (square) cell
    pickerPaletteRows = 4, -- colorpicker palette grid: rows visible before scrolling
    gradientBarH  = 12, -- gradientBar: the bar itself
    gradientMarkH = 6,  -- gradientBar: stop markers below the bar
    uiScale  = 1,
  }
end

--- A copy of `theme` with every metric multiplied by `uiScale` and
--- floored to an integer (colours untouched). `uiScale` < 1 is clamped
--- to 1 so nothing collapses to zero.
function M.scaled(theme, uiScale)
  local s = tonumber(uiScale) or 1
  if s < 1 then s = 1 end
  local out = {}
  for k, v in pairs(theme) do
    if type(v) == "table" then
      out[k] = { v[1], v[2], v[3] }
    elseif type(v) == "number" and k ~= "uiScale" then
      out[k] = floor(v * s)
    else
      out[k] = v
    end
  end
  out.uiScale = s
  return out
end

return M
