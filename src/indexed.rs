//! Support for bindings that report query results as input positions.
//!
//! A foreign caller (C, Python, JavaScript) hands over a sequence of geometries and
//! expects query results as positions into that sequence, the shapely `STRtree`
//! convention. Two pieces serve that:
//!
//! - [`IndexedLeaf`] pairs a geometry with its position, so a linestring or polygon tree
//!   can report the position directly from the matched leaf.
//! - [`IndexedPoint`] is a point with its position attached; `GeodeticRTree<IndexedPoint>`
//!   keeps the point-only queries (the rectangle query and the `wgs84` refine) through
//!   [`PointLeaf`], and [`nearest_position`] adds a deterministic tie-break among
//!   bit-identical duplicates.

use core::ops::Deref;

use rstar::primitives::GeomWithData;
use rstar::{AABB, PointDistance, RTreeObject};

use crate::{GeodeticCoord, GeodeticPoint, PointLeaf, UnitVec};

/// A point that carries its position in the caller's input sequence, for a point tree
/// that reports query results as positions: `GeodeticRTree<IndexedPoint>` has the
/// point-only queries through [`PointLeaf`], and each hit yields `.data` directly.
pub type IndexedPoint = GeomWithData<GeodeticPoint, usize>;

/// Pairs each point with its position in the iteration order.
pub fn indexed_points(
    points: impl IntoIterator<Item = GeodeticPoint>,
) -> alloc::vec::Vec<IndexedPoint> {
    points
        .into_iter()
        .enumerate()
        .map(|(index, point)| IndexedPoint::new(point, index))
        .collect()
}

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

impl<G> PointLeaf for IndexedLeaf<G>
where
    G: Deref<Target = GeodeticPoint>,
{
    fn coord(&self) -> GeodeticCoord {
        self.geometry.coord()
    }
}

/// The position and great-circle distance of the nearest indexed point to `query`, or
/// `None` if the tree is empty. Among points at the nearest coordinate (bit-identical
/// duplicates embed to the same vector, so they tie exactly), the lowest position wins,
/// so the result does not depend on tree order.
pub fn nearest_position(
    tree: &crate::GeodeticRTree<IndexedPoint>,
    query: GeodeticCoord,
) -> Option<(usize, f64)> {
    let mut candidates = tree.nearest_neighbor_iter_with_distance(query);
    let (first, metres) = candidates.next()?;
    let mut position = first.data;
    for (leaf, other_metres) in candidates {
        if other_metres > metres {
            break;
        }
        position = position.min(leaf.data);
    }
    Some((position, metres))
}

/// The position and WGS84 geodesic distance of the nearest indexed point to `query`, or
/// `None` if the tree is empty, with the same lowest-position tie-break as
/// [`nearest_position`].
#[cfg(feature = "wgs84")]
pub fn nearest_position_wgs84(
    tree: &crate::GeodeticRTree<IndexedPoint>,
    query: GeodeticCoord,
) -> Option<(usize, f64)> {
    let (leaf, metres) = tree.nearest_neighbor_with_distance_wgs84(query)?;
    let position = tree
        .locate_all_at_point(leaf.coord())
        .map(|other| other.data)
        .min()
        .unwrap_or(leaf.data);
    Some((position, metres))
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;
    use crate::GeodeticRTree;

    fn tree() -> GeodeticRTree<IndexedPoint> {
        GeodeticRTree::bulk_load(indexed_points(vec![
            GeodeticPoint::new(10.0, 10.0),
            GeodeticPoint::new(20.0, 20.0),
            GeodeticPoint::new(10.0, 10.0),
            GeodeticPoint::new(179.0, 0.0),
            GeodeticPoint::new(-179.0, 0.0),
        ]))
    }

    #[test]
    fn nearest_position_prefers_the_lowest_duplicate() {
        let tree = tree();
        let query = GeodeticCoord {
            lon: 10.0,
            lat: 10.0,
        };
        assert_eq!(nearest_position(&tree, query), Some((0, 0.0)));
        let (position, metres) = nearest_position(
            &tree,
            GeodeticCoord {
                lon: 21.0,
                lat: 20.0,
            },
        )
        .unwrap();
        assert_eq!(position, 1);
        assert!(metres > 0.0);
        assert_eq!(
            nearest_position(&GeodeticRTree::<IndexedPoint>::new(), query),
            None
        );
    }

    #[test]
    fn point_only_queries_report_positions() {
        let tree = tree();
        let lower = GeodeticCoord {
            lon: 170.0,
            lat: -1.0,
        };
        let upper = GeodeticCoord {
            lon: -170.0,
            lat: 1.0,
        };
        let mut across_the_seam: alloc::vec::Vec<usize> = tree
            .locate_in_rectangle(lower, upper)
            .map(|leaf| leaf.data)
            .collect();
        across_the_seam.sort_unstable();
        assert_eq!(across_the_seam, vec![3, 4]);
        let mut at_duplicate: alloc::vec::Vec<usize> = tree
            .locate_all_at_point(GeodeticCoord {
                lon: 10.0,
                lat: 10.0,
            })
            .map(|leaf| leaf.data)
            .collect();
        at_duplicate.sort_unstable();
        assert_eq!(at_duplicate, vec![0, 2]);
    }

    #[cfg(feature = "wgs84")]
    #[test]
    fn nearest_position_wgs84_prefers_the_lowest_duplicate() {
        let tree = tree();
        let query = GeodeticCoord {
            lon: 10.0,
            lat: 10.0,
        };
        assert_eq!(nearest_position_wgs84(&tree, query), Some((0, 0.0)));
        let (position, metres) = nearest_position_wgs84(
            &tree,
            GeodeticCoord {
                lon: 21.0,
                lat: 20.0,
            },
        )
        .unwrap();
        assert_eq!(position, 1);
        assert!(metres > 0.0);
    }
}
