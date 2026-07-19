//! Nearest-neighbour, within-distance, and rectangle queries over the three tree types.
//!
//! Distances are great-circle metres on the spherical model (the wgs84 refine lives in a
//! separate module). Nearest-neighbour writes a single [`RsgNeighbor`] and a `found` flag;
//! within-distance and the rectangle query allocate a result buffer the caller must
//! release with [`rsg_neighbors_free`] or [`rsg_indices_free`]. An empty result yields a
//! null buffer pointer and a length of zero.

use std::collections::HashSet;

use rstar::PointDistance;

use crate::{GeodeticCoord, GeodeticObject, GeodeticRTree, UnitVec, squared_chord_to_metres};

use super::construct::{RsgLineTree, RsgPointTree, RsgPolygonTree};
use super::error::RsgStatus;
use super::{IndexedLeaf, coord_key, ffi_guard};

/// A query result: the caller's input position and the great-circle distance in metres
/// from the query point to the nearest point of that geometry.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct RsgNeighbor {
    /// The input position of the geometry (its index at construction).
    pub index: usize,
    /// Great-circle distance from the query point to the geometry, in metres.
    pub distance_metres: f64,
}

/// Writes an optional nearest result to the out-parameters. `None` sets `found` to false
/// and leaves the neighbour untouched.
///
/// # Safety
///
/// `out_neighbor` and `out_found` must be valid, writable pointers.
unsafe fn write_nearest(
    result: Option<RsgNeighbor>,
    out_neighbor: *mut RsgNeighbor,
    out_found: *mut bool,
) {
    match result {
        Some(neighbor) => unsafe {
            *out_neighbor = neighbor;
            *out_found = true;
        },
        None => unsafe { *out_found = false },
    }
}

/// Hands a neighbour buffer to the caller: an empty vector becomes a null pointer and a
/// length of zero, otherwise the boxed slice is leaked for the caller to reclaim through
/// [`rsg_neighbors_free`].
///
/// # Safety
///
/// `out_items` and `out_len` must be valid, writable pointers.
unsafe fn write_neighbor_buffer(
    neighbors: Vec<RsgNeighbor>,
    out_items: *mut *mut RsgNeighbor,
    out_len: *mut usize,
) {
    if neighbors.is_empty() {
        unsafe {
            *out_items = core::ptr::null_mut();
            *out_len = 0;
        }
        return;
    }
    let mut boxed = neighbors.into_boxed_slice();
    let len = boxed.len();
    let ptr = boxed.as_mut_ptr();
    core::mem::forget(boxed);
    unsafe {
        *out_items = ptr;
        *out_len = len;
    }
}

/// As [`write_neighbor_buffer`], for a buffer of bare input positions (the rectangle
/// query), reclaimed through [`rsg_indices_free`].
///
/// # Safety
///
/// `out_items` and `out_len` must be valid, writable pointers.
unsafe fn write_index_buffer(indices: Vec<usize>, out_items: *mut *mut usize, out_len: *mut usize) {
    if indices.is_empty() {
        unsafe {
            *out_items = core::ptr::null_mut();
            *out_len = 0;
        }
        return;
    }
    let mut boxed = indices.into_boxed_slice();
    let len = boxed.len();
    let ptr = boxed.as_mut_ptr();
    core::mem::forget(boxed);
    unsafe {
        *out_items = ptr;
        *out_len = len;
    }
}

// --- shared query logic for the IndexedLeaf (linestring / polygon) trees ---

/// The nearest indexed leaf, reporting its carried input position and spherical distance.
fn indexed_nearest<G>(
    tree: &GeodeticRTree<IndexedLeaf<G>>,
    query: GeodeticCoord,
) -> Option<RsgNeighbor>
where
    IndexedLeaf<G>: GeodeticObject,
{
    tree.nearest_neighbor_with_distance(query)
        .map(|(leaf, metres)| RsgNeighbor {
            index: leaf.index,
            distance_metres: metres,
        })
}

