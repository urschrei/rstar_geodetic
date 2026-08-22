/**
 * The tree classes wrapping the wasm module's, adding the input conversions of
 * `./geometry.ts`, GeoJSON constructors, and camelCase parameter names. Query results
 * are plain objects and typed arrays owned by the caller.
 */

import {
  LineStringTree as WasmLineStringTree,
  PointTree as WasmPointTree,
  PolygonTree as WasmPolygonTree,
  type Neighbor,
  type Neighbors,
} from "./wasm/rstar_geodetic_wasm.js";
import {
  type Coordinates,
  type GeoJSONInput,
  type GeoJSONLineString,
  type GeoJSONPoint,
  type GeoJSONPolygon,
  type LineStrings,
  type Polygons,
  flattenCoordinates,
  flattenLineStrings,
  flattenPolygons,
  geoJSONGeometries,
} from "./geometry.js";

export type { Neighbor, Neighbors };

/**
 * A geodetic R-tree over points, queried by great-circle (or, with the `Wgs84` variants,
 * WGS84 geodesic) distance. Query results are positions into the input sequence.
 *
 * The tree lives in wasm memory: call {@link PointTree.free} when done, or use it in a
 * `using` declaration.
 */
export class PointTree {
  readonly #inner: WasmPointTree;

  /**
   * Builds a tree from point coordinates in degrees, longitude first. Throws a
   * `GeodeticError` if a coordinate is out of range or not finite.
   */
  constructor(coords: Coordinates) {
    this.#inner = new WasmPointTree(flattenCoordinates(coords));
  }

  /**
   * Builds a tree from GeoJSON `Point` geometries (a `FeatureCollection`, or an array
   * of `Feature`s or geometries). Result positions index the input features.
   */
  static fromGeoJSON(input: GeoJSONInput<GeoJSONPoint>): PointTree {
    const geometries = geoJSONGeometries(input, "Point");
    return new PointTree(geometries.map((geometry) => geometry.coordinates));
  }

  /** The number of points in the tree. */
  get length(): number {
    return this.#inner.length;
  }

  /**
   * The input position of the nearest point to `(lon, lat)` by great-circle distance,
   * or `undefined` if the tree is empty.
   */
  nearest(lon: number, lat: number): number | undefined {
    return this.#inner.nearest(lon, lat);
  }

  /**
   * The nearest point to `(lon, lat)` by great-circle distance as `{ index, distance }`
   * (distance in metres), or `undefined` if the tree is empty.
   */
  nearestWithDistance(lon: number, lat: number): Neighbor | undefined {
    return this.#inner.nearestWithDistance(lon, lat);
  }

  /**
   * Every point within `radiusMetres` great-circle metres of `(lon, lat)`, as
   * `{ indices, distances }`.
   */
  withinDistance(lon: number, lat: number, radiusMetres: number): Neighbors {
    return this.#inner.withinDistance(lon, lat, radiusMetres);
  }

  /**
   * The input positions of every point inside the longitude/latitude rectangle from
   * `(minLon, minLat)` to `(maxLon, maxLat)`. `minLon > maxLon` denotes a window
   * crossing the antimeridian (RFC 7946); `minLat <= maxLat` is required.
   */
  inRectangle(
    minLon: number,
    minLat: number,
    maxLon: number,
    maxLat: number,
  ): Uint32Array {
    return this.#inner.inRectangle(minLon, minLat, maxLon, maxLat);
  }

  /**
   * The input position of the nearest point to `(lon, lat)` by WGS84-ellipsoid geodesic
   * distance, or `undefined` if the tree is empty.
   */
  nearestWgs84(lon: number, lat: number): number | undefined {
    return this.#inner.nearestWgs84(lon, lat);
  }

  /**
   * The nearest point to `(lon, lat)` on the WGS84 ellipsoid as `{ index, distance }`
   * (geodesic distance in metres), or `undefined` if the tree is empty.
   */
  nearestWithDistanceWgs84(lon: number, lat: number): Neighbor | undefined {
    return this.#inner.nearestWithDistanceWgs84(lon, lat);
  }

  /**
   * Every point within `radiusMetres` WGS84-ellipsoid geodesic metres of `(lon, lat)`,
   * as `{ indices, distances }`.
   */
  withinDistanceWgs84(lon: number, lat: number, radiusMetres: number): Neighbors {
    return this.#inner.withinDistanceWgs84(lon, lat, radiusMetres);
  }

  /** Releases the tree's wasm memory. The tree must not be used afterwards. */
  free(): void {
    this.#inner.free();
  }

  [Symbol.dispose](): void {
    this.free();
  }
}

