/**
 * Throughput measurements: rtree-geodetic queries against a brute-force scan in
 * JavaScript. Each phase yields to the browser so the page stays responsive.
 */

import type { PointTree } from "rtree-geodetic";

import { randomCoords } from "./data.js";

export interface BenchOptions {
  /** Number of random `nearest` queries. */
  queries: number;
  k: number;
  radiusMetres: number;
}

export interface BenchResult {
  n: number;
  queries: number;
  /** Mean microseconds per query. */
  nearestUs: number;
  nearestNUs: number;
  k: number;
  withinUs: number;
  withinQueries: number;
  /** Mean number of hits per within-distance query. */
  withinMeanHits: number;
  radiusMetres: number;
  bruteUs: number;
  bruteQueries: number;
  /** Brute-force time over `nearest` time. */
  speedup: number;
}

/** Lets the browser paint between phases. */
function yieldToBrowser(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => setTimeout(resolve, 0));
  });
}

/**
 * The position of the point with the largest dot product with the query's unit vector,
 * which is the nearest by great-circle distance. Unit vectors are precomputed once, the
 * same preprocessing the tree gets.
 */
function bruteForce(
  x: Float64Array,
  y: Float64Array,
  z: Float64Array,
  lon: number,
  lat: number,
): number {
  const RADIANS = Math.PI / 180;
  const cosLat = Math.cos(lat * RADIANS);
  const qx = cosLat * Math.cos(lon * RADIANS);
  const qy = cosLat * Math.sin(lon * RADIANS);
  const qz = Math.sin(lat * RADIANS);
  let best = -2;
  let bestIndex = -1;
  for (let i = 0; i < x.length; i++) {
    const d = qx * x[i]! + qy * y[i]! + qz * z[i]!;
    if (d > best) {
      best = d;
      bestIndex = i;
    }
  }
  return bestIndex;
}

export async function runBenchmark(
  tree: PointTree,
  coords: Float64Array,
  options: BenchOptions,
  progress: (message: string) => void,
): Promise<BenchResult> {
  const n = coords.length >> 1;
  const queries = randomCoords(options.queries, 7);
  let sink = 0;

  progress(`nearest: ${options.queries.toLocaleString("en-GB")} queries`);
  await yieldToBrowser();
  let start = performance.now();
  for (let i = 0; i < options.queries; i++) {
    sink += tree.nearest(queries[2 * i]!, queries[2 * i + 1]!) ?? 0;
  }
  const nearestUs = ((performance.now() - start) * 1000) / options.queries;

  progress(`nearestN (k = ${options.k}): ${options.queries.toLocaleString("en-GB")} queries`);
  await yieldToBrowser();
  start = performance.now();
  for (let i = 0; i < options.queries; i++) {
    sink += tree.nearestN(queries[2 * i]!, queries[2 * i + 1]!, options.k).indices.length;
  }
  const nearestNUs = ((performance.now() - start) * 1000) / options.queries;

  // Within-distance cost grows with the hit count, so this phase is bounded by time
  // (half a second) as well as by query count.
  const maxWithinQueries = Math.max(50, Math.round(options.queries / 20));
  progress(`withinDistance: up to ${maxWithinQueries.toLocaleString("en-GB")} queries`);
  await yieldToBrowser();
  let hits = 0;
  let withinQueries = 0;
  start = performance.now();
  while (withinQueries < maxWithinQueries) {
    const i = withinQueries++;
    hits += tree.withinDistance(queries[2 * i]!, queries[2 * i + 1]!, options.radiusMetres)
      .indices.length;
    if ((withinQueries & 15) === 0 && performance.now() - start > 500) {
      break;
    }
  }
  const withinUs = ((performance.now() - start) * 1000) / withinQueries;

  progress("brute force in JavaScript");
  await yieldToBrowser();
  const x = new Float64Array(n);
  const y = new Float64Array(n);
  const z = new Float64Array(n);
  const RADIANS = Math.PI / 180;
  for (let i = 0; i < n; i++) {
    const lon = coords[2 * i]! * RADIANS;
    const lat = coords[2 * i + 1]! * RADIANS;
    const cosLat = Math.cos(lat);
    x[i] = cosLat * Math.cos(lon);
    y[i] = cosLat * Math.sin(lon);
    z[i] = Math.sin(lat);
  }
  // Enough queries to fill a few hundred milliseconds at most.
  const bruteQueries = Math.max(20, Math.min(200, Math.round(5e7 / n)));
  start = performance.now();
  for (let i = 0; i < bruteQueries; i++) {
    sink += bruteForce(x, y, z, queries[2 * i]!, queries[2 * i + 1]!);
  }
  const bruteUs = ((performance.now() - start) * 1000) / bruteQueries;
  if (!Number.isFinite(sink)) {
    throw new Error("benchmark produced a non-finite checksum");
  }

  return {
    n,
    queries: options.queries,
    nearestUs,
    nearestNUs,
    k: options.k,
    withinUs,
    withinQueries,
    withinMeanHits: hits / withinQueries,
    radiusMetres: options.radiusMetres,
    bruteUs,
    bruteQueries,
    speedup: bruteUs / nearestUs,
  };
}
