/**
 * An orthographic globe on a canvas: land and graticule through d3-geo, a point cloud
 * projected by hand from precomputed unit vectors (fast enough for a million points
 * per frame), and a query overlay of great-circle arcs and a spherical cap. Drag to
 * rotate (with inertia), scroll to zoom, arrow keys to rotate from the keyboard.
 */

import {
  geoCircle,
  geoGraticule10,
  geoOrthographic,
  geoPath,
  geoRotation,
  type GeoPath,
  type GeoProjection,
} from "d3-geo";
import { feature } from "topojson-client";
import type { GeometryCollection, Topology } from "topojson-specification";
import land110 from "world-atlas/land-110m.json";

import {
  angle,
  cartesian,
  conjugate,
  delta,
  fromEuler,
  multiply,
  toEuler,
  withAngle,
  type Euler,
  type Quaternion,
  type Vec3,
} from "./versor.js";

/** Mean Earth radius used by rtree-geodetic's great-circle distances, in metres. */
export const EARTH_RADIUS_METRES = 6_371_008.8;

const DEGREES = 180 / Math.PI;
const RADIANS = Math.PI / 180;

const topology = land110 as unknown as Topology;
const LAND = feature(topology, topology.objects.land as GeometryCollection);
const GRATICULE = geoGraticule10();
const SPHERE = { type: "Sphere" } as const;

/** Canvas colours; the page's CSS variables are the source of truth for the chrome. */
const COLOURS = {
  ocean: "#0b1220",
  land: "#1b2a41",
  landStroke: "#2f4466",
  graticule: "rgba(120, 150, 200, 0.12)",
  horizon: "rgba(140, 170, 230, 0.35)",
  // Pixel colours are ABGR, little-endian, for the Uint32 view of the image buffer.
  point: 0xffb88f6f, // #6f8fb8, a muted blue
  pointBright: 0xfff5e2d5, // #d5e2f5
  within: 0xff3dc4ff, // #ffc43d
  cap: "rgba(255, 196, 61, 0.12)",
  capStroke: "rgba(255, 196, 61, 0.9)",
  arc: "rgba(94, 234, 212, 0.85)",
  neighbour: "#5eead4",
  nearest: "#ff6b6b",
  query: "#ffffff",
};

/** A point cloud as Cartesian unit vectors, one component per array. */
export interface PointLayer {
  x: Float32Array;
  y: Float32Array;
  z: Float32Array;
  count: number;
  /** Points with index below this are drawn brighter (for example, large cities). */
  brightCount: number;
}

/** Builds a point layer from interleaved `[lon, lat, ...]` degrees. */
export function pointLayer(coords: ArrayLike<number>, brightCount = 0): PointLayer {
  const count = coords.length >> 1;
  const x = new Float32Array(count);
  const y = new Float32Array(count);
  const z = new Float32Array(count);
  for (let i = 0; i < count; i++) {
    const lon = coords[2 * i]! * RADIANS;
    const lat = coords[2 * i + 1]! * RADIANS;
    const cosLat = Math.cos(lat);
    x[i] = cosLat * Math.cos(lon);
    y[i] = cosLat * Math.sin(lon);
    z[i] = Math.sin(lat);
  }
  return { x, y, z, count, brightCount };
}

/** What to draw on top of the point cloud for the current query. */
export interface Overlay {
  /** The query point as `[lon, lat]`. */
  query: [number, number];
  /** Radius of the within-distance cap, in metres. */
  radiusMetres: number;
  /** Input positions of the points inside the cap. */
  within: ArrayLike<number>;
  /** Input positions of the k nearest points, nearest first. */
  neighbours: ArrayLike<number>;
  /** `[lon, lat]` of each neighbour, for the arcs. */
  neighbourCoords: ArrayLike<number>;
}

export interface GlobeCallbacks {
  /** The pointer is over `[lon, lat]`, or has left the sphere. */
  onHover(lonLat: [number, number] | null): void;
  /** The sphere was clicked (without dragging) at `[lon, lat]`. */
  onClick(lonLat: [number, number]): void;
  /** Called after each frame with its duration in milliseconds. */
  onFrame?(ms: number): void;
}

/** Milliseconds per inertia step at which the rotation velocity halves. */
const INERTIA_HALF_LIFE_MS = 250;
const MIN_SCALE_FACTOR = 0.7;
const MAX_SCALE_FACTOR = 12;

