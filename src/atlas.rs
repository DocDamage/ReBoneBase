//! Packed sprite atlases: transparent-border trimming, reuse of identical
//! frame images, nonrotated MaxRects packing, padding, edge extrusion and
//! additional pages when one texture is not enough.
//!
//! Reuse only shares pixels. Every exported frame keeps its own placement
//! entry, so timing, events and collision records are never merged.
use crate::{
    jobs::Progress,
    project::Project,
    sprite_package::{self, Offset, PageKind, Rect, SpritePackage},
};
use anyhow::{Result, bail, ensure};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::PathBuf,
};

pub const MAX_TEXTURE: u32 = 4096;
pub const MAX_PADDING: u32 = 64;
pub const MAX_EXTRUDE: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AtlasOptions {
    /// Pack frames into shared pages; otherwise one full-frame grid per clip.
    pub packed: bool,
    /// Remove fully transparent rows and columns around each frame.
    pub trim: bool,
    /// Store identical frame images once.
    pub reuse: bool,
    /// Transparent pixels between packed images.
    pub padding: u32,
    /// Edge pixels repeated around each image to avoid filtering seams.
    pub extrude: u32,
    /// Largest page width and height.
    pub max_size: u32,
}
impl Default for AtlasOptions {
    fn default() -> Self {
        Self {
            packed: false,
            trim: true,
            reuse: true,
            padding: 2,
            extrude: 0,
            max_size: 2048,
        }
    }
}
impl AtlasOptions {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (16..=MAX_TEXTURE).contains(&self.max_size),
            "Maximum texture size must be 16 to {MAX_TEXTURE} pixels"
        );
        ensure!(
            self.padding <= MAX_PADDING && self.extrude <= MAX_EXTRUDE,
            "Atlas padding or extrusion is too large"
        );
        Ok(())
    }
}

/// Where one exported frame's pixels live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub page: usize,
    /// Never empty: a fully transparent trimmed frame is one transparent pixel.
    pub rect: Rect,
    pub trim: Offset,
}
pub struct Atlas {
    pub pages: Vec<RgbaImage>,
    /// One entry per [`Packer::add`] call, in call order.
    pub frames: Vec<Placement>,
}

struct Frame {
    image: usize,
    trim: Offset,
}
pub struct Packer {
    options: AtlasOptions,
    images: Vec<RgbaImage>,
    index: HashMap<u64, Vec<usize>>,
    frames: Vec<Frame>,
}
impl Packer {
    pub fn new(options: AtlasOptions) -> Result<Self> {
        options.validate()?;
        Ok(Self {
            options,
            images: vec![],
            index: HashMap::new(),
            frames: vec![],
        })
    }
    /// Record a rendered frame and return its index in [`Atlas::frames`].
    pub fn add(&mut self, frame: &RgbaImage) -> usize {
        let (image, trim) = if self.options.trim {
            match opaque_bounds(frame) {
                Some(r) => (
                    image::imageops::crop_imm(frame, r.x, r.y, r.w, r.h).to_image(),
                    Offset { x: r.x, y: r.y },
                ),
                // Engines read a zero-sized region as "the whole texture", so a
                // blank frame keeps one transparent pixel instead.
                None => (RgbaImage::new(1, 1), Offset::default()),
            }
        } else {
            (frame.clone(), Offset::default())
        };
        let image = self.intern(image);
        self.frames.push(Frame { image, trim });
        self.frames.len() - 1
    }
    fn intern(&mut self, image: RgbaImage) -> usize {
        if !self.options.reuse {
            self.images.push(image);
            return self.images.len() - 1;
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        image.dimensions().hash(&mut hasher);
        image.as_raw().hash(&mut hasher);
        let bucket = self.index.entry(hasher.finish()).or_default();
        if let Some(&same) = bucket.iter().find(|&&i| self.images[i] == image) {
            return same;
        }
        bucket.push(self.images.len());
        self.images.push(image);
        self.images.len() - 1
    }
    /// Unique images stored so far, for progress and tests.
    pub fn unique_images(&self) -> usize {
        self.images.len()
    }
    pub fn finish(self, progress: &Progress) -> Result<Atlas> {
        progress.set_stage("Packing atlas");
        let o = self.options;
        let edge = 2 * o.extrude + o.padding;
        // The bin carries one extra padding so the last image may touch the limit.
        let side = o.max_size + o.padding;
        let mut order: Vec<usize> = (0..self.images.len()).collect();
        order.sort_by_key(|&i| {
            let (w, h) = self.images[i].dimensions();
            (std::cmp::Reverse(w.max(h)), std::cmp::Reverse(w * h), i)
        });
        let mut sizes = vec![(0, 0); self.images.len()];
        for &i in &order {
            let (w, h) = self.images[i].dimensions();
            let (fw, fh) = (w + edge, h + edge);
            if fw > side || fh > side {
                bail!(
                    "A {w} × {h} frame does not fit in a {} px texture with this padding",
                    o.max_size
                );
            }
            sizes[i] = (fw, fh);
        }
        let (mut bins, mut spots) = layout(&order, &sizes, side, progress)?;
        if bins.len() == 1 {
            // Everything fits on one page: find the smallest square bin that
            // still holds it, so a page is compact rather than a long strip.
            let area: u64 = sizes.iter().map(|&(w, h)| w as u64 * h as u64).sum();
            let widest = sizes.iter().map(|&(w, h)| w.max(h)).max().unwrap_or(1);
            let mut lo = widest.max((area as f64).sqrt().ceil() as u32).min(side);
            let mut hi = side;
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                let (b, s) = layout(&order, &sizes, mid, progress)?;
                if b.len() == 1 {
                    (bins, spots) = (b, s);
                    hi = mid;
                } else {
                    lo = mid + 1;
                }
            }
        }
        let mut pages: Vec<RgbaImage> = bins
            .iter()
            .map(|bin| {
                RgbaImage::new(
                    bin.used_w.saturating_sub(o.padding).max(1),
                    bin.used_h.saturating_sub(o.padding).max(1),
                )
            })
            .collect();
        let mut rects = vec![Rect::default(); self.images.len()];
        for (i, &(b, x, y)) in spots.iter().enumerate() {
            let image = &self.images[i];
            blit_extruded(&mut pages[b], image, x, y, o.extrude);
            rects[i] = Rect {
                x: x + o.extrude,
                y: y + o.extrude,
                w: image.width(),
                h: image.height(),
            };
        }
        let frames = self
            .frames
            .iter()
            .map(|f| Placement {
                page: spots[f.image].0,
                rect: rects[f.image],
                trim: f.trim,
            })
            .collect();
        for page in &pages {
            ensure!(
                page.width() <= o.max_size && page.height() <= o.max_size,
                "Packed page exceeds the maximum texture size"
            );
        }
        Ok(Atlas { pages, frames })
    }
}

