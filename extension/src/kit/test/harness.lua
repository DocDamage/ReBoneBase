-- harness.lua — the small checker every kit test uses, `lua`/`luajit`
-- headless. Format matches VeSprite's `scripts/run-tests.ps1` expectation:
-- one line per check, `ok   - msg` or `FAIL - msg`, so the runner can
-- count `^ok` lines and grep `^FAIL` ones out of the output.
--
-- Usage:
--   local H = dofile("kit/test/harness.lua")
--   H.check(cond, "msg")
--   H.eq(got, want, "msg")
--   H.done()  -- prints a summary and os.exit(1) on any failure

local M = { failures = 0 }

function M.check(cond, msg)
  if cond then
    print("ok   - " .. msg)
  else
    M.failures = M.failures + 1
    print("FAIL - " .. msg)
  end
  return cond
end

--- Same as check, but the message gets `got` vs `want` on failure.
function M.eq(got, want, msg)
  local cond = got == want
  if cond then
    print("ok   - " .. msg)
  else
    M.failures = M.failures + 1
    print(("FAIL - %s (got %s, want %s)"):format(msg, tostring(got), tostring(want)))
  end
  return cond
end

function M.done()
  print(M.failures == 0 and "ALL OK" or (M.failures .. " FAILURES"))
  os.exit(M.failures == 0 and 0 or 1)
end

return M
