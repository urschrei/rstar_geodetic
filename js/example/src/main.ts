/**
 * Wires the globe, the datasets, the wasm tree, and the control panel together.
 */

import { init, PointTree } from "rtree-geodetic";

import { runBenchmark, type BenchResult } from "./bench.js";
import {
  loadCities,
  loadCityMeta,
  randomDataset,
  type CityMeta,
  type Dataset,
} from "./data.js";
import { Globe, pointLayer, type Overlay } from "./globe.js";
import "./style.css";

const MIN_RADIUS_METRES = 10_000;
const MAX_RADIUS_METRES = 5_000_000;

function element<T extends HTMLElement>(id: string): T {
  const node = document.getElementById(id);
  if (!node) {
    throw new Error(`missing #${id}`);
  }
  return node as T;
}

const dom = {
  canvas: element<HTMLCanvasElement>("globe"),
  hud: element<HTMLDivElement>("hud"),
  loading: element<HTMLDivElement>("loading"),
  dataset: element<HTMLSelectElement>("dataset"),
  statCount: element<HTMLElement>("stat-count"),
  statBuild: element<HTMLElement>("stat-build"),
  k: element<HTMLInputElement>("k"),
  kValue: element<HTMLOutputElement>("k-value"),
  radius: element<HTMLInputElement>("radius"),
  radiusValue: element<HTMLOutputElement>("radius-value"),
  wgs84: element<HTMLInputElement>("wgs84"),
  qPoint: element<HTMLElement>("q-point"),
  qNearest: element<HTMLElement>("q-nearest"),
  qGc: element<HTMLElement>("q-gc"),
  qWgs84: element<HTMLElement>("q-wgs84"),
  tNearest: element<HTMLElement>("t-nearest"),
  tNearestN: element<HTMLElement>("t-nearestn"),
  tWithin: element<HTMLElement>("t-within"),
  tWgs84: element<HTMLElement>("t-wgs84"),
  qWithin: element<HTMLElement>("q-within"),
  pinHint: element<HTMLElement>("pin-hint"),
  bench: element<HTMLButtonElement>("bench"),
  benchStatus: element<HTMLElement>("bench-status"),
  benchResults: element<HTMLDListElement>("bench-results"),
};

interface State {
  dataset: Dataset | null;
  tree: PointTree | null;
  meta: CityMeta | null;
  k: number;
  radiusMetres: number;
  wgs84: boolean;
  pinned: [number, number] | null;
  hover: [number, number] | null;
  frameMs: number;
  benchmarking: boolean;
}

const state: State = {
  dataset: null,
  tree: null,
  meta: null,
  k: Number(dom.k.value),
  radiusMetres: 0,
  wgs84: dom.wgs84.checked,
  pinned: null,
  hover: null,
  frameMs: 0,
  benchmarking: false,
};

// Formatting ---------------------------------------------------------------------

const integer = new Intl.NumberFormat("en-GB", { maximumFractionDigits: 0 });

function formatDistance(metres: number): string {
  if (metres < 1000) {
    return `${metres.toFixed(0)} m`;
  }
  const km = metres / 1000;
  return `${km.toLocaleString("en-GB", { maximumSignificantDigits: km < 100 ? 4 : 5 })} km`;
}

/** A distance difference, shown with little precision: "+99 m", "+1.3 km". */
function formatDelta(metres: number): string {
  const sign = metres >= 0 ? "+" : "−";
  const size = Math.abs(metres);
  if (size < 1000) {
    return `${sign}${size.toFixed(0)} m`;
  }
  return `${sign}${(size / 1000).toFixed(1)} km`;
}

function formatMicros(us: number): string {
  if (us < 1000) {
    return `${us.toFixed(us < 10 ? 1 : 0)} µs`;
  }
  return `${(us / 1000).toFixed(2)} ms`;
}

function formatLonLat([lon, lat]: [number, number]): string {
  const ns = lat >= 0 ? "N" : "S";
  const ew = lon >= 0 ? "E" : "W";
  return `${Math.abs(lat).toFixed(3)}°${ns} ${Math.abs(lon).toFixed(3)}°${ew}`;
}

/** Radius slider position (0..1000) to metres on a logarithmic scale. */
function sliderToRadius(value: number): number {
  const t = value / 1000;
  return MIN_RADIUS_METRES * Math.pow(MAX_RADIUS_METRES / MIN_RADIUS_METRES, t);
}

// Timing ----------------------------------------------------------------------------

/**
 * Runs `fn` repeatedly until about half a millisecond has elapsed (or `maxReps`), and
 * returns the last result with the mean time per call in microseconds. Browser timers
 * are coarse, so a single call of a few microseconds cannot be measured directly.
 */
