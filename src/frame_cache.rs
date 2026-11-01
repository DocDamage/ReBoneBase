//! Rendered-frame reuse for previews and exports.
use crate::{jobs::Progress, project::Project};
use anyhow::Result;
use image::RgbaImage;
use std::{
    collections::{HashMap, VecDeque},
    hash::Hash,
    sync::Arc,
};

/// Least-recently-used values bounded by an approximate byte budget.
pub struct FrameCache<K, V> {
    budget: usize,
    bytes: usize,
    entries: HashMap<K, (V, usize)>,
    order: VecDeque<K>,
}
impl<K: Clone + Eq + Hash, V: Clone> FrameCache<K, V> {
    pub fn new(budget: usize) -> Self {
        Self {
            budget,
            bytes: 0,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }
    pub fn get(&mut self, key: &K) -> Option<V> {
        let value = self.entries.get(key)?.0.clone();
        self.touch(key);
        Some(value)
    }
    /// Values larger than the whole budget are not retained.
    pub fn insert(&mut self, key: K, value: V, bytes: usize) {
        self.remove(&key);
        if bytes > self.budget {
            return;
        }
        while self.bytes + bytes > self.budget {
            let Some(old) = self.order.pop_front() else {
                break;
            };
            if let Some((_, size)) = self.entries.remove(&old) {
                self.bytes -= size;
            }
        }
        self.bytes += bytes;
        self.order.push_back(key.clone());
        self.entries.insert(key, (value, bytes));
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.bytes = 0;
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    fn remove(&mut self, key: &K) {
        if let Some((_, size)) = self.entries.remove(key) {
            self.bytes -= size;
            self.order.retain(|k| k != key);
        }
    }
    fn touch(&mut self, key: &K) {
        if let Some(i) = self.order.iter().position(|k| k == key) {
            let k = self.order.remove(i).expect("position is in range");
            self.order.push_back(k);
        }
    }
}

/// Renders an export's frame sequence, rendering each distinct frame once.
/// Ping-pong playback and repeated passes reuse earlier renders; a frame is
/// held only until its last use, so memory stays near one clip's repeats.
pub struct SequenceRenderer<'a> {
    project: &'a Project,
    progress: &'a Progress,
    remaining: HashMap<u32, usize>,
    held: HashMap<u32, Arc<RgbaImage>>,
    renders: usize,
}
impl<'a> SequenceRenderer<'a> {
    pub fn new(project: &'a Project, sequence: &[u32], progress: &'a Progress) -> Self {
        let mut remaining = HashMap::new();
        for f in sequence {
            *remaining.entry(*f).or_insert(0) += 1;
        }
        progress.add_total(sequence.len() as u64);
        Self {
            project,
            progress,
            remaining,
            held: HashMap::new(),
            renders: 0,
        }
    }
    pub fn frame(&mut self, frame: u32) -> Result<Arc<RgbaImage>> {
        self.progress.checkpoint()?;
        let image = match self.held.get(&frame) {
            Some(image) => image.clone(),
            None => {
                self.renders += 1;
                Arc::new(self.project.try_render_at(frame as f32)?)
            }
        };
        let left = self.remaining.get_mut(&frame).map_or(0, |n| {
            *n = n.saturating_sub(1);
            *n
        });
        if left > 0 {
            self.held.insert(frame, image.clone());
        } else {
            self.held.remove(&frame);
        }
        self.progress.advance(1)?;
        Ok(image)
    }
    /// Distinct frames actually rendered so far.
    pub fn renders(&self) -> usize {
        self.renders
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{animation::Playback, model::Rig};

    #[test]
    fn lru_evicts_oldest_until_budget_fits() {
        let mut cache = FrameCache::new(10);
        cache.insert(1, 'a', 4);
        cache.insert(2, 'b', 4);
        assert_eq!(cache.get(&1), Some('a'));
        cache.insert(3, 'c', 4);
        assert_eq!(cache.get(&2), None, "least recently used entry is evicted");
        assert_eq!(cache.get(&1), Some('a'));
        assert_eq!(cache.bytes(), 8);
        cache.insert(4, 'd', 11);
        assert_eq!(cache.get(&4), None, "oversized values are not retained");
        cache.insert(1, 'z', 2);
        assert_eq!((cache.get(&1), cache.bytes()), (Some('z'), 6));
        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn ping_pong_sequences_render_each_frame_once() {
        let mut rig = Rig::new("Cache", 16, 16);
        rig.active_clip_mut().length = 6;
        rig.active_clip_mut().playback = Playback::PingPong;
        let project = Project::new(rig);
        let sequence = project.rig.active_clip().frame_sequence();
        assert_eq!(sequence.len(), 10);
        let progress = Progress::default();
        let mut renderer = SequenceRenderer::new(&project, &sequence, &progress);
        for f in &sequence {
            let image = renderer.frame(*f).unwrap();
            assert_eq!(*image, project.try_render_at(*f as f32).unwrap());
        }
        assert_eq!(renderer.renders(), 6);
        assert!(
            renderer.held.is_empty(),
            "frames are released after last use"
        );
        let s = progress.snapshot();
        assert_eq!((s.done, s.total), (10, 10));
    }

    #[test]
    fn cancelled_renderer_stops_before_rendering() {
        let project = Project::new(Rig::new("Cancel", 8, 8));
        let progress = Progress::default();
        progress.cancel();
        let mut renderer = SequenceRenderer::new(&project, &[1], &progress);
        assert!(crate::jobs::is_cancelled(&renderer.frame(1).unwrap_err()));
        assert_eq!(renderer.renders(), 0);
    }
}
