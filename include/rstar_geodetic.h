/*
 * rstar_geodetic C API.
 *
 * Generated from the Rust `ffi` module by cbindgen; do not edit by hand. Regenerate with
 * `just ffi-header`. The CI drift check (`just ffi-check-header`) fails if this file is out
 * of step with the source.
 *
 * Ownership and lifetimes:
 *   - A tree is an opaque handle from a `*_tree_new` constructor, released by the matching
 *     `*_tree_free`. Passing NULL to a `*_free` function is a no-op.
 *   - A handle is immutable after construction and is safe to query from several threads.
 *   - Input arrays (coordinates, CSR offsets) are borrowed for the duration of a call
 *     only; the caller keeps ownership and may free them once the call returns.
 *   - A `*_within_distance` or `rsg_point_tree_in_rectangle` call allocates a result
 *     buffer that the caller must release with `rsg_neighbors_free` or `rsg_indices_free`.
 *     An empty result is reported as a NULL buffer with length zero.
 *   - Coordinates are longitude first, latitude second, in degrees. Point arrays are
 *     interleaved [lon0, lat0, lon1, lat1, ...]; linestrings and polygons use the same
 *     interleaving with CSR offset arrays (the GeoArrow layout).
 *   - Every fallible function returns an RsgStatus; RSG_OK (zero) is success. A caught
 *     panic is reported as RSG_ERR_INTERNAL_PANIC rather than unwinding across the ABI.
 */


#ifndef RSTAR_GEODETIC_H
#define RSTAR_GEODETIC_H

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

// Earth radius in metres. Matches `geo::MEAN_EARTH_RADIUS` (GRS80 mean radius).
#define EARTH_RADIUS_METRES 6371008.8

// The outcome of a C API call. `RSG_OK` is zero; all other values are errors.
//
// `#[repr(C)]` fixes the discriminants, so the values are a stable part of the ABI. The
// variant names are the C constant names verbatim (hence the non-CamelCase spelling).
typedef enum {
  // The call succeeded and the out-parameters are populated.
  RSG_OK = 0,
  // A longitude was outside `[-180, 180]`.
  RSG_ERR_LON_OUT_OF_RANGE,
  // A latitude was outside `[-90, 90]`.
  RSG_ERR_LAT_OUT_OF_RANGE,
  // A coordinate was NaN or infinite.
  RSG_ERR_NOT_FINITE,
  // A geometry had fewer vertices than its structure requires.
  RSG_ERR_TOO_FEW_POINTS,
  // An edge spanned 180 degrees or more, so its shorter great-circle arc is undefined.
  RSG_ERR_EDGE_SPANS_HALF_CIRCLE,
  // A polygon ring's first and last vertices differ (a ring must be explicitly closed).
  RSG_ERR_RING_NOT_CLOSED,
  // A required pointer argument was null.
  RSG_ERR_NULL_ARGUMENT,
  // A CSR offset array was not non-decreasing, or its final entry did not equal the
  // count of the elements it partitions.
  RSG_ERR_INVALID_OFFSETS,
  // A panic was caught at the boundary. The handle or buffer is left in a safe state,
  // but the operation did not complete.
  RSG_ERR_INTERNAL_PANIC,
} RsgStatus;

// An opaque handle to a linestring tree. Construct with [`rsg_line_tree_new`] and release
// with [`rsg_line_tree_free`].
typedef struct RsgLineTree RsgLineTree;

// An opaque handle to a point tree. Construct with [`rsg_point_tree_new`] and release with
// [`rsg_point_tree_free`].
typedef struct RsgPointTree RsgPointTree;

// An opaque handle to a polygon tree. Construct with [`rsg_polygon_tree_new`] and release
// with [`rsg_polygon_tree_free`].
typedef struct RsgPolygonTree RsgPolygonTree;

// A query result: the caller's input position and the great-circle distance in metres
// from the query point to the nearest point of that geometry.
typedef struct {
  // The input position of the geometry (its index at construction).
  size_t index;
  // Great-circle distance from the query point to the geometry, in metres.
  double distance_metres;
} RsgNeighbor;





