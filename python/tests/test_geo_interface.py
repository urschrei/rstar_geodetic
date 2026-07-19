"""Inbound parsing of the __geo_interface__ protocol and GeoJSON mappings."""

import pytest
import rstar_geodetic as rg


class FakePoint:
    """A minimal object exposing the geo interface, like a shapely geometry."""

    def __init__(self, lon, lat):
        self._coords = (lon, lat)

    @property
    def __geo_interface__(self):
        return {"type": "Point", "coordinates": self._coords}


def test_point_from_geo_interface_object():
    tree = rg.GeodeticPointTree([FakePoint(2.0, 48.0)])
    assert len(tree) == 1
    assert tree.geometry(0).__geo_interface__["coordinates"] == pytest.approx(
        (2.0, 48.0)
    )


def test_point_from_multipoint_mapping():
    mapping = {"type": "MultiPoint", "coordinates": [[0.0, 0.0], [10.0, 10.0]]}
    tree = rg.GeodeticPointTree(mapping)
    assert len(tree) == 2


def test_point_from_feature_collection():
    collection = {
        "type": "FeatureCollection",
        "features": [
            {
                "type": "Feature",
                "geometry": {"type": "Point", "coordinates": [1.0, 2.0]},
                "properties": {},
            },
            {
                "type": "Feature",
                "geometry": {"type": "Point", "coordinates": [3.0, 4.0]},
                "properties": {},
            },
        ],
    }
    tree = rg.GeodeticPointTree(collection)
    assert len(tree) == 2
    assert tree.geometry(1).__geo_interface__["coordinates"] == pytest.approx(
        (3.0, 4.0)
    )


def test_point_from_geometry_collection():
    collection = {
        "type": "GeometryCollection",
        "geometries": [
            {"type": "Point", "coordinates": [0.0, 0.0]},
            {"type": "Point", "coordinates": [5.0, 5.0]},
        ],
    }
    assert len(rg.GeodeticPointTree(collection)) == 2


def test_linestring_from_dict():
    mapping = {"type": "LineString", "coordinates": [[0.0, 0.0], [1.0, 1.0]]}
    tree = rg.GeodeticLineStringTree([mapping])
    assert len(tree) == 1
    assert len(tree.geometry(0)) == 2


def test_multilinestring():
    mapping = {
        "type": "MultiLineString",
        "coordinates": [[[0.0, 0.0], [1.0, 1.0]], [[5.0, 5.0], [6.0, 4.0]]],
    }
    assert len(rg.GeodeticLineStringTree(mapping)) == 2


def test_polygon_from_dict_with_hole():
    mapping = {
        "type": "Polygon",
        "coordinates": [
            [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [0.0, 0.0]],
            [[3.0, 3.0], [3.0, 7.0], [7.0, 7.0], [7.0, 3.0], [3.0, 3.0]],
        ],
    }
    tree = rg.GeodeticPolygonTree([mapping])
    assert len(tree) == 1
    interface = tree.geometry(0).__geo_interface__
    assert len(interface["coordinates"]) == 2  # exterior plus one hole


def test_multipolygon():
    mapping = {
        "type": "MultiPolygon",
        "coordinates": [
            [[[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0], [0.0, 0.0]]],
            [[[10.0, 10.0], [14.0, 10.0], [14.0, 14.0], [10.0, 14.0], [10.0, 10.0]]],
        ],
    }
    assert len(rg.GeodeticPolygonTree(mapping)) == 2


def test_wrong_geometry_type_raises():
    linestring = {"type": "LineString", "coordinates": [[0.0, 0.0], [1.0, 1.0]]}
    with pytest.raises(ValueError):
        rg.GeodeticPointTree([linestring])


def test_polygon_geo_interface_roundtrip():
    ring = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [0.0, 0.0]]
    tree = rg.GeodeticPolygonTree([{"type": "Polygon", "coordinates": [ring]}])
    interface = tree.geometry(0).__geo_interface__
    assert interface["type"] == "Polygon"
    assert interface["coordinates"][0][0] == pytest.approx((0.0, 0.0))
