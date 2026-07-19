//! Tree constructors, destructors, and size accessors.
//!
//! The point tree keeps the concrete [`GeodeticRTree<GeodeticPoint>`] so it retains the
//! crate's indexed rectangle query and its tuned wgs84 refine. Input positions are
//! recovered from query results through a coordinate-keyed multimap (see
//! [`super::coord_key`]).

use std::collections::HashMap;

use crate::{GeodeticLineString, GeodeticPoint, GeodeticPolygon, GeodeticRTree, GeodeticRing};

use super::error::RsgStatus;
use super::{IndexedLeaf, coord_key, ffi_guard};

/// Input positions keyed by coordinate bits: the recovery map from a query result's
/// coordinate back to the original input positions at that coordinate.
pub(crate) type CoordIndex = HashMap<(u64, u64), Vec<usize>>;

/// An opaque handle to a point tree. Construct with [`rsg_point_tree_new`] and release with
/// [`rsg_point_tree_free`].
pub struct RsgPointTree {
    pub(crate) tree: GeodeticRTree<GeodeticPoint>,
    // Original input positions keyed by coordinate bits, for STRtree-style index recovery.
    pub(crate) index_by_coord: CoordIndex,
}

/// Reads `count` interleaved `[lon, lat]` pairs from `coords`, validating each and
/// recording its input position. Returns the points in input order and the recovery map,
/// or the first validation error.
///
/// # Safety
///
/// `coords` must point to at least `2 * count` readable `f64`s, unless `count` is zero.
pub(crate) unsafe fn read_points(
    coords: *const f64,
    count: usize,
) -> Result<(Vec<GeodeticPoint>, CoordIndex), RsgStatus> {
    let values: &[f64] = if count == 0 {
        &[]
    } else {
        // Safety: the caller guarantees `2 * count` readable values.
        unsafe { core::slice::from_raw_parts(coords, count * 2) }
    };

    let mut points = Vec::with_capacity(count);
    let mut index_by_coord: CoordIndex = HashMap::new();
    for index in 0..count {
        let lon = values[index * 2];
        let lat = values[index * 2 + 1];
        match GeodeticPoint::try_new(lon, lat) {
            Ok(point) => {
                index_by_coord
                    .entry(coord_key(point.coord()))
                    .or_default()
                    .push(index);
                points.push(point);
            }
            Err(error) => return Err(RsgStatus::from(error)),
        }
    }
    Ok((points, index_by_coord))
}

/// Builds a point tree from `num_points` interleaved `[lon, lat]` degree pairs.
///
/// On success writes a fresh handle to `*out_tree` and returns `RSG_OK`; the caller owns
/// the handle and must release it with [`rsg_point_tree_free`]. `coords` may be null only
/// when `num_points` is zero.
///
/// # Safety
///
/// `coords` must point to at least `2 * num_points` readable `f64`s (or be null when
/// `num_points` is zero), and `out_tree` must be a valid, writable pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_point_tree_new(
    coords: *const f64,
    num_points: usize,
    out_tree: *mut *mut RsgPointTree,
) -> RsgStatus {
    ffi_guard(|| {
        if out_tree.is_null() || (coords.is_null() && num_points != 0) {
            return RsgStatus::RSG_ERR_NULL_ARGUMENT;
        }
        let (points, index_by_coord) = match unsafe { read_points(coords, num_points) } {
            Ok(built) => built,
            Err(status) => return status,
        };
        let handle = Box::new(RsgPointTree {
            tree: GeodeticRTree::bulk_load(points),
            index_by_coord,
        });
        // Safety: `out_tree` was checked non-null above.
        unsafe { *out_tree = Box::into_raw(handle) };
        RsgStatus::RSG_OK
    })
}

/// Releases a point tree. Passing null is a no-op.
///
/// # Safety
///
/// `tree` must be a handle returned by [`rsg_point_tree_new`] that has not already been
/// freed, or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_point_tree_free(tree: *mut RsgPointTree) {
    if tree.is_null() {
        return;
    }
    // Safety: `tree` is a live handle from `rsg_point_tree_new`; reconstruct and drop it.
    drop(unsafe { Box::from_raw(tree) });
}

