"""Benchmark rtree-geodetic against planar spatial indexes on realistic workloads.

Compares GeodeticPointTree / GeodeticLineStringTree / GeodeticPolygonTree with
shapely's STRtree (GEOS) and the Rtree package (libspatialindex) on large synthetic
datasets: one million globally distributed points and one hundred thousand small
linestrings and polygons.

The planar libraries index raw lon/lat degrees, so their distances are in degrees
and their nearest-neighbour answers degrade with latitude; this script also reports
how often the planar nearest neighbour disagrees with the geodesic one. Run with:

    uv run --group bench python benchmarks/bench.py [--points N] [--extents N]
"""

import argparse
import math
import random
import time

from rtree import index as rtree_index
from rtree_geodetic import (
    GeodeticLineStringTree,
    GeodeticPointTree,
    GeodeticPolygonTree,
)
from shapely import STRtree
from shapely.geometry import LineString, Point, Polygon

SEED = 20260719
EARTH_METRES_PER_DEGREE = 111_320.0


def uniform_sphere_points(rng: random.Random, n: int) -> list[tuple[float, float]]:
    """Uniformly distributed points on the sphere (not on the lon/lat rectangle)."""
    out = []
    for _ in range(n):
        lon = rng.uniform(-180.0, 180.0)
        lat = math.degrees(math.asin(rng.uniform(-1.0, 1.0)))
        out.append((lon, lat))
    return out


def small_linestrings(rng: random.Random, n: int) -> list[LineString]:
    """Short polylines (3 to 10 vertices, roughly 10 to 100 km across)."""
    out = []
    for _ in range(n):
        lon = rng.uniform(-179.0, 179.0)
        lat = math.degrees(math.asin(rng.uniform(-0.99, 0.99)))
        coords = [(lon, lat)]
        for _ in range(rng.randint(2, 9)):
            lon += rng.uniform(-0.15, 0.15)
            lat += rng.uniform(-0.15, 0.15)
            coords.append((lon, round(max(-89.9, min(89.9, lat)), 6)))
        out.append(LineString(coords))
    return out


def small_polygons(rng: random.Random, n: int) -> list[Polygon]:
    """Small convex polygons (buffered points, 8 vertices, 2 to 20 km across)."""
    out = []
    for _ in range(n):
        lon = rng.uniform(-179.0, 179.0)
        lat = math.degrees(math.asin(rng.uniform(-0.99, 0.99)))
        out.append(Point(lon, lat).buffer(rng.uniform(0.01, 0.1), quad_segs=2))
    return out


def timed(fn, *args):
    start = time.perf_counter()
    result = fn(*args)
    return result, time.perf_counter() - start


def per_query_us(fn, queries) -> float:
    start = time.perf_counter()
    for q in queries:
        fn(q)
    return (time.perf_counter() - start) / len(queries) * 1e6


def row(label: str, value: str) -> None:
    print(f"  {label:<58} {value:>14}")


