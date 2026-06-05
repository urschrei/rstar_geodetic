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

## Example

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
