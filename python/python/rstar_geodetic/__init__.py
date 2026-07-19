"""Geodetic R-tree with great-circle and WGS84 nearest-neighbour and radius queries.

Coordinates are longitude first, latitude second, in degrees; distances are returned in
metres. The classes interoperate with shapely and geopandas through the
``__geo_interface__`` protocol.
"""

from ._rstar_geodetic import __version__

__all__ = ["__version__"]
