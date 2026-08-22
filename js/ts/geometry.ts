/**
 * The input forms the tree constructors accept, and their conversion to the flat,
 * interleaved layout the wasm module takes.
 *
 * Every geometry type has a flat form (typed arrays in the GeoArrow layout, the fastest
 * path: each array crosses into wasm with a single copy), a nested-array form, and a
 * GeoJSON form. The GeoJSON types here are structural, so values typed with
 * `@types/geojson` are accepted without conversion.
 */

/** A `[lon, lat]` position in degrees; any further elements (elevation) are ignored. */
export type Position = readonly number[];

/**
 * Point coordinates: an interleaved `[lon0, lat0, lon1, lat1, ...]` array (typed or
 * plain) or an array of `[lon, lat]` positions.
 */
export type Coordinates = Float64Array | readonly number[] | ReadonlyArray<Position>;

/** Linestrings in the flat CSR layout: linestring `i` spans vertices `offsets[i]..offsets[i + 1]`. */
export interface LineStringArrays {
  /** Interleaved `[lon, lat]` vertex coordinates of every linestring, concatenated. */
  coords: Float64Array | readonly number[];
  /** One entry per linestring plus a final end marker (`length + 1` entries). */
  offsets: Uint32Array | readonly number[];
}

/** Linestrings: the flat CSR layout or an array of linestrings, each an array of positions. */
export type LineStrings = LineStringArrays | ReadonlyArray<ReadonlyArray<Position>>;

/**
 * Polygons in the flat two-level CSR layout: `ringOffsets` partitions the vertices into
 * rings and `polygonOffsets` partitions the rings into polygons. Within a polygon the
 * first ring is the exterior and the rest are holes.
 */
export interface PolygonArrays {
  /** Interleaved `[lon, lat]` vertex coordinates of every ring, concatenated. */
  coords: Float64Array | readonly number[];
  /** One entry per ring plus a final end marker. */
  ringOffsets: Uint32Array | readonly number[];
  /** One entry per polygon plus a final end marker. */
  polygonOffsets: Uint32Array | readonly number[];
}

/**
 * Polygons: the flat CSR layout or an array of polygons, each an array of rings
 * (exterior first), each an array of positions — the GeoJSON `Polygon.coordinates` shape.
 */
export type Polygons =
  | PolygonArrays
  | ReadonlyArray<ReadonlyArray<ReadonlyArray<Position>>>;

export interface GeoJSONPoint {
  type: "Point";
  coordinates: Position;
}

export interface GeoJSONLineString {
  type: "LineString";
  coordinates: ReadonlyArray<Position>;
}

export interface GeoJSONPolygon {
  type: "Polygon";
  coordinates: ReadonlyArray<ReadonlyArray<Position>>;
}

export interface GeoJSONFeature<G> {
  type: "Feature";
  geometry: G | null;
}

export interface GeoJSONFeatureCollection<G> {
  type: "FeatureCollection";
  features: ReadonlyArray<GeoJSONFeature<G>>;
}

/**
 * GeoJSON input for a tree of geometry type `G`: a `FeatureCollection`, or an array of
 * `Feature`s or bare geometries. Every geometry must be of type `G` (no `Multi*`
 * geometries and no `null` geometries), so that result positions index the input
 * one-to-one.
 */
export type GeoJSONInput<G> =
  | GeoJSONFeatureCollection<G>
  | ReadonlyArray<GeoJSONFeature<G> | G>;

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
  const positions = coords as ReadonlyArray<Position>;
  const flat = new Float64Array(positions.length * 2);
  for (let i = 0; i < positions.length; i++) {
    writePosition(flat, i, positions[i]);
  }
  return flat;
}

/** Converts any accepted {@link LineStrings} form to the flat CSR layout, as typed arrays. */
export function flattenLineStrings(lines: LineStrings): {
  coords: Float64Array;
  offsets: Uint32Array;
} {
  if (!Array.isArray(lines)) {
    const arrays = lines as LineStringArrays;
    return {
      coords: toFloat64(arrays.coords),
      offsets: toUint32(arrays.offsets),
    };
  }
  const parts = lines as ReadonlyArray<ReadonlyArray<Position>>;
  const offsets = new Uint32Array(parts.length + 1);
  let total = 0;
  for (let i = 0; i < parts.length; i++) {
    offsets[i] = total;
    total += parts[i].length;
  }
  offsets[parts.length] = total;
  const coords = new Float64Array(total * 2);
  let vertex = 0;
  for (const line of parts) {
    for (const position of line) {
      writePosition(coords, vertex++, position);
    }
  }
  return { coords, offsets };
}

/** Converts any accepted {@link Polygons} form to the flat two-level CSR layout, as typed arrays. */
export function flattenPolygons(polygons: Polygons): {
  coords: Float64Array;
  ringOffsets: Uint32Array;
  polygonOffsets: Uint32Array;
} {
  if (!Array.isArray(polygons)) {
    const arrays = polygons as PolygonArrays;
    return {
      coords: toFloat64(arrays.coords),
      ringOffsets: toUint32(arrays.ringOffsets),
      polygonOffsets: toUint32(arrays.polygonOffsets),
    };
  }
  const parts = polygons as ReadonlyArray<ReadonlyArray<ReadonlyArray<Position>>>;
  let ringCount = 0;
  let vertexCount = 0;
  for (const polygon of parts) {
    ringCount += polygon.length;
    for (const ring of polygon) {
      vertexCount += ring.length;
    }
  }
  const polygonOffsets = new Uint32Array(parts.length + 1);
  const ringOffsets = new Uint32Array(ringCount + 1);
  const coords = new Float64Array(vertexCount * 2);
  let ring = 0;
  let vertex = 0;
  for (let i = 0; i < parts.length; i++) {
    polygonOffsets[i] = ring;
    for (const positions of parts[i]) {
      ringOffsets[ring++] = vertex;
      for (const position of positions) {
        writePosition(coords, vertex++, position);
      }
    }
  }
  polygonOffsets[parts.length] = ring;
  ringOffsets[ringCount] = vertex;
  return { coords, ringOffsets, polygonOffsets };
}

/**
 * The geometries of a GeoJSON input, in order, checked to all be of `type`. Throws a
 * `TypeError` naming the offending position on a `null` geometry or a type mismatch.
 */
export function geoJSONGeometries<G extends { type: string }>(
  input: GeoJSONInput<G>,
  type: G["type"],
): G[] {
  const items = Array.isArray(input)
    ? (input as ReadonlyArray<GeoJSONFeature<G> | G>)
    : (input as GeoJSONFeatureCollection<G>).features;
  const geometries: G[] = [];
  for (let i = 0; i < items.length; i++) {
    const item = items[i];
    const geometry = item.type === "Feature" ? (item as GeoJSONFeature<G>).geometry : (item as G);
    if (geometry === null) {
      throw new TypeError(`GeoJSON feature ${i} has a null geometry`);
    }
    if (geometry.type !== type) {
      throw new TypeError(`GeoJSON geometry ${i} is a ${geometry.type}; expected ${type}`);
    }
    geometries.push(geometry);
  }
  return geometries;
}

function writePosition(flat: Float64Array, index: number, position: Position): void {
  flat[index * 2] = position[0];
  flat[index * 2 + 1] = position[1];
}

function toFloat64(values: Float64Array | readonly number[]): Float64Array {
  return values instanceof Float64Array ? values : Float64Array.from(values);
}

function toUint32(values: Uint32Array | readonly number[]): Uint32Array {
  return values instanceof Uint32Array ? values : Uint32Array.from(values);
}