/// An image's bin and the top-left corner of its footprint.
type Spot = (usize, u32, u32);
/// Place footprints in `order` into as many `side` × `side` bins as needed.
/// Returns the bins and each image's bin and top-left corner.
fn layout(
    order: &[usize],
    sizes: &[(u32, u32)],
    side: u32,
    progress: &Progress,
) -> Result<(Vec<Bin>, Vec<Spot>)> {
    let mut bins: Vec<Bin> = vec![];
    let mut spots = vec![(0, 0, 0); sizes.len()];
    for &i in order {
        progress.checkpoint()?;
        let (w, h) = sizes[i];
        let found = bins
            .iter()
            .enumerate()
            .find_map(|(b, bin)| bin.find(w, h).map(|spot| (b, spot)));
        let (b, (x, y)) = match found {
            Some(found) => found,
            None => {
                bins.push(Bin::new(side));
                let spot = bins.last().unwrap().find(w, h).expect("fits an empty bin");
                (bins.len() - 1, spot)
            }
        };
        bins[b].place(R { x, y, w, h });
        spots[i] = (b, x, y);
    }
    Ok((bins, spots))
}

/// Smallest rectangle holding every pixel with nonzero alpha.
pub fn opaque_bounds(image: &RgbaImage) -> Option<Rect> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in image.enumerate_pixels() {
        if p[3] != 0 {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x0 != u32::MAX).then(|| Rect {
        x: x0,
        y: y0,
        w: x1 - x0 + 1,
        h: y1 - y0 + 1,
    })
}