/// Every indexed leaf within `radius_metres`, each reporting its carried input position and
/// its own spherical distance to the query.
fn indexed_within<G>(
    tree: &GeodeticRTree<IndexedLeaf<G>>,
    query: GeodeticCoord,
    radius_metres: f64,
) -> Vec<RsgNeighbor>
where
    IndexedLeaf<G>: GeodeticObject,
{
    let query_vector = UnitVec::from(query);
    tree.locate_within_distance(query, radius_metres)
        .map(|leaf| RsgNeighbor {
            index: leaf.index,
            distance_metres: squared_chord_to_metres(leaf.distance_2(&query_vector)),
        })
        .collect()
}

// --- point tree ---

/// Writes the nearest point to `(lon, lat)` to `*out_neighbor`, setting `*out_found`. On
/// an empty tree `*out_found` is false.
///
/// # Safety
///
/// `tree` must be a valid handle; `out_neighbor` and `out_found` valid, writable pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_point_tree_nearest_neighbor(
    tree: *const RsgPointTree,
    lon: f64,
    lat: f64,
    out_neighbor: *mut RsgNeighbor,
    out_found: *mut bool,
) -> RsgStatus {
    ffi_guard(|| {
        if tree.is_null() || out_neighbor.is_null() || out_found.is_null() {
            return RsgStatus::RSG_ERR_NULL_ARGUMENT;
        }
        // Safety: `tree` was checked non-null above.
        let handle = unsafe { &*tree };
        let query = GeodeticCoord { lon, lat };
        let result = handle
            .tree
            .nearest_neighbor_with_distance(query)
            .map(|(point, metres)| RsgNeighbor {
                index: handle.index_by_coord[&coord_key(point.coord())][0],
                distance_metres: metres,
            });
        // Safety: both out-pointers were checked non-null above.
        unsafe { write_nearest(result, out_neighbor, out_found) };
        RsgStatus::RSG_OK
    })
}

/// Writes every point within `radius_metres` of `(lon, lat)` to a fresh buffer at
/// `*out_items` (length `*out_len`), which the caller frees with [`rsg_neighbors_free`].
///
/// # Safety
///
/// `tree` must be a valid handle; `out_items` and `out_len` valid, writable pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_point_tree_within_distance(
    tree: *const RsgPointTree,
    lon: f64,
    lat: f64,
    radius_metres: f64,
    out_items: *mut *mut RsgNeighbor,
    out_len: *mut usize,
) -> RsgStatus {
    ffi_guard(|| {
        if tree.is_null() || out_items.is_null() || out_len.is_null() {
            return RsgStatus::RSG_ERR_NULL_ARGUMENT;
        }
        // Safety: `tree` was checked non-null above.
        let handle = unsafe { &*tree };
        let query = GeodeticCoord { lon, lat };
        let query_vector = UnitVec::from(query);
        // A stored leaf is yielded once per input position, so points at a shared
        // coordinate arrive together; dedupe by coordinate and emit every input position
        // recorded there (they share one distance).
        let mut seen: HashSet<(u64, u64)> = HashSet::new();
        let mut neighbors: Vec<RsgNeighbor> = Vec::new();
        for point in handle.tree.locate_within_distance(query, radius_metres) {
            let key = coord_key(point.coord());
            if seen.insert(key) {
                let metres = squared_chord_to_metres(point.distance_2(&query_vector));
                for &index in &handle.index_by_coord[&key] {
                    neighbors.push(RsgNeighbor {
                        index,
                        distance_metres: metres,
                    });
                }
            }
        }
        // Safety: both out-pointers were checked non-null above.
        unsafe { write_neighbor_buffer(neighbors, out_items, out_len) };
        RsgStatus::RSG_OK
    })
}

