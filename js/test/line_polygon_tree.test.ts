import { beforeAll, describe, expect, test } from "vitest";

import {
  LineStringTree,
  PointTree,
  PolygonTree,
  flattenLineStrings,
  flattenPolygons,
  init,
} from "../ts/node.js";

beforeAll(async () => {
  await init();
});

// Two short east-west segments on the equator, 10° apart, and one far north.
const LINES = [
  [
    [0.0, 0.0],
    [1.0, 0.0],
  ],
  [
    [10.0, 0.0],
    [11.0, 0.0],
  ],
  [
    [0.0, 60.0],
    [1.0, 60.0],
  ],
];

// A 2°x2° square at the origin, a square with a hole at 10°E, and a square at 60°N.
const SQUARE = [
  [-1.0, -1.0],
  [1.0, -1.0],
  [1.0, 1.0],
  [-1.0, 1.0],
  [-1.0, -1.0],
];
const WITH_HOLE = [
  [
    [9.0, -1.0],
    [11.0, -1.0],
    [11.0, 1.0],
    [9.0, 1.0],
    [9.0, -1.0],
  ],
  [
    [9.5, -0.5],
    [10.5, -0.5],
    [10.5, 0.5],
    [9.5, 0.5],
    [9.5, -0.5],
  ],
];
const NORTH = [
  [-1.0, 59.0],
  [1.0, 59.0],
  [1.0, 61.0],
  [-1.0, 61.0],
  [-1.0, 59.0],
];
const POLYGONS = [[SQUARE], WITH_HOLE, [NORTH]];

describe("flatteners", () => {
  test("flattenLineStrings builds CSR offsets", () => {
    const { coords, offsets } = flattenLineStrings(LINES);
    expect(Array.from(offsets)).toEqual([0, 2, 4, 6]);
    expect(coords.length).toBe(12);
    expect(coords[4]).toBe(10.0);
  });

  test("flattenLineStrings passes typed arrays through", () => {
    const coords = new Float64Array([0, 0, 1, 0]);
    const offsets = new Uint32Array([0, 2]);
    const flat = flattenLineStrings({ coords, offsets });
    expect(flat.coords).toBe(coords);
    expect(flat.offsets).toBe(offsets);
  });

  test("flattenPolygons builds two-level CSR offsets", () => {
    const { coords, ringOffsets, polygonOffsets } = flattenPolygons(POLYGONS);
    expect(Array.from(polygonOffsets)).toEqual([0, 1, 3, 4]);
    expect(Array.from(ringOffsets)).toEqual([0, 5, 10, 15, 20]);
    expect(coords.length).toBe(40);
  });
});

describe("LineStringTree", () => {
  test("builds from nested arrays and from the flat layout", () => {
    using nested = new LineStringTree(LINES);
    using flat = new LineStringTree(flattenLineStrings(LINES));
    expect(nested.length).toBe(3);
    expect(flat.length).toBe(3);
  });

  test("nearest reports the input position and distance to the line", () => {
    using tree = new LineStringTree(LINES);
    expect(tree.nearest(10.5, 0.1)).toBe(1);
    const hit = tree.nearestWithDistance(0.5, 0.0)!;
    expect(hit.index).toBe(0);
    expect(hit.distance).toBeCloseTo(0, 6);
  });

  test("withinDistance returns every line in range", () => {
    using tree = new LineStringTree(LINES);
    const { indices, distances } = tree.withinDistance(5.0, 0.0, 600_000);
    expect(Array.from(indices).sort()).toEqual([0, 1]);
    expect(distances.length).toBe(2);
    for (const d of distances) {
      expect(d).toBeLessThanOrEqual(600_000);
    }
  });

  test("rejects bad offsets and too-short lines", () => {
    expect(
      () => new LineStringTree({ coords: [0, 0, 1, 0], offsets: [0, 1, 2, 3] }),
    ).toThrow(/offsets must/);
    expect(() => new LineStringTree([[[0, 0]]])).toThrow();
  });

  test("builds from GeoJSON", () => {
    const fc = {
      type: "FeatureCollection" as const,
      features: LINES.map((coordinates) => ({
        type: "Feature" as const,
        geometry: { type: "LineString" as const, coordinates },
        properties: {},
      })),
    };
    using tree = LineStringTree.fromGeoJSON(fc);
    expect(tree.length).toBe(3);
    expect(tree.nearest(10.5, 0.1)).toBe(1);
  });
});

describe("PolygonTree", () => {
  test("builds from nested rings and from the flat layout", () => {
    using nested = new PolygonTree(POLYGONS);
    using flat = new PolygonTree(flattenPolygons(POLYGONS));
    expect(nested.length).toBe(3);
    expect(flat.length).toBe(3);
  });

  test("a point inside a polygon is at distance zero", () => {
    using tree = new PolygonTree(POLYGONS);
    const hit = tree.nearestWithDistance(0.0, 0.0)!;
    expect(hit.index).toBe(0);
    expect(hit.distance).toBe(0);
  });

  test("a point in a hole is outside, at the distance to the hole's edge", () => {
    using tree = new PolygonTree(POLYGONS);
    const hit = tree.nearestWithDistance(10.0, 0.0)!;
    expect(hit.index).toBe(1);
    expect(hit.distance).toBeGreaterThan(50_000);
  });

  test("withinDistance includes containing polygons", () => {
    using tree = new PolygonTree(POLYGONS);
    const { indices, distances } = tree.withinDistance(0.0, 0.0, 1.0);
    expect(Array.from(indices)).toEqual([0]);
    expect(Array.from(distances)).toEqual([0]);
  });

  test("rejects a polygon with no rings and an unclosed ring", () => {
    expect(
      () =>
        new PolygonTree({
          coords: [],
          ringOffsets: [0],
          polygonOffsets: [0, 0],
        }),
    ).toThrow(/no rings/);
    expect(
      () =>
        new PolygonTree([
          [
            [
              [0, 0],
              [1, 0],
              [1, 1],
            ],
          ],
        ]),
    ).toThrow();
  });

  test("builds from GeoJSON geometries", () => {
    using tree = PolygonTree.fromGeoJSON(
      POLYGONS.map((coordinates) => ({ type: "Polygon" as const, coordinates })),
    );
    expect(tree.nearest(0.0, 60.0)).toBe(2);
  });
});

describe("GeoJSON input checks", () => {
  test("PointTree.fromGeoJSON accepts features and ignores elevation", () => {
    using tree = PointTree.fromGeoJSON([
      { type: "Feature", geometry: { type: "Point", coordinates: [179.0, -17.0, 5.0] } },
      { type: "Point", coordinates: [-175.0, -21.0] },
    ]);
    expect(tree.length).toBe(2);
    expect(tree.nearest(-176.0, -21.0)).toBe(1);
  });

  test("rejects a null geometry and a geometry type mismatch with the position", () => {
    expect(() => PointTree.fromGeoJSON([{ type: "Feature", geometry: null }])).toThrow(
      /feature 0 has a null geometry/,
    );
    expect(() =>
      PointTree.fromGeoJSON([
        { type: "Point", coordinates: [0, 0] },
        // A MultiPoint would not map one-to-one onto input positions.
        { type: "MultiPoint", coordinates: [[0, 0]] } as never,
      ]),
    ).toThrow(/geometry 1 is a MultiPoint; expected Point/);
  });
});
