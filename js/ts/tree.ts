/**
 * The `PointTree` wrapper and the input conversions it accepts.
 *
 * The wasm class takes an interleaved `Float64Array`; this layer also accepts a plain
 * array of numbers or an array of `[lon, lat]` pairs, and presents camelCase parameter
 * names. Query results are plain objects and typed arrays owned by the caller.
 */

import {
  PointTree as WasmPointTree,
  type Neighbor,
  type Neighbors,
} from "./wasm/rstar_geodetic_wasm.js";

export type { Neighbor, Neighbors };

/**
 * Point coordinates in degrees, longitude first: an interleaved
 * `[lon0, lat0, lon1, lat1, ...]` array (typed or plain) or an array of `[lon, lat]`
 * pairs. The interleaved typed array is the fastest path: it crosses into wasm with a
 * single copy.
 */
export type Coordinates =
  | Float64Array
  | readonly number[]
  | ReadonlyArray<readonly [number, number]>;

/**
 * Converts any accepted {@link Coordinates} form to the interleaved `Float64Array` the
 * wasm module takes. A `Float64Array` is returned as is.
 */
export function flattenCoordinates(coords: Coordinates): Float64Array {
  if (coords instanceof Float64Array) {
    return coords;
  }
  if (coords.length === 0) {
    return new Float64Array(0);
  }
  if (typeof coords[0] === "number") {
    return Float64Array.from(coords as readonly number[]);
  }
  const pairs = coords as ReadonlyArray<readonly [number, number]>;
  const flat = new Float64Array(pairs.length * 2);
  for (let i = 0; i < pairs.length; i++) {
    const [lon, lat] = pairs[i];
    flat[i * 2] = lon;
    flat[i * 2 + 1] = lat;
  }
  return flat;
}

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