/// Copy `image` to `(x + e, y + e)` and repeat its edge pixels `e` times outward.
fn blit_extruded(page: &mut RgbaImage, image: &RgbaImage, x: u32, y: u32, e: u32) {
    let (w, h) = image.dimensions();
    for dy in 0..h + 2 * e {
        for dx in 0..w + 2 * e {
            let sx = dx.saturating_sub(e).min(w - 1);
            let sy = dy.saturating_sub(e).min(h - 1);
            page.put_pixel(x + dx, y + dy, *image.get_pixel(sx, sy));
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct R {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}
impl R {
    fn right(self) -> u32 {
        self.x + self.w
    }
    fn bottom(self) -> u32 {
        self.y + self.h
    }
    fn contains(self, o: R) -> bool {
        o.x >= self.x && o.y >= self.y && o.right() <= self.right() && o.bottom() <= self.bottom()
    }
    fn overlaps(self, o: R) -> bool {
        o.x < self.right() && self.x < o.right() && o.y < self.bottom() && self.y < o.bottom()
    }
}
/// MaxRects bin with best-short-side-fit placement.
struct Bin {
    free: Vec<R>,
    used_w: u32,
    used_h: u32,
}
impl Bin {
    fn new(side: u32) -> Self {
        Self {
            free: vec![R {
                x: 0,
                y: 0,
                w: side,
                h: side,
            }],
            used_w: 0,
            used_h: 0,
        }
    }
    fn find(&self, w: u32, h: u32) -> Option<(u32, u32)> {
        self.free
            .iter()
            .filter(|f| f.w >= w && f.h >= h)
            .min_by_key(|f| {
                let (dw, dh) = (f.w - w, f.h - h);
                // Prefer snug fits, then the top-left to keep pages compact.
                (dw.min(dh), dw.max(dh), f.y, f.x)
            })
            .map(|f| (f.x, f.y))
    }
    fn place(&mut self, used: R) {
        self.used_w = self.used_w.max(used.right());
        self.used_h = self.used_h.max(used.bottom());
        let mut next = Vec::with_capacity(self.free.len() + 4);
        for f in self.free.drain(..) {
            if !f.overlaps(used) {
                next.push(f);
                continue;
            }
            if used.x > f.x {
                next.push(R {
                    w: used.x - f.x,
                    ..f
                });
            }
            if used.right() < f.right() {
                next.push(R {
                    x: used.right(),
                    w: f.right() - used.right(),
                    ..f
                });
            }
            if used.y > f.y {
                next.push(R {
                    h: used.y - f.y,
                    ..f
                });
            }
            if used.bottom() < f.bottom() {
                next.push(R {
                    y: used.bottom(),
                    h: f.bottom() - used.bottom(),
                    ..f
                });
            }
        }
        // Drop free rectangles wholly inside another; keep the first of equals.
        let mut keep = vec![true; next.len()];
        for i in 0..next.len() {
            for j in 0..next.len() {
                if i != j
                    && keep[j]
                    && next[j].contains(next[i])
                    && (!next[i].contains(next[j]) || j < i)
                {
                    keep[i] = false;
                    break;
                }
            }
        }
        self.free = next
            .into_iter()
            .zip(keep)
            .filter_map(|(r, k)| k.then_some(r))
            .collect();
    }
}

/// Packed frames awaiting placement, keyed by package clip and frame.
#[derive(Default)]
pub struct Pending {
    entries: Vec<(usize, usize, usize)>,
}
impl Pending {
    /// Render a clip's sequence into the packer, flipped horizontally when
    /// `mirror` is set. `clip` is the index the clip will have in
    /// [`SpritePackage::clips`]; `project` has it active.
    pub fn add_clip(
        &mut self,
        packer: &mut Packer,
        project: &Project,
        clip: usize,
        sequence: &[u32],
        mirror: bool,
        progress: &Progress,
    ) -> Result<()> {
        let mut renderer = crate::frame_cache::SequenceRenderer::new(project, sequence, progress);
        for (j, f) in sequence.iter().enumerate() {
            let image = renderer.frame(*f)?;
            let index = if mirror {
                packer.add(&image::imageops::flip_horizontal(&*image))
            } else {
                packer.add(&image)
            };
            self.entries.push((clip, j, index));
        }
        Ok(())
    }
    /// Pack, add the pages to `package`, point every pending frame at its
    /// pixels and return the encoded page files.
    pub fn place(
        self,
        packer: Packer,
        package: &mut SpritePackage,
        page_file: impl Fn(usize) -> String,
        encode: impl Fn(&RgbaImage) -> Result<Vec<u8>>,
        progress: &Progress,
    ) -> Result<Vec<(PathBuf, Vec<u8>)>> {
        let atlas = packer.finish(progress)?;
        let base = package.pages.len();
        let mut files = vec![];
        for (i, page) in atlas.pages.iter().enumerate() {
            progress.checkpoint()?;
            let file = page_file(i);
            package.add_page(&file, PageKind::Atlas, page.width(), page.height());
            files.push((PathBuf::from(file), encode(page)?));
        }
        for (clip, frame, packed) in self.entries {
            let p = atlas.frames[packed];
            let f = &mut package.clips[clip].frames[frame];
            f.page = base + p.page;
            f.rect = Some(p.rect);
            f.trim = p.trim;
        }
        Ok(files)
    }
}
/// Frame metadata for a packed frame; [`Pending::place`] fills in its pixels.
pub fn unplaced_frame(project: &Project, source: u32) -> sprite_package::Frame {
    sprite_package::frame_in_active(project, source, 0, None, Offset::default())
}
