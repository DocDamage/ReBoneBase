-- gif.lua — pure-Lua animated GIF decoder + encoder.
--
-- No host APIs, runs on Lua 5.1 (LuaJIT) and 5.4 alike: bit twiddling is
-- done with arithmetic, not bit libraries. Copied unchanged from
-- Spaghetti's `src/gif.lua` into AseKit's `kit/`, part of the shared
-- library so an extension can render a preview without Aseprite itself.
--
-- decode(raw, opts) -> { w, h, frames = { {w,h,d} ... }, fps }, or nil,
--                 err. Frames are engine buffers (flat RGBA 0..255),
--                 fully composed: disposal methods, local palettes,
--                 interlace and transparency are resolved here, per the
--                 GIF89a spec. `opts.maxFrames` stops after that many --
--                 a caller that only wants the first picture pays for
--                 one frame instead of the whole animation.
-- encode(frames, fps) -> binary string (GIF89a, looping forever).
--                 Colors are quantized to 255 + transparent when the
--                 frames use more; alpha < 128 becomes transparent.

local M = {}

local floor = math.floor
local char = string.char

---------------------------------------
-- Decoder --
---------------------------------------

local function u16le(s, pos)
  return s:byte(pos) + s:byte(pos + 1) * 256
end

--- GIF-variant LZW: codes are packed LSB-first; the code size grows as
--- the dictionary fills and resets on a clear code. Returns a string of
--- palette indices (one byte each).
---
--- The code-size timing is giflib's (what Aseprite and almost everything
--- else writes): the encoder bumps AFTER emitting the code that fills the
--- current width, and the decoder mirrors that by counting CODES READ
--- (`running`), not dictionary entries — the first code after a clear
--- adds no entry but still counts. Getting this off by one decodes the
--- first few hundred pixels and then reads garbage.
local function lzwDecode(minCode, data)
  local clear = 2 ^ minCode
  local eoi = clear + 1
  local codeSize = minCode + 1
  local maxCode = 2 ^ codeSize
  local dict = {}
  local nextCode = eoi + 1
  local running = eoi + 1
  local function resetDict()
    dict = {}
    for i = 0, clear - 1 do dict[i] = char(i) end
    nextCode = eoi + 1
    running = eoi + 1
    codeSize = minCode + 1
    maxCode = 2 ^ codeSize
  end
  resetDict()

  local out = {}
  local pos, bitbuf, bitcnt = 1, 0, 0
  local prev = nil
  while true do
    while bitcnt < codeSize do
      local b = data:byte(pos)
      if not b then return table.concat(out) end -- truncated: keep what we have
      pos = pos + 1
      bitbuf = bitbuf + b * 2 ^ bitcnt
      bitcnt = bitcnt + 8
    end
    local code = bitbuf % (2 ^ codeSize)
    bitbuf = floor(bitbuf / 2 ^ codeSize)
    bitcnt = bitcnt - codeSize
    running = running + 1
    if running > maxCode and codeSize < 12 then
      codeSize = codeSize + 1
      maxCode = 2 ^ codeSize
    end

    if code == clear then
      resetDict()
      prev = nil
    elseif code == eoi then
      return table.concat(out)
    else
      local entry
      if dict[code] then
        entry = dict[code]
      elseif code == nextCode and prev then
        entry = prev .. prev:sub(1, 1) -- the KwKwK case
      else
        return table.concat(out) -- corrupt stream: stop cleanly
      end
      out[#out + 1] = entry
      if prev and nextCode < 4096 then
        dict[nextCode] = prev .. entry:sub(1, 1)
        nextCode = nextCode + 1
      end
      prev = entry
    end
  end
end

local function readPalette(raw, pos, count)
  local pal = {}
  for i = 0, count - 1 do
    pal[i] = { raw:byte(pos), raw:byte(pos + 1), raw:byte(pos + 2) }
    pos = pos + 3
  end
  return pal, pos
end

--- Data sub-blocks: length byte + payload, until a 0 length. Returns the
--- concatenated payload and the position after the terminator.
local function readSubBlocks(raw, pos)
  local parts = {}
  while true do
    local n = raw:byte(pos)
    if not n then return table.concat(parts), pos end
    pos = pos + 1
    if n == 0 then return table.concat(parts), pos end
    parts[#parts + 1] = raw:sub(pos, pos + n - 1)
    pos = pos + n
  end
end

-- Interlaced GIFs store rows in four passes.
local function interlaceRows(h)
  local rows = {}
  for y = 0, h - 1, 8 do rows[#rows + 1] = y end
  for y = 4, h - 1, 8 do rows[#rows + 1] = y end
  for y = 2, h - 1, 4 do rows[#rows + 1] = y end
  for y = 1, h - 1, 2 do rows[#rows + 1] = y end
  return rows
end

function M.decode(raw, opts)
  if type(raw) ~= "string" or #raw < 13 then return nil, "not a GIF" end
  local maxFrames = tonumber(opts and opts.maxFrames)
  if maxFrames and maxFrames < 1 then maxFrames = 1 end
  local sig = raw:sub(1, 6)
  if sig ~= "GIF89a" and sig ~= "GIF87a" then return nil, "not a GIF" end
  local W = u16le(raw, 7)
  local H = u16le(raw, 9)
  if W < 1 or H < 1 then return nil, "empty canvas" end
  local packed = raw:byte(11)
  local pos = 14
  local gct = nil
  if packed >= 128 then -- GCT flag (bit 7)
    local gctSize = 2 ^ ((packed % 8) + 1)
    gct, pos = readPalette(raw, pos, gctSize)
  end

  -- the composition canvas persists across frames
  local canvas = {}
  for i = 1, W * H * 4 do canvas[i] = 0 end

  local frames = {}
  local delays = {}
  -- pending graphic-control state (set by the GCE before an image)
  local transIdx, delay, disposal = nil, 10, 0
  -- what the PREVIOUS frame asked to happen to its pixels afterwards
  local prevDisposal, prevRect, prevSnapshot = 0, nil, nil

  while true do
    local block = raw:byte(pos)
    if not block or block == 0x3B then break end
    pos = pos + 1

    if block == 0x21 then -- extension
      local label = raw:byte(pos)
      pos = pos + 1
      if label == 0xF9 then -- graphic control
        local size = raw:byte(pos)
        local p2 = raw:byte(pos + 1)
        delay = u16le(raw, pos + 2)
        if delay < 1 then delay = 10 end -- the 0-delay convention
        disposal = floor(p2 / 4) % 8
        transIdx = (p2 % 2 == 1) and raw:byte(pos + 4) or nil
        pos = pos + size + 1
        -- the terminator (and any stray sub-blocks)
        local _, after = readSubBlocks(raw, pos)
        pos = after
      else -- comment / application / plain text: skip
        local _, after = readSubBlocks(raw, pos)
        pos = after
      end
    elseif block == 0x2C then -- image descriptor
      local ix = u16le(raw, pos)
      local iy = u16le(raw, pos + 2)
      local iw = u16le(raw, pos + 4)
      local ih = u16le(raw, pos + 6)
      local ipacked = raw:byte(pos + 8)
      pos = pos + 9
      local pal = gct
      if ipacked >= 128 then -- local palette
        local lctSize = 2 ^ ((ipacked % 8) + 1)
        pal, pos = readPalette(raw, pos, lctSize)
      end
      if not pal then return nil, "no palette" end
      local interlaced = floor(ipacked / 64) % 2 == 1

      -- settle the previous frame's disposal before drawing this one
      if prevDisposal == 2 and prevRect then
        for y = prevRect.y, prevRect.y + prevRect.h - 1 do
          if y >= 0 and y < H then
            local base = (y * W + prevRect.x) * 4
            for x = 0, prevRect.w - 1 do
              if prevRect.x + x >= 0 and prevRect.x + x < W then
                local i = base + x * 4
                canvas[i + 1], canvas[i + 2] = 0, 0
                canvas[i + 3], canvas[i + 4] = 0, 0
              end
            end
          end
        end
      elseif prevDisposal == 3 and prevSnapshot then
        for i = 1, W * H * 4 do canvas[i] = prevSnapshot[i] end
      end

      -- this frame wants its "before" state back afterwards
      if disposal == 3 then
        prevSnapshot = {}
        for i = 1, W * H * 4 do prevSnapshot[i] = canvas[i] end
      end

      local minCode = raw:byte(pos)
      pos = pos + 1
      local data, after = readSubBlocks(raw, pos)
      pos = after
      local indices = lzwDecode(minCode, data)

      local rows
      if interlaced then
        rows = interlaceRows(ih)
      else
        rows = {}
        for y = 0, ih - 1 do rows[y + 1] = y end
      end
      local k = 1
      for r = 1, #rows do
        local y = iy + rows[r]
        for x = 0, iw - 1 do
          local idx = indices:byte(k)
          k = k + 1
          if idx and idx ~= transIdx then
            local px, py = ix + x, y
            if px >= 0 and px < W and py >= 0 and py < H then
              local c = pal[idx]
              if c then
                local i = (py * W + px) * 4
                canvas[i + 1], canvas[i + 2] = c[1], c[2]
                canvas[i + 3], canvas[i + 4] = c[3], 255
              end
            end
          end
        end
      end

      local buf = { w = W, h = H, d = {} }
      local d = buf.d
      for i = 1, W * H * 4 do d[i] = canvas[i] end
      frames[#frames + 1] = buf
      delays[#delays + 1] = delay

      prevDisposal = disposal
      prevRect = { x = ix, y = iy, w = iw, h = ih }
      transIdx, delay, disposal = nil, 10, 0
      -- the caller asked for a prefix of the animation: everything past
      -- it is LZW nobody is going to look at
      if maxFrames and #frames >= maxFrames then break end
    else
      return nil, "unknown block 0x" .. string.format("%02X", block)
    end
  end

  if #frames == 0 then return nil, "no frames" end
  local total = 0
  for _, cs in ipairs(delays) do total = total + cs end
  local avg = total / #delays -- centiseconds
  local fps = floor(100 / math.max(1, avg) + 0.5)
  if fps < 1 then fps = 1 end
  if fps > 60 then fps = 60 end
  return { w = W, h = H, frames = frames, fps = fps }
end

---------------------------------------
-- Encoder --
---------------------------------------

--- The palette for a frame set: up to 255 colors + index 0 reserved for
--- transparency. Exact colors when they fit; otherwise a 4-bit-per-channel
--- popularity quantization (pixel art rarely gets that far).
local function buildPalette(frames)
  local counts, order = {}, {}
  for _, f in ipairs(frames) do
    local d = f.d
    for i = 1, f.w * f.h * 4, 4 do
      if d[i + 3] >= 128 then
        local key = d[i] * 65536 + d[i + 1] * 256 + d[i + 2]
        if not counts[key] then order[#order + 1] = key end
        counts[key] = (counts[key] or 0) + 1
      end
    end
  end

  local exact = #order <= 255
  if not exact then
    -- rebin to 4 bits per channel, remember each bin's weighted mean
    local bins, binOrder = {}, {}
    for _, key in ipairs(order) do
      local n = counts[key]
      local r = floor(key / 65536)
      local g = floor(key / 256) % 256
      local b = key % 256
      local bkey = floor(r / 16) * 256 + floor(g / 16) * 16 + floor(b / 16)
      local bin = bins[bkey]
      if not bin then
        bin = { n = 0, r = 0, g = 0, b = 0 }
        bins[bkey] = bin
        binOrder[#binOrder + 1] = bkey
      end
      bin.n = bin.n + n
      bin.r = bin.r + r * n
      bin.g = bin.g + g * n
      bin.b = bin.b + b * n
    end
    table.sort(binOrder, function(a, b) return bins[a].n > bins[b].n end)
    counts, order = {}, {}
    for i = 1, math.min(255, #binOrder) do
      local bin = bins[binOrder[i]]
      local key = floor(bin.r / bin.n + 0.5) * 65536
        + floor(bin.g / bin.n + 0.5) * 256 + floor(bin.b / bin.n + 0.5)
      if not counts[key] then
        order[#order + 1] = key
        counts[key] = bin.n
      end
    end
  end

  -- palette entry i (1-based list) = gif index i; index 0 = transparent
  local indexOf = {}
  local pal = {}
  for i, key in ipairs(order) do
    pal[i] = { floor(key / 65536), floor(key / 256) % 256, key % 256 }
    indexOf[key] = i
  end
  return pal, indexOf, exact
end

--- Nearest palette entry for a color that was quantized away. Memoized:
--- the same missing color is asked for once.
local function nearestIndex(pal, cacheTbl, r, g, b)
  local key = r * 65536 + g * 256 + b
  local hit = cacheTbl[key]
  if hit then return hit end
  local best, bestD = 1, math.huge
  for i = 1, #pal do
    local p = pal[i]
    local dr, dg, db = p[1] - r, p[2] - g, p[3] - b
    local dist = 2 * dr * dr + 4 * dg * dg + 3 * db * db
    if dist < bestD then
      bestD = dist
      best = i
    end
  end
  cacheTbl[key] = best
  return best
end

--- GIF-variant LZW compression, the exact mirror of lzwDecode (and of
--- giflib): codes LSB-first, dictionary keyed by (prefix code, next byte).
--- The code size bumps AFTER emitting — a code that just fits the old
--- width still travels at the old width — and the table clears at 4095.
local function lzwEncode(minCode, indices)
  local clear = 2 ^ minCode
  local eoi = clear + 1
  local codeSize = minCode + 1
  local maxCode = 2 ^ codeSize
  local dict = {}
  local nextCode = eoi + 1

  local out = {}
  local bitbuf, bitcnt = 0, 0
  -- giflib's EGifCompressOutput: write at the CURRENT width, then widen
  -- if the next free slot no longer fits it
  local function emit(code)
    bitbuf = bitbuf + code * 2 ^ bitcnt
    bitcnt = bitcnt + codeSize
    while bitcnt >= 8 do
      out[#out + 1] = char(bitbuf % 256)
      bitbuf = floor(bitbuf / 256)
      bitcnt = bitcnt - 8
    end
    if nextCode >= maxCode and codeSize < 12 then
      codeSize = codeSize + 1
      maxCode = 2 ^ codeSize
    end
  end

  emit(clear)
  local prefix = nil
  for i = 1, #indices do
    local b = indices:byte(i)
    if prefix == nil then
      prefix = b
    else
      local key = prefix * 256 + b
      local hit = dict[key]
      if hit then
        prefix = hit
      else
        emit(prefix)
        if nextCode >= 4095 then
          emit(clear)
          dict = {}
          nextCode = eoi + 1
          codeSize = minCode + 1
          maxCode = 2 ^ codeSize
        else
          dict[key] = nextCode
          nextCode = nextCode + 1
        end
        prefix = b
      end
    end
  end
  if prefix ~= nil then emit(prefix) end
  emit(eoi)
  if bitcnt > 0 then out[#out + 1] = char(bitbuf % 256) end
  return table.concat(out)
end

local function w16le(v)
  return char(v % 256, floor(v / 256) % 256)
end

--- Split a payload into 255-byte data sub-blocks with terminator.
local function subBlocks(payload)
  local parts = {}
  for i = 1, #payload, 255 do
    local chunk = payload:sub(i, i + 254)
    parts[#parts + 1] = char(#chunk) .. chunk
  end
  parts[#parts + 1] = char(0)
  return table.concat(parts)
end

--- frames = { {w,h,d} ... } (all the same size), fps 1..60.
--- Returns the GIF89a file as a string, or nil, err.
function M.encode(frames, fps)
  if type(frames) ~= "table" or #frames == 0 then return nil, "no frames" end
  local W, H = frames[1].w, frames[1].h
  if not W or not H or W < 1 or H < 1 then return nil, "bad frame" end
  fps = math.max(1, math.min(60, floor(tonumber(fps) or 12)))
  local delayCs = math.max(2, floor(100 / fps + 0.5))

  local pal, indexOf, exact = buildPalette(frames)
  local nearCache = {}
  -- palette field: 2^(N+1) slots covering #pal + 1 (the transparent 0)
  local depth = 1
  while 2 ^ depth < #pal + 1 do depth = depth + 1 end
  if depth > 8 then return nil, "palette overflow" end
  local slots = 2 ^ depth
  local minCode = math.max(2, depth)

  local outp = {
    "GIF89a",
    w16le(W), w16le(H),
    char(128 + (depth - 1)), -- GCT present, size 2^depth
    char(0), -- background = index 0
    char(0), -- no aspect ratio
  }
  -- the global palette: transparent slot, colors, padding
  local palBytes = { char(0, 0, 0) }
  for i = 1, #pal do
    palBytes[#palBytes + 1] = char(pal[i][1], pal[i][2], pal[i][3])
  end
  for _ = #pal + 2, slots do palBytes[#palBytes + 1] = char(0, 0, 0) end
  outp[#outp + 1] = table.concat(palBytes)

  -- NETSCAPE2.0: loop forever
  outp[#outp + 1] = char(0x21, 0xFF, 11) .. "NETSCAPE2.0"
    .. char(3, 1) .. w16le(0) .. char(0)

  for _, f in ipairs(frames) do
    -- graphic control: dispose to background + transparency on index 0
    outp[#outp + 1] = char(0x21, 0xF9, 4, 2 * 4 + 1)
      .. w16le(delayCs) .. char(0, 0)
    -- image descriptor: full frame, no local palette, no interlace
    outp[#outp + 1] = char(0x2C) .. w16le(0) .. w16le(0)
      .. w16le(W) .. w16le(H) .. char(0)
    local idx = {}
    local d = f.d
    local k = 1
    for i = 1, W * H * 4, 4 do
      if d[i + 3] < 128 then
        idx[k] = "\0"
      else
        local key = d[i] * 65536 + d[i + 1] * 256 + d[i + 2]
        local pi = indexOf[key]
        if not pi then
          pi = nearestIndex(pal, nearCache, d[i], d[i + 1], d[i + 2])
          if exact then indexOf[key] = pi end
        end
        idx[k] = char(pi)
      end
      k = k + 1
    end
    outp[#outp + 1] = char(minCode)
      .. subBlocks(lzwEncode(minCode, table.concat(idx)))
  end

  outp[#outp + 1] = char(0x3B)
  return table.concat(outp)
end

return M
