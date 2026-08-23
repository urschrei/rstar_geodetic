/**
 * Point datasets for the demo: GeoNames populated places fetched at build time by
 * `scripts/fetch-data.mjs`, and uniformly random points on the sphere.
 */

export interface CityMeta {
  names: string[];
  countries: string[];
  populations: Uint32Array;
}

export interface Dataset {
  label: string;
  /** Interleaved `[lon, lat, ...]` in degrees. */
  coords: Float64Array;
  count: number;
  /** Points below this index are drawn brighter. */
  brightCount: number;
  /** Per-point names, when the dataset has them. */
  meta?: CityMeta;
}

interface CityManifest {
  count: number;
  brightCount: number;
  source: string;
  fetched: string;
}

const DATA_URL = `${import.meta.env.BASE_URL}data/`;

async function fetchOk(url: string): Promise<Response> {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`${url}: HTTP ${response.status}`);
  }
  return response;
}

/** Loads the city coordinates. Names come separately through {@link loadCityMeta}. */
export async function loadCities(): Promise<Dataset> {
  const [manifest, buffer] = await Promise.all([
    fetchOk(`${DATA_URL}cities.json`).then((r) => r.json() as Promise<CityManifest>),
    fetchOk(`${DATA_URL}cities.bin`).then((r) => r.arrayBuffer()),
  ]);
  const coords = Float64Array.from(new Float32Array(buffer));
  const count = coords.length >> 1;
  if (count !== manifest.count) {
    throw new Error(`cities.bin holds ${count} points, manifest says ${manifest.count}`);
  }
  return {
    label: "GeoNames populated places",
    coords,
    count,
    brightCount: manifest.brightCount,
  };
}

/** Loads the city names, country codes, and populations, in coordinate order. */
export async function loadCityMeta(): Promise<CityMeta> {
  const text = await fetchOk(`${DATA_URL}cities.tsv`).then((r) => r.text());
  const lines = text.split("\n");
  if (lines.at(-1) === "") {
    lines.pop();
  }
  const names: string[] = new Array(lines.length);
  const countries: string[] = new Array(lines.length);
  const populations = new Uint32Array(lines.length);
  for (let i = 0; i < lines.length; i++) {
    const [name = "", country = "", population = "0"] = lines[i]!.split("\t");
    names[i] = name;
    countries[i] = country;
    populations[i] = Number(population) || 0;
  }
  return { names, countries, populations };
}

/** A small deterministic PRNG (mulberry32), so a dataset size always gives the same points. */
export function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** `n` points uniformly distributed over the sphere, as interleaved `[lon, lat, ...]`. */
export function randomCoords(n: number, seed = 1): Float64Array {
  const random = mulberry32(seed);
  const coords = new Float64Array(2 * n);
  for (let i = 0; i < n; i++) {
    coords[2 * i] = random() * 360 - 180;
    coords[2 * i + 1] = Math.asin(2 * random() - 1) * (180 / Math.PI);
  }
  return coords;
}

export function randomDataset(n: number): Dataset {
  return {
    label: "random points",
    coords: randomCoords(n),
    count: n,
    brightCount: 0,
  };
}
