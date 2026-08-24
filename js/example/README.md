# rtree-geodetic demo

An interactive globe that runs `rtree-geodetic` queries in the browser.

The globe is an orthographic projection drawn with d3-geo on a canvas. The point cloud
(GeoNames populated places, or up to two million random points) is projected by hand from
precomputed unit vectors each frame. Moving the pointer runs `nearestWithDistance`,
`nearestN`, `withinDistance`, and `nearestWithDistanceWgs84` against a `PointTree`; the
k nearest are joined to the query by great-circle arcs, and the radius query is drawn as
a spherical cap, so it wraps the antimeridian and the poles the way the query does. A
benchmark button times random queries against a brute-force scan in JavaScript.

## Develop

The example links the package from the parent directory, so build that first:

```sh
cd js && npm ci && npm run build
cd example && npm ci
npm run data   # downloads GeoNames cities500 (CC BY 4.0) into public/data/
npm run dev
```

`npm run build` writes the static site to `dist/`, with paths under `/rstar_geodetic/`;
set `BASE_PATH=/` to build for another root.

## Layout

- `src/globe.ts`: projection, rendering, drag with inertia, zoom, keyboard rotation.
- `src/versor.ts`: quaternion helpers for the drag, after Bostock and Rivière's `versor`.
- `src/data.ts`: dataset loading and the random point generator.
- `src/bench.ts`: the throughput measurement.
- `src/main.ts`: the control panel and query wiring.
- `scripts/fetch-data.mjs`: downloads and converts the GeoNames dump.