/// Writes the input positions of every point inside the longitude/latitude rectangle
/// `[lower, upper]` to a fresh buffer at `*out_indices` (length `*out_len`), freed with
/// [`rsg_indices_free`]. `lower_lon > upper_lon` denotes a window crossing the
/// antimeridian (RFC 7946); `lower_lat <= upper_lat` is required.
///
/// # Safety
///
/// `tree` must be a valid handle; `out_indices` and `out_len` valid, writable pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_point_tree_in_rectangle(
    tree: *const RsgPointTree,
    lower_lon: f64,
    lower_lat: f64,
    upper_lon: f64,
    upper_lat: f64,
    out_indices: *mut *mut usize,
    out_len: *mut usize,
) -> RsgStatus {
    ffi_guard(|| {
        if tree.is_null() || out_indices.is_null() || out_len.is_null() {
            return RsgStatus::RSG_ERR_NULL_ARGUMENT;
        }
        // Safety: `tree` was checked non-null above.
        let handle = unsafe { &*tree };
        let lower = GeodeticCoord {
            lon: lower_lon,
            lat: lower_lat,
        };
        let upper = GeodeticCoord {
            lon: upper_lon,
            lat: upper_lat,
        };
        let mut seen: HashSet<(u64, u64)> = HashSet::new();
        let mut indices: Vec<usize> = Vec::new();
        for point in handle.tree.locate_in_rectangle(lower, upper) {
            let key = coord_key(point.coord());
            if seen.insert(key) {
                indices.extend_from_slice(&handle.index_by_coord[&key]);
            }
        }
        // Safety: both out-pointers were checked non-null above.
        unsafe { write_index_buffer(indices, out_indices, out_len) };
        RsgStatus::RSG_OK
    })
}

// --- linestring tree ---

/// Nearest linestring to `(lon, lat)`; see [`rsg_point_tree_nearest_neighbor`].
///
/// # Safety
///
/// `tree` must be a valid handle; `out_neighbor` and `out_found` valid, writable pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_line_tree_nearest_neighbor(
    tree: *const RsgLineTree,
    lon: f64,
    lat: f64,
    out_neighbor: *mut RsgNeighbor,
    out_found: *mut bool,
) -> RsgStatus {
    ffi_guard(|| {
        if tree.is_null() || out_neighbor.is_null() || out_found.is_null() {
            return RsgStatus::RSG_ERR_NULL_ARGUMENT;
        }
        // Safety: `tree` was checked non-null above.
        let handle = unsafe { &*tree };
        let result = indexed_nearest(&handle.tree, GeodeticCoord { lon, lat });
        // Safety: both out-pointers were checked non-null above.
        unsafe { write_nearest(result, out_neighbor, out_found) };
        RsgStatus::RSG_OK
    })
}

/// Every linestring within `radius_metres` of `(lon, lat)`; see
/// [`rsg_point_tree_within_distance`].
///
/// # Safety
///
/// `tree` must be a valid handle; `out_items` and `out_len` valid, writable pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_line_tree_within_distance(
    tree: *const RsgLineTree,
    lon: f64,
    lat: f64,
    radius_metres: f64,
    out_items: *mut *mut RsgNeighbor,
    out_len: *mut usize,
) -> RsgStatus {
    ffi_guard(|| {
        if tree.is_null() || out_items.is_null() || out_len.is_null() {
            return RsgStatus::RSG_ERR_NULL_ARGUMENT;
        }
        // Safety: `tree` was checked non-null above.
        let handle = unsafe { &*tree };
        let neighbors = indexed_within(&handle.tree, GeodeticCoord { lon, lat }, radius_metres);
        // Safety: both out-pointers were checked non-null above.
        unsafe { write_neighbor_buffer(neighbors, out_items, out_len) };
        RsgStatus::RSG_OK
    })
}

// --- polygon tree ---

