//! Support for bindings that report query results as input positions.
//!
//! A foreign caller (C, Python, JavaScript) hands over a sequence of geometries and
//! expects query results as positions into that sequence, the shapely `STRtree`
//! convention. Two pieces serve that:
//!
//! - [`IndexedLeaf`] pairs a geometry with its position, so a linestring or polygon tree
//!   can report the position directly from the matched leaf.
//! - [`coord_key`] and [`CoordIndex`] recover positions for a point tree, which keeps the
//!   concrete [`GeodeticRTree<GeodeticPoint>`] (and with it the rectangle query and the
//!   `wgs84` refine) and so cannot carry a position in the leaf. Two inputs at bit-identical
//!   coordinates share a key, which is why a key maps to every position at that coordinate.
//!
//! [`GeodeticRTree<GeodeticPoint>`]: crate::GeodeticRTree

use core::ops::Deref;

use rstar::{AABB, PointDistance, RTreeObject};

use crate::{GeodeticCoord, UnitVec};

/// A tree leaf that carries a geometry alongside its input position and delegates the two
/// index traits to the geometry. `G` is a pointer-like type (`Box<T>`, `Arc<T>`, `&T`)
/// whose target is the indexed geometry, so a binding can share one allocation between
/// the tree and a by-position geometry store.
pub struct IndexedLeaf<G> {
    /// The indexed geometry, behind the pointer type `G`.
    pub geometry: G,
    /// The geometry's position in the caller's input sequence.
    pub index: usize,
}

impl<G> RTreeObject for IndexedLeaf<G>
where
    G: Deref,
    G::Target: RTreeObject<Envelope = AABB<UnitVec>>,
{
    type Envelope = AABB<UnitVec>;

    fn envelope(&self) -> Self::Envelope {
        self.geometry.envelope()
    }
}

impl<G> PointDistance for IndexedLeaf<G>
where
    G: Deref,
    G::Target: RTreeObject<Envelope = AABB<UnitVec>> + PointDistance,
{
    fn distance_2(&self, point: &UnitVec) -> f64 {
        self.geometry.distance_2(point)
    }

    fn contains_point(&self, point: &UnitVec) -> bool {
        self.geometry.contains_point(point)
    }

    fn distance_2_if_less_or_equal(&self, point: &UnitVec, max_distance_2: f64) -> Option<f64> {
        self.geometry
            .distance_2_if_less_or_equal(point, max_distance_2)
    }
}

/// The key under which an input position is recorded for coordinate-based recovery: the
/// bit patterns of the longitude and latitude in degrees.
pub fn coord_key(coord: GeodeticCoord) -> (u64, u64) {
    (coord.lon.to_bits(), coord.lat.to_bits())
}

/// Input positions keyed by [`coord_key`]: the map from a point-tree result back to the
/// input positions at that coordinate. Requires the `std` feature.
#[cfg(feature = "std")]
pub type CoordIndex = std::collections::HashMap<(u64, u64), alloc::vec::Vec<usize>>;

/// Builds the [`CoordIndex`] for `points`, recording each point's position in the
/// iteration order. Requires the `std` feature.
#[cfg(feature = "std")]
pub fn coord_index<'a>(points: impl IntoIterator<Item = &'a crate::GeodeticPoint>) -> CoordIndex {
    let mut index_by_coord = CoordIndex::new();
    for (index, point) in points.into_iter().enumerate() {
        index_by_coord
            .entry(coord_key(point.coord()))
            .or_default()
            .push(index);
    }
    index_by_coord
}