export class Globe {
  readonly projection: GeoProjection;
  readonly #canvas: HTMLCanvasElement;
  readonly #ctx: CanvasRenderingContext2D;
  readonly #path: GeoPath;
  readonly #callbacks: GlobeCallbacks;

  #points: PointLayer | null = null;
  #overlay: Overlay | null = null;
  #dpr = 1;
  #width = 0;
  #height = 0;
  #baseScale = 1;
  #scaleFactor = 1;

  /** Per-point projected device-pixel positions from the last frame, for highlights. */
  #screenX = new Float32Array(0);
  #screenY = new Float32Array(0);
  #visible = new Uint8Array(0);
  /** The point layer is drawn into this buffer and composited onto the canvas. */
  #pointImage: ImageData | null = null;
  #pointBuffer: Uint32Array | null = null;
  #pointCanvas: OffscreenCanvas | HTMLCanvasElement | null = null;

  #frameRequested = false;
  #pointerDown = false;
  #dragging = false;
  #dragStart: { v0: Vec3; q0: Quaternion; r0: Euler; x: number; y: number } | null =
    null;
  #lastMove: { q: Quaternion; time: number } | null = null;
  /**
   * Coasting after a drag: the last incremental rotation between two pointer events,
   * its angular speed in radians per millisecond, and when it started decaying.
   */
  #inertia: { step: Quaternion; speed: number; start: number; lastFrame: number } | null =
    null;

  constructor(canvas: HTMLCanvasElement, callbacks: GlobeCallbacks) {
    this.#canvas = canvas;
    this.#callbacks = callbacks;
    const ctx = canvas.getContext("2d", { alpha: false });
    if (!ctx) {
      throw new Error("2D canvas context unavailable");
    }
    this.#ctx = ctx;
    this.projection = geoOrthographic().rotate([-10, -40, 0]).clipAngle(90).precision(0.5);
    this.#path = geoPath(this.projection, ctx);
    this.resize();
    this.#bindEvents();
  }

  /** Replaces the point cloud. */
  setPoints(points: PointLayer | null): void {
    this.#points = points;
    const count = points?.count ?? 0;
    this.#screenX = new Float32Array(count);
    this.#screenY = new Float32Array(count);
    this.#visible = new Uint8Array(count);
    this.requestFrame();
  }

  /** Replaces the query overlay. */
  setOverlay(overlay: Overlay | null): void {
    this.#overlay = overlay;
    this.requestFrame();
  }

  /** Rotates so that `[lon, lat]` faces the viewer. */
  rotateTo(lonLat: [number, number]): void {
    this.projection.rotate([-lonLat[0], -lonLat[1], 0]);
    this.requestFrame();
  }

  /** Rotates by `[dLon, dLat]` degrees (for the keyboard). */
  rotateBy(dLon: number, dLat: number): void {
    const [l, p, g] = this.projection.rotate();
    this.projection.rotate([l + dLon, Math.max(-90, Math.min(90, p + dLat)), g]);
    this.requestFrame();
  }