#ifdef __cplusplus
extern "C" {
#endif // __cplusplus

// Builds a point tree from `num_points` interleaved `[lon, lat]` degree pairs.
//
// On success writes a fresh handle to `*out_tree` and returns `RSG_OK`; the caller owns
// the handle and must release it with [`rsg_point_tree_free`]. `coords` may be null only
// when `num_points` is zero.
//
// # Safety
//
// `coords` must point to at least `2 * num_points` readable `f64`s (or be null when
// `num_points` is zero), and `out_tree` must be a valid, writable pointer.
RsgStatus rsg_point_tree_new(const double *coords, size_t num_points, RsgPointTree **out_tree);

// Releases a point tree. Passing null is a no-op.
//
// # Safety
//
// `tree` must be a handle returned by [`rsg_point_tree_new`] that has not already been
// freed, or null.
void rsg_point_tree_free(RsgPointTree *tree);

// Writes the number of points in `tree` to `*out_size`.
//
// # Safety
//
// `tree` must be a valid handle and `out_size` a valid, writable pointer.
RsgStatus rsg_point_tree_size(const RsgPointTree *tree, size_t *out_size);

// Builds a linestring tree from `num_vertices` interleaved `[lon, lat]` degree pairs
// partitioned into `num_lines` polylines by `line_offsets` (a `num_lines + 1` entry CSR
// array; the GeoArrow layout).
//
// On success writes a fresh handle to `*out_tree`; the caller owns it and must release it
// with [`rsg_line_tree_free`]. Any pointer may be null only when its count is zero.
//
// # Safety
//
// `coords` must have `2 * num_vertices` readable `f64`s and `line_offsets`
// `num_lines + 1` readable `usize`s (or each may be null when its count is zero), and
// `out_tree` must be a valid, writable pointer.
RsgStatus rsg_line_tree_new(const double *coords,
                            size_t num_vertices,
                            const size_t *line_offsets,
                            size_t num_lines,
                            RsgLineTree **out_tree);

// Releases a linestring tree. Passing null is a no-op.
//
// # Safety
//
// `tree` must be a handle from [`rsg_line_tree_new`] that has not been freed, or null.
void rsg_line_tree_free(RsgLineTree *tree);

// Writes the number of linestrings in `tree` to `*out_size`.
//
// # Safety
//
// `tree` must be a valid handle and `out_size` a valid, writable pointer.
RsgStatus rsg_line_tree_size(const RsgLineTree *tree, size_t *out_size);

// Builds a polygon tree from `num_vertices` interleaved `[lon, lat]` degree pairs, a
// two-level CSR layout: `ring_offsets` (`num_rings + 1` entries) partitions the vertices
// into rings, and `polygon_offsets` (`num_polygons + 1` entries) partitions the rings
// into polygons. Within each polygon the first ring is the exterior and the rest are
// holes (the GeoArrow layout).
//
// On success writes a fresh handle to `*out_tree`; the caller owns it and must release it
// with [`rsg_polygon_tree_free`]. A polygon with no rings is rejected as
// `RSG_ERR_INVALID_OFFSETS`.
//
// # Safety
//
// `coords` must have `2 * num_vertices` readable `f64`s, `ring_offsets`
// `num_rings + 1` and `polygon_offsets` `num_polygons + 1` readable `usize`s (or each
// null when its count is zero), and `out_tree` must be a valid, writable pointer.
RsgStatus rsg_polygon_tree_new(const double *coords,
                               size_t num_vertices,
                               const size_t *ring_offsets,
                               size_t num_rings,
                               const size_t *polygon_offsets,
                               size_t num_polygons,
                               RsgPolygonTree **out_tree);

// Releases a polygon tree. Passing null is a no-op.
//
// # Safety
//
// `tree` must be a handle from [`rsg_polygon_tree_new`] that has not been freed, or null.
void rsg_polygon_tree_free(RsgPolygonTree *tree);

// Writes the number of polygons in `tree` to `*out_size`.
//
// # Safety
//
// `tree` must be a valid handle and `out_size` a valid, writable pointer.
RsgStatus rsg_polygon_tree_size(const RsgPolygonTree *tree, size_t *out_size);

// Returns a static, NUL-terminated description of `status`. The pointer is valid for the
// lifetime of the process and must not be freed.
const char *rsg_status_message(RsgStatus status);

// Writes the nearest point to `(lon, lat)` to `*out_neighbor`, setting `*out_found`. On
// an empty tree `*out_found` is false.
//
// # Safety
//
// `tree` must be a valid handle; `out_neighbor` and `out_found` valid, writable pointers.
RsgStatus rsg_point_tree_nearest_neighbor(const RsgPointTree *tree,
                                          double lon,
                                          double lat,
                                          RsgNeighbor *out_neighbor,
                                          bool *out_found);

// Writes every point within `radius_metres` of `(lon, lat)` to a fresh buffer at
// `*out_items` (length `*out_len`), which the caller frees with [`rsg_neighbors_free`].
//
// # Safety
//
// `tree` must be a valid handle; `out_items` and `out_len` valid, writable pointers.
RsgStatus rsg_point_tree_within_distance(const RsgPointTree *tree,
                                         double lon,
                                         double lat,
                                         double radius_metres,
                                         RsgNeighbor **out_items,
                                         size_t *out_len);

// Writes the input positions of every point inside the longitude/latitude rectangle
// `[lower, upper]` to a fresh buffer at `*out_indices` (length `*out_len`), freed with
// [`rsg_indices_free`]. `lower_lon > upper_lon` denotes a window crossing the
// antimeridian (RFC 7946); `lower_lat <= upper_lat` is required.
//
// # Safety
//
// `tree` must be a valid handle; `out_indices` and `out_len` valid, writable pointers.
RsgStatus rsg_point_tree_in_rectangle(const RsgPointTree *tree,
                                      double lower_lon,
                                      double lower_lat,
                                      double upper_lon,
                                      double upper_lat,
                                      size_t **out_indices,
                                      size_t *out_len);

// Nearest linestring to `(lon, lat)`; see [`rsg_point_tree_nearest_neighbor`].
//
// # Safety
//
// `tree` must be a valid handle; `out_neighbor` and `out_found` valid, writable pointers.
RsgStatus rsg_line_tree_nearest_neighbor(const RsgLineTree *tree,
                                         double lon,
                                         double lat,
                                         RsgNeighbor *out_neighbor,
                                         bool *out_found);

// Every linestring within `radius_metres` of `(lon, lat)`; see
// [`rsg_point_tree_within_distance`].
//
// # Safety
//
// `tree` must be a valid handle; `out_items` and `out_len` valid, writable pointers.
RsgStatus rsg_line_tree_within_distance(const RsgLineTree *tree,
                                        double lon,
                                        double lat,
                                        double radius_metres,
                                        RsgNeighbor **out_items,
                                        size_t *out_len);

// Nearest polygon to `(lon, lat)` (zero distance if the point is inside one); see
// [`rsg_point_tree_nearest_neighbor`].
//
// # Safety
//
// `tree` must be a valid handle; `out_neighbor` and `out_found` valid, writable pointers.
RsgStatus rsg_polygon_tree_nearest_neighbor(const RsgPolygonTree *tree,
                                            double lon,
                                            double lat,
                                            RsgNeighbor *out_neighbor,
                                            bool *out_found);

// Every polygon within `radius_metres` of `(lon, lat)`; see
// [`rsg_point_tree_within_distance`].
//
// # Safety
//
// `tree` must be a valid handle; `out_items` and `out_len` valid, writable pointers.
RsgStatus rsg_polygon_tree_within_distance(const RsgPolygonTree *tree,
                                           double lon,
                                           double lat,
                                           double radius_metres,
                                           RsgNeighbor **out_items,
                                           size_t *out_len);

// Releases a buffer returned by a `*_within_distance` query. Null is a no-op.
//
// # Safety
//
// `items`/`len` must be a buffer and length returned together by a `*_within_distance`
// call and not already freed, or `items` null.
void rsg_neighbors_free(RsgNeighbor *items, size_t len);

// Releases a buffer returned by [`rsg_point_tree_in_rectangle`]. Null is a no-op.
//
// # Safety
//
// `items`/`len` must be a buffer and length returned together by the rectangle query and
// not already freed, or `items` null.
void rsg_indices_free(size_t *items, size_t len);

#if defined(RSG_HAVE_WGS84)
// Writes the nearest point to `(lon, lat)` by WGS84-ellipsoid geodesic distance to
// `*out_neighbor`, setting `*out_found`. The distance is exact geodesic metres, not the
// spherical approximation. On an empty tree `*out_found` is false.
//
// # Safety
//
// `tree` must be a valid handle; `out_neighbor` and `out_found` valid, writable pointers.
RsgStatus rsg_point_tree_nearest_neighbor_wgs84(const RsgPointTree *tree,
                                                double lon,
                                                double lat,
                                                RsgNeighbor *out_neighbor,
                                                bool *out_found);
#endif

#if defined(RSG_HAVE_WGS84)
// Writes every point within `radius_metres` WGS84-ellipsoid geodesic metres of
// `(lon, lat)` to a fresh buffer at `*out_items` (length `*out_len`), freed with
// [`rsg_neighbors_free`]. Each distance is exact geodesic metres.
//
// # Safety
//
// `tree` must be a valid handle; `out_items` and `out_len` valid, writable pointers.
RsgStatus rsg_point_tree_within_distance_wgs84(const RsgPointTree *tree,
                                               double lon,
                                               double lat,
                                               double radius_metres,
                                               RsgNeighbor **out_items,
                                               size_t *out_len);
#endif

#ifdef __cplusplus
}  // extern "C"
#endif  // __cplusplus

#endif  /* RSTAR_GEODETIC_H */