function timed<T>(fn: () => T, maxReps = 64): { value: T; us: number } {
  const start = performance.now();
  let value = fn();
  let reps = 1;
  let elapsed = performance.now() - start;
  while (elapsed < 0.5 && reps < maxReps) {
    value = fn();
    reps++;
    elapsed = performance.now() - start;
  }
  return { value, us: (elapsed * 1000) / reps };
}

// Globe -------------------------------------------------------------------------------

const globe = new Globe(dom.canvas, {
  onHover(lonLat) {
    state.hover = lonLat;
    if (state.pinned) {
      return;
    }
    if (lonLat) {
      query(lonLat);
    } else {
      clearQuery();
    }
  },
  onClick(lonLat) {
    if (state.pinned) {
      state.pinned = null;
      dom.pinHint.textContent = "Click the globe to fix the query point.";
      query(lonLat);
    } else {
      state.pinned = lonLat;
      dom.pinHint.textContent = "Fixed. Click again to release.";
      query(lonLat);
    }
  },
  onFrame(ms) {
    state.frameMs = state.frameMs * 0.8 + ms * 0.2;
    updateHud();
  },
});

function updateHud(): void {
  const count = state.dataset ? integer.format(state.dataset.count) : "0";
  dom.hud.textContent = `${count} points · frame ${state.frameMs.toFixed(1)} ms · drag to rotate, scroll to zoom`;
}

// Queries -------------------------------------------------------------------------------

function describePoint(index: number): string {
  const meta = state.meta;
  if (meta && index < meta.names.length) {
    const name = meta.names[index]!;
    const country = meta.countries[index]!;
    const population = meta.populations[index]!;
    const pop = population > 0 ? `, pop. ${integer.format(population)}` : "";
    return `${name} (${country}${pop})`;
  }
  return `point #${integer.format(index)}`;
}

function query(lonLat: [number, number]): void {
  const tree = state.tree;
  const dataset = state.dataset;
  if (!tree || !dataset) {
    return;
  }
  const [lon, lat] = lonLat;
  const nearest = timed(() => tree.nearestWithDistance(lon, lat));
  const nearestN = timed(() => tree.nearestN(lon, lat, state.k));
  const within = timed(() => tree.withinDistance(lon, lat, state.radiusMetres));
  const wgs84 = state.wgs84 ? timed(() => tree.nearestWithDistanceWgs84(lon, lat)) : null;

  dom.qPoint.textContent = formatLonLat(lonLat);
  const hit = nearest.value;
  if (hit) {
    dom.qNearest.textContent = describePoint(hit.index);
    dom.qGc.textContent = formatDistance(hit.distance);
  } else {
    dom.qNearest.textContent = "—";
    dom.qGc.textContent = "—";
  }
  if (wgs84?.value) {
    const delta = wgs84.value.distance - (hit?.distance ?? 0);
    dom.qWgs84.textContent = `${formatDistance(wgs84.value.distance)} (${formatDelta(delta)})`;
    dom.tWgs84.textContent = formatMicros(wgs84.us);
  } else {
    dom.qWgs84.textContent = state.wgs84 ? "—" : "off";
    dom.tWgs84.textContent = state.wgs84 ? "—" : "off";
  }
  dom.tNearest.textContent = formatMicros(nearest.us);
  dom.tNearestN.textContent = `${formatMicros(nearestN.us)} (k = ${state.k})`;
  dom.tWithin.textContent = formatMicros(within.us);
  dom.qWithin.textContent = `${integer.format(within.value.indices.length)} points`;

  const indices = nearestN.value.indices;
  const neighbourCoords = new Float64Array(2 * indices.length);
  for (let j = 0; j < indices.length; j++) {
    neighbourCoords[2 * j] = dataset.coords[2 * indices[j]!]!;
    neighbourCoords[2 * j + 1] = dataset.coords[2 * indices[j]! + 1]!;
  }
  const overlay: Overlay = {
    query: lonLat,
    radiusMetres: state.radiusMetres,
    within: within.value.indices,
    neighbours: indices,
    neighbourCoords,
  };
  globe.setOverlay(overlay);
}

function clearQuery(): void {
  globe.setOverlay(null);
  dom.qPoint.textContent = "hover over the globe";
  for (const node of [
    dom.qNearest,
    dom.qGc,
    dom.qWgs84,
    dom.tNearest,
    dom.tNearestN,
    dom.tWithin,
    dom.tWgs84,
    dom.qWithin,
  ]) {
    node.textContent = "—";
  }
}

function requery(): void {
  const point = state.pinned ?? state.hover;
  if (point) {
    query(point);
  }
}

// Datasets -------------------------------------------------------------------------------

