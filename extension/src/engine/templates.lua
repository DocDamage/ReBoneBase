-- templates.lua — ready-made rigs to start from: Humanoid, Profile (a
-- slim humanoid seen from the side), Chibi, Quadruped, Winged and
-- Serpent, plus numbered variations of the first three (Humanoid 2..6,
-- Profile 2..5, Chibi 2..6: other builds, the last chibis from the side).
-- Pure Lua, dual-VM.
--
-- A template is DATA drawn on a 64 x 64 canvas (y down, feet near y = 60).
-- Heads are round: a short bone (about 2 px) under a radius twice that,
-- not a long capsule (an egg).
-- and `M.build(name, w, h)` scales it to the real canvas (by min(w, h) / 64,
-- centred, standing on the bottom edge). Bones are listed parents first:
--
--   { name, part, parent = "name", h = { x, y }, t = { x, y }, r = { head, tail },
--     bl = { { t, d } }, br = { { t, d } }, side = true }
--
-- No `h` = the head sits on the parent's tail (connected). `side = true`
-- marks a bone of the character's LEFT side (screen right in a front
-- view): the builder adds its twin on the right, reflected across x = 32,
-- with `_L` -> `_R` in bone and part names, the parent swapped for its
-- twin when it has one, bulges swapped side for side, and both linked as
-- `twin` so the editor's mirror keeps them in step. Front-view templates
-- also turn the mirror on.
--
-- Parts: { name, z }. Limbs of the far side go below the body (z < 0),
-- near or front limbs above it.

local M = {}

M.Rig = nil

function M.bind(Rig)
  M.Rig = Rig
  return M
end

M.NAMES = {
  "Humanoid", "Humanoid 2", "Humanoid 3", "Humanoid 4", "Humanoid 5", "Humanoid 6",
  "Profile", "Profile 2", "Profile 3", "Profile 4", "Profile 5",
  "Chibi", "Chibi 2", "Chibi 3", "Chibi 4", "Chibi 5", "Chibi 6",
  "Quadruped", "Winged", "Serpent",
}
M.SIZE = 64

local T = {}

T.Humanoid = {
  mirror = true,
  parts = { { "leg_L", -1 }, { "leg_R", -1 }, { "torso", 0 }, { "head", 1 },
    { "arm_L", 2 }, { "arm_R", 2 } },
  bones = {
    { "hips", "torso", h = { 32, 37 }, t = { 32, 31 }, r = { 5.5, 5.5 } },
    { "chest", "torso", parent = "hips", t = { 32, 20 }, r = { 5.5, 6.5 },
      bl = { { 0.75, 0.5 } }, br = { { 0.75, 0.5 } } },
    { "neck", "torso", parent = "chest", t = { 32, 16 }, r = { 2.5, 2.5 } },
    { "head", "head", parent = "neck", h = { 32, 12 }, t = { 32, 11 }, r = { 5.4, 5.3 } },
    { "upper_arm_L", "arm_L", parent = "chest", h = { 38.5, 21 }, t = { 41, 31 },
      r = { 2.5, 2 }, side = true, bl = { { 0.35, 0.5 } } },
    { "forearm_L", "arm_L", parent = "upper_arm_L", t = { 43, 40 }, r = { 2, 1.7 }, side = true },
    { "hand_L", "arm_L", parent = "forearm_L", t = { 44, 44 }, r = { 1.8, 1.5 }, side = true },
    { "thigh_L", "leg_L", parent = "hips", h = { 35, 37 }, t = { 36, 48 },
      r = { 3.5, 2.6 }, side = true, br = { { 0.3, 0.6 } } },
    { "shin_L", "leg_L", parent = "thigh_L", t = { 36, 58 }, r = { 2.6, 2 }, side = true,
      br = { { 0.3, 0.6 } } },
    { "foot_L", "leg_L", parent = "shin_L", t = { 39, 60 }, r = { 1.8, 1.5 }, side = true },
  },
}