  zoomBy(factor: number): void {
    this.#scaleFactor = Math.max(
      MIN_SCALE_FACTOR,
      Math.min(MAX_SCALE_FACTOR, this.#scaleFactor * factor),
    );
    this.projection.scale(this.#baseScale * this.#scaleFactor);
    this.requestFrame();
  }

  /** The `[lon, lat]` under the CSS pixel `(x, y)`, or `null` off the sphere. */
  invert(x: number, y: number): [number, number] | null {
    const [tx, ty] = this.projection.translate();
    const r = this.projection.scale();
    if ((x - tx) ** 2 + (y - ty) ** 2 > r * r) {
      return null;
    }
    const p = this.projection.invert?.([x, y]);
    if (!p || !Number.isFinite(p[0]) || !Number.isFinite(p[1])) {
      return null;
    }
    return [p[0], p[1]];
  }

  /** Fits the canvas to its element and the device pixel ratio. */
  resize(): void {
    const rect = this.#canvas.getBoundingClientRect();
    this.#dpr = Math.min(window.devicePixelRatio || 1, 2);
    this.#width = Math.max(1, Math.round(rect.width));
    this.#height = Math.max(1, Math.round(rect.height));
    this.#canvas.width = Math.round(this.#width * this.#dpr);
    this.#canvas.height = Math.round(this.#height * this.#dpr);
    this.#baseScale = Math.min(this.#width, this.#height) * 0.47;
    this.projection
      .translate([this.#width / 2, this.#height / 2])
      .scale(this.#baseScale * this.#scaleFactor);
    this.#pointImage = null;
    this.requestFrame();
  }

  requestFrame(): void {
    if (this.#frameRequested) {
      return;
    }
    this.#frameRequested = true;
    requestAnimationFrame(() => {
      this.#frameRequested = false;
      this.#stepInertia();
      const start = performance.now();
      this.#render();
      this.#callbacks.onFrame?.(performance.now() - start);
    });
  }

  #render(): void {
    const ctx = this.#ctx;
    const dpr = this.#dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.fillStyle = "#05080f";
    ctx.fillRect(0, 0, this.#width, this.#height);

    ctx.beginPath();
    this.#path(SPHERE);
    ctx.fillStyle = COLOURS.ocean;
    ctx.fill();

    ctx.beginPath();
    this.#path(GRATICULE);
    ctx.strokeStyle = COLOURS.graticule;
    ctx.lineWidth = 0.5;
    ctx.stroke();

    ctx.beginPath();
    this.#path(LAND);
    ctx.fillStyle = COLOURS.land;
    ctx.fill();
    ctx.strokeStyle = COLOURS.landStroke;
    ctx.lineWidth = 0.6;
    ctx.stroke();

    this.#renderPoints();

    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    this.#renderOverlay();

    ctx.beginPath();
    this.#path(SPHERE);
    ctx.strokeStyle = COLOURS.horizon;
    ctx.lineWidth = 1;
    ctx.stroke();
  }

  /**
   * Projects every point with the same rotation d3 applies, writing pixels straight
   * into an image buffer. After rotation, d3's orthographic projection maps the unit
   * vector `(x', y', z')` to `[tx + k y', ty - k z']` and shows it when `x' > 0`.
   */
  #renderPoints(): void {
    const points = this.#points;
    if (!points || points.count === 0) {
      return;
    }
    const dpr = this.#dpr;
    const w = this.#canvas.width;
    const h = this.#canvas.height;
    if (!this.#pointImage || this.#pointImage.width !== w || this.#pointImage.height !== h) {
      this.#pointImage = new ImageData(w, h);
      this.#pointBuffer = new Uint32Array(this.#pointImage.data.buffer);
      const offscreen =
        typeof OffscreenCanvas === "undefined"
          ? document.createElement("canvas")
          : new OffscreenCanvas(w, h);
      offscreen.width = w;
      offscreen.height = h;
      this.#pointCanvas = offscreen;
    }
    const buffer = this.#pointBuffer!;
    buffer.fill(0);

    // Columns of the rotation matrix: the images of the three basis vectors.
    const rotate = geoRotation(this.projection.rotate());
    const c0 = cartesian(rotate([0, 0]) as [number, number]);
    const c1 = cartesian(rotate([90, 0]) as [number, number]);
    const c2 = cartesian(rotate([0, 90]) as [number, number]);
    const [tx0, ty0] = this.projection.translate();
    const k = this.projection.scale() * dpr;
    const tx = tx0 * dpr;
    const ty = ty0 * dpr;
    const size = dpr >= 2 ? 2 : 1;

    const { x, y, z, count, brightCount } = points;
    const sx = this.#screenX;
    const sy = this.#screenY;
    const visible = this.#visible;
    for (let i = 0; i < count; i++) {
      const px = x[i]!;
      const py = y[i]!;
      const pz = z[i]!;
      const depth = c0[0] * px + c1[0] * py + c2[0] * pz;
      if (depth < 0) {
        visible[i] = 0;
        continue;
      }
      const ry = c0[1] * px + c1[1] * py + c2[1] * pz;
      const rz = c0[2] * px + c1[2] * py + c2[2] * pz;
      const fx = tx + k * ry;
      const fy = ty - k * rz;
      sx[i] = fx;
      sy[i] = fy;
      const ix = fx | 0;
      const iy = fy | 0;
      if (ix < 0 || iy < 0 || ix >= w - size || iy >= h - size) {
        visible[i] = 0;
        continue;
      }
      visible[i] = 1;
      const colour = i < brightCount ? COLOURS.pointBright : COLOURS.point;
      const base = iy * w + ix;
      buffer[base] = colour;
      if (size === 2) {
        buffer[base + 1] = colour;
        buffer[base + w] = colour;
        buffer[base + w + 1] = colour;
      }
    }

    // Points inside the query cap, re-plotted in the highlight colour.
    const within = this.#overlay?.within;
    if (within) {
      for (let j = 0; j < within.length; j++) {
        const i = within[j]!;
        if (!visible[i]) {
          continue;
        }
        const base = (sy[i]! | 0) * w + (sx[i]! | 0);
        buffer[base] = COLOURS.within;
        if (size === 2) {
          buffer[base + 1] = COLOURS.within;
          buffer[base + w] = COLOURS.within;
          buffer[base + w + 1] = COLOURS.within;
        }
      }
    }

    const offscreen = this.#pointCanvas!;
    const octx = offscreen.getContext("2d") as
      | CanvasRenderingContext2D
      | OffscreenCanvasRenderingContext2D;
    octx.putImageData(this.#pointImage, 0, 0);
    this.#ctx.setTransform(1, 0, 0, 1, 0, 0);
    this.#ctx.drawImage(offscreen, 0, 0);
  }

  #renderOverlay(): void {
    const overlay = this.#overlay;
    if (!overlay) {
      return;
    }
    const ctx = this.#ctx;
    const dpr = this.#dpr;
    const [qx, qy] = this.projection(overlay.query) ?? [NaN, NaN];

    // The within-distance cap: a spherical circle of the query's angular radius.
    const radiusDegrees = (overlay.radiusMetres / EARTH_RADIUS_METRES) * DEGREES;
    const cap = geoCircle().center(overlay.query).radius(radiusDegrees).precision(0.5)();
    ctx.beginPath();
    this.#path(cap);
    ctx.fillStyle = COLOURS.cap;
    ctx.fill();
    ctx.strokeStyle = COLOURS.capStroke;
    ctx.lineWidth = 1.2;
    ctx.stroke();

    // Great-circle arcs to the k nearest; d3 resamples the straight segment along the
    // geodesic, so the arc bends correctly across the seam and near the poles.
    const coords = overlay.neighbourCoords;
    ctx.strokeStyle = COLOURS.arc;
    ctx.lineWidth = 1.2;
    for (let j = 0; j < overlay.neighbours.length; j++) {
      const lon = coords[2 * j]!;
      const lat = coords[2 * j + 1]!;
      ctx.beginPath();
      this.#path({ type: "LineString", coordinates: [overlay.query, [lon, lat]] });
      ctx.stroke();
    }
    const sx = this.#screenX;
    const sy = this.#screenY;
    const visible = this.#visible;
    // Farthest first, so the nearest point is drawn on top of any it overlaps.
    for (let j = overlay.neighbours.length - 1; j >= 0; j--) {
      const i = overlay.neighbours[j]!;
      if (!visible[i]) {
        continue;
      }
      const x = sx[i]! / dpr;
      const y = sy[i]! / dpr;
      ctx.fillStyle = j === 0 ? COLOURS.nearest : COLOURS.neighbour;
      ctx.strokeStyle = COLOURS.ocean;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.arc(x, y, j === 0 ? 4 : 2.5, 0, 2 * Math.PI);
      ctx.fill();
      ctx.stroke();
      if (j === 0) {
        ctx.beginPath();
        ctx.arc(x, y, 7.5, 0, 2 * Math.PI);
        ctx.strokeStyle = COLOURS.nearest;
        ctx.lineWidth = 1.5;
        ctx.stroke();
      }
    }

    if (Number.isFinite(qx) && Number.isFinite(qy)) {
      ctx.beginPath();
      ctx.arc(qx, qy, 5, 0, 2 * Math.PI);
      ctx.strokeStyle = COLOURS.query;
      ctx.lineWidth = 1.5;
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(qx, qy, 1.5, 0, 2 * Math.PI);
      ctx.fillStyle = COLOURS.query;
      ctx.fill();
    }
  }

  // Interaction -----------------------------------------------------------------

  #bindEvents(): void {
    const canvas = this.#canvas;
    canvas.addEventListener("pointerdown", (event) => this.#onPointerDown(event));
    canvas.addEventListener("pointermove", (event) => this.#onPointerMove(event));
    canvas.addEventListener("pointerup", (event) => this.#onPointerUp(event));
    canvas.addEventListener("pointercancel", (event) => this.#onPointerUp(event));
    canvas.addEventListener("pointerleave", () => {
      if (!this.#pointerDown) {
        this.#callbacks.onHover(null);
      }
    });
    canvas.addEventListener(
      "wheel",
      (event) => {
        event.preventDefault();
        this.zoomBy(Math.exp(-event.deltaY * 0.0015));
      },
      { passive: false },
    );
    canvas.addEventListener("keydown", (event) => {
      const step = event.shiftKey ? 15 : 5;
      switch (event.key) {
        case "ArrowLeft":
          this.rotateBy(-step, 0);
          break;
        case "ArrowRight":
          this.rotateBy(step, 0);
          break;
        case "ArrowUp":
          this.rotateBy(0, -step);
          break;
        case "ArrowDown":
          this.rotateBy(0, step);
          break;
        case "+":
        case "=":
          this.zoomBy(1.25);
          break;
        case "-":
          this.zoomBy(0.8);
          break;
        default:
          return;
      }
      event.preventDefault();
    });
    const observer = new ResizeObserver(() => this.resize());
    observer.observe(canvas);
  }

  #pointerPosition(event: PointerEvent): [number, number] {
    const rect = this.#canvas.getBoundingClientRect();
    return [event.clientX - rect.left, event.clientY - rect.top];
  }

  #onPointerDown(event: PointerEvent): void {
    if (event.button !== 0) {
      return;
    }
    const [x, y] = this.#pointerPosition(event);
    const lonLat = this.invert(x, y);
    if (!lonLat) {
      return;
    }
    this.#inertia = null;
    this.#pointerDown = true;
    this.#dragging = false;
    this.#canvas.setPointerCapture(event.pointerId);
    this.#dragStart = {
      v0: cartesian(lonLat),
      q0: fromEuler(this.projection.rotate()),
      r0: this.projection.rotate(),
      x,
      y,
    };
    this.#lastMove = null;
  }

  #onPointerMove(event: PointerEvent): void {
    const [x, y] = this.#pointerPosition(event);
    if (this.#pointerDown && this.#dragStart) {
      const start = this.#dragStart;
      if (!this.#dragging && Math.hypot(x - start.x, y - start.y) < 3) {
        return;
      }
      this.#dragging = true;
      this.#canvas.classList.add("is-dragging");
      // Invert under the rotation at drag start, then compose the quaternions.
      this.projection.rotate(start.r0);
      const p = this.projection.invert?.([x, y]);
      if (!p || !Number.isFinite(p[0])) {
        this.projection.rotate(toEuler(start.q0));
        return;
      }
      const v1 = cartesian([p[0], p[1]]);
      const d = delta(start.v0, v1);
      const q1 = multiply(start.q0, d);
      this.projection.rotate(toEuler(q1));
      const now = performance.now();
      if (this.#lastMove && now > this.#lastMove.time) {
        const step = multiply(conjugate(this.#lastMove.q), q1);
        this.#inertia = {
          step,
          speed: angle(step) / (now - this.#lastMove.time),
          start: now,
          lastFrame: now,
        };
      }
      this.#lastMove = { q: q1, time: now };
      // Near the antipode of the start the rotation is unstable; restart from here.
      if (d[0] < 0.7) {
        this.#dragStart = { v0: v1, q0: q1, r0: this.projection.rotate(), x, y };
      }
      this.requestFrame();
      this.#callbacks.onHover(this.invert(x, y));
      return;
    }
    this.#callbacks.onHover(this.invert(x, y));
  }

  #onPointerUp(event: PointerEvent): void {
    if (!this.#pointerDown) {
      return;
    }
    this.#pointerDown = false;
    this.#canvas.classList.remove("is-dragging");
    if (this.#canvas.hasPointerCapture(event.pointerId)) {
      this.#canvas.releasePointerCapture(event.pointerId);
    }
    const [x, y] = this.#pointerPosition(event);
    if (!this.#dragging) {
      this.#inertia = null;
      const lonLat = this.invert(x, y);
      if (lonLat) {
        this.#callbacks.onClick(lonLat);
      }
      return;
    }
    this.#dragging = false;
    // Only coast if the pointer was still moving when released.
    if (this.#inertia && performance.now() - this.#inertia.start < 80) {
      this.#inertia.start = this.#inertia.lastFrame = performance.now();
      this.requestFrame();
    } else {
      this.#inertia = null;
    }
  }

  /** Applies one frame of coasting rotation after a drag is released. */
  #stepInertia(): void {
    const inertia = this.#inertia;
    if (!inertia || this.#pointerDown) {
      return;
    }
    const now = performance.now();
    const speed = inertia.speed * Math.pow(0.5, (now - inertia.start) / INERTIA_HALF_LIFE_MS);
    if (speed < 1e-5) {
      this.#inertia = null;
      return;
    }
    const frameMs = Math.min(50, now - inertia.lastFrame);
    inertia.lastFrame = now;
    const q = multiply(
      fromEuler(this.projection.rotate()),
      withAngle(inertia.step, speed * frameMs),
    );
    this.projection.rotate(toEuler(q));
    this.requestFrame();
  }
}
