-- m3d.lua — the small 3D math the mocap parsers (bvh.lua, gltf.lua) need.
-- Pure Lua, dual-VM, no state.
--
--   vec3        three plain numbers (x, y, z), never a table
--   quaternion  { x, y, z, w } (glTF order), unit length when it is a rotation
--   mat4        flat array of 16 numbers, COLUMN-major (glTF layout): the
--               translation lives at 13..15, column c at 4c+1 .. 4c+4

local M = {}

local sqrt, sin, cos, acos, rad = math.sqrt, math.sin, math.cos, math.acos, math.rad

---------------------------------------
-- Quaternions
---------------------------------------

--- The product a * b (apply b first, then a).
function M.quatMul(a, b)
  local ax, ay, az, aw = a[1], a[2], a[3], a[4]
  local bx, by, bz, bw = b[1], b[2], b[3], b[4]
  return {
    aw * bx + ax * bw + ay * bz - az * by,
    aw * by - ax * bz + ay * bw + az * bx,
    aw * bz + ax * by - ay * bx + az * bw,
    aw * bw - ax * bx - ay * by - az * bz,
  }
end

--- A unit copy of q (identity when q is degenerate).
function M.quatNormalize(q)
  local x, y, z, w = q[1], q[2], q[3], q[4]
  local n = sqrt(x * x + y * y + z * z + w * w)
  if n < 1e-12 then return { 0, 0, 0, 1 } end
  return { x / n, y / n, z / n, w / n }
end

--- Spherical interpolation along the shortest path, t in 0..1.
function M.quatSlerp(a, b, t)
  local ax, ay, az, aw = a[1], a[2], a[3], a[4]
  local bx, by, bz, bw = b[1], b[2], b[3], b[4]
  local d = ax * bx + ay * by + az * bz + aw * bw
  if d < 0 then d, bx, by, bz, bw = -d, -bx, -by, -bz, -bw end
  local ka, kb
  if d > 0.9995 then -- nearly parallel: lerp, then normalize
    ka, kb = 1 - t, t
  else
    local th = acos(d)
    local s = sin(th)
    ka, kb = sin((1 - t) * th) / s, sin(t * th) / s
  end
  return M.quatNormalize({ ka * ax + kb * bx, ka * ay + kb * by, ka * az + kb * bz, ka * aw + kb * bw })
end

--- Rotation of `deg` degrees about one axis ("x", "y" or "z").
function M.quatAxis(axis, deg)
  local h = rad(deg) / 2
  local s, c = sin(h), cos(h)
  if axis == "x" then return { s, 0, 0, c } end
  if axis == "y" then return { 0, s, 0, c } end
  return { 0, 0, s, c }
end

--- The vector (x, y, z) rotated by q; returns x, y, z.
function M.quatRotate(q, x, y, z)
  local qx, qy, qz, qw = q[1], q[2], q[3], q[4]
  -- t = 2 * (u x v); v' = v + w t + u x t
  local tx = 2 * (qy * z - qz * y)
  local ty = 2 * (qz * x - qx * z)
  local tz = 2 * (qx * y - qy * x)
  return x + qw * tx + (qy * tz - qz * ty),
         y + qw * ty + (qz * tx - qx * tz),
         z + qw * tz + (qx * ty - qy * tx)
end

---------------------------------------
-- 4x4 matrices
---------------------------------------

function M.mat4Identity()
  return { 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1 }
end

--- Translation t {x,y,z}, rotation r {x,y,z,w}, scale s {x,y,z}; any may be nil.
function M.mat4FromTRS(t, r, s)
  local x, y, z, w = 0, 0, 0, 1
  if r then x, y, z, w = r[1], r[2], r[3], r[4] end
  local sx, sy, sz = 1, 1, 1
  if s then sx, sy, sz = s[1], s[2], s[3] end
  local xx, yy, zz = x * x, y * y, z * z
  local xy, xz, yz = x * y, x * z, y * z
  local wx, wy, wz = w * x, w * y, w * z
  return {
    (1 - 2 * (yy + zz)) * sx, 2 * (xy + wz) * sx, 2 * (xz - wy) * sx, 0,
    2 * (xy - wz) * sy, (1 - 2 * (xx + zz)) * sy, 2 * (yz + wx) * sy, 0,
    2 * (xz + wy) * sz, 2 * (yz - wx) * sz, (1 - 2 * (xx + yy)) * sz, 0,
    t and t[1] or 0, t and t[2] or 0, t and t[3] or 0, 1,
  }
end

--- The product a * b (apply b first, then a).
function M.mat4Mul(a, b)
  local out = {}
  for c = 0, 3 do
    local b1, b2, b3, b4 = b[c * 4 + 1], b[c * 4 + 2], b[c * 4 + 3], b[c * 4 + 4]
    for r = 1, 4 do
      out[c * 4 + r] = a[r] * b1 + a[4 + r] * b2 + a[8 + r] * b3 + a[12 + r] * b4
    end
  end
  return out
end

--- The point (x, y, z) transformed by m; returns x, y, z.
function M.mat4Point(m, x, y, z)
  return m[1] * x + m[5] * y + m[9] * z + m[13],
         m[2] * x + m[6] * y + m[10] * z + m[14],
         m[3] * x + m[7] * y + m[11] * z + m[15]
end

return M
