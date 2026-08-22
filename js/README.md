# rtree-geodetic

A geodetic R-tree for JavaScript and TypeScript, compiled to WebAssembly from the Rust
[`rstar_geodetic`](https://github.com/urschrei/rstar_geodetic) crate. It indexes points by
longitude and latitude and answers nearest-neighbour, within-distance, and rectangle
queries by great-circle distance, or by exact WGS84 geodesic distance. The ±180°
antimeridian and the poles need no special handling.

Coordinates are longitude first, latitude second, in degrees. Distances are in metres.
Query results are positions into the input sequence, so they index whatever array of
features or properties the coordinates came from.

## Install

```sh
npm install rtree-geodetic
```

The package ships an ES module and a `.wasm` binary (about 115 kB). It runs in browsers,
in bundlers such as Vite and webpack, and in Node 20 or later; the `node` export
condition selects an entry point that reads the binary from disk, since Node's `fetch`
does not read `file:` URLs.

## Use

The module must be instantiated once, asynchronously, before a tree is built:

```ts
import { init, PointTree } from "rtree-geodetic";

await init();

// Interleaved [lon, lat, lon, lat, ...]; an array of [lon, lat] pairs also works.
const coords = new Float64Array([179.0, -17.0, -175.0, -21.0, -77.0, -12.0]);
const tree = new PointTree(coords);

tree.nearest(-176.0, -21.0);
// 1 (the point at 175°W; the query is just east of the antimeridian)

tree.nearestWithDistance(-176.0, -21.0);
// { index: 1, distance: 103809.38 }

tree.withinDistance(-176.0, -21.0, 1_000_000);
// { indices: Uint32Array [0, 1], distances: Float64Array [...] }

tree.inRectangle(170.0, -30.0, -170.0, 0.0); // minLon > maxLon: the window crosses the seam
// Uint32Array [0, 1]

// Exact WGS84 geodesic distances; the spherical index still drives the search.
tree.nearestWithDistanceWgs84(-176.0, -21.0);
tree.withinDistanceWgs84(-176.0, -21.0, 1_000_000);

tree.free(); // or: using tree = new PointTree(coords);
```

A tree is immutable once built; it lives in wasm memory and is released by `free()` (or
automatically at the end of a `using` block where explicit resource management is
available). Query results are ordinary objects and typed arrays in JavaScript memory and
stay valid after the tree is freed. Invalid input (a longitude or latitude out of range, a
non-finite value, an odd-length coordinate array) throws an `Error` whose `name` is
`"GeodeticError"`.

`init()` accepts an optional source for the binary (a URL, a `Response`, bytes, or a
`WebAssembly.Module`) for deployments that serve it from elsewhere.

## Build from source

Requires a Rust toolchain with the `wasm32-unknown-unknown` target,
[`wasm-pack`](https://github.com/drager/wasm-pack), and Node 20 or later.

```sh
cd js
npm install
npm run build   # wasm-pack, then tsc; output in dist/
npm test        # vitest, against the wasm module built into ts/wasm/
```

The Rust side of the bindings is the `rstar_geodetic_wasm` crate in this directory; the
TypeScript wrapper is in `ts/`.

## Licence

MIT or Apache-2.0, at your option.
