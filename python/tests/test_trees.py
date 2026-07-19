"""Tree construction and query behaviour for the three geodetic tree classes."""

import math

import pytest
import rstar_geodetic as rg

CAPITALS = [
    (-0.1278, 51.5074),  # 0 London
    (2.3522, 48.8566),  # 1 Paris
    (13.4050, 52.5200),  # 2 Berlin
    (-3.7038, 40.4168),  # 3 Madrid
]


def haversine(a, b):
    """Great-circle metres on the GRS80 mean-radius sphere the crate uses."""
    radius = 6371008.8
    lon1, lat1 = math.radians(a[0]), math.radians(a[1])
    lon2, lat2 = math.radians(b[0]), math.radians(b[1])
    h = (
        math.sin((lat2 - lat1) / 2) ** 2
        + math.cos(lat1) * math.cos(lat2) * math.sin((lon2 - lon1) / 2) ** 2
    )
    return 2 * radius * math.asin(math.sqrt(h))


def test_construction_from_sequences():
    tree = rg.GeodeticPointTree(CAPITALS)
    assert len(tree) == 4


def test_construction_from_dicts():
    features = [
        {"type": "Point", "coordinates": [0.0, 0.0]},
        {"type": "Point", "coordinates": [10.0, 10.0]},
    ]
    tree = rg.GeodeticPointTree(features)
    assert len(tree) == 2


def test_nearest_index_matches_brute_force():
    tree = rg.GeodeticPointTree(CAPITALS)
    query = (0.0, 50.0)
    expected = min(range(len(CAPITALS)), key=lambda i: haversine(CAPITALS[i], query))
    assert tree.nearest(query) == expected


def test_nearest_with_distance_matches_haversine():
    tree = rg.GeodeticPointTree(CAPITALS)
    query = (0.0, 50.0)
    index, distance = tree.nearest_with_distance(query)
    assert distance == pytest.approx(haversine(CAPITALS[index], query), rel=1e-6)


def test_london_paris_distance():
    tree = rg.GeodeticPointTree([CAPITALS[0], CAPITALS[1]])
    # The nearest to Paris's own location is Paris, at distance zero.
    index, distance = tree.nearest_with_distance(CAPITALS[1])
    assert index == 1
    assert distance == pytest.approx(0.0, abs=1.0)
    # London to Paris is about 343 km.
    assert 340_000.0 < haversine(CAPITALS[0], CAPITALS[1]) < 345_000.0


def test_within_distance_matches_brute_force():
    tree = rg.GeodeticPointTree(CAPITALS)
    query = (0.0, 50.0)
    got = set(tree.within_distance(query, 1_000_000.0))
    expected = {i for i, c in enumerate(CAPITALS) if haversine(c, query) <= 1_000_000.0}
    assert got == expected


def test_within_distance_return_distance():
    tree = rg.GeodeticPointTree(CAPITALS)
    query = (0.0, 50.0)
    for index, distance in tree.within_distance(
        query, 1_000_000.0, return_distance=True
    ):
        assert distance == pytest.approx(haversine(CAPITALS[index], query), rel=1e-6)


def test_within_distance_duplicate_coordinates():
    # Two identical points collapse in the coordinate map but must yield both indices.
    tree = rg.GeodeticPointTree([(0.0, 0.0), (0.0, 0.0), (40.0, 40.0)])
    got = set(tree.within_distance((0.0, 0.0), 1_000.0))
    assert got == {0, 1}


def test_geometry_roundtrip_geo_interface():
    tree = rg.GeodeticPointTree([CAPITALS[0]])
    interface = tree.geometry(0).__geo_interface__
    assert interface["type"] == "Point"
    assert interface["coordinates"] == pytest.approx(CAPITALS[0])


def test_geometries_returns_all():
    tree = rg.GeodeticPointTree(CAPITALS)
    assert len(tree.geometries()) == 4
    assert tree[2].__geo_interface__["coordinates"] == pytest.approx(CAPITALS[2])


def test_error_mapping_out_of_range():
    with pytest.raises(rg.GeodeticError):
        rg.GeodeticPointTree([(0.0, 200.0)])
    # The exception is a ValueError subclass.
    with pytest.raises(ValueError):
        rg.GeodeticPointTree([(0.0, 200.0)])


def test_in_rectangle_wraps_antimeridian():
    points = [(179.0, 0.0), (-178.0, 1.0), (0.0, 0.0)]
    tree = rg.GeodeticPointTree(points)
    got = set(tree.in_rectangle((170.0, -10.0), (-170.0, 10.0)))
    assert got == {0, 1}


def test_index_out_of_range_raises():
    tree = rg.GeodeticPointTree([(0.0, 0.0)])
    with pytest.raises(IndexError):
        tree.geometry(5)


def test_empty_tree_nearest_is_none():
    tree = rg.GeodeticPointTree([])
    assert tree.nearest((0.0, 0.0)) is None
    assert tree.within_distance((0.0, 0.0), 1e6) == []


def test_linestring_tree():
    lines = [
        [(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)],
        [(10.0, 10.0), (11.0, 11.0)],
    ]
    tree = rg.GeodeticLineStringTree(lines)
    assert len(tree) == 2
    assert tree.nearest((1.0, 0.5)) == 0
    within = set(tree.within_distance((1.0, 0.5), 100_000.0))
    assert within == {0}
    assert tree.geometry(0).__geo_interface__["type"] == "LineString"


def test_polygon_tree_contains_point():
    square = [[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0), (0.0, 0.0)]]
    tree = rg.GeodeticPolygonTree([square])
    index, distance = tree.nearest_with_distance((5.0, 5.0))
    assert index == 0
    assert distance == 0.0
    assert tree.geometry(0).__geo_interface__["type"] == "Polygon"


def test_nearest_wgs84_agrees_with_spherical_ranking():
    tree = rg.GeodeticPointTree(CAPITALS)
    query = (0.0, 50.0)
    assert tree.nearest_wgs84(query) == tree.nearest(query)
    _, geodesic = tree.nearest_with_distance_wgs84(query)
    _, spherical = tree.nearest_with_distance(query)
    # The ellipsoidal distance is within ~0.5% of the spherical one at this scale.
    assert geodesic == pytest.approx(spherical, rel=0.01)


def test_within_distance_wgs84():
    tree = rg.GeodeticPointTree(CAPITALS)
    # Within 100 km of a point near Paris: Paris only.
    assert set(tree.within_distance_wgs84((2.0, 49.0), 100_000.0)) == {1}
    pairs = tree.within_distance_wgs84((2.0, 49.0), 100_000.0, return_distance=True)
    assert all(distance <= 100_000.0 for _, distance in pairs)