-- A slim, stylised humanoid seen three-quarters from the side, facing
-- right (screen +x) and turned a little towards the viewer: shoulders and
-- hips apart, the far side (_L) a step ahead and under the body, the near
-- side (_R, the character's right when it faces right) a step behind and
-- over it. No mirror: both sides are drawn. The Import 3D auto map and a
-- side camera agree with these names. Bulges: on a bone pointing up `br`
-- is the front (chest), on one pointing down `bl` is the front (thigh)
-- and `br` the back (calf).
T.Profile = {
  parts = { { "arm_L", -2 }, { "leg_L", -1 }, { "torso", 0 }, { "head", 1 },
    { "leg_R", 2 }, { "arm_R", 3 } },
  bones = {
    { "hips", "torso", h = { 31, 33 }, t = { 31.5, 28 }, r = { 4, 2.9 },
      bl = { { 0.2, 1.2 } } },
    { "chest", "torso", parent = "hips", t = { 32.5, 19.5 }, r = { 2.9, 4.4 },
      br = { { 0.65, 1.2 } }, bl = { { 0.75, 0.4 } } },
    { "neck", "torso", parent = "chest", t = { 33.5, 15 }, r = { 1.6, 1.5 } },
    { "head", "head", parent = "neck", h = { 34, 11.5 }, t = { 34.6, 9.6 }, r = { 4.5, 4.4 } },
    { "upper_arm_L", "arm_L", parent = "chest", h = { 35.8, 20.5 }, t = { 36.3, 28.5 },
      r = { 1.6, 1.25 } },
    { "forearm_L", "arm_L", parent = "upper_arm_L", t = { 37.8, 35 }, r = { 1.25, 0.95 } },
    { "hand_L", "arm_L", parent = "forearm_L", t = { 38.8, 38 }, r = { 1.05, 0.85 } },
    { "thigh_L", "leg_L", parent = "hips", h = { 33, 34 }, t = { 34, 45.5 },
      r = { 2.5, 1.7 }, bl = { { 0.35, 0.7 } } },
    { "shin_L", "leg_L", parent = "thigh_L", t = { 34, 57 }, r = { 1.6, 1.05 },
      br = { { 0.25, 0.8 } } },
    { "foot_L", "leg_L", parent = "shin_L", t = { 38, 59 }, r = { 1.15, 0.8 } },
    { "upper_arm_R", "arm_R", parent = "chest", h = { 28.8, 20.5 }, t = { 28.3, 28.5 },
      r = { 1.8, 1.4 } },
    { "forearm_R", "arm_R", parent = "upper_arm_R", t = { 29.8, 35.5 }, r = { 1.4, 1.05 } },
    { "hand_R", "arm_R", parent = "forearm_R", t = { 30.8, 38.5 }, r = { 1.15, 0.9 } },
    { "thigh_R", "leg_R", parent = "hips", h = { 29.5, 34 }, t = { 30.5, 45.5 },
      r = { 2.8, 1.9 }, bl = { { 0.35, 0.9 } } },
    { "shin_R", "leg_R", parent = "thigh_R", t = { 30, 57.5 }, r = { 1.8, 1.2 },
      br = { { 0.25, 1 } } },
    { "foot_R", "leg_R", parent = "shin_R", t = { 34.5, 59.5 }, r = { 1.3, 0.9 } },
  },
}

T.Chibi = {
  mirror = true,
  parts = { { "leg_L", -1 }, { "leg_R", -1 }, { "torso", 0 }, { "arm_L", 1 },
    { "arm_R", 1 }, { "head", 2 } },
  bones = {
    { "hips", "torso", h = { 32, 47 }, t = { 32, 42 }, r = { 5.5, 5.5 } },
    { "chest", "torso", parent = "hips", t = { 32, 35 }, r = { 5.5, 5 } },
    { "neck", "torso", parent = "chest", t = { 32, 32 }, r = { 2.5, 2.5 } },
    { "head", "head", parent = "neck", h = { 32, 26.8 }, t = { 32, 24.8 }, r = { 6.2, 6.15 } },
    { "upper_arm_L", "arm_L", parent = "chest", h = { 36.5, 36 }, t = { 39.5, 42 },
      r = { 2.4, 2.2 }, side = true },
    { "forearm_L", "arm_L", parent = "upper_arm_L", t = { 41, 45 }, r = { 2.2, 2.2 }, side = true },
    { "hand_L", "arm_L", parent = "forearm_L", t = { 41.5, 47.5 }, r = { 2.4, 2.4 }, side = true },
    { "thigh_L", "leg_L", parent = "hips", h = { 35, 47 }, t = { 35.5, 53 },
      r = { 3.2, 2.8 }, side = true },
    { "shin_L", "leg_L", parent = "thigh_L", t = { 35.5, 58 }, r = { 2.8, 2.6 }, side = true },
    { "foot_L", "leg_L", parent = "shin_L", t = { 37.5, 60 }, r = { 2.4, 2.2 }, side = true },
  },
}

-- Variations, numbered: the same bones and parts as the template they
-- vary (the IK, the mirror and the Import 3D auto map work alike), other
-- proportions.

local FRONT_PARTS = { { "leg_L", -1 }, { "leg_R", -1 }, { "torso", 0 }, { "head", 1 },
  { "arm_L", 2 }, { "arm_R", 2 } }
local SIDE_PARTS = { { "arm_L", -2 }, { "leg_L", -1 }, { "torso", 0 }, { "head", 1 },
  { "leg_R", 2 }, { "arm_R", 3 } }
local CHIBI_PARTS = { { "leg_L", -1 }, { "leg_R", -1 }, { "torso", 0 }, { "arm_L", 1 },
  { "arm_R", 1 }, { "head", 2 } }
local CHIBI_SIDE_PARTS = { { "arm_L", -2 }, { "leg_L", -1 }, { "torso", 0 }, { "leg_R", 1 },
  { "arm_R", 2 }, { "head", 3 } }

-- stocky: short and broad, thick limbs
T["Humanoid 2"] = {
  mirror = true,
  parts = FRONT_PARTS,
  bones = {
    { "hips", "torso", h = { 32, 44 }, t = { 32, 38 }, r = { 7, 7 } },
    { "chest", "torso", parent = "hips", t = { 32, 28 }, r = { 7.5, 8.5 },
      bl = { { 0.7, 0.6 } }, br = { { 0.7, 0.6 } } },
    { "neck", "torso", parent = "chest", t = { 32, 24 }, r = { 3.2, 3.2 } },
    { "head", "head", parent = "neck", h = { 32, 18.5 }, t = { 32, 17.5 }, r = { 5.6, 5.5 } },
    { "upper_arm_L", "arm_L", parent = "chest", h = { 40, 29 }, t = { 43, 38 },
      r = { 3.2, 2.8 }, side = true, bl = { { 0.35, 0.6 } } },
    { "forearm_L", "arm_L", parent = "upper_arm_L", t = { 45, 45 }, r = { 2.8, 2.4 }, side = true },
    { "hand_L", "arm_L", parent = "forearm_L", t = { 46, 48.5 }, r = { 2.4, 2.1 }, side = true },
    { "thigh_L", "leg_L", parent = "hips", h = { 36, 44 }, t = { 36.5, 52 },
      r = { 4, 3.4 }, side = true, br = { { 0.3, 0.6 } } },
    { "shin_L", "leg_L", parent = "thigh_L", t = { 36.5, 58 }, r = { 3.4, 2.8 }, side = true },
    { "foot_L", "leg_L", parent = "shin_L", t = { 39.5, 60 }, r = { 2.4, 2 }, side = true },
  },
}

-- tall and slim: long legs, a small head
T["Humanoid 3"] = {
  mirror = true,
  parts = FRONT_PARTS,
  bones = {
    { "hips", "torso", h = { 32, 34 }, t = { 32, 29 }, r = { 4.5, 4.5 } },
    { "chest", "torso", parent = "hips", t = { 32, 17 }, r = { 4.5, 5.8 },
      bl = { { 0.75, 0.5 } }, br = { { 0.75, 0.5 } } },
    { "neck", "torso", parent = "chest", t = { 32, 13 }, r = { 2, 2 } },
    { "head", "head", parent = "neck", h = { 32, 9.5 }, t = { 32, 8.5 }, r = { 4.4, 4.3 } },
    { "upper_arm_L", "arm_L", parent = "chest", h = { 37.5, 18 }, t = { 39.5, 28 },
      r = { 2, 1.7 }, side = true, bl = { { 0.35, 0.4 } } },
    { "forearm_L", "arm_L", parent = "upper_arm_L", t = { 41, 37 }, r = { 1.7, 1.4 }, side = true },
    { "hand_L", "arm_L", parent = "forearm_L", t = { 41.8, 41 }, r = { 1.5, 1.2 }, side = true },
    { "thigh_L", "leg_L", parent = "hips", h = { 34.5, 34 }, t = { 35, 46.5 },
      r = { 3, 2.2 }, side = true, br = { { 0.3, 0.5 } } },
    { "shin_L", "leg_L", parent = "thigh_L", t = { 35, 58 }, r = { 2.2, 1.6 }, side = true,
      br = { { 0.3, 0.5 } } },
    { "foot_L", "leg_L", parent = "shin_L", t = { 38, 60 }, r = { 1.5, 1.2 }, side = true },
  },
}

-- narrow waist, wide hips, slender arms
T["Humanoid 4"] = {
  mirror = true,
  parts = FRONT_PARTS,
  bones = {
    { "hips", "torso", h = { 32, 38 }, t = { 32, 32 }, r = { 6.5, 4.2 } },
    { "chest", "torso", parent = "hips", t = { 32, 20 }, r = { 4.2, 5.5 },
      bl = { { 0.6, 0.8 } }, br = { { 0.6, 0.8 } } },
    { "neck", "torso", parent = "chest", t = { 32, 16 }, r = { 2, 2 } },
    { "head", "head", parent = "neck", h = { 32, 12 }, t = { 32, 11 }, r = { 5, 4.9 } },
    { "upper_arm_L", "arm_L", parent = "chest", h = { 37, 21 }, t = { 39, 30.5 },
      r = { 2, 1.7 }, side = true },
    { "forearm_L", "arm_L", parent = "upper_arm_L", t = { 40.5, 38.5 }, r = { 1.7, 1.4 }, side = true },
    { "hand_L", "arm_L", parent = "forearm_L", t = { 41.2, 42.5 }, r = { 1.5, 1.2 }, side = true },
    { "thigh_L", "leg_L", parent = "hips", h = { 35.5, 38 }, t = { 35.5, 48.5 },
      r = { 3.6, 2.4 }, side = true, br = { { 0.25, 0.8 } } },
    { "shin_L", "leg_L", parent = "thigh_L", t = { 35, 58 }, r = { 2.3, 1.6 }, side = true,
      br = { { 0.3, 0.6 } } },
    { "foot_L", "leg_L", parent = "shin_L", t = { 37.5, 60 }, r = { 1.5, 1.2 }, side = true },
  },
}

-- brute: a huge chest and arms, a small head, short legs
T["Humanoid 5"] = {
  mirror = true,
  parts = FRONT_PARTS,
  bones = {
    { "hips", "torso", h = { 32, 40 }, t = { 32, 34 }, r = { 5.5, 6 } },
    { "chest", "torso", parent = "hips", t = { 32, 21 }, r = { 7, 9.5 },
      bl = { { 0.7, 1 } }, br = { { 0.7, 1 } } },
    { "neck", "torso", parent = "chest", t = { 32, 17.5 }, r = { 3.5, 3.5 } },
    { "head", "head", parent = "neck", h = { 32, 14 }, t = { 32, 13.5 }, r = { 4.2, 4.2 } },
    { "upper_arm_L", "arm_L", parent = "chest", h = { 41, 21 }, t = { 45, 31 },
      r = { 4, 3.6 }, side = true, bl = { { 0.35, 1 } } },
    { "forearm_L", "arm_L", parent = "upper_arm_L", t = { 47, 40 }, r = { 3.8, 3.4 }, side = true },
    { "hand_L", "arm_L", parent = "forearm_L", t = { 47.5, 45 }, r = { 3.2, 2.8 }, side = true },
    { "thigh_L", "leg_L", parent = "hips", h = { 35.5, 40 }, t = { 36.5, 50 },
      r = { 4.2, 3.4 }, side = true, br = { { 0.3, 0.6 } } },
    { "shin_L", "leg_L", parent = "thigh_L", t = { 36.5, 57.5 }, r = { 3.4, 3 }, side = true },
    { "foot_L", "leg_L", parent = "shin_L", t = { 39.5, 60 }, r = { 2.6, 2.2 }, side = true },
  },
}

-- a child: a big head on a short body
T["Humanoid 6"] = {
  mirror = true,
  parts = FRONT_PARTS,
  bones = {
    { "hips", "torso", h = { 32, 45 }, t = { 32, 40.5 }, r = { 4.5, 4.5 } },
    { "chest", "torso", parent = "hips", t = { 32, 32 }, r = { 4.5, 5 } },
    { "neck", "torso", parent = "chest", t = { 32, 29.5 }, r = { 2, 2 } },
    { "head", "head", parent = "neck", h = { 32, 23 }, t = { 32, 22 }, r = { 7, 6.9 } },
    { "upper_arm_L", "arm_L", parent = "chest", h = { 36.5, 33 }, t = { 38.5, 40 },
      r = { 1.9, 1.7 }, side = true },
    { "forearm_L", "arm_L", parent = "upper_arm_L", t = { 39.5, 46 }, r = { 1.7, 1.5 }, side = true },
    { "hand_L", "arm_L", parent = "forearm_L", t = { 40, 49 }, r = { 1.6, 1.4 }, side = true },
    { "thigh_L", "leg_L", parent = "hips", h = { 34.5, 45 }, t = { 35, 52.5 },
      r = { 2.6, 2.2 }, side = true },
    { "shin_L", "leg_L", parent = "thigh_L", t = { 35, 58.5 }, r = { 2.2, 1.8 }, side = true },
    { "foot_L", "leg_L", parent = "shin_L", t = { 37.5, 60 }, r = { 1.7, 1.4 }, side = true },
  },
}

-- profiles: as Profile, facing right, the far side (_L) ahead and under

-- stocky, a round belly
T["Profile 2"] = {
  parts = SIDE_PARTS,
  bones = {
    { "hips", "torso", h = { 31, 39 }, t = { 31.5, 33 }, r = { 5.5, 4.5 },
      bl = { { 0.2, 1.5 } } },
    { "chest", "torso", parent = "hips", t = { 32.5, 23 }, r = { 4.5, 6 },
      br = { { 0.4, 2 } }, bl = { { 0.75, 0.6 } } },
    { "neck", "torso", parent = "chest", t = { 33.5, 19.5 }, r = { 2.4, 2.2 } },
    { "head", "head", parent = "neck", h = { 34, 16 }, t = { 34.8, 14.5 }, r = { 5, 4.9 } },
    { "upper_arm_L", "arm_L", parent = "chest", h = { 36.5, 24 }, t = { 37.5, 32 },
      r = { 2.4, 2 } },
    { "forearm_L", "arm_L", parent = "upper_arm_L", t = { 39, 38.5 }, r = { 2, 1.6 } },
    { "hand_L", "arm_L", parent = "forearm_L", t = { 40, 42 }, r = { 1.7, 1.4 } },
    { "thigh_L", "leg_L", parent = "hips", h = { 33.5, 39.5 }, t = { 34.5, 50 },
      r = { 3.3, 2.5 }, bl = { { 0.35, 0.9 } } },
    { "shin_L", "leg_L", parent = "thigh_L", t = { 34.5, 57.5 }, r = { 2.4, 1.7 },
      br = { { 0.25, 0.9 } } },
    { "foot_L", "leg_L", parent = "shin_L", t = { 39, 59.5 }, r = { 1.7, 1.2 } },
    { "upper_arm_R", "arm_R", parent = "chest", h = { 28.5, 24 }, t = { 28, 32 },
      r = { 2.6, 2.2 } },
    { "forearm_R", "arm_R", parent = "upper_arm_R", t = { 29.5, 39 }, r = { 2.2, 1.8 } },
    { "hand_R", "arm_R", parent = "forearm_R", t = { 30.5, 42.5 }, r = { 1.9, 1.5 } },
    { "thigh_R", "leg_R", parent = "hips", h = { 29.5, 40 }, t = { 30.5, 50 },
      r = { 3.6, 2.7 }, bl = { { 0.35, 1 } } },
    { "shin_R", "leg_R", parent = "thigh_R", t = { 30, 57.5 }, r = { 2.6, 1.9 },
      br = { { 0.25, 1 } } },
    { "foot_R", "leg_R", parent = "shin_R", t = { 35, 59.5 }, r = { 1.9, 1.4 } },
  },
}

-- tall and slim, long legs
T["Profile 3"] = {
  parts = SIDE_PARTS,
  bones = {
    { "hips", "torso", h = { 31, 31 }, t = { 31.5, 26.5 }, r = { 3.5, 2.6 },
      bl = { { 0.2, 1 } } },
    { "chest", "torso", parent = "hips", t = { 32.5, 16.5 }, r = { 2.6, 4 },
      br = { { 0.65, 1 } }, bl = { { 0.75, 0.4 } } },
    { "neck", "torso", parent = "chest", t = { 33.5, 12.5 }, r = { 1.4, 1.3 } },
    { "head", "head", parent = "neck", h = { 34, 9.2 }, t = { 34.6, 7.6 }, r = { 3.9, 3.8 } },
    { "upper_arm_L", "arm_L", parent = "chest", h = { 35.5, 17.5 }, t = { 36, 26.5 },
      r = { 1.4, 1.1 } },
    { "forearm_L", "arm_L", parent = "upper_arm_L", t = { 37.5, 34 }, r = { 1.1, 0.85 } },
    { "hand_L", "arm_L", parent = "forearm_L", t = { 38.5, 37.5 }, r = { 0.95, 0.75 } },
    { "thigh_L", "leg_L", parent = "hips", h = { 33, 32 }, t = { 34, 45 },
      r = { 2.3, 1.5 }, bl = { { 0.35, 0.6 } } },
    { "shin_L", "leg_L", parent = "thigh_L", t = { 34, 57.5 }, r = { 1.5, 0.95 },
      br = { { 0.25, 0.7 } } },
    { "foot_L", "leg_L", parent = "shin_L", t = { 38, 59.5 }, r = { 1.05, 0.75 } },
    { "upper_arm_R", "arm_R", parent = "chest", h = { 29, 17.5 }, t = { 28.5, 26.5 },
      r = { 1.6, 1.3 } },
    { "forearm_R", "arm_R", parent = "upper_arm_R", t = { 30, 34.5 }, r = { 1.3, 1 } },
    { "hand_R", "arm_R", parent = "forearm_R", t = { 31, 38 }, r = { 1.1, 0.85 } },
    { "thigh_R", "leg_R", parent = "hips", h = { 29.5, 32 }, t = { 30.5, 45 },
      r = { 2.6, 1.7 }, bl = { { 0.35, 0.8 } } },
    { "shin_R", "leg_R", parent = "thigh_R", t = { 30, 58 }, r = { 1.7, 1.1 },
      br = { { 0.25, 0.9 } } },
    { "foot_R", "leg_R", parent = "shin_R", t = { 34.5, 59.8 }, r = { 1.2, 0.85 } },
  },
}

-- brute, hunched: the chest and head forward, heavy arms hanging
T["Profile 4"] = {
  parts = SIDE_PARTS,
  bones = {
    { "hips", "torso", h = { 29, 37 }, t = { 30, 31 }, r = { 4.5, 4 },
      bl = { { 0.2, 1.2 } } },
    { "chest", "torso", parent = "hips", t = { 34, 20 }, r = { 4.5, 7 },
      br = { { 0.6, 2 } }, bl = { { 0.8, 1.2 } } },
    { "neck", "torso", parent = "chest", t = { 38.5, 18.5 }, r = { 2.6, 2.4 } },
    { "head", "head", parent = "neck", h = { 41.5, 18 }, t = { 43, 17 }, r = { 4, 3.9 } },
    { "upper_arm_L", "arm_L", parent = "chest", h = { 38, 21 }, t = { 40, 30 },
      r = { 2.8, 2.4 } },
    { "forearm_L", "arm_L", parent = "upper_arm_L", t = { 42, 38 }, r = { 2.6, 2.2 } },
    { "hand_L", "arm_L", parent = "forearm_L", t = { 43, 42.5 }, r = { 2.4, 2 } },
    { "thigh_L", "leg_L", parent = "hips", h = { 31.5, 37.5 }, t = { 33, 48 },
      r = { 3.4, 2.6 }, bl = { { 0.35, 0.8 } } },
    { "shin_L", "leg_L", parent = "thigh_L", t = { 32.5, 57.5 }, r = { 2.6, 1.9 },
      br = { { 0.25, 0.9 } } },
    { "foot_L", "leg_L", parent = "shin_L", t = { 37, 59.5 }, r = { 2, 1.5 } },
    { "upper_arm_R", "arm_R", parent = "chest", h = { 31, 21 }, t = { 32, 30.5 },
      r = { 3.2, 2.7 } },
    { "forearm_R", "arm_R", parent = "upper_arm_R", t = { 34, 39 }, r = { 2.9, 2.5 } },
    { "hand_R", "arm_R", parent = "forearm_R", t = { 35, 44 }, r = { 2.7, 2.2 } },
    { "thigh_R", "leg_R", parent = "hips", h = { 28, 38 }, t = { 29, 48 },
      r = { 3.7, 2.8 }, bl = { { 0.35, 1 } } },
    { "shin_R", "leg_R", parent = "thigh_R", t = { 28.5, 57.5 }, r = { 2.8, 2.1 },
      br = { { 0.25, 1 } } },
    { "foot_R", "leg_R", parent = "shin_R", t = { 33, 59.5 }, r = { 2.2, 1.6 } },
  },
}

-- strictly from the side: both sides one behind the other
T["Profile 5"] = {
  parts = SIDE_PARTS,
  bones = {
    { "hips", "torso", h = { 31, 34 }, t = { 31.5, 29 }, r = { 4.2, 3.2 },
      bl = { { 0.2, 1.2 } } },
    { "chest", "torso", parent = "hips", t = { 32.3, 19 }, r = { 3.2, 4.6 },
      br = { { 0.65, 1.4 } }, bl = { { 0.75, 0.4 } } },
    { "neck", "torso", parent = "chest", t = { 33, 15 }, r = { 1.8, 1.7 } },
    { "head", "head", parent = "neck", h = { 33.5, 11.5 }, t = { 34.3, 10 }, r = { 4.7, 4.6 } },
    { "upper_arm_L", "arm_L", parent = "chest", h = { 33.5, 20 }, t = { 34, 28.5 },
      r = { 1.5, 1.2 } },
    { "forearm_L", "arm_L", parent = "upper_arm_L", t = { 35.5, 35.5 }, r = { 1.2, 0.9 } },
    { "hand_L", "arm_L", parent = "forearm_L", t = { 36.3, 39 }, r = { 1, 0.8 } },
    { "thigh_L", "leg_L", parent = "hips", h = { 32, 34.5 }, t = { 33, 46 },
      r = { 2.6, 1.8 }, bl = { { 0.35, 0.7 } } },
    { "shin_L", "leg_L", parent = "thigh_L", t = { 33, 57.5 }, r = { 1.7, 1.1 },
      br = { { 0.25, 0.8 } } },
    { "foot_L", "leg_L", parent = "shin_L", t = { 37.5, 59.5 }, r = { 1.2, 0.85 } },
    { "upper_arm_R", "arm_R", parent = "chest", h = { 32, 20 }, t = { 32, 28.5 },
      r = { 1.7, 1.4 } },
    { "forearm_R", "arm_R", parent = "upper_arm_R", t = { 33.5, 35.5 }, r = { 1.4, 1.05 } },
    { "hand_R", "arm_R", parent = "forearm_R", t = { 34.3, 39 }, r = { 1.15, 0.9 } },
    { "thigh_R", "leg_R", parent = "hips", h = { 31, 34.5 }, t = { 31.5, 46 },
      r = { 2.8, 1.9 }, bl = { { 0.35, 0.9 } } },
    { "shin_R", "leg_R", parent = "thigh_R", t = { 31.5, 57.5 }, r = { 1.8, 1.2 },
      br = { { 0.25, 1 } } },
    { "foot_R", "leg_R", parent = "shin_R", t = { 36, 59.5 }, r = { 1.3, 0.9 } },
  },
}

-- super deformed: two heads tall
T["Chibi 2"] = {
  mirror = true,
  parts = CHIBI_PARTS,
  bones = {
    { "hips", "torso", h = { 32, 52 }, t = { 32, 48.5 }, r = { 4.5, 4.5 } },
    { "chest", "torso", parent = "hips", t = { 32, 43 }, r = { 4.5, 4.2 } },
    { "neck", "torso", parent = "chest", t = { 32, 41 }, r = { 2.2, 2.2 } },
    { "head", "head", parent = "neck", h = { 32, 34.75 }, t = { 32, 32.75 }, r = { 7.25, 7.2 } },
    { "upper_arm_L", "arm_L", parent = "chest", h = { 35.5, 44 }, t = { 38, 48 },
      r = { 2, 1.9 }, side = true },
    { "forearm_L", "arm_L", parent = "upper_arm_L", t = { 39, 50.5 }, r = { 1.9, 1.9 }, side = true },
    { "hand_L", "arm_L", parent = "forearm_L", t = { 39.5, 52.5 }, r = { 2.1, 2.1 }, side = true },
    { "thigh_L", "leg_L", parent = "hips", h = { 34.5, 52 }, t = { 34.8, 56 },
      r = { 2.6, 2.4 }, side = true },
    { "shin_L", "leg_L", parent = "thigh_L", t = { 34.8, 58.5 }, r = { 2.4, 2.2 }, side = true },
    { "foot_L", "leg_L", parent = "shin_L", t = { 36.5, 60 }, r = { 2.1, 1.9 }, side = true },
  },
}

-- three heads tall, longer limbs
T["Chibi 3"] = {
  mirror = true,
  parts = CHIBI_PARTS,
  bones = {
    { "hips", "torso", h = { 32, 43 }, t = { 32, 38.5 }, r = { 4.5, 4.5 } },
    { "chest", "torso", parent = "hips", t = { 32, 30.5 }, r = { 4.6, 4.8 } },
    { "neck", "torso", parent = "chest", t = { 32, 28 }, r = { 2.2, 2.2 } },
    { "head", "head", parent = "neck", h = { 32, 23.75 }, t = { 32, 22.25 }, r = { 5.25, 5.2 } },
    { "upper_arm_L", "arm_L", parent = "chest", h = { 36, 31 }, t = { 39, 37.5 },
      r = { 2, 1.8 }, side = true },
    { "forearm_L", "arm_L", parent = "upper_arm_L", t = { 40.5, 43 }, r = { 1.8, 1.7 }, side = true },
    { "hand_L", "arm_L", parent = "forearm_L", t = { 41, 46 }, r = { 1.9, 1.9 }, side = true },
    { "thigh_L", "leg_L", parent = "hips", h = { 34.5, 43 }, t = { 35, 51 },
      r = { 2.8, 2.4 }, side = true },
    { "shin_L", "leg_L", parent = "thigh_L", t = { 35, 58 }, r = { 2.4, 2.1 }, side = true },
    { "foot_L", "leg_L", parent = "shin_L", t = { 37.5, 60 }, r = { 2.1, 1.8 }, side = true },
  },
}

-- chubby: a round body
T["Chibi 4"] = {
  mirror = true,
  parts = CHIBI_PARTS,
  bones = {
    { "hips", "torso", h = { 32, 49 }, t = { 32, 44 }, r = { 7.5, 7.5 } },
    { "chest", "torso", parent = "hips", t = { 32, 37 }, r = { 7.5, 6 } },
    { "neck", "torso", parent = "chest", t = { 32, 34.5 }, r = { 3, 3 } },
    { "head", "head", parent = "neck", h = { 32, 29.5 }, t = { 32, 28 }, r = { 6, 5.95 } },
    { "upper_arm_L", "arm_L", parent = "chest", h = { 38.5, 37 }, t = { 42, 42 },
      r = { 2.8, 2.6 }, side = true },
    { "forearm_L", "arm_L", parent = "upper_arm_L", t = { 43.5, 45.5 }, r = { 2.6, 2.5 }, side = true },
    { "hand_L", "arm_L", parent = "forearm_L", t = { 44, 48 }, r = { 2.6, 2.6 }, side = true },
    { "thigh_L", "leg_L", parent = "hips", h = { 35.5, 49 }, t = { 36, 54 },
      r = { 3.6, 3.3 }, side = true },
    { "shin_L", "leg_L", parent = "thigh_L", t = { 36, 58 }, r = { 3.3, 3 }, side = true },
    { "foot_L", "leg_L", parent = "shin_L", t = { 38.5, 60 }, r = { 2.8, 2.5 }, side = true },
  },
}

-- chibi profiles: facing right like Profile, the head over everything

T["Chibi 5"] = {
  parts = CHIBI_SIDE_PARTS,
  bones = {
    { "hips", "torso", h = { 31, 48 }, t = { 31.5, 44 }, r = { 4.2, 3.8 },
      bl = { { 0.2, 1 } } },
    { "chest", "torso", parent = "hips", t = { 32.2, 37 }, r = { 3.8, 4.4 },
      br = { { 0.6, 1 } } },
    { "neck", "torso", parent = "chest", t = { 32.8, 34.5 }, r = { 2, 2 } },
    { "head", "head", parent = "neck", h = { 33.5, 29.5 }, t = { 34.5, 28 }, r = { 6, 5.95 } },
    { "upper_arm_L", "arm_L", parent = "chest", h = { 35, 37 }, t = { 36.5, 42 },
      r = { 1.9, 1.7 } },
    { "forearm_L", "arm_L", parent = "upper_arm_L", t = { 37.5, 45.5 }, r = { 1.7, 1.7 } },
    { "hand_L", "arm_L", parent = "forearm_L", t = { 38, 48 }, r = { 1.9, 1.9 } },
    { "thigh_L", "leg_L", parent = "hips", h = { 33, 48.5 }, t = { 33.8, 53.5 },
      r = { 2.6, 2.3 } },
    { "shin_L", "leg_L", parent = "thigh_L", t = { 33.8, 57.5 }, r = { 2.3, 2.1 } },
    { "foot_L", "leg_L", parent = "shin_L", t = { 37, 59.8 }, r = { 2, 1.7 } },
    { "upper_arm_R", "arm_R", parent = "chest", h = { 29.5, 37 }, t = { 29, 42 },
      r = { 2.1, 1.9 } },
    { "forearm_R", "arm_R", parent = "upper_arm_R", t = { 30, 45.5 }, r = { 1.9, 1.9 } },
    { "hand_R", "arm_R", parent = "forearm_R", t = { 30.5, 48 }, r = { 2.1, 2.1 } },
    { "thigh_R", "leg_R", parent = "hips", h = { 29.8, 48.5 }, t = { 30.3, 53.5 },
      r = { 2.8, 2.5 } },
    { "shin_R", "leg_R", parent = "thigh_R", t = { 30.3, 57.5 }, r = { 2.5, 2.3 } },
    { "foot_R", "leg_R", parent = "shin_R", t = { 33.8, 59.8 }, r = { 2.2, 1.9 } },
  },
}

-- super deformed, from the side
T["Chibi 6"] = {
  parts = CHIBI_SIDE_PARTS,
  bones = {
    { "hips", "torso", h = { 31, 52.5 }, t = { 31.5, 49 }, r = { 3.8, 3.6 },
      bl = { { 0.2, 0.8 } } },
    { "chest", "torso", parent = "hips", t = { 32, 43.5 }, r = { 3.6, 3.8 },
      br = { { 0.6, 0.8 } } },
    { "neck", "torso", parent = "chest", t = { 32.5, 41.5 }, r = { 1.8, 1.8 } },
    { "head", "head", parent = "neck", h = { 33.5, 35 }, t = { 34.5, 33 }, r = { 7, 6.95 } },
    { "upper_arm_L", "arm_L", parent = "chest", h = { 34.5, 44 }, t = { 36, 48 },
      r = { 1.7, 1.6 } },
    { "forearm_L", "arm_L", parent = "upper_arm_L", t = { 36.8, 50.5 }, r = { 1.6, 1.6 } },
    { "hand_L", "arm_L", parent = "forearm_L", t = { 37.2, 52.5 }, r = { 1.8, 1.8 } },
    { "thigh_L", "leg_L", parent = "hips", h = { 32.8, 52.5 }, t = { 33.4, 56 },
      r = { 2.3, 2.1 } },
    { "shin_L", "leg_L", parent = "thigh_L", t = { 33.4, 58.5 }, r = { 2.1, 1.9 } },
    { "foot_L", "leg_L", parent = "shin_L", t = { 36, 60 }, r = { 1.8, 1.5 } },
    { "upper_arm_R", "arm_R", parent = "chest", h = { 29.5, 44 }, t = { 29, 48 },
      r = { 1.9, 1.8 } },
    { "forearm_R", "arm_R", parent = "upper_arm_R", t = { 29.8, 50.5 }, r = { 1.8, 1.8 } },
    { "hand_R", "arm_R", parent = "forearm_R", t = { 30.2, 52.5 }, r = { 2, 2 } },
    { "thigh_R", "leg_R", parent = "hips", h = { 29.8, 52.5 }, t = { 30.2, 56 },
      r = { 2.5, 2.3 } },
    { "shin_R", "leg_R", parent = "thigh_R", t = { 30.2, 58.5 }, r = { 2.3, 2.1 } },
    { "foot_R", "leg_R", parent = "shin_R", t = { 33, 60 }, r = { 2, 1.7 } },
  },
}

T.Quadruped = {
  parts = { { "leg_front_far", -2 }, { "leg_back_far", -2 }, { "tail", -1 }, { "body", 0 },
    { "head", 1 }, { "leg_front_near", 2 }, { "leg_back_near", 2 } },
  bones = {
    { "hips", "body", h = { 20, 36 }, t = { 32, 35 }, r = { 6, 6 } },
    { "chest", "body", parent = "hips", t = { 44, 34 }, r = { 6, 7 },
      br = { { 0.4, 1 } } },
    { "neck", "body", parent = "chest", t = { 50, 26 }, r = { 4, 3.5 } },
    { "head", "head", parent = "neck", t = { 58, 28 }, r = { 4.5, 2.8 } },
    { "tail", "tail", parent = "hips", h = { 18, 33 }, t = { 10, 29 }, r = { 2, 1.4 } },
    { "tail_tip", "tail", parent = "tail", t = { 5, 33 }, r = { 1.4, 0.8 } },
    { "upper_front_near", "leg_front_near", parent = "chest", h = { 43, 39 }, t = { 44, 48 },
      r = { 3, 2.2 } },
    { "lower_front_near", "leg_front_near", parent = "upper_front_near", t = { 44, 57 },
      r = { 2, 1.6 } },
    { "paw_front_near", "leg_front_near", parent = "lower_front_near", t = { 46, 59 },
      r = { 1.6, 1.4 } },
    { "upper_front_far", "leg_front_far", parent = "chest", h = { 40, 39 }, t = { 41, 48 },
      r = { 3, 2.2 } },
    { "lower_front_far", "leg_front_far", parent = "upper_front_far", t = { 41, 57 },
      r = { 2, 1.6 } },
    { "paw_front_far", "leg_front_far", parent = "lower_front_far", t = { 43, 59 },
      r = { 1.6, 1.4 } },
    { "thigh_back_near", "leg_back_near", parent = "hips", h = { 23, 38 }, t = { 25, 48 },
      r = { 3.8, 2.4 }, bl = { { 0.35, 1 } } },
    { "hock_back_near", "leg_back_near", parent = "thigh_back_near", t = { 22, 56 },
      r = { 2.2, 1.6 } },
    { "paw_back_near", "leg_back_near", parent = "hock_back_near", t = { 24, 59 },
      r = { 1.6, 1.4 } },
    { "thigh_back_far", "leg_back_far", parent = "hips", h = { 20, 38 }, t = { 22, 48 },
      r = { 3.8, 2.4 }, bl = { { 0.35, 1 } } },
    { "hock_back_far", "leg_back_far", parent = "thigh_back_far", t = { 19, 56 },
      r = { 2.2, 1.6 } },
    { "paw_back_far", "leg_back_far", parent = "hock_back_far", t = { 21, 59 },
      r = { 1.6, 1.4 } },
  },
}

T.Winged = {
  mirror = true,
  parts = { { "wing_L", -2 }, { "wing_R", -2 }, { "tail", -1 }, { "leg_L", -1 },
    { "leg_R", -1 }, { "body", 0 }, { "head", 1 } },
  bones = {
    { "body", "body", h = { 32, 42 }, t = { 32, 27 }, r = { 6.5, 5.5 },
      bl = { { 0.35, 1 } }, br = { { 0.35, 1 } } },
    { "head", "head", parent = "body", h = { 32, 24 }, t = { 32, 23 }, r = { 4.4, 4.3 } },
    { "tail", "tail", parent = "body", h = { 32, 43 }, t = { 32, 52 }, r = { 3, 1.5 } },
    { "wing_upper_L", "wing_L", parent = "body", h = { 36.5, 29 }, t = { 45, 23 },
      r = { 2.6, 2.2 }, side = true, br = { { 0.5, 2 } } },
    { "wing_fore_L", "wing_L", parent = "wing_upper_L", t = { 54, 25 }, r = { 2.2, 1.8 },
      side = true, br = { { 0.45, 3 } } },
    { "wing_tip_L", "wing_L", parent = "wing_fore_L", t = { 61, 31 }, r = { 1.8, 0.7 },
      side = true, br = { { 0.35, 2 } } },
    { "thigh_L", "leg_L", parent = "body", h = { 34.5, 44 }, t = { 35, 52 }, r = { 2, 1.4 },
      side = true },
    { "shank_L", "leg_L", parent = "thigh_L", t = { 35.5, 58 }, r = { 1.2, 1 }, side = true },
    { "foot_L", "leg_L", parent = "shank_L", t = { 38, 60 }, r = { 1, 0.8 }, side = true },
  },
}

T.Serpent = {
  parts = { { "body", 0 }, { "head", 1 } },
  bones = {
    { "tail_tip", "body", h = { 4, 55 }, t = { 12, 57 }, r = { 0.6, 1.4 } },
    { "tail", "body", parent = "tail_tip", t = { 20, 56 }, r = { 1.4, 2.2 } },
    { "body_1", "body", parent = "tail", t = { 27, 51 }, r = { 2.2, 3 } },
    { "body_2", "body", parent = "body_1", t = { 31, 44 }, r = { 3, 3.4 } },
    { "body_3", "body", parent = "body_2", t = { 34, 37 }, r = { 3.4, 3.4 } },
    { "body_4", "body", parent = "body_3", t = { 39, 31 }, r = { 3.4, 3 } },
    { "neck", "body", parent = "body_4", t = { 46, 28 }, r = { 3, 2.4 } },
    { "head", "head", parent = "neck", t = { 55, 30 }, r = { 3.8, 2.4 } },
  },
}

M.DATA = T

local function swapName(s)
  return (s:gsub("_L$", "_R"))
end

local function swapBulges(list)
  local out = {}
  for i, q in ipairs(list or {}) do out[i] = { q[1], q[2] } end
  return out
end

--- A new rig from template `name` for a w x h canvas, or nil + why.
function M.build(name, w, h)
  local Rig = M.Rig
  local tpl = T[name]
  if not tpl then return nil, "no template " .. tostring(name) end
  w, h = w or M.SIZE, h or M.SIZE
  local u = math.min(w, h) / M.SIZE
  local ox = (w - M.SIZE * u) / 2
  local oy = h - M.SIZE * u
  local function X(x) return ox + x * u end
  local function Y(y) return oy + y * u end
  local mx = function(x) return M.SIZE - x end

  local doc = Rig.new(w, h)
  local partId = {}
  for _, p in ipairs(tpl.parts) do
    local part = Rig.addPart(doc, p[1])
    part.z = p[2]
    partId[p[1]] = part.id
  end
  local byName = {}
  local function add(spec, name, partName, parentName, hx, hy, tx, ty, bl, br)
    local parent = parentName and byName[parentName] or nil
    local opts = {
      name = name, part = partId[partName], parent = parent and parent.id or 0,
      rHead = math.max(Rig.MIN_RADIUS, spec.r[1] * u),
      rTail = math.max(Rig.MIN_RADIUS, spec.r[2] * u),
    }
    local w0 = Rig.solve(doc)
    if not hx then
      local pw = w0[parent.id]
      hx, hy = pw.tx, pw.ty
      opts.connected = true
    else
      hx, hy = X(hx), Y(hy)
      opts.connected = false
    end
    -- on a tiny canvas a scaled bone can fall under MIN_LEN: stretch it
    -- along its own direction (twins stretch the mirrored way)
    tx, ty = X(tx), Y(ty)
    local dx, dy = tx - hx, ty - hy
    local l = math.sqrt(dx * dx + dy * dy)
    if l < Rig.MIN_LEN then
      if l < 1e-9 then dx, dy, l = 0, -1, 1 end
      local m = Rig.MIN_LEN * 1.001 -- a hair over: rounding must not undercut it
      tx, ty = hx + dx / l * m, hy + dy / l * m
    end
    local b = assert(Rig.addBone(doc, hx, hy, tx, ty, opts))
    for i, q in ipairs(bl or {}) do b.bulgeL[i] = { q[1], q[2] * u } end
    for i, q in ipairs(br or {}) do b.bulgeR[i] = { q[1], q[2] * u } end
    byName[name] = b
    return b
  end
  for _, s in ipairs(tpl.bones) do
    local hx, hy = s.h and s.h[1], s.h and s.h[2]
    local b = add(s, s[1], s[2], s.parent, hx, hy, s.t[1], s.t[2], s.bl, s.br)
    if s.side then
      local parentName = s.parent and (byName[swapName(s.parent)] and swapName(s.parent) or s.parent)
      -- the twin is built from the template too, so it has its own part
      -- and lands exactly mirrored even when the source is connected
      local tw = add(s, swapName(s[1]), swapName(s[2]), parentName,
        hx and mx(hx), hy, mx(s.t[1]), s.t[2], swapBulges(s.br), swapBulges(s.bl))
      b.twin, tw.twin = tw.id, b.id
    end
  end
  doc.mirror = { on = tpl.mirror == true, x = X(M.SIZE / 2) }
  return doc
end

return M