/// Writes the number of points in `tree` to `*out_size`.
///
/// # Safety
///
/// `tree` must be a valid handle and `out_size` a valid, writable pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_point_tree_size(
    tree: *const RsgPointTree,
    out_size: *mut usize,
) -> RsgStatus {
    ffi_guard(|| {
        if tree.is_null() || out_size.is_null() {
            return RsgStatus::RSG_ERR_NULL_ARGUMENT;
        }
        // Safety: both pointers were checked non-null above.
        let tree = unsafe { &*tree };
        unsafe { *out_size = tree.tree.size() };
        RsgStatus::RSG_OK
    })
}

// --- CSR-partitioned linestring and polygon trees ---

/// An opaque handle to a linestring tree. Construct with [`rsg_line_tree_new`] and release
/// with [`rsg_line_tree_free`].
pub struct RsgLineTree {
    pub(crate) tree: GeodeticRTree<IndexedLeaf<GeodeticLineString>>,
}

/// An opaque handle to a polygon tree. Construct with [`rsg_polygon_tree_new`] and release
/// with [`rsg_polygon_tree_free`].
pub struct RsgPolygonTree {
    pub(crate) tree: GeodeticRTree<IndexedLeaf<GeodeticPolygon>>,
}

/// Reads a length-`num + 1` CSR offset array and validates it partitions `total` elements:
/// the first entry is `0`, the last is `total`, and the sequence is non-decreasing.
///
/// # Safety
///
/// `offsets` must point to at least `num + 1` readable `usize`s.
unsafe fn read_offsets<'a>(
    offsets: *const usize,
    num: usize,
    total: usize,
) -> Result<&'a [usize], RsgStatus> {
    // Safety: the caller guarantees `num + 1` readable values.
    let offsets = unsafe { core::slice::from_raw_parts(offsets, num + 1) };
    let valid = offsets.first() == Some(&0)
        && offsets.last() == Some(&total)
        && offsets.windows(2).all(|pair| pair[0] <= pair[1]);
    if valid {
        Ok(offsets)
    } else {
        Err(RsgStatus::RSG_ERR_INVALID_OFFSETS)
    }
}

/// The interleaved `[lon, lat]` degree pairs at vertex positions `start..end`.
fn vertex_pairs(values: &[f64], start: usize, end: usize) -> impl Iterator<Item = (f64, f64)> {
    (start..end).map(move |i| (values[i * 2], values[i * 2 + 1]))
}

/// Builds a linestring tree from `num_vertices` interleaved `[lon, lat]` degree pairs
/// partitioned into `num_lines` polylines by `line_offsets` (a `num_lines + 1` entry CSR
/// array; the GeoArrow layout).
///
/// On success writes a fresh handle to `*out_tree`; the caller owns it and must release it
/// with [`rsg_line_tree_free`]. Any pointer may be null only when its count is zero.
///
/// # Safety
///
/// `coords` must have `2 * num_vertices` readable `f64`s and `line_offsets`
/// `num_lines + 1` readable `usize`s (or each may be null when its count is zero), and
/// `out_tree` must be a valid, writable pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_line_tree_new(
    coords: *const f64,
    num_vertices: usize,
    line_offsets: *const usize,
    num_lines: usize,
    out_tree: *mut *mut RsgLineTree,
) -> RsgStatus {
    ffi_guard(|| {
        if out_tree.is_null()
            || (coords.is_null() && num_vertices != 0)
            || (line_offsets.is_null() && num_lines != 0)
        {
            return RsgStatus::RSG_ERR_NULL_ARGUMENT;
        }
        let values: &[f64] = if num_vertices == 0 {
            &[]
        } else {
            // Safety: the caller guarantees `2 * num_vertices` readable values.
            unsafe { core::slice::from_raw_parts(coords, num_vertices * 2) }
        };
        let empty_offsets = [0usize];
        let offsets = if num_lines == 0 {
            if num_vertices != 0 {
                return RsgStatus::RSG_ERR_INVALID_OFFSETS;
            }
            &empty_offsets[..]
        } else {
            match unsafe { read_offsets(line_offsets, num_lines, num_vertices) } {
                Ok(offsets) => offsets,
                Err(status) => return status,
            }
        };

        let mut leaves = Vec::with_capacity(num_lines);
        for index in 0..num_lines {
            let pairs = vertex_pairs(values, offsets[index], offsets[index + 1]);
            match GeodeticLineString::try_from_lonlat(pairs) {
                Ok(geometry) => leaves.push(IndexedLeaf { geometry, index }),
                Err(error) => return RsgStatus::from(error),
            }
        }
        let handle = Box::new(RsgLineTree {
            tree: GeodeticRTree::bulk_load(leaves),
        });
        // Safety: `out_tree` was checked non-null above.
        unsafe { *out_tree = Box::into_raw(handle) };
        RsgStatus::RSG_OK
    })
}