/// Nearest polygon to `(lon, lat)` (zero distance if the point is inside one); see
/// [`rsg_point_tree_nearest_neighbor`].
///
/// # Safety
///
/// `tree` must be a valid handle; `out_neighbor` and `out_found` valid, writable pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_polygon_tree_nearest_neighbor(
    tree: *const RsgPolygonTree,
    lon: f64,
    lat: f64,
    out_neighbor: *mut RsgNeighbor,
    out_found: *mut bool,
) -> RsgStatus {
    ffi_guard(|| {
        if tree.is_null() || out_neighbor.is_null() || out_found.is_null() {
            return RsgStatus::RSG_ERR_NULL_ARGUMENT;
        }
        // Safety: `tree` was checked non-null above.
        let handle = unsafe { &*tree };
        let result = indexed_nearest(&handle.tree, GeodeticCoord { lon, lat });
        // Safety: both out-pointers were checked non-null above.
        unsafe { write_nearest(result, out_neighbor, out_found) };
        RsgStatus::RSG_OK
    })
}

/// Every polygon within `radius_metres` of `(lon, lat)`; see
/// [`rsg_point_tree_within_distance`].
///
/// # Safety
///
/// `tree` must be a valid handle; `out_items` and `out_len` valid, writable pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_polygon_tree_within_distance(
    tree: *const RsgPolygonTree,
    lon: f64,
    lat: f64,
    radius_metres: f64,
    out_items: *mut *mut RsgNeighbor,
    out_len: *mut usize,
) -> RsgStatus {
    ffi_guard(|| {
        if tree.is_null() || out_items.is_null() || out_len.is_null() {
            return RsgStatus::RSG_ERR_NULL_ARGUMENT;
        }
        // Safety: `tree` was checked non-null above.
        let handle = unsafe { &*tree };
        let neighbors = indexed_within(&handle.tree, GeodeticCoord { lon, lat }, radius_metres);
        // Safety: both out-pointers were checked non-null above.
        unsafe { write_neighbor_buffer(neighbors, out_items, out_len) };
        RsgStatus::RSG_OK
    })
}

// --- result-buffer release ---

/// Releases a buffer returned by a `*_within_distance` query. Null is a no-op.
///
/// # Safety
///
/// `items`/`len` must be a buffer and length returned together by a `*_within_distance`
/// call and not already freed, or `items` null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_neighbors_free(items: *mut RsgNeighbor, len: usize) {
    if items.is_null() {
        return;
    }
    // Safety: the buffer was created by `Box<[RsgNeighbor]>::into` of this exact length.
    drop(unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(items, len)) });
}

