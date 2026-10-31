-- undo.lua — a generic undo history: snapshots of a document with
-- coalescing by tag, one undo per user gesture.
--
-- Generalizes VeSprite's `doc.lua` (its `newHistory`/`push`/`breakTag`/
-- `undo`/`redo`, doc.lua:433-478): the document type is whatever the
-- caller's `copy`/`restore` functions understand, so this module has no
-- idea what a "layer" or a "node" is.
--
-- Semantics, unchanged from VeSprite:
--   * `push` snapshots the document as it is BEFORE the caller's change;
--   * a tag equal to the PREVIOUS tag does not snapshot again -- that is
--     what turns twenty mousemoves into one undo step;
--   * `push` always sets `hist.dirty = true` and empties the redo stack;
--   * the undo stack is capped at `hist.cap`, trimmed from the front;
--   * `undo`/`redo` move the CURRENT state to the other stack, restore the
--     popped snapshot into `doc` IN PLACE (via `restore`), clear the tag,
--     and set `dirty = true`.

local M = {}

--- A fresh history. `copy(doc) -> snapshot` and `restore(doc, snapshot)`
--- are supplied by the caller: this module never looks inside a document.
function M.new(cap, copy, restore)
  assert(type(copy) == "function", "undo.new: copy function required")
  assert(type(restore) == "function", "undo.new: restore function required")
  return {
    undo = {}, redo = {}, tag = nil, cap = cap or 100,
    dirty = false,
    copy = copy, restore = restore,
  }
end

--- Snapshot `doc` BEFORE a change, unless `tag` merges with the last one.
--- Returns true when it actually snapshotted.
function M.push(hist, doc, tag)
  hist.dirty = true
  if tag ~= nil and tag == hist.tag then return false end
  hist.tag = tag
  local stack = hist.undo
  stack[#stack + 1] = hist.copy(doc)
  if #stack > hist.cap then table.remove(stack, 1) end
  hist.redo = {}
  return true
end

--- A discrete step that must never merge with the one before it.
function M.breakTag(hist)
  hist.tag = nil
end

function M.undo(hist, doc)
  local stack = hist.undo
  if #stack == 0 then return false end
  hist.redo[#hist.redo + 1] = hist.copy(doc)
  hist.restore(doc, table.remove(stack))
  hist.tag = nil
  hist.dirty = true
  return true
end

function M.redo(hist, doc)
  local stack = hist.redo
  if #stack == 0 then return false end
  hist.undo[#hist.undo + 1] = hist.copy(doc)
  hist.restore(doc, table.remove(stack))
  hist.tag = nil
  hist.dirty = true
  return true
end

return M
