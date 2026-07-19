# rstar_geodetic

A geodetic (longitude/latitude) R-tree built on [`rstar`](https://crates.io/crates/rstar),
using great-circle distance for nearest-neighbour and radius queries on point, linestring, and polygon geometries.

Each `(lon, lat)` is mapped to a unit vector on the sphere (an
[n-vector](https://en.wikipedia.org/wiki/N-vector)) and indexed in a stock
`rstar::RTree`. The embedding is continuous over the whole sphere, so the ±180°
antimeridian and the poles are ordinary interior points: no wrapping, duplication, or
special cases are required, and nearest-neighbour ordering matches great-circle distance.

`GeodeticRTree` indexes `GeodeticPoint`, `GeodeticLineString`, and `GeodeticPolygon`
leaves. Coordinates are longitude first, latitude second (the `geo`/OGC convention),
taken in degrees; distances are returned in metres.

## Note
This library may become part of rstar in the future. There are currently no plans to publish a crate.

## Examples

### Points

```rust
use rstar_geodetic::{GeodeticRTree, GeodeticCoord, GeodeticPoint};

let tree = GeodeticRTree::bulk_load(vec![
    GeodeticPoint::new(179.0, -17.0),  // 179°E
    GeodeticPoint::new(-175.0, -21.0), // 175°W
    GeodeticPoint::new(-77.0, -12.0),  // distant
]);

let query = GeodeticCoord { lon: -176.0, lat: -21.0 };
let (nn, distance_m) = tree.nearest_neighbor_with_distance(query).unwrap();

assert_eq!((nn.coord().lon, nn.coord().lat), (-175.0, -21.0));
assert!(distance_m < 200_000.0);
```

### Linestrings

A `GeodeticLineString` leaf measures the minimum great-circle distance to a polyline,
which may fall on the interior of an edge, not only at a vertex. Each edge must span
less than 180 degrees; densify longer edges first.

```rust
use rstar_geodetic::{GeodeticRTree, GeodeticCoord, GeodeticLineString};

let near = GeodeticLineString::try_from_lonlat([(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)]).unwrap();
let far = GeodeticLineString::try_from_lonlat([(10.0, 10.0), (11.0, 11.0)]).unwrap();
let tree = GeodeticRTree::bulk_load(vec![near, far]);

// The nearest linestring to the query, with the great-circle distance in metres.
let (nearest, metres) = tree
    .nearest_neighbor_with_distance(GeodeticCoord { lon: 1.0, lat: 0.5 })
    .unwrap();
assert_eq!(nearest.coords()[0], GeodeticCoord { lon: 0.0, lat: 0.0 });
assert!(metres < 100_000.0); // within ~100 km of the nearer linestring
```

### Polygons

A `GeodeticPolygon` leaf is a filled spherical polygon: a query strictly inside is at
distance zero, otherwise the distance is to the nearest boundary point. Ring orientation
follows OGC/GeoJSON (exterior counter-clockwise as seen from outside, holes clockwise) and
is respected as given, not auto-corrected, so polygons larger than a hemisphere are
expressible.

```rust
use rstar_geodetic::{GeodeticRTree, GeodeticCoord, GeodeticPolygon, GeodeticRing};

// A lon/lat square, counter-clockwise as seen from outside (interior to the left).
let ring = GeodeticRing::try_from_lonlat([
    (0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0), (0.0, 0.0),
])
.unwrap();
let poly = GeodeticPolygon::try_new(ring, Vec::new()).unwrap();
let tree = GeodeticRTree::bulk_load(vec![poly]);

// A query inside the polygon is at distance zero; one outside is positive.
let (_, inside_m) = tree
    .nearest_neighbor_with_distance(GeodeticCoord { lon: 5.0, lat: 5.0 })
    .unwrap();
assert_eq!(inside_m, 0.0);
let (_, outside_m) = tree
    .nearest_neighbor_with_distance(GeodeticCoord { lon: -5.0, lat: 5.0 })
    .unwrap();
assert!(outside_m > 0.0);
```

## geo-types interoperability

The default `geo-types` feature adds `From`/`TryFrom` conversions between the geodetic
geometries and [`geo-types`](https://crates.io/crates/geo-types). Coordinate order
matches on both sides (`x = lon`, `y = lat`). Conversions *into* the validated geometries
are fallible (they range-check coordinates and enforce structural preconditions, yielding
a `GeodeticError`); conversions back *out* are infallible. Disable with
`default-features = false` to drop the dependency.

```rust
use geo_types::{Coord, LineString};
use rstar_geodetic::{GeodeticLineString, GeodeticPoint};

// geo-types -> geodetic is validating (TryFrom): a GeodeticError on out-of-range or
// structurally invalid input.
let line: GeodeticLineString =
    LineString::new(vec![Coord { x: 0.0, y: 0.0 }, Coord { x: 1.0, y: 1.0 }])
        .try_into()
        .unwrap();
assert_eq!(line.coords().len(), 2);

// geodetic -> geo-types is infallible.
let geo_point: geo_types::Point = GeodeticPoint::new(2.5, 48.8).into();
assert_eq!((geo_point.x(), geo_point.y()), (2.5, 48.8));
```

A geo-types multi-geometry builds an index directly: `TryFrom<MultiPoint>`,
`TryFrom<MultiLineString>`, and `TryFrom<MultiPolygon>` for the matching `GeodeticRTree`.

```rust
use geo_types::{Coord, LineString, MultiLineString};
use rstar_geodetic::{GeodeticLineString, GeodeticRTree};

let lines = MultiLineString(vec![
    LineString::new(vec![Coord { x: 0.0, y: 0.0 }, Coord { x: 1.0, y: 1.0 }]),
    LineString::new(vec![Coord { x: 5.0, y: 5.0 }, Coord { x: 6.0, y: 4.0 }]),
]);
let tree: GeodeticRTree<GeodeticLineString> = lines.try_into().unwrap();
assert_eq!(tree.size(), 2);
```

The orphan rule prevents converting straight to a `Vec<GeodeticPolygon>` (the `Vec` is a
foreign type), so the collection conversions target `GeodeticRTree`. For a plain `Vec`, or
to convert back to `geo-types`, map the element conversions over the geometry's iterator:

```rust
use geo_types::{Coord, LineString, MultiPolygon, Polygon};
use rstar_geodetic::GeodeticPolygon;

let mp = MultiPolygon(vec![Polygon::new(
    LineString::new(vec![
        Coord { x: 0.0, y: 0.0 }, Coord { x: 4.0, y: 0.0 },
        Coord { x: 4.0, y: 4.0 }, Coord { x: 0.0, y: 4.0 },
    ]),
    vec![],
)]);

// geo-types -> Vec<GeodeticPolygon> (validating)
let geodetic: Vec<GeodeticPolygon> = mp
    .into_iter()
    .map(GeodeticPolygon::try_from)
    .collect::<Result<_, _>>()
    .unwrap();

// Vec<GeodeticPolygon> -> geo-types MultiPolygon (infallible)
let multi: MultiPolygon = geodetic.into_iter().map(Polygon::from).collect();
assert_eq!(multi.0.len(), 1);
```

## Earth model and the `wgs84` feature

The base index uses a spherical Earth (the GRS80 mean radius, 6 371 008.8 m); against
an ellipsoid the error is at most about 0.5%. The base crate is `no_std`.

Enable the optional `wgs84` feature for exact ellipsoidal distances on the point tree
(Karney's geodesic, via `geographiclib-rs`). It adds the `_on_ellipsoid` /`_wgs84`
query methods and the standalone `geodesic_distance`. That feature requires `std`.

```toml
[dependencies]
rstar_geodetic = { version = "0.1", features = ["wgs84"] }
```

## C API

The optional `ffi` feature exposes a C-ABI surface over all three tree types: bulk-load
constructors, nearest-neighbour and within-distance queries, a longitude/latitude rectangle
query on point trees, and the WGS84 geodesic variants on point trees. It pulls in `std`, so
it is off by default and the base crate stays `no_std`. Build the C library with the
committed `justfile`:

```sh
just ffi-build      # cdylib + staticlib in target/release, with ffi,wgs84
just ffi-smoke      # compile and run examples/c/smoke.c against the cdylib
```

The header at [`include/rstar_geodetic.h`](include/rstar_geodetic.h) is generated with
[cbindgen](https://github.com/mozilla/cbindgen) and committed; `just ffi-check-header`
regenerates it and fails on drift (also run in CI). The WGS84 entry points are guarded by
`#if defined(RSG_HAVE_WGS84)`; define `RSG_HAVE_WGS84` when the library is built with the
`wgs84` feature.

Coordinates are longitude first, latitude second, in degrees, interleaved
`[lon0, lat0, ...]`; linestrings and polygons add CSR offset arrays (the GeoArrow layout).
Every fallible function returns an `RsgStatus` (`RSG_OK` is zero); queries write their
result through out-parameters. Opaque handles are freed with the matching `*_tree_free`, and
result buffers with `rsg_neighbors_free` or `rsg_indices_free`. The header preamble states
the full ownership rules.

```c
double coords[] = { -0.1278, 51.5074, 2.3522, 48.8566 }; /* London, Paris */
RsgPointTree *tree = NULL;
if (rsg_point_tree_new(coords, 2, &tree) != RSG_OK) { /* handle error */ }

RsgNeighbor nearest;
bool found = false;
rsg_point_tree_nearest_neighbor(tree, 4.9041, 52.3676, &nearest, &found);
/* nearest.index is the input position; nearest.distance_metres is great-circle metres. */

rsg_point_tree_free(tree);
```

## Python bindings

A PyO3 package in [`python/`](python/) exposes the three trees to Python with
`__geo_interface__`-compatible classes, so it interoperates with shapely and geopandas.
Queries return integer input positions (the shapely `STRtree` convention), and the WGS84
geodesic queries are compiled in. See [`python/README.md`](python/README.md) for the
install and usage instructions.

## Testing

Property tests use [Hegel](https://crates.io/crates/hegeltest). The arc oracle is
anchored to an external reference computed with `mpmath` and `s2sphere`; see
`tests/reference/arc_distance_reference.py`. The multi-hour soak search
`soak_arc_distance_vs_oracle` in `tests/geodetic_arc_property.rs` is `#[ignore]`d and
run locally:

```text
cargo test --release --features wgs84 -- --ignored --nocapture soak_arc_distance_vs_oracle
```

## Licence

Licensed under either of Apache License, Version 2.0 or MIT licence at your option.