/// Releases a buffer returned by [`rsg_point_tree_in_rectangle`]. Null is a no-op.
///
/// # Safety
///
/// `items`/`len` must be a buffer and length returned together by the rectangle query and
/// not already freed, or `items` null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_indices_free(items: *mut usize, len: usize) {
    if items.is_null() {
        return;
    }
    // Safety: the buffer was created by `Box<[usize]>::into` of this exact length.
    drop(unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(items, len)) });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::construct::{
        rsg_line_tree_free, rsg_line_tree_new, rsg_point_tree_free, rsg_point_tree_new,
        rsg_polygon_tree_free, rsg_polygon_tree_new,
    };
    use core::ptr;

    fn point_tree(coords: &[f64]) -> *mut RsgPointTree {
        let mut tree: *mut RsgPointTree = ptr::null_mut();
        let n = coords.len() / 2;
        assert_eq!(
            unsafe { rsg_point_tree_new(coords.as_ptr(), n, &mut tree) },
            RsgStatus::RSG_OK
        );
        tree
    }

    // London, Paris, Berlin, Madrid.
    const CAPITALS: [f64; 8] = [
        -0.1278, 51.5074, 2.3522, 48.8566, 13.4050, 52.5200, -3.7038, 40.4168,
    ];

    #[test]
    fn point_nearest_returns_input_index() {
        let tree = point_tree(&CAPITALS);
        let mut neighbor = RsgNeighbor {
            index: 99,
            distance_metres: -1.0,
        };
        let mut found = false;
        // A query near Paris (index 1).
        let status =
            unsafe { rsg_point_tree_nearest_neighbor(tree, 2.0, 49.0, &mut neighbor, &mut found) };
        assert_eq!(status, RsgStatus::RSG_OK);
        assert!(found);
        assert_eq!(neighbor.index, 1);
        assert!(neighbor.distance_metres > 0.0 && neighbor.distance_metres < 50_000.0);
        unsafe { rsg_point_tree_free(tree) };
    }

    #[test]
    fn point_nearest_empty_tree_sets_found_false() {
        let tree = point_tree(&[]);
        let mut neighbor = RsgNeighbor {
            index: 0,
            distance_metres: 0.0,
        };
        let mut found = true;
        let status =
            unsafe { rsg_point_tree_nearest_neighbor(tree, 0.0, 0.0, &mut neighbor, &mut found) };
        assert_eq!(status, RsgStatus::RSG_OK);
        assert!(!found);
        unsafe { rsg_point_tree_free(tree) };
    }

    #[test]
    fn point_within_distance_returns_in_range_indices() {
        let tree = point_tree(&CAPITALS);
        let mut items: *mut RsgNeighbor = ptr::null_mut();
        let mut len = 0usize;
        // Within 400 km of a point over the Channel: London (0) and Paris (1).
        let status = unsafe {
            rsg_point_tree_within_distance(tree, 0.0, 50.0, 400_000.0, &mut items, &mut len)
        };
        assert_eq!(status, RsgStatus::RSG_OK);
        let slice = unsafe { core::slice::from_raw_parts(items, len) };
        let mut indices: Vec<usize> = slice.iter().map(|n| n.index).collect();
        indices.sort_unstable();
        assert_eq!(indices, vec![0, 1]);
        unsafe { rsg_neighbors_free(items, len) };
        unsafe { rsg_point_tree_free(tree) };
    }

    #[test]
    fn point_within_distance_duplicate_coords_yield_both_indices() {
        // Two identical points plus a distant one.
        let coords = [0.0_f64, 0.0, 0.0, 0.0, 40.0, 40.0];
        let tree = point_tree(&coords);
        let mut items: *mut RsgNeighbor = ptr::null_mut();
        let mut len = 0usize;
        let status = unsafe {
            rsg_point_tree_within_distance(tree, 0.0, 0.0, 1_000.0, &mut items, &mut len)
        };
        assert_eq!(status, RsgStatus::RSG_OK);
        assert_eq!(len, 2);
        let slice = unsafe { core::slice::from_raw_parts(items, len) };
        let mut indices: Vec<usize> = slice.iter().map(|n| n.index).collect();
        indices.sort_unstable();
        assert_eq!(indices, vec![0, 1]);
        unsafe { rsg_neighbors_free(items, len) };
        unsafe { rsg_point_tree_free(tree) };
    }

    #[test]
    fn point_within_distance_empty_result_is_null_buffer() {
        let tree = point_tree(&CAPITALS);
        let mut items: *mut RsgNeighbor = ptr::null_mut();
        let mut len = 7usize;
        // A tiny radius over open ocean matches nothing.
        let status = unsafe {
            rsg_point_tree_within_distance(tree, -30.0, -30.0, 1.0, &mut items, &mut len)
        };
        assert_eq!(status, RsgStatus::RSG_OK);
        assert!(items.is_null());
        assert_eq!(len, 0);
        unsafe { rsg_neighbors_free(items, len) };
        unsafe { rsg_point_tree_free(tree) };
    }

    #[test]
    fn point_in_rectangle_returns_indices_across_antimeridian() {
        // 179E (0), 178W (1), lon 0 (2). A wrapping window 170E -> 170W selects 0 and 1.
        let coords = [179.0_f64, 0.0, -178.0, 1.0, 0.0, 0.0];
        let tree = point_tree(&coords);
        let mut items: *mut usize = ptr::null_mut();
        let mut len = 0usize;
        let status = unsafe {
            rsg_point_tree_in_rectangle(tree, 170.0, -10.0, -170.0, 10.0, &mut items, &mut len)
        };
        assert_eq!(status, RsgStatus::RSG_OK);
        let slice = unsafe { core::slice::from_raw_parts(items, len) };
        let mut indices = slice.to_vec();
        indices.sort_unstable();
        assert_eq!(indices, vec![0, 1]);
        unsafe { rsg_indices_free(items, len) };
        unsafe { rsg_point_tree_free(tree) };
    }

    #[test]
    fn line_nearest_and_within() {
        // Two lines: near (index 0) and far (index 1).
        let coords = [0.0_f64, 0.0, 1.0, 1.0, 2.0, 0.0, 10.0, 10.0, 11.0, 11.0];
        let offsets = [0usize, 3, 5];
        let mut tree: *mut RsgLineTree = ptr::null_mut();
        assert_eq!(
            unsafe { rsg_line_tree_new(coords.as_ptr(), 5, offsets.as_ptr(), 2, &mut tree) },
            RsgStatus::RSG_OK
        );
        let mut neighbor = RsgNeighbor {
            index: 9,
            distance_metres: -1.0,
        };
        let mut found = false;
        assert_eq!(
            unsafe { rsg_line_tree_nearest_neighbor(tree, 1.0, 0.5, &mut neighbor, &mut found) },
            RsgStatus::RSG_OK
        );
        assert!(found);
        assert_eq!(neighbor.index, 0);

        let mut items: *mut RsgNeighbor = ptr::null_mut();
        let mut len = 0usize;
        assert_eq!(
            unsafe {
                rsg_line_tree_within_distance(tree, 1.0, 0.5, 100_000.0, &mut items, &mut len)
            },
            RsgStatus::RSG_OK
        );
        assert_eq!(len, 1);
        unsafe { rsg_neighbors_free(items, len) };
        unsafe { rsg_line_tree_free(tree) };
    }

    #[test]
    fn polygon_nearest_is_zero_inside() {
        // A single lon/lat square, index 0.
        let coords = [0.0_f64, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0, 0.0, 0.0];
        let ring_offsets = [0usize, 5];
        let polygon_offsets = [0usize, 1];
        let mut tree: *mut RsgPolygonTree = ptr::null_mut();
        assert_eq!(
            unsafe {
                rsg_polygon_tree_new(
                    coords.as_ptr(),
                    5,
                    ring_offsets.as_ptr(),
                    1,
                    polygon_offsets.as_ptr(),
                    1,
                    &mut tree,
                )
            },
            RsgStatus::RSG_OK
        );
        let mut neighbor = RsgNeighbor {
            index: 9,
            distance_metres: -1.0,
        };
        let mut found = false;
        // A point inside the square: distance zero.
        assert_eq!(
            unsafe { rsg_polygon_tree_nearest_neighbor(tree, 5.0, 5.0, &mut neighbor, &mut found) },
            RsgStatus::RSG_OK
        );
        assert!(found);
        assert_eq!(neighbor.index, 0);
        assert_eq!(neighbor.distance_metres, 0.0);
        unsafe { rsg_polygon_tree_free(tree) };
    }

    #[test]
    fn nearest_rejects_null_arguments() {
        let tree = point_tree(&CAPITALS);
        let mut found = false;
        assert_eq!(
            unsafe { rsg_point_tree_nearest_neighbor(tree, 0.0, 0.0, ptr::null_mut(), &mut found) },
            RsgStatus::RSG_ERR_NULL_ARGUMENT
        );
        unsafe { rsg_point_tree_free(tree) };
    }

    #[test]
    fn free_null_buffers_are_no_ops() {
        unsafe { rsg_neighbors_free(ptr::null_mut(), 0) };
        unsafe { rsg_indices_free(ptr::null_mut(), 0) };
    }
}
