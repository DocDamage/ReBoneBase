-- host.lua — the common boot sequence every AseKit extension's plugin.lua
-- needs, extracted from VeSprite/plugin.lua and Spaghetti/plugin.lua:1-25,
-- 440-466.
--
-- Not a dependency at runtime beyond this one file: `app.*` is resolved
-- lazily, inside the functions `M.boot` returns, never at load time, so
-- this module loads fine under `kit/test/stub.lua` too.
--
-- Usage in a template's plugin.lua: plugin.lua computes its own root with
-- `Host.scriptRoot()` (same trick, called at file scope so the stack still
-- shows plugin.lua) and passes it in as `spec.root`, since `onclick`
-- fires later from Aseprite's own stack and cannot resolve it lazily:
--   local Host = dofile(app.fs.joinPath(ROOT, "src", "kit/host.lua"))
--   local H = Host.boot{
--     id = "__id__",
--     version = VERSION,
--     root = ROOT,
--     command = { id = "__ID___Open", title = "__NAME__" },
--     open = function(State, plugin) State.Ui.open(State) end,
--     modules = function(State, load)
--       State.Buffer = State.Buffer or load("kit/buffer.lua")
--     end,
--     persist = function(State, plugin) end,
--     close = function(State) end,
--   }
--   function init(plugin) H.init(plugin) end
--   function exit(plugin) H.exit(plugin) end

local M = {}

--- The directory a script lives in, from inside it: `debug.getinfo(level,
--- "S").source`, minus the leading `@`, minus the file name. `level`
--- defaults to 2 (the caller of this function), so plugin.lua can call
--- `Host.scriptRoot()` directly.
function M.scriptRoot(level)
  local src = debug.getinfo(level or 2, "S").source
  if src:sub(1, 1) == "@" then src = src:sub(2) end
  return app.fs.filePath(src)
end

--- `Host.loader(root)` returns a `load(rel)` that dofiles a path relative
--- to `<root>/src/`, e.g. `load("kit/buffer.lua")`.
function M.loader(root)
  return function(rel)
    return dofile(app.fs.joinPath(root, "src", rel))
  end
end

--- Boots one extension: registers the command, and builds `init`/`exit`
--- entry points that manage a single global state table.
---
--- `spec`:
---   id       - string, used for the `_G` keys (`__<id>_State`, `__<id>_plugin`)
---   version  - string, State resets when it does not match `State.__version`
---   command  - { id, title, group = "file_scripts" }
---   shortcut - optional, passed through to plugin:newCommand
---   open(State, plugin)    - called on every click, after modules load
---   modules(State, load)   - populate State.* with `load("path.lua")`
---   persist(State, plugin) - optional, called from exit before close
---   close(State)           - optional, called from exit after persist
function M.boot(spec)
  assert(spec and spec.id, "Host.boot: spec.id required")
  assert(spec.command and spec.command.id, "Host.boot: spec.command.id required")
  assert(spec.open, "Host.boot: spec.open required")

  -- `spec.root` is the recommended path (plugin.lua computes it with
  -- `Host.scriptRoot()` at file scope and passes it in); `M.scriptRoot(3)`
  -- is a fallback for callers that invoke `Host.boot{...}` directly from
  -- plugin.lua's top level without precomputing it themselves.
  local root = spec.root or M.scriptRoot(3)

  local stateKey = "__" .. spec.id .. "_State"
  local pluginKey = "__" .. spec.id .. "_plugin"

  local function ensureState()
    local State = _G[stateKey]
    if State and State.__version ~= spec.version then
      State = nil
    end
    if not State then
      State = { __version = spec.version, VERSION = spec.version }
      _G[stateKey] = State
    end
    return State
  end

  local function loadModules(State)
    if spec.modules then
      spec.modules(State, M.loader(root))
    end
  end

  local function openIt(plugin)
    local State = ensureState()
    loadModules(State)
    spec.open(State, plugin)
  end

  local function init(plugin)
    _G[pluginKey] = plugin
    plugin:newCommand{
      id = spec.command.id,
      title = spec.command.title,
      group = spec.command.group or "file_scripts",
      shortcut = spec.shortcut,
      onclick = function() openIt(plugin) end,
    }
  end

  local function exit(plugin)
    _G[pluginKey] = nil
    local State = _G[stateKey]
    if State then
      if spec.persist then pcall(spec.persist, State, plugin) end
      if spec.close then pcall(spec.close, State) end
    end
    _G[stateKey] = nil
  end

  return { init = init, exit = exit }
end

return M
