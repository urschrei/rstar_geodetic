//! A C-ABI surface over the geodetic trees.
//!
//! # Ownership and error handling
//!
//! - Every tree is an opaque handle created by a `*_tree_new` constructor and released by
//!   the matching `*_tree_free`. Passing a null handle to a `*_free` function is a no-op.
//!   A handle is immutable after construction and is `Send`/`Sync`, so it may be queried
//!   concurrently from several threads.
//! - Input arrays (coordinates, CSR offsets) are borrowed for the duration of a call
//!   only; the callee copies whatever it needs. The caller retains ownership.
//! - Result buffers are heap-allocated by the callee and must be released by the caller
//!   through the matching `*_free` function ([`rsg_neighbors_free`], [`rsg_indices_free`]).
//! - Coordinates are longitude first, latitude second, in degrees. Point coordinate
//!   arrays are interleaved `[lon0, lat0, lon1, lat1, ...]`. Linestring and polygon
//!   vertices use the same interleaving with CSR offset arrays (the GeoArrow layout); see
//!   the constructors.
//! - Every fallible function returns an [`RsgStatus`]; `RSG_OK` (zero) is success. Any
//!   panic is caught at the boundary and reported as `RSG_ERR_INTERNAL_PANIC` rather than
//!   unwinding across the ABI.
//!
//! [`rsg_neighbors_free`]: query::rsg_neighbors_free
//! [`rsg_indices_free`]: query::rsg_indices_free

use std::panic::{AssertUnwindSafe, catch_unwind};

use rstar::{AABB, PointDistance, RTreeObject};

use crate::{GeodeticCoord, UnitVec};

mod construct;
mod error;

pub use construct::{
    RsgLineTree, RsgPointTree, RsgPolygonTree, rsg_line_tree_free, rsg_line_tree_new,
    rsg_line_tree_size, rsg_point_tree_free, rsg_point_tree_new, rsg_point_tree_size,
    rsg_polygon_tree_free, rsg_polygon_tree_new, rsg_polygon_tree_size,
};
pub use error::{RsgStatus, rsg_status_message};

/// A leaf that carries a geometry alongside its original input position, delegating the
/// two index traits to the geometry. The position rides along so a query result reports
/// the caller's input index directly (the shapely STRtree convention). Used for the
/// linestring and polygon trees; the point tree recovers positions by coordinate instead
/// so it can keep the concrete-type rectangle and wgs84 queries.
pub(crate) struct IndexedLeaf<G> {
    pub(crate) geometry: G,
    // Read by the query module (a later step) to report the caller's input index.
    #[allow(dead_code)]
    pub(crate) index: usize,
}

impl<G> RTreeObject for IndexedLeaf<G>
where
    G: RTreeObject<Envelope = AABB<UnitVec>>,
{
    type Envelope = AABB<UnitVec>;

    fn envelope(&self) -> Self::Envelope {
        self.geometry.envelope()
    }
}

impl<G> PointDistance for IndexedLeaf<G>
where
    G: RTreeObject<Envelope = AABB<UnitVec>> + PointDistance,
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

/// Runs `body` at an ABI boundary, converting any panic into
/// [`RsgStatus::RSG_ERR_INTERNAL_PANIC`] so it never unwinds across the C ABI.
pub(crate) fn ffi_guard(body: impl FnOnce() -> RsgStatus) -> RsgStatus {
    catch_unwind(AssertUnwindSafe(body)).unwrap_or(RsgStatus::RSG_ERR_INTERNAL_PANIC)
}

/// The key under which an input position is recorded for coordinate-based recovery: the
/// bit patterns of the original degrees. Two inputs at bit-identical coordinates share a
/// key, which is why a query result maps to the set of positions at that coordinate.
pub(crate) fn coord_key(coord: GeodeticCoord) -> (u64, u64) {
    (coord.lon.to_bits(), coord.lat.to_bits())
}
