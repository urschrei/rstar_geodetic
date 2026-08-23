import { beforeAll, describe, expect, test } from "vitest";

import { PointTree, flattenCoordinates, init } from "../ts/node.js";

// Two islands either side of the antimeridian plus a distant point (the crate's own
// README example), in interleaved [lon, lat] order.
const ISLANDS = new Float64Array([179.0, -17.0, -175.0, -21.0, -77.0, -12.0]);

beforeAll(async () => {
  await init();
});

describe("construction", () => {
  test("accepts an interleaved Float64Array", () => {
    using tree = new PointTree(ISLANDS);
    expect(tree.length).toBe(3);
  });

  test("accepts a plain number array and [lon, lat] pairs", () => {
    using fromFlat = new PointTree([179.0, -17.0, -175.0, -21.0]);
    using fromPairs = new PointTree([
      [179.0, -17.0],
      [-175.0, -21.0],
    ]);
    expect(fromFlat.length).toBe(2);
    expect(fromPairs.length).toBe(2);
  });

  test("accepts an empty input", () => {
    using tree = new PointTree([]);
    expect(tree.length).toBe(0);
    expect(tree.nearest(0, 0)).toBeUndefined();
    expect(tree.withinDistance(0, 0, 1e6).indices.length).toBe(0);
  });

  test("rejects an odd-length coordinate array", () => {
    expect(() => new PointTree([1, 2, 3])).toThrow(/even length/);
  });

  test("rejects an out-of-range coordinate with a GeodeticError", () => {
    let caught: unknown;
    try {
      new PointTree([200.0, 0.0]);
    } catch (error) {
      caught = error;
    }
    expect(caught).toBeInstanceOf(Error);
    expect((caught as Error).name).toBe("GeodeticError");
  });

  test("rejects a non-finite coordinate", () => {
    expect(() => new PointTree([0.0, NaN])).toThrow();
  });
});

describe("flattenCoordinates", () => {
  test("returns a Float64Array unchanged", () => {
    const flat = new Float64Array([1, 2]);
    expect(flattenCoordinates(flat)).toBe(flat);
  });

  test("interleaves pairs", () => {
    expect(Array.from(flattenCoordinates([[1, 2], [3, 4]]))).toEqual([1, 2, 3, 4]);
  });
});

describe("nearest", () => {
  test("crosses the antimeridian without wrapping", () => {
    using tree = new PointTree(ISLANDS);
    // Just east of the seam: the 175°W island is nearer than the 179°E one.
    expect(tree.nearest(-176.0, -21.0)).toBe(1);
    const hit = tree.nearestWithDistance(-176.0, -21.0);
    expect(hit?.index).toBe(1);
    expect(hit?.distance).toBeGreaterThan(0);
    expect(hit?.distance).toBeLessThan(200_000);
  });

  test("reports the first position among bit-identical duplicates", () => {
    using tree = new PointTree([10.0, 10.0, 10.0, 10.0, 20.0, 20.0]);
    expect(tree.nearest(10.0, 10.0)).toBe(0);
    expect(tree.nearestWithDistance(10.0, 10.0)?.distance).toBe(0);
  });

  test("rejects an invalid query coordinate", () => {
    using tree = new PointTree(ISLANDS);
    expect(() => tree.nearest(0, 91)).toThrow();
  });
});

describe("withinDistance", () => {
  test("returns every point in range with its distance", () => {
    using tree = new PointTree(ISLANDS);
    const { indices, distances } = tree.withinDistance(-176.0, -21.0, 1_000_000);
    expect(indices).toBeInstanceOf(Uint32Array);
    expect(distances).toBeInstanceOf(Float64Array);
    expect(Array.from(indices).sort()).toEqual([0, 1]);
    expect(distances.length).toBe(2);
    for (const d of distances) {
      expect(d).toBeLessThanOrEqual(1_000_000);
    }
  });

  test("expands duplicates to every input position", () => {
    using tree = new PointTree([10.0, 10.0, 20.0, 20.0, 10.0, 10.0]);
    const { indices, distances } = tree.withinDistance(10.0, 10.0, 1.0);
    expect(Array.from(indices).sort()).toEqual([0, 2]);
    expect(Array.from(distances)).toEqual([0, 0]);
  });

  test("result arrays outlive the tree", () => {
    const tree = new PointTree(ISLANDS);
    const { indices } = tree.withinDistance(-176.0, -21.0, 1_000_000);
    tree.free();
    expect(indices.length).toBe(2);
  });
});

describe("nearestN", () => {
  test("returns the k nearest points, nearest first", () => {
    using tree = new PointTree(ISLANDS);
    const { indices, distances } = tree.nearestN(-176.0, -21.0, 2);
    expect(Array.from(indices)).toEqual([1, 0]);
    expect(distances.length).toBe(2);
    expect(distances[0]).toBeLessThan(distances[1]);
    expect(distances[0]).toBeCloseTo(103809.38, 0);
  });

  test("returns every point when k exceeds the tree size", () => {
    using tree = new PointTree(ISLANDS);
    expect(tree.nearestN(0.0, 0.0, 10).indices.length).toBe(3);
    expect(tree.nearestN(0.0, 0.0, 0).indices.length).toBe(0);
  });

  test("counts duplicate positions towards k", () => {
    using tree = new PointTree([10.0, 10.0, 20.0, 20.0, 10.0, 10.0]);
    const { indices, distances } = tree.nearestN(10.0, 10.0, 2);
    expect(Array.from(indices).sort()).toEqual([0, 2]);
    expect(Array.from(distances)).toEqual([0, 0]);
  });
});

describe("inRectangle", () => {
  test("handles a window crossing the antimeridian", () => {
    using tree = new PointTree([179.0, 0.0, -178.0, 0.0, 0.0, 0.0]);
    // 170°E eastward to 170°W.
    const hits = tree.inRectangle(170.0, -10.0, -170.0, 10.0);
    expect(hits).toBeInstanceOf(Uint32Array);
    expect(Array.from(hits).sort()).toEqual([0, 1]);
  });

  test("rejects an inverted latitude range", () => {
    using tree = new PointTree(ISLANDS);
    expect(() => tree.inRectangle(0, 10, 10, 0)).toThrow(/minLat <= maxLat/);
  });
});

describe("wgs84", () => {
  test("agrees with the spherical queries on which point is nearest", () => {
    using tree = new PointTree(ISLANDS);
    expect(tree.nearestWgs84(-176.0, -21.0)).toBe(tree.nearest(-176.0, -21.0));
    const spherical = tree.nearestWithDistance(-176.0, -21.0)!;
    const geodesic = tree.nearestWithDistanceWgs84(-176.0, -21.0)!;
    expect(geodesic.index).toBe(spherical.index);
    // Ellipsoidal and spherical distances differ by well under 1%.
    expect(Math.abs(geodesic.distance - spherical.distance) / spherical.distance).toBeLessThan(0.01);
  });

  test("withinDistanceWgs84 reports geodesic distances", () => {
    using tree = new PointTree(ISLANDS);
    const { indices, distances } = tree.withinDistanceWgs84(-176.0, -21.0, 1_000_000);
    expect(Array.from(indices).sort()).toEqual([0, 1]);
    for (const d of distances) {
      expect(d).toBeLessThanOrEqual(1_000_000);
    }
  });
});

describe("lifetime", () => {
  test("a freed tree rejects further use", () => {
    const tree = new PointTree(ISLANDS);
    tree.free();
    expect(() => tree.nearest(0, 0)).toThrow();
  });

  test("init is idempotent", async () => {
    await expect(init()).resolves.toBeUndefined();
  });
});