/**
 * A geodetic R-tree over linestrings, queried by great-circle distance to the nearest
 * point of each linestring. Query results are positions into the input sequence.
 *
 * The tree lives in wasm memory: call {@link LineStringTree.free} when done, or use it
 * in a `using` declaration.
 */
export class LineStringTree {
  readonly #inner: WasmLineStringTree;

  /**
   * Builds a tree from linestrings. Throws a `GeodeticError` on an invalid offset
   * array, an out-of-range or non-finite coordinate, or a linestring with fewer than
   * two vertices.
   */
  constructor(lines: LineStrings) {
    const { coords, offsets } = flattenLineStrings(lines);
    this.#inner = new WasmLineStringTree(coords, offsets);
  }

  /**
   * Builds a tree from GeoJSON `LineString` geometries (a `FeatureCollection`, or an
   * array of `Feature`s or geometries). Result positions index the input features.
   */
  static fromGeoJSON(input: GeoJSONInput<GeoJSONLineString>): LineStringTree {
    const geometries = geoJSONGeometries(input, "LineString");
    return new LineStringTree(geometries.map((geometry) => geometry.coordinates));
  }

  /** The number of linestrings in the tree. */
  get length(): number {
    return this.#inner.length;
  }

  /**
   * The input position of the linestring nearest to `(lon, lat)` by great-circle
   * distance, or `undefined` if the tree is empty.
   */
  nearest(lon: number, lat: number): number | undefined {
    return this.#inner.nearest(lon, lat);
  }

  /**
   * The linestring nearest to `(lon, lat)` as `{ index, distance }` (distance in
   * metres), or `undefined` if the tree is empty.
   */
  nearestWithDistance(lon: number, lat: number): Neighbor | undefined {
    return this.#inner.nearestWithDistance(lon, lat);
  }

  /**
   * Every linestring within `radiusMetres` great-circle metres of `(lon, lat)`, as
   * `{ indices, distances }`.
   */
  withinDistance(lon: number, lat: number, radiusMetres: number): Neighbors {
    return this.#inner.withinDistance(lon, lat, radiusMetres);
  }

  /** Releases the tree's wasm memory. The tree must not be used afterwards. */
  free(): void {
    this.#inner.free();
  }

  [Symbol.dispose](): void {
    this.free();
  }
}

/**
 * A geodetic R-tree over polygons, queried by great-circle distance to the nearest
 * point of each polygon (zero for a polygon containing the query). Query results are
 * positions into the input sequence.
 *
 * The tree lives in wasm memory: call {@link PolygonTree.free} when done, or use it in
 * a `using` declaration.
 */
export class PolygonTree {
  readonly #inner: WasmPolygonTree;

  /**
   * Builds a tree from polygons. Throws a `GeodeticError` on an invalid offset array, a
   * polygon with no rings, an out-of-range or non-finite coordinate, or a ring that is
   * not closed or has too few vertices.
   */
  constructor(polygons: Polygons) {
    const { coords, ringOffsets, polygonOffsets } = flattenPolygons(polygons);
    this.#inner = new WasmPolygonTree(coords, ringOffsets, polygonOffsets);
  }

  /**
   * Builds a tree from GeoJSON `Polygon` geometries (a `FeatureCollection`, or an array
   * of `Feature`s or geometries). Result positions index the input features.
   */
  static fromGeoJSON(input: GeoJSONInput<GeoJSONPolygon>): PolygonTree {
    const geometries = geoJSONGeometries(input, "Polygon");
    return new PolygonTree(geometries.map((geometry) => geometry.coordinates));
  }

  /** The number of polygons in the tree. */
  get length(): number {
    return this.#inner.length;
  }

  /**
   * The input position of the polygon nearest to `(lon, lat)` by great-circle distance
   * (zero if the point is inside one), or `undefined` if the tree is empty.
   */
  nearest(lon: number, lat: number): number | undefined {
    return this.#inner.nearest(lon, lat);
  }

  /**
   * The polygon nearest to `(lon, lat)` as `{ index, distance }` (distance in metres,
   * zero if the point is inside it), or `undefined` if the tree is empty.
   */
  nearestWithDistance(lon: number, lat: number): Neighbor | undefined {
    return this.#inner.nearestWithDistance(lon, lat);
  }

  /**
   * Every polygon within `radiusMetres` great-circle metres of `(lon, lat)`, including
   * any containing it, as `{ indices, distances }`.
   */
  withinDistance(lon: number, lat: number, radiusMetres: number): Neighbors {
    return this.#inner.withinDistance(lon, lat, radiusMetres);
  }

  /** Releases the tree's wasm memory. The tree must not be used afterwards. */
  free(): void {
    this.#inner.free();
  }

  [Symbol.dispose](): void {
    this.free();
  }
}
