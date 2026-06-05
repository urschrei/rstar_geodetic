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
