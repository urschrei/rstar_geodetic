"""Interoperability with geopandas GeoSeries (skipped if geopandas is unavailable)."""

import pytest

gpd = pytest.importorskip("geopandas")
import rstar_geodetic as rg  # noqa: E402
from shapely.geometry import Point  # noqa: E402


def test_geoseries_construction():
    series = gpd.GeoSeries([Point(-0.1278, 51.5074), Point(2.3522, 48.8566)])
    tree = rg.GeodeticPointTree(series)
    assert len(tree) == 2
    assert tree.nearest(Point(2.0, 49.0)) == 1


def test_geoseries_nearest_with_distance_wgs84():
    # Mirrors the plan's verification example: London, Paris, query near Amsterdam.
    series = gpd.GeoSeries([Point(-0.1278, 51.5074), Point(2.3522, 48.8566)])
    tree = rg.GeodeticPointTree(series)
    index, distance = tree.nearest_with_distance_wgs84((4.9041, 52.3676))
    assert index == 0  # London is nearest
    assert 350_000.0 < distance < 370_000.0


def test_geoseries_of_polygons():
    from shapely.geometry import Polygon

    series = gpd.GeoSeries(
        [
            Polygon([(0, 0), (10, 0), (10, 10), (0, 10)]),
            Polygon([(20, 20), (24, 20), (24, 24), (20, 24)]),
        ]
    )
    tree = rg.GeodeticPolygonTree(series)
    assert len(tree) == 2
    assert tree.nearest(Point(5.0, 5.0)) == 0
