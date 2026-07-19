"""Interoperability with shapely geometries (a development dependency)."""

import pytest

shapely_geometry = pytest.importorskip("shapely.geometry")
import rtree_geodetic as rg  # noqa: E402
from shapely.geometry import (  # noqa: E402
    LineString,
    MultiPoint,
    Point,
    Polygon,
    shape,
)


def test_point_tree_from_shapely_points():
    points = [Point(-0.1278, 51.5074), Point(2.3522, 48.8566)]
    tree = rg.GeodeticPointTree(points)
    assert len(tree) == 2
    assert tree.nearest(Point(2.0, 49.0)) == 1


def test_geometry_roundtrips_through_shapely_shape():
    tree = rg.GeodeticPointTree([Point(2.3522, 48.8566)])
    restored = shape(tree.geometry(0).__geo_interface__)
    assert restored.geom_type == "Point"
    assert restored.x == pytest.approx(2.3522)
    assert restored.y == pytest.approx(48.8566)


def test_multipoint_construction():
    tree = rg.GeodeticPointTree(MultiPoint([(0.0, 0.0), (10.0, 10.0)]))
    assert len(tree) == 2


def test_linestring_tree_from_shapely():
    lines = [LineString([(0, 0), (1, 1), (2, 0)]), LineString([(10, 10), (11, 11)])]
    tree = rg.GeodeticLineStringTree(lines)
    assert tree.nearest(Point(1.0, 0.5)) == 0
    assert shape(tree.geometry(0).__geo_interface__).geom_type == "LineString"


def test_polygon_tree_from_shapely():
    polygon = Polygon([(0, 0), (10, 0), (10, 10), (0, 10)])
    tree = rg.GeodeticPolygonTree([polygon])
    index, distance = tree.nearest_with_distance(Point(5.0, 5.0))
    assert index == 0
    assert distance == 0.0
    assert shape(tree.geometry(0).__geo_interface__).geom_type == "Polygon"
