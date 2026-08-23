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
  qWithin: element<HTMLElement>("q-within"),
  pinHint: element<HTMLElement>("pin-hint"),
  bench: element<HTMLButtonElement>("bench"),
  benchStatus: element<HTMLElement>("bench-status"),
  benchTable: element<HTMLTableElement>("bench-table"),
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
      dom.pinHint.textContent = "Click the globe to pin the query point.";
      query(lonLat);
    } else {
      state.pinned = lonLat;
      dom.pinHint.textContent = "Pinned. Click the globe again to follow the pointer.";
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
    const sign = delta >= 0 ? "+" : "−";
    dom.qWgs84.textContent = `${formatDistance(wgs84.value.distance)} (${sign}${formatDistance(Math.abs(delta))}, ${formatMicros(wgs84.us)})`;
  } else {
    dom.qWgs84.textContent = state.wgs84 ? "—" : "off";
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
  dom.statCount.textContent = integer.format(dataset.count);
  dom.statBuild.textContent = `${buildMs.toFixed(0)} ms`;
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

function renderBenchmark(result: BenchResult): void {
  const rows: [string, number][] = [
    ["nearest", result.nearestUs],
    [`nearestN (k = ${result.k})`, result.nearestNUs],
    [
      `withinDistance (${formatDistance(result.radiusMetres)}, ${result.withinMeanHits.toFixed(0)} hits on average)`,
      result.withinUs,
    ],
    ["nearest, brute force in JS", result.bruteUs],
  ];
  const body = dom.benchTable.tBodies[0]!;
  body.replaceChildren(
    ...rows.map(([label, us]) => {
      const tr = document.createElement("tr");
      const th = document.createElement("td");
      th.textContent = label;
      const perQuery = document.createElement("td");
      perQuery.textContent = formatMicros(us);
      const perSecond = document.createElement("td");
      perSecond.textContent = integer.format(1e6 / us);
      tr.append(th, perQuery, perSecond);
      return tr;
    }),
  );
  let foot = dom.benchTable.tFoot;
  if (!foot) {
    foot = dom.benchTable.createTFoot();
  }
  foot.replaceChildren();
  const tr = foot.insertRow();
  const td = tr.insertCell();
  td.colSpan = 3;
  td.textContent = `${integer.format(result.queries)} queries over ${integer.format(result.n)} points; brute force measured over ${integer.format(result.bruteQueries)} queries. The tree answers nearest ${result.speedup.toFixed(0)}× faster.`;
  dom.benchTable.hidden = false;
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
    dom.benchStatus.textContent = "";
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
