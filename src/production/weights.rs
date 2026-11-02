//! Weight painting and rebinding for 2D artwork meshes. Every edit keeps each
//! vertex normalized, respects locked influences and the influence limit, and
//! adds a bind transform for any bone that newly influences a bound mesh.
use super::{BindTransform, Mesh, Vertex, Weight, mesh_vertices};
use crate::model::{Rig, Vec2, WorldBone};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Weights at or below this are removed from a vertex.
const EPSILON: f32 = 0.0005;
/// The project format stores at most four influences per vertex.
pub const MAX_INFLUENCES: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrushMode {
    #[default]
    Add,
    Subtract,
    Smooth,
}
impl BrushMode {
    pub const ALL: [Self; 3] = [Self::Add, Self::Subtract, Self::Smooth];
    pub fn label(self) -> &'static str {
        match self {
            Self::Add => "Add",
            Self::Subtract => "Subtract",
            Self::Smooth => "Smooth",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Falloff {
    Constant,
    Linear,
    #[default]
    Smooth,
}
impl Falloff {
    pub const ALL: [Self; 3] = [Self::Constant, Self::Linear, Self::Smooth];
    pub fn label(self) -> &'static str {
        match self {
            Self::Constant => "Constant",
            Self::Linear => "Linear",
            Self::Smooth => "Smooth",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Brush {
    pub mode: BrushMode,
    /// Radius in canvas pixels.
    pub radius: f32,
    /// Weight change per dab at the brush center, 0–1.
    pub strength: f32,
    pub falloff: Falloff,
}
impl Default for Brush {
    fn default() -> Self {
        Self {
            mode: BrushMode::Add,
            radius: 8.,
            strength: 0.25,
            falloff: Falloff::Smooth,
        }
    }
}
impl Brush {
    /// Strength at `distance` from the brush center; zero outside the radius.
    pub fn factor(&self, distance: f32) -> f32 {
        let radius = self.radius.max(0.0001);
        if !distance.is_finite() || distance > radius {
            return 0.;
        }
        let t = (distance / radius).clamp(0., 1.);
        let shape = match self.falloff {
            Falloff::Constant => 1.,
            Falloff::Linear => 1. - t,
            Falloff::Smooth => {
                let u = 1. - t;
                u * u * (3. - 2. * u)
            }
        };
        shape * self.strength.clamp(0., 1.)
    }
}

pub fn weight_of(v: &Vertex, bone: u32) -> f32 {
    v.weights
        .iter()
        .find(|w| w.bone == bone)
        .map_or(0., |w| w.weight)
}

/// Bones that influence any vertex of the mesh, in id order.
pub fn influences(mesh: &Mesh) -> Vec<u32> {
    mesh.vertices
        .iter()
        .flat_map(|v| v.weights.iter().map(|w| w.bone))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn bind_of(w: &WorldBone) -> BindTransform {
    BindTransform {
        head: w.head,
        angle: w.angle,
        length: (w.tail - w.head).length(),
    }
}

/// A bound mesh needs a bind snapshot for every influence. Meshes without any
/// bind snapshots follow the current rest skeleton and stay that way.
fn ensure_binds(mesh: &mut Mesh, rest: &[WorldBone]) -> Result<()> {
    if mesh.bind.is_empty() {
        return Ok(());
    }
    for bone in influences(mesh) {
        if let std::collections::btree_map::Entry::Vacant(entry) = mesh.bind.entry(bone) {
            let w = rest
                .iter()
                .find(|w| w.id == bone)
                .context("Painted bone is not in the rig")?;
            entry.insert(bind_of(w));
        }
    }
    Ok(())
}

/// Replace a vertex's weights with `wanted`, keeping locked influences fixed,
/// spreading the unlocked share by the wanted proportions, and dropping the
/// smallest unlocked influences beyond `limit`. The result sums to one; when
/// nothing unlocked can take a share the vertex keeps its weights.
fn settle(v: &mut Vertex, wanted: BTreeMap<u32, f32>, locked: &[u32], limit: usize) -> bool {
    let limit = limit.clamp(1, MAX_INFLUENCES);
    let fixed: Vec<Weight> = v
        .weights
        .iter()
        .filter(|w| locked.contains(&w.bone))
        .cloned()
        .collect();
    let free = (1. - fixed.iter().map(|w| w.weight).sum::<f32>()).max(0.);
    let mut open: Vec<Weight> = wanted
        .into_iter()
        .filter(|(bone, weight)| !locked.contains(bone) && weight.is_finite() && *weight > EPSILON)
        .map(|(bone, weight)| Weight { bone, weight })
        .collect();
    if free <= EPSILON {
        open.clear();
    }
    open.sort_by(|a, b| b.weight.total_cmp(&a.weight).then(a.bone.cmp(&b.bone)));
    open.truncate(
        limit
            .saturating_sub(fixed.len())
            .max(usize::from(free > EPSILON)),
    );
    let total: f32 = open.iter().map(|w| w.weight).sum();
    if free > EPSILON && total <= EPSILON {
        return false;
    }
    for w in &mut open {
        w.weight *= free / total.max(EPSILON);
    }
    let mut weights: Vec<Weight> = fixed.into_iter().chain(open).collect();
    weights.sort_by_key(|w| w.bone);
    if weights.is_empty() {
        return false;
    }
    // Absorb float drift so the stored sum is exactly one.
    let sum: f32 = weights.iter().map(|w| w.weight).sum();
    if let Some(w) = weights
        .iter_mut()
        .filter(|w| !locked.contains(&w.bone))
        .max_by(|a, b| a.weight.total_cmp(&b.weight))
    {
        w.weight = (w.weight + 1. - sum).max(0.);
    }
    let mut old = v.weights.clone();
    old.sort_by_key(|w| w.bone);
    let changed = old.len() != weights.len()
        || old
            .iter()
            .zip(&weights)
            .any(|(a, b)| a.bone != b.bone || (a.weight - b.weight).abs() > 0.00001);
    if changed {
        v.weights = weights;
    }
    changed
}

/// Set one bone's weight on a vertex, scaling the other unlocked influences to
/// fill the rest. Locked bones cannot be painted.
fn set_weight(v: &mut Vertex, bone: u32, target: f32, locked: &[u32], limit: usize) -> bool {
    if locked.contains(&bone) {
        return false;
    }
    let free = (1.
        - v.weights
            .iter()
            .filter(|w| locked.contains(&w.bone))
            .map(|w| w.weight)
            .sum::<f32>())
    .max(0.);
    let target = target.clamp(0., free);
    let others: BTreeMap<u32, f32> = v
        .weights
        .iter()
        .filter(|w| w.bone != bone && !locked.contains(&w.bone))
        .map(|w| (w.bone, w.weight))
        .collect();
    let others_total: f32 = others.values().sum();
    let mut wanted = BTreeMap::new();
    if others_total <= EPSILON {
        // Nothing else can take or give a share: the bone keeps all of it.
        wanted.insert(bone, free.max(EPSILON * 2.));
    } else {
        wanted.insert(bone, target);
        for (b, w) in others {
            wanted.insert(b, w / others_total * (free - target));
        }
    }
    settle(v, wanted, locked, limit)
}

/// One brush dab at `center`, measured against the displayed vertex
/// positions `points`. Returns the number of changed vertices.
pub fn paint(
    mesh: &mut Mesh,
    rest: &[WorldBone],
    points: &[Vec2],
    center: Vec2,
    brush: &Brush,
    bone: u32,
    limit: usize,
) -> Result<usize> {
    ensure!(
        points.len() == mesh.vertices.len(),
        "Mesh points do not match its vertices"
    );
    ensure!(
        rest.iter().any(|w| w.id == bone),
        "Choose a bone in the rig to paint"
    );
    let factors: Vec<f32> = points
        .iter()
        .map(|p| brush.factor((*p - center).length()))
        .collect();
    let locked = mesh.locked.clone();
    let mut changed = 0;
    match brush.mode {
        BrushMode::Add | BrushMode::Subtract => {
            let sign = if brush.mode == BrushMode::Add {
                1.
            } else {
                -1.
            };
            for (v, f) in mesh.vertices.iter_mut().zip(&factors) {
                if *f > 0. && set_weight(v, bone, weight_of(v, bone) + sign * f, &locked, limit) {
                    changed += 1;
                }
            }
        }
        BrushMode::Smooth => {
            let neighbors = neighbors(mesh);
            let before = mesh.vertices.clone();
            for (i, v) in mesh.vertices.iter_mut().enumerate() {
                let f = factors[i];
                if f <= 0. || neighbors[i].is_empty() {
                    continue;
                }
                let mut average: BTreeMap<u32, f32> = BTreeMap::new();
                for n in &neighbors[i] {
                    for w in &before[*n].weights {
                        *average.entry(w.bone).or_default() += w.weight / neighbors[i].len() as f32;
                    }
                }
                let mut wanted: BTreeMap<u32, f32> =
                    average.iter().map(|(bone, w)| (*bone, w * f)).collect();
                for w in &before[i].weights {
                    *wanted.entry(w.bone).or_default() += w.weight * (1. - f);
                }
                if settle(v, wanted, &locked, limit) {
                    changed += 1;
                }
            }
        }
    }
    ensure_binds(mesh, rest)?;
    Ok(changed)
}

/// Vertices sharing a triangle edge with each vertex.
fn neighbors(mesh: &Mesh) -> Vec<Vec<usize>> {
    let mut sets = vec![BTreeSet::new(); mesh.vertices.len()];
    for t in &mesh.triangles {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            if a < sets.len() && b < sets.len() && a != b {
                sets[a].insert(b);
                sets[b].insert(a);
            }
        }
    }
    sets.into_iter().map(|s| s.into_iter().collect()).collect()
}

/// Give `bone` exactly `weight` on every listed vertex.
pub fn assign(
    mesh: &mut Mesh,
    rest: &[WorldBone],
    vertices: &[usize],
    bone: u32,
    weight: f32,
    limit: usize,
) -> Result<usize> {
    ensure!(
        rest.iter().any(|w| w.id == bone),
        "Choose a bone in the rig to assign"
    );
    ensure!(
        !mesh.locked.contains(&bone),
        "Unlock the bone before assigning it"
    );
    let locked = mesh.locked.clone();
    let mut changed = 0;
    for i in vertices.iter().copied().collect::<BTreeSet<_>>() {
        let v = mesh
            .vertices
            .get_mut(i)
            .context("Vertex is not in the mesh")?;
        if set_weight(v, bone, weight, &locked, limit) {
            changed += 1;
        }
    }
    ensure_binds(mesh, rest)?;
    Ok(changed)
}

/// Rescale every vertex to sum to one, dropping zero and non-finite weights and
/// any influence beyond `limit`. Locked influences are kept as they are.
pub fn normalize(mesh: &mut Mesh, limit: usize) -> usize {
    let locked = mesh.locked.clone();
    let mut changed = 0;
    for v in &mut mesh.vertices {
        let wanted = v
            .weights
            .iter()
            .filter(|w| !locked.contains(&w.bone))
            .map(|w| (w.bone, w.weight))
            .collect();
        if settle(v, wanted, &locked, limit) {
            changed += 1;
        }
    }
    changed
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RebindMode {
    /// Artwork stays where it is displayed in the rest pose now.
    #[default]
    KeepShape,
    /// Artwork returns to where it was drawn, over the moved skeleton.
    OriginalPlacement,
}

/// The mesh rebound to the current rest skeleton: new bind snapshots for every
/// influence and, for [`RebindMode::KeepShape`], vertex rest positions moved to
/// their current rest-pose positions.
pub fn rebind(rig: &Rig, mesh: &Mesh, mode: RebindMode) -> Result<Mesh> {
    let rest = rig.world(None);
    let mut out = mesh.clone();
    if mode == RebindMode::KeepShape {
        for (v, p) in out
            .vertices
            .iter_mut()
            .zip(mesh_vertices(rig, None, mesh, 0.))
        {
            ensure!(p.is_finite(), "Mesh deforms to an invalid position");
            v.rest = p;
        }
    }
    out.bind.clear();
    for bone in influences(mesh) {
        let w = rest
            .iter()
            .find(|w| w.id == bone)
            .context("Mesh influence is not in the rig")?;
        out.bind.insert(bone, bind_of(w));
    }
    Ok(out)
}
