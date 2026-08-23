/**
 * Unit-quaternion helpers for rotating the globe by dragging, after Mike Bostock's and
 * Philippe Rivière's `versor` (ISC licence). Rotations are d3-geo Euler angles
 * `[lambda, phi, gamma]` in degrees; points are Cartesian unit vectors.
 */

export type Quaternion = [number, number, number, number];
export type Vec3 = [number, number, number];
export type Euler = [number, number, number];

const RADIANS = Math.PI / 180;
const DEGREES = 180 / Math.PI;

/** The quaternion for the Euler rotation `[lambda, phi, gamma]`. */
export function fromEuler(e: Euler): Quaternion {
  const l = (e[0] / 2) * RADIANS;
  const p = (e[1] / 2) * RADIANS;
  const g = (e[2] / 2) * RADIANS;
  const sl = Math.sin(l);
  const cl = Math.cos(l);
  const sp = Math.sin(p);
  const cp = Math.cos(p);
  const sg = Math.sin(g);
  const cg = Math.cos(g);
  return [
    cl * cp * cg + sl * sp * sg,
    sl * cp * cg - cl * sp * sg,
    cl * sp * cg + sl * cp * sg,
    cl * cp * sg - sl * sp * cg,
  ];
}

/** The Euler rotation `[lambda, phi, gamma]` for a quaternion. */
export function toEuler(q: Quaternion): Euler {
  return [
    Math.atan2(2 * (q[0] * q[1] + q[2] * q[3]), 1 - 2 * (q[1] * q[1] + q[2] * q[2])) *
      DEGREES,
    Math.asin(Math.max(-1, Math.min(1, 2 * (q[0] * q[2] - q[3] * q[1])))) * DEGREES,
    Math.atan2(2 * (q[0] * q[3] + q[1] * q[2]), 1 - 2 * (q[2] * q[2] + q[3] * q[3])) *
      DEGREES,
  ];
}

/** The Cartesian unit vector of the spherical point `[lon, lat]` in degrees. */
export function cartesian(lonLat: [number, number]): Vec3 {
  const l = lonLat[0] * RADIANS;
  const p = lonLat[1] * RADIANS;
  const cp = Math.cos(p);
  return [cp * Math.cos(l), cp * Math.sin(l), Math.sin(p)];
}

function dot(a: Vec3, b: Vec3): number {
  return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
}

function cross(a: Vec3, b: Vec3): Vec3 {
  return [
    a[1] * b[2] - a[2] * b[1],
    a[2] * b[0] - a[0] * b[2],
    a[0] * b[1] - a[1] * b[0],
  ];
}

/**
 * The quaternion rotating unit vector `v0` onto `v1`, scaled by `alpha` (1 is the full
 * rotation; smaller values give a partial one, used for inertia).
 */
export function delta(v0: Vec3, v1: Vec3, alpha = 1): Quaternion {
  const w = cross(v0, v1);
  const l = Math.sqrt(dot(w, w));
  if (!l) {
    return [1, 0, 0, 0];
  }
  const t = (alpha * Math.acos(Math.max(-1, Math.min(1, dot(v0, v1))))) / 2;
  const s = Math.sin(t);
  return [Math.cos(t), (w[2] / l) * s, (-w[1] / l) * s, (w[0] / l) * s];
}

/** The product `q0 * q1`. */
export function multiply(q0: Quaternion, q1: Quaternion): Quaternion {
  return [
    q0[0] * q1[0] - q0[1] * q1[1] - q0[2] * q1[2] - q0[3] * q1[3],
    q0[0] * q1[1] + q0[1] * q1[0] + q0[2] * q1[3] - q0[3] * q1[2],
    q0[0] * q1[2] - q0[1] * q1[3] + q0[2] * q1[0] + q0[3] * q1[1],
    q0[0] * q1[3] + q0[1] * q1[2] - q0[2] * q1[1] + q0[3] * q1[0],
  ];
}

/** The inverse of a unit quaternion. */
export function conjugate(q: Quaternion): Quaternion {
  return [q[0], -q[1], -q[2], -q[3]];
}

/** The rotation angle of a unit quaternion, in radians. */
export function angle(q: Quaternion): number {
  return 2 * Math.acos(Math.max(-1, Math.min(1, q[0])));
}

/** The same rotation axis as `q`, through `radians` instead of its own angle. */
export function withAngle(q: Quaternion, radians: number): Quaternion {
  const half = angle(q) / 2;
  const s = Math.sin(half);
  if (s < 1e-12) {
    return [1, 0, 0, 0];
  }
  const c = Math.cos(radians / 2);
  const k = Math.sin(radians / 2) / s;
  return [c, q[1] * k, q[2] * k, q[3] * k];
}