/// Releases a linestring tree. Passing null is a no-op.
///
/// # Safety
///
/// `tree` must be a handle from [`rsg_line_tree_new`] that has not been freed, or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_line_tree_free(tree: *mut RsgLineTree) {
    if tree.is_null() {
        return;
    }
    // Safety: `tree` is a live handle from `rsg_line_tree_new`.
    drop(unsafe { Box::from_raw(tree) });
}

/// Writes the number of linestrings in `tree` to `*out_size`.
///
/// # Safety
///
/// `tree` must be a valid handle and `out_size` a valid, writable pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_line_tree_size(
    tree: *const RsgLineTree,
    out_size: *mut usize,
) -> RsgStatus {
    ffi_guard(|| {
        if tree.is_null() || out_size.is_null() {
            return RsgStatus::RSG_ERR_NULL_ARGUMENT;
        }
        // Safety: both pointers were checked non-null above.
        let tree = unsafe { &*tree };
        unsafe { *out_size = tree.tree.size() };
        RsgStatus::RSG_OK
    })
}

/// Builds a polygon tree from `num_vertices` interleaved `[lon, lat]` degree pairs, a
/// two-level CSR layout: `ring_offsets` (`num_rings + 1` entries) partitions the vertices
/// into rings, and `polygon_offsets` (`num_polygons + 1` entries) partitions the rings
/// into polygons. Within each polygon the first ring is the exterior and the rest are
/// holes (the GeoArrow layout).
///
/// On success writes a fresh handle to `*out_tree`; the caller owns it and must release it
/// with [`rsg_polygon_tree_free`]. A polygon with no rings is rejected as
/// `RSG_ERR_INVALID_OFFSETS`.
///
/// # Safety
///
/// `coords` must have `2 * num_vertices` readable `f64`s, `ring_offsets`
/// `num_rings + 1` and `polygon_offsets` `num_polygons + 1` readable `usize`s (or each
/// null when its count is zero), and `out_tree` must be a valid, writable pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_polygon_tree_new(
    coords: *const f64,
    num_vertices: usize,
    ring_offsets: *const usize,
    num_rings: usize,
    polygon_offsets: *const usize,
    num_polygons: usize,
    out_tree: *mut *mut RsgPolygonTree,
) -> RsgStatus {
    ffi_guard(|| {
        if out_tree.is_null()
            || (coords.is_null() && num_vertices != 0)
            || (ring_offsets.is_null() && num_rings != 0)
            || (polygon_offsets.is_null() && num_polygons != 0)
        {
            return RsgStatus::RSG_ERR_NULL_ARGUMENT;
        }
        let values: &[f64] = if num_vertices == 0 {
            &[]
        } else {
            // Safety: the caller guarantees `2 * num_vertices` readable values.
            unsafe { core::slice::from_raw_parts(coords, num_vertices * 2) }
        };
        let empty = [0usize];
        let ring_offsets = if num_rings == 0 {
            if num_vertices != 0 {
                return RsgStatus::RSG_ERR_INVALID_OFFSETS;
            }
            &empty[..]
        } else {
            match unsafe { read_offsets(ring_offsets, num_rings, num_vertices) } {
                Ok(offsets) => offsets,
                Err(status) => return status,
            }
        };
        let polygon_offsets = if num_polygons == 0 {
            if num_rings != 0 {
                return RsgStatus::RSG_ERR_INVALID_OFFSETS;
            }
            &empty[..]
        } else {
            match unsafe { read_offsets(polygon_offsets, num_polygons, num_rings) } {
                Ok(offsets) => offsets,
                Err(status) => return status,
            }
        };

        let mut leaves = Vec::with_capacity(num_polygons);
        for index in 0..num_polygons {
            let first_ring = polygon_offsets[index];
            let last_ring = polygon_offsets[index + 1];
            if first_ring == last_ring {
                // A polygon needs at least an exterior ring.
                return RsgStatus::RSG_ERR_INVALID_OFFSETS;
            }
            let mut rings = Vec::with_capacity(last_ring - first_ring);
            for ring in first_ring..last_ring {
                let pairs = vertex_pairs(values, ring_offsets[ring], ring_offsets[ring + 1]);
                match GeodeticRing::try_from_lonlat(pairs) {
                    Ok(ring) => rings.push(ring),
                    Err(error) => return RsgStatus::from(error),
                }
            }
            let mut rings = rings.into_iter();
            let exterior = rings.next().expect("a polygon has at least one ring");
            let interiors = rings.collect();
            match GeodeticPolygon::try_new(exterior, interiors) {
                Ok(geometry) => leaves.push(IndexedLeaf { geometry, index }),
                Err(error) => return RsgStatus::from(error),
            }
        }
        let handle = Box::new(RsgPolygonTree {
            tree: GeodeticRTree::bulk_load(leaves),
        });
        // Safety: `out_tree` was checked non-null above.
        unsafe { *out_tree = Box::into_raw(handle) };
        RsgStatus::RSG_OK
    })
}

