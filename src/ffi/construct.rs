//! Tree constructors, destructors, and size accessors.
//!
//! The point tree keeps the concrete [`GeodeticRTree<GeodeticPoint>`] so it retains the
//! crate's indexed rectangle query and its tuned wgs84 refine. Input positions are
//! recovered from query results through a coordinate-keyed multimap (see
//! [`super::coord_key`]).

use std::collections::HashMap;

use crate::{GeodeticPoint, GeodeticRTree};

use super::error::RsgStatus;
use super::{coord_key, ffi_guard};

/// Input positions keyed by coordinate bits: the recovery map from a query result's
/// coordinate back to the original input positions at that coordinate.
pub(crate) type CoordIndex = HashMap<(u64, u64), Vec<usize>>;

/// An opaque handle to a point tree. Construct with [`rsg_point_tree_new`] and release with
/// [`rsg_point_tree_free`].
pub struct RsgPointTree {
    pub(crate) tree: GeodeticRTree<GeodeticPoint>,
    // Original input positions keyed by coordinate bits, for STRtree-style index recovery.
    // Read by the query module (a later step).
    #[allow(dead_code)]
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
}