def bench_points(num_points: int, num_queries: int) -> None:
    rng = random.Random(SEED)
    coords = uniform_sphere_points(rng, num_points)
    queries = uniform_sphere_points(rng, num_queries)
    radius_queries = queries[: num_queries // 5]

    print(f"\nPoints: {num_points:,} uniform on the sphere, {num_queries:,} queries")

    tree, t = timed(GeodeticPointTree, coords)
    row("GeodeticPointTree build (from tuples)", f"{t:8.2f} s")

    shapely_points = [Point(lon, lat) for lon, lat in coords]
    strtree, t = timed(STRtree, shapely_points)
    row("shapely STRtree build", f"{t:8.2f} s")

    def stream():
        for i, (lon, lat) in enumerate(coords):
            yield (i, (lon, lat, lon, lat), None)

    rt, t = timed(lambda: rtree_index.Index(stream()))
    row("Rtree (libspatialindex) build (stream)", f"{t:8.2f} s")

    row(
        "GeodeticPointTree nearest (great-circle metres)",
        f"{per_query_us(tree.nearest, queries):8.1f} us",
    )
    row(
        "GeodeticPointTree nearest_wgs84 (geodesic metres)",
        f"{per_query_us(tree.nearest_wgs84, queries):8.1f} us",
    )
    shapely_queries = [Point(lon, lat) for lon, lat in queries]
    row(
        "shapely STRtree query_nearest (planar degrees)",
        f"{per_query_us(strtree.query_nearest, shapely_queries):8.1f} us",
    )
    _, t = timed(strtree.query_nearest, shapely_queries)
    row(
        "shapely STRtree query_nearest, one batch call",
        f"{t / len(queries) * 1e6:8.1f} us",
    )

    def rtree_nearest(q):
        next(rt.nearest((q[0], q[1], q[0], q[1]), 1))

    row(
        "Rtree nearest (planar degrees)",
        f"{per_query_us(rtree_nearest, queries):8.1f} us",
    )

    radius_m = 50_000.0
    hits = 0

    def within(q):
        nonlocal hits
        hits += len(tree.within_distance(q, radius_m))

    us = per_query_us(within, radius_queries)
    row(
        f"GeodeticPointTree within_distance {radius_m / 1000:.0f} km "
        f"(avg {hits / len(radius_queries):.1f} hits)",
        f"{us:8.1f} us",
    )
    radius_deg = radius_m / EARTH_METRES_PER_DEGREE

    def dwithin(q):
        strtree.query(q, predicate="dwithin", distance=radius_deg)

    row(
        "shapely STRtree dwithin (equator-equivalent degrees)",
        f"{per_query_us(dwithin, [Point(*q) for q in radius_queries]):8.1f} us",
    )

    # Planar answers are wrong where meridians converge: count disagreements with
    # the geodesic nearest neighbour over the same data.
    mismatches = sum(
        1
        for q, sq in zip(queries, shapely_queries)
        if tree.nearest_wgs84(q) != int(strtree.query_nearest(sq)[0])
    )
    row(
        "planar (STRtree) nearest != geodesic nearest",
        f"{mismatches / len(queries) * 100:7.1f} %",
    )


def bench_extents(num_geoms: int, num_queries: int) -> None:
    rng = random.Random(SEED + 1)
    queries = uniform_sphere_points(rng, num_queries)

    lines = small_linestrings(rng, num_geoms)
    print(f"\nLinestrings: {num_geoms:,} short polylines, {num_queries:,} queries")
    tree, t = timed(GeodeticLineStringTree, lines)
    row("GeodeticLineStringTree build (via __geo_interface__)", f"{t:8.2f} s")
    strtree, t = timed(STRtree, lines)
    row("shapely STRtree build", f"{t:8.2f} s")
    row(
        "GeodeticLineStringTree nearest (metres to nearest point)",
        f"{per_query_us(tree.nearest, queries):8.1f} us",
    )
    row(
        "shapely STRtree query_nearest (planar degrees)",
        f"{per_query_us(strtree.query_nearest, [Point(*q) for q in queries]):8.1f} us",
    )

    polys = small_polygons(rng, num_geoms)
    print(f"\nPolygons: {num_geoms:,} small convex polygons, {num_queries:,} queries")
    tree, t = timed(GeodeticPolygonTree, polys)
    row("GeodeticPolygonTree build (via __geo_interface__)", f"{t:8.2f} s")
    strtree, t = timed(STRtree, polys)
    row("shapely STRtree build", f"{t:8.2f} s")
    row(
        "GeodeticPolygonTree nearest (0 m inside)",
        f"{per_query_us(tree.nearest, queries):8.1f} us",
    )
    row(
        "shapely STRtree query_nearest (planar degrees)",
        f"{per_query_us(strtree.query_nearest, [Point(*q) for q in queries]):8.1f} us",
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--points", type=int, default=1_000_000)
    parser.add_argument("--extents", type=int, default=100_000)
    parser.add_argument("--queries", type=int, default=10_000)
    args = parser.parse_args()

    bench_points(args.points, args.queries)
    bench_extents(args.extents, max(args.queries // 5, 1))


if __name__ == "__main__":
    main()