async function useDataset(dataset: Dataset): Promise<void> {
  dom.loading.hidden = false;
  dom.loading.textContent = `Building a tree over ${integer.format(dataset.count)} points…`;
  // Let the overlay paint before the synchronous build.
  await new Promise((resolve) => requestAnimationFrame(() => setTimeout(resolve, 0)));
  state.tree?.free();
  state.tree = null;
  globe.setOverlay(null);
  const start = performance.now();
  const tree = new PointTree(dataset.coords);
  const buildMs = performance.now() - start;
  state.tree = tree;
  state.dataset = dataset;
  globe.setPoints(pointLayer(dataset.coords, dataset.brightCount));
  dom.statCount.textContent = `${integer.format(dataset.count)} points`;
  dom.statBuild.textContent = `tree built in ${buildMs.toFixed(0)} ms`;
  dom.loading.hidden = true;
  updateHud();
  requery();
}

async function selectDataset(value: string): Promise<void> {
  if (value === "cities") {
    const dataset = await loadCities();
    dataset.meta = state.meta ?? undefined;
    await useDataset(dataset);
    if (!state.meta) {
      loadCityMeta().then((meta) => {
        state.meta = meta;
        if (state.dataset?.label.startsWith("GeoNames")) {
          state.dataset.meta = meta;
          requery();
        }
      });
    }
    return;
  }
  const [, count] = value.split(":");
  await useDataset(randomDataset(Number(count)));
}

// Benchmark ----------------------------------------------------------------------------------

/** Queries per second, compactly: "360k/s". */
function formatRate(us: number): string {
  const perSecond = 1e6 / us;
  if (perSecond >= 1e6) {
    return `${(perSecond / 1e6).toFixed(1)}M/s`;
  }
  if (perSecond >= 1e3) {
    return `${(perSecond / 1e3).toFixed(0)}k/s`;
  }
  return `${perSecond.toFixed(0)}/s`;
}

function renderBenchmark(result: BenchResult): void {
  // The last row is the comparison, set apart by a rule.
  const rows: [string, number, boolean][] = [
    ["nearest", result.nearestUs, false],
    [`nearestN (k=${result.k})`, result.nearestNUs, false],
    ["withinDistance", result.withinUs, false],
    ["nearest (without tree)", result.bruteUs, true],
  ];
  dom.benchResults.replaceChildren(
    ...rows.flatMap(([label, us, divider]) => {
      const dt = document.createElement("dt");
      dt.className = divider ? "kv__code kv__divider" : "kv__code";
      dt.textContent = label;
      const dd = document.createElement("dd");
      dd.className = divider ? "mono kv__divider" : "mono";
      dd.textContent = `${formatMicros(us)} · ${formatRate(us)}`;
      return [dt, dd];
    }),
  );
  dom.benchResults.hidden = false;
  dom.benchStatus.textContent = `${integer.format(result.queries)} queries over ${integer.format(result.n)} points (linear scan: ${integer.format(result.bruteQueries)}); ${result.withinMeanHits.toFixed(0)} hits per radius query.`;
}

async function benchmark(): Promise<void> {
  const tree = state.tree;
  const dataset = state.dataset;
  if (!tree || !dataset || state.benchmarking) {
    return;
  }
  state.benchmarking = true;
  dom.bench.disabled = true;
  try {
    const result = await runBenchmark(
      tree,
      dataset.coords,
      { queries: 10_000, k: state.k, radiusMetres: state.radiusMetres },
      (message) => {
        dom.benchStatus.textContent = `Running: ${message}…`;
      },
    );
    renderBenchmark(result);
  } catch (error) {
    dom.benchStatus.textContent = `Benchmark failed: ${String(error)}`;
  } finally {
    state.benchmarking = false;
    dom.bench.disabled = false;
  }
}

// Controls -----------------------------------------------------------------------------------

function bindControls(): void {
  dom.k.addEventListener("input", () => {
    state.k = Number(dom.k.value);
    dom.kValue.value = String(state.k);
    requery();
  });
  const updateRadius = (): void => {
    state.radiusMetres = sliderToRadius(Number(dom.radius.value));
    dom.radiusValue.value = formatDistance(state.radiusMetres);
  };
  updateRadius();
  dom.radius.addEventListener("input", () => {
    updateRadius();
    requery();
  });
  dom.wgs84.addEventListener("change", () => {
    state.wgs84 = dom.wgs84.checked;
    requery();
  });
  dom.dataset.addEventListener("change", () => {
    selectDataset(dom.dataset.value).catch(showError);
  });
  dom.bench.addEventListener("click", () => {
    benchmark().catch(showError);
  });
}

function showError(error: unknown): void {
  dom.loading.hidden = false;
  dom.loading.textContent = `Something went wrong: ${String(error)}`;
  console.error(error);
}

async function main(): Promise<void> {
  bindControls();
  await init();
  await selectDataset(dom.dataset.value);
}

main().catch(showError);
