//! Canvas renders reused across frame changes. Playback, scrubbing and onion
//! skins revisit the same frames of an unchanged project; any project edit
//! empties the cache, so a stale frame is never shown.
use super::*;
use rebonebase::frame_cache::FrameCache;
use std::sync::Arc;

const TEXTURE_BUDGET: usize = 96 * 1024 * 1024;
const IMAGE_BUDGET: usize = 64 * 1024 * 1024;

/// Everything besides the project that changes the displayed skin render.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(super) struct PreviewKey {
    frame: u32,
    posed: bool,
    transition: Option<(usize, u32)>,
}

pub(super) struct PreviewCache {
    project: Option<Project>,
    textures: FrameCache<PreviewKey, egui::TextureHandle>,
    images: FrameCache<u32, Arc<image::RgbaImage>>,
    pub hits: u64,
    pub misses: u64,
}
impl Default for PreviewCache {
    fn default() -> Self {
        Self {
            project: None,
            textures: FrameCache::new(TEXTURE_BUDGET),
            images: FrameCache::new(IMAGE_BUDGET),
            hits: 0,
            misses: 0,
        }
    }
}
impl PreviewCache {
    /// Drop every render made for a different project state.
    fn sync(&mut self, project: &Project) {
        if self.project.as_ref() != Some(project) {
            self.textures.clear();
            self.images.clear();
            self.project = Some(project.clone());
        }
    }
    pub fn clear(&mut self) {
        self.textures.clear();
        self.images.clear();
        self.project = None;
    }
    pub fn texture(&mut self, project: &Project, key: PreviewKey) -> Option<egui::TextureHandle> {
        self.sync(project);
        let found = self.textures.get(&key);
        if found.is_some() {
            self.hits += 1;
        } else {
            self.misses += 1;
        }
        found
    }
    pub fn store_texture(&mut self, key: PreviewKey, texture: egui::TextureHandle) {
        let [w, h] = texture.size();
        self.textures.insert(key, texture, w * h * 4);
    }
    /// The plain animated render of `frame`, as used by onion skins.
    pub fn image(&mut self, project: &Project, frame: f32) -> Arc<image::RgbaImage> {
        self.sync(project);
        let key = frame.to_bits();
        if let Some(image) = self.images.get(&key) {
            self.hits += 1;
            return image;
        }
        self.misses += 1;
        let image = Arc::new(project.render_at(frame));
        let bytes = image.as_raw().len();
        self.images.insert(key, image.clone(), bytes);
        image
    }
}

impl ReboneApp {
    /// None while an edit gesture changes the project every frame; caching
    /// those renders would only cost a project snapshot per frame.
    pub(super) fn preview_key(&self) -> Option<PreviewKey> {
        let editing = self.gesture.is_some()
            || self.studio.key_drag.is_some()
            || self.studio.curve_drag.is_some();
        (!editing).then(|| PreviewKey {
            frame: self.frame.to_bits(),
            posed: self.mode == Mode::Pose,
            transition: (self.mode == Mode::Pose && self.production.transition_preview).then(
                || {
                    (
                        self.production.transition_to,
                        self.production.transition_weight.to_bits(),
                    )
                },
            ),
        })
    }
}
