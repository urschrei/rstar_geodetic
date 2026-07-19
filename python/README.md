# rstar-geodetic

Python bindings for [`rstar_geodetic`](https://github.com/urschrei/rstar_geodetic), a
geodetic (longitude/latitude) R-tree with great-circle and WGS84 nearest-neighbour and
radius queries over point, linestring, and polygon geometries.

Each `(lon, lat)` is mapped to a unit vector on the sphere and indexed in an R-tree, so the
antimeridian and the poles are ordinary interior points: no wrapping or special cases.
Coordinates are longitude first, latitude second, in degrees; distances are returned in
metres. Queries return integer input positions (the shapely `STRtree` convention).

## Install

```sh
pip install rstar-geodetic
```

Wheels are built for CPython 3.10 and newer (a single abi3 wheel per platform).

## Quick start

A tree is built from any iterable of geometry-likes: `(lon, lat)` pairs, GeoJSON mappings,
objects exposing `__geo_interface__` (shapely geometries), or a single object whose
`__geo_interface__` is a collection (a geopandas `GeoSeries`, a shapely `Multi*`, a
`GeometryCollection`, or a `FeatureCollection`).

```python
from rstar_geodetic import GeodeticPointTree

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
from rstar_geodetic import GeodeticLineStringTree, GeodeticPolygonTree

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
from rstar_geodetic import GeodeticPointTree

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

## Licence

Licensed under either of Apache License, Version 2.0 or MIT licence at your option.
