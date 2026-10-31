-- promo.lua — contents of the "More" tab (other extensions by the same
-- author). Data only: adding an extension here never needs UI code.
-- No popups and no auto-opening: the tab is there for people who go
-- looking, and the window always opens on the Rig tab.
--
-- Taglines follow how each extension describes itself on its own store
-- page. GhostFrames uses RIFE frame interpolation; do not call it AI.

return {
  profile = "https://icemaan.itch.io",
  profileText = "View all extensions",

  items = {
    {
      name = "FastFX Full",
      tagline = "Multi-emitter particle FX, live preview and bake",
      url = "https://icemaan.itch.io/fastfx-full-particle-system-for-aseprite",
    },
    {
      name = "Spaghetti",
      tagline = "Node-based textures for Aseprite",
      url = "https://icemaan.itch.io/spaghetti-node-based-textures-for-aseprite",
    },
    {
      name = "VeSprite",
      tagline = "Vector drawing for pixel art inside Aseprite",
      url = "https://icemaan.itch.io/vesprite-vector-drawing-for-pixel-art-inside-aseprite",
    },
    {
      name = "GhostFrames",
      tagline = "RIFE frame interpolation for inbetweens",
      url = "https://icemaan.itch.io/ghostframe-aseprite-extension",
    },
  },
}