/// Releases a polygon tree. Passing null is a no-op.
///
/// # Safety
///
/// `tree` must be a handle from [`rsg_polygon_tree_new`] that has not been freed, or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_polygon_tree_free(tree: *mut RsgPolygonTree) {
    if tree.is_null() {
        return;
    }
    // Safety: `tree` is a live handle from `rsg_polygon_tree_new`.
    drop(unsafe { Box::from_raw(tree) });
}

/// Writes the number of polygons in `tree` to `*out_size`.
///
/// # Safety
///
/// `tree` must be a valid handle and `out_size` a valid, writable pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsg_polygon_tree_size(
    tree: *const RsgPolygonTree,
    out_size: *mut usize,
) -> RsgStatus {
    ffi_guard(|| {
        if tree.is_null() || out_size.is_null() {
            return RsgStatus::RSG_ERR_NULL_ARGUMENT;
        }
        // Safety: both pointers were checked non-null above.
        let tree = unsafe { &*tree };
        unsafe { *out_size = tree.tree.size() };
        RsgStatus::RSG_OK
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ptr;

    #[test]
    fn new_and_size_round_trip() {
        let coords = [-0.1278_f64, 51.5074, 2.3522, 48.8566];
        let mut tree: *mut RsgPointTree = ptr::null_mut();
        let status = unsafe { rsg_point_tree_new(coords.as_ptr(), 2, &mut tree) };
        assert_eq!(status, RsgStatus::RSG_OK);
        assert!(!tree.is_null());

        let mut size = 0usize;
        assert_eq!(
            unsafe { rsg_point_tree_size(tree, &mut size) },
            RsgStatus::RSG_OK
        );
        assert_eq!(size, 2);
        unsafe { rsg_point_tree_free(tree) };
    }

    #[test]
    fn empty_input_is_allowed_with_null_coords() {
        let mut tree: *mut RsgPointTree = ptr::null_mut();
        let status = unsafe { rsg_point_tree_new(ptr::null(), 0, &mut tree) };
        assert_eq!(status, RsgStatus::RSG_OK);
        let mut size = 1usize;
        assert_eq!(
            unsafe { rsg_point_tree_size(tree, &mut size) },
            RsgStatus::RSG_OK
        );
        assert_eq!(size, 0);
        unsafe { rsg_point_tree_free(tree) };
    }

    #[test]
    fn null_out_pointer_is_rejected() {
        let coords = [0.0_f64, 0.0];
        let status = unsafe { rsg_point_tree_new(coords.as_ptr(), 1, ptr::null_mut()) };
        assert_eq!(status, RsgStatus::RSG_ERR_NULL_ARGUMENT);
    }

    #[test]
    fn null_coords_with_nonzero_count_is_rejected() {
        let mut tree: *mut RsgPointTree = ptr::null_mut();
        let status = unsafe { rsg_point_tree_new(ptr::null(), 3, &mut tree) };
        assert_eq!(status, RsgStatus::RSG_ERR_NULL_ARGUMENT);
        assert!(tree.is_null());
    }

    #[test]
    fn out_of_range_latitude_is_reported() {
        // A swapped lat/lon (139.6917 in the latitude slot) is out of range.
        let coords = [35.6895_f64, 139.6917];
        let mut tree: *mut RsgPointTree = ptr::null_mut();
        let status = unsafe { rsg_point_tree_new(coords.as_ptr(), 1, &mut tree) };
        assert_eq!(status, RsgStatus::RSG_ERR_LAT_OUT_OF_RANGE);
        assert!(tree.is_null());
    }

    #[test]
    fn non_finite_coordinate_is_reported() {
        let coords = [f64::NAN, 0.0];
        let mut tree: *mut RsgPointTree = ptr::null_mut();
        let status = unsafe { rsg_point_tree_new(coords.as_ptr(), 1, &mut tree) };
        assert_eq!(status, RsgStatus::RSG_ERR_NOT_FINITE);
    }

    #[test]
    fn free_null_is_a_no_op() {
        unsafe { rsg_point_tree_free(ptr::null_mut()) };
    }

    #[test]
    fn size_rejects_null_arguments() {
        let mut size = 0usize;
        assert_eq!(
            unsafe { rsg_point_tree_size(ptr::null(), &mut size) },
            RsgStatus::RSG_ERR_NULL_ARGUMENT
        );
    }

    // --- linestring construction ---

    #[test]
    fn line_tree_builds_two_lines() {
        // Two polylines: the first with three vertices, the second with two.
        let coords = [0.0_f64, 0.0, 1.0, 1.0, 2.0, 0.0, 10.0, 10.0, 11.0, 11.0];
        let offsets = [0usize, 3, 5];
        let mut tree: *mut RsgLineTree = ptr::null_mut();
        let status =
            unsafe { rsg_line_tree_new(coords.as_ptr(), 5, offsets.as_ptr(), 2, &mut tree) };
        assert_eq!(status, RsgStatus::RSG_OK);
        let mut size = 0usize;
        assert_eq!(
            unsafe { rsg_line_tree_size(tree, &mut size) },
            RsgStatus::RSG_OK
        );
        assert_eq!(size, 2);
        unsafe { rsg_line_tree_free(tree) };
    }

    #[test]
    fn line_tree_rejects_non_monotonic_offsets() {
        let coords = [0.0_f64, 0.0, 1.0, 1.0, 2.0, 0.0];
        let offsets = [0usize, 2, 1]; // decreasing
        let mut tree: *mut RsgLineTree = ptr::null_mut();
        let status =
            unsafe { rsg_line_tree_new(coords.as_ptr(), 3, offsets.as_ptr(), 2, &mut tree) };
        assert_eq!(status, RsgStatus::RSG_ERR_INVALID_OFFSETS);
        assert!(tree.is_null());
    }

    #[test]
    fn line_tree_rejects_offsets_not_spanning_input() {
        let coords = [0.0_f64, 0.0, 1.0, 1.0];
        let offsets = [0usize, 1]; // final entry 1 != 2 vertices
        let mut tree: *mut RsgLineTree = ptr::null_mut();
        let status =
            unsafe { rsg_line_tree_new(coords.as_ptr(), 2, offsets.as_ptr(), 1, &mut tree) };
        assert_eq!(status, RsgStatus::RSG_ERR_INVALID_OFFSETS);
    }

    #[test]
    fn line_tree_surfaces_geometry_error() {
        // A single-vertex line is too few points.
        let coords = [0.0_f64, 0.0];
        let offsets = [0usize, 1];
        let mut tree: *mut RsgLineTree = ptr::null_mut();
        let status =
            unsafe { rsg_line_tree_new(coords.as_ptr(), 1, offsets.as_ptr(), 1, &mut tree) };
        assert_eq!(status, RsgStatus::RSG_ERR_TOO_FEW_POINTS);
    }

    #[test]
    fn line_tree_empty_is_allowed() {
        let mut tree: *mut RsgLineTree = ptr::null_mut();
        let status = unsafe { rsg_line_tree_new(ptr::null(), 0, ptr::null(), 0, &mut tree) };
        assert_eq!(status, RsgStatus::RSG_OK);
        let mut size = 1usize;
        assert_eq!(
            unsafe { rsg_line_tree_size(tree, &mut size) },
            RsgStatus::RSG_OK
        );
        assert_eq!(size, 0);
        unsafe { rsg_line_tree_free(tree) };
    }

    // --- polygon construction ---

    /// A closed lon/lat square (counter-clockwise) plus a second smaller square: two
    /// polygons, one ring each.
    #[test]
    fn polygon_tree_builds_two_polygons() {
        let coords = [
            0.0_f64, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0, 0.0, 0.0, // polygon 0 exterior
            20.0, 20.0, 24.0, 20.0, 24.0, 24.0, 20.0, 24.0, 20.0, 20.0, // polygon 1 exterior
        ];
        let ring_offsets = [0usize, 5, 10];
        let polygon_offsets = [0usize, 1, 2];
        let mut tree: *mut RsgPolygonTree = ptr::null_mut();
        let status = unsafe {
            rsg_polygon_tree_new(
                coords.as_ptr(),
                10,
                ring_offsets.as_ptr(),
                2,
                polygon_offsets.as_ptr(),
                2,
                &mut tree,
            )
        };
        assert_eq!(status, RsgStatus::RSG_OK);
        let mut size = 0usize;
        assert_eq!(
            unsafe { rsg_polygon_tree_size(tree, &mut size) },
            RsgStatus::RSG_OK
        );
        assert_eq!(size, 2);
        unsafe { rsg_polygon_tree_free(tree) };
    }

    /// A polygon with an exterior ring and one hole (interior ring).
    #[test]
    fn polygon_tree_builds_polygon_with_hole() {
        let coords = [
            0.0_f64, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0, 0.0, 0.0, // exterior
            3.0, 3.0, 3.0, 7.0, 7.0, 7.0, 7.0, 3.0, 3.0, 3.0, // hole (clockwise)
        ];
        let ring_offsets = [0usize, 5, 10];
        let polygon_offsets = [0usize, 2];
        let mut tree: *mut RsgPolygonTree = ptr::null_mut();
        let status = unsafe {
            rsg_polygon_tree_new(
                coords.as_ptr(),
                10,
                ring_offsets.as_ptr(),
                2,
                polygon_offsets.as_ptr(),
                1,
                &mut tree,
            )
        };
        assert_eq!(status, RsgStatus::RSG_OK);
        let mut size = 0usize;
        unsafe { rsg_polygon_tree_size(tree, &mut size) };
        assert_eq!(size, 1);
        unsafe { rsg_polygon_tree_free(tree) };
    }

    #[test]
    fn polygon_tree_rejects_unclosed_ring() {
        // Exterior ring first vertex != last.
        let coords = [0.0_f64, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0];
        let ring_offsets = [0usize, 4];
        let polygon_offsets = [0usize, 1];
        let mut tree: *mut RsgPolygonTree = ptr::null_mut();
        let status = unsafe {
            rsg_polygon_tree_new(
                coords.as_ptr(),
                4,
                ring_offsets.as_ptr(),
                1,
                polygon_offsets.as_ptr(),
                1,
                &mut tree,
            )
        };
        assert_eq!(status, RsgStatus::RSG_ERR_RING_NOT_CLOSED);
    }

    #[test]
    fn polygon_tree_rejects_polygon_with_no_rings() {
        // polygon_offsets [0, 0] gives the single polygon zero rings.
        let ring_offsets = [0usize];
        let polygon_offsets = [0usize, 0];
        let mut tree: *mut RsgPolygonTree = ptr::null_mut();
        let status = unsafe {
            rsg_polygon_tree_new(
                ptr::null(),
                0,
                ring_offsets.as_ptr(),
                0,
                polygon_offsets.as_ptr(),
                1,
                &mut tree,
            )
        };
        assert_eq!(status, RsgStatus::RSG_ERR_INVALID_OFFSETS);
    }

    #[test]
    fn polygon_tree_rejects_bad_ring_offsets() {
        let coords = [0.0_f64, 0.0, 10.0, 0.0, 10.0, 10.0, 0.0, 10.0, 0.0, 0.0];
        let ring_offsets = [0usize, 4]; // final 4 != 5 vertices
        let polygon_offsets = [0usize, 1];
        let mut tree: *mut RsgPolygonTree = ptr::null_mut();
        let status = unsafe {
            rsg_polygon_tree_new(
                coords.as_ptr(),
                5,
                ring_offsets.as_ptr(),
                1,
                polygon_offsets.as_ptr(),
                1,
                &mut tree,
            )
        };
        assert_eq!(status, RsgStatus::RSG_ERR_INVALID_OFFSETS);
    }

    #[test]
    fn line_and_polygon_free_null_is_a_no_op() {
        unsafe { rsg_line_tree_free(ptr::null_mut()) };
        unsafe { rsg_polygon_tree_free(ptr::null_mut()) };
    }
}
