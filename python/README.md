# rtree-geodetic

Python bindings for [`rstar_geodetic`](https://github.com/urschrei/rstar_geodetic), a
geodetic (longitude/latitude) R-tree with great-circle and WGS84 nearest-neighbour and
radius queries over point, linestring, and polygon geometries.

Each `(lon, lat)` is mapped to a unit vector on the sphere and indexed in an R-tree, so the
antimeridian and the poles are ordinary interior points: no wrapping or special cases.
Coordinates are longitude first, latitude second, in degrees; distances are returned in
metres. Queries return integer input positions (the shapely `STRtree` convention).

## Install

```sh
pip install rtree-geodetic
```

Wheels are built for CPython 3.10 and newer (a single abi3 wheel per platform).

## Quick start

A tree is built from any iterable of geometry-likes: `(lon, lat)` pairs, GeoJSON mappings,
objects exposing `__geo_interface__` (shapely geometries), or a single object whose
`__geo_interface__` is a collection (a geopandas `GeoSeries`, a shapely `Multi*`, a
`GeometryCollection`, or a `FeatureCollection`).

```python
from rtree_geodetic import GeodeticPointTree

tree = GeodeticPointTree([
    (-0.1278, 51.5074),  # 0 London
    (2.3522, 48.8566),   # 1 Paris
    (13.4050, 52.5200),  # 2 Berlin
])

tree.nearest((4.9041, 52.3676))                 # 0 (London)
tree.nearest_with_distance((4.9041, 52.3676))   # (0, 357888.0)  metres
tree.within_distance((4.9041, 52.3676), 400_000.0)                      # [0, 1]
tree.within_distance((4.9041, 52.3676), 400_000.0, return_distance=True)
# [(0, 357888.0), (1, 430123.5)]

tree.geometry(0).__geo_interface__
# {'type': 'Point', 'coordinates': (-0.1278, 51.5074)}
```

Point trees also offer a longitude/latitude rectangle query. Order the corners west-then-east
to cross the antimeridian (the GeoJSON RFC 7946 convention):

```python
tree = GeodeticPointTree([(179.0, 0.0), (-178.0, 1.0), (0.0, 0.0)])
tree.in_rectangle((170.0, -10.0), (-170.0, 10.0))   # [0, 1] (the two near the seam)
```

Linestring and polygon trees measure the minimum great-circle distance to the geometry
(zero for a query inside a polygon):

```python
from rtree_geodetic import GeodeticLineStringTree, GeodeticPolygonTree

lines = GeodeticLineStringTree([[(0, 0), (1, 1), (2, 0)], [(10, 10), (11, 11)]])
lines.nearest((1.0, 0.5))                       # 0

square = [[(0, 0), (10, 0), (10, 10), (0, 10), (0, 0)]]
polys = GeodeticPolygonTree([square])
polys.nearest_with_distance((5.0, 5.0))         # (0, 0.0) -- inside
```

## shapely and geopandas

The geometry views expose `__geo_interface__`, so `shapely.geometry.shape` reconstructs
them, and trees accept shapely geometries and geopandas `GeoSeries` directly:

```python
import geopandas as gpd
from shapely.geometry import Point, shape
from rtree_geodetic import GeodeticPointTree

series = gpd.GeoSeries([Point(-0.1278, 51.5074), Point(2.3522, 48.8566)])
tree = GeodeticPointTree(series)

index = tree.nearest(Point(2.0, 49.0))          # 1 (Paris)
geom = shape(tree.geometry(index).__geo_interface__)   # a shapely Point
```

## Distance semantics

Distances are great-circle **metres**. By default they use a spherical Earth (the GRS80
mean radius, 6 371 008.8 m); against an ellipsoid the error is at most about 0.5%.

`GeodeticPointTree` also offers exact WGS84-ellipsoid geodesic distances (Karney's method)
through `nearest_wgs84`, `nearest_with_distance_wgs84`, and `within_distance_wgs84`:

```python
tree.nearest_with_distance_wgs84((4.9041, 52.3676))   # (0, 358968.7) geodesic metres
```

Invalid coordinates or geometry (an out-of-range longitude or latitude, a non-finite value,
too few vertices, an edge spanning half the sphere, or an unclosed ring) raise
`GeodeticError`, a subclass of `ValueError`.

## Performance

Measured with [`benchmarks/bench.py`](benchmarks/bench.py) (seeded synthetic data;
run it with `uv run --group bench python benchmarks/bench.py`) on an Apple M2 Pro,
Python 3.14, shapely 2.1.2 (GEOS STRtree), rtree 1.4.1 (libspatialindex), release
build. Datasets: one million points distributed uniformly on the sphere, and one
hundred thousand small linestrings and polygons, queried per call from Python.

One million points, 10,000 queries:

| Operation | rtree-geodetic | shapely STRtree | Rtree |
|---|---|---|---|
| Build | 0.79 s | 0.23 s | 1.63 s |
| Nearest neighbour, per call | 3.5 us | 7.9 us | 21.4 us |
| Nearest neighbour, WGS84 geodesic | 2.6 us | not offered | not offered |
| Within 50 km (radius query, ~16 hits) | 10.5 us | 10.3 us* | not offered |

100,000 extent geometries, 2,000 queries:

| Operation | rtree-geodetic | shapely STRtree |
|---|---|---|
| Linestring build | 0.66 s | 0.02 s |
| Linestring nearest, per call | 4.4 us | 11.1 us |
| Polygon build | 2.44 s | 0.02 s |
| Polygon nearest, per call | 7.8 us | 12.6 us |

> [!NOTE] 
> First, the planar libraries answer a
different question: they index raw lon/lat degrees, so their distances are in
degrees and their answers degrade as meridians converge. On the uniform global
dataset above, the `STRtree` planar nearest neighbour differs from the true geodesic
nearest neighbour for 10.9% of queries; `rtree-geodetic` returns the geodesically
correct answer with the distance already in metres (this is the correctness you
would otherwise get from the PostGIS `geography` type, without a database round
trip). The starred `STRtree` radius query uses an equator-equivalent degree radius,
which returns the wrong set away from the equator. Second, shapely's batch API
(`query_nearest` with an array of geometries) **amortises** the Python boundary to
5.7 us per query at 1M points; rtree-geodetic currently offers per-call queries **only**.

Build times for rtree-geodetic include validating every coordinate and, for
extent geometries, precomputing per-edge great-circle envelopes; construction
currently traverses Python objects (`__geo_interface__` or sequences): a
numpy / GeoArrow fast path is future work.

## Licence

Licensed under either of Apache License, Version 2.0 or MIT licence at your option.
