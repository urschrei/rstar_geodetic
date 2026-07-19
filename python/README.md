# rstar-geodetic

Python bindings for [`rstar_geodetic`](https://github.com/urschrei/rstar_geodetic), a
geodetic (longitude/latitude) R-tree with great-circle and WGS84 nearest-neighbour and
radius queries over point, linestring, and polygon geometries.

Coordinates are longitude first, latitude second, in degrees; distances are returned in
metres. The tree classes interoperate with shapely and geopandas through the
`__geo_interface__` protocol.
