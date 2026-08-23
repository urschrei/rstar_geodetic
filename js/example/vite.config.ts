import { defineConfig } from "vite";

// The site is served from https://urschrei.github.io/rstar_geodetic/.
export default defineConfig({
  base: process.env.BASE_PATH ?? "/rstar_geodetic/",
  server: {
    // `rtree-geodetic` is linked from the parent directory, so its built files and the
    // wasm binary are served from outside the project root.
    fs: { allow: [".."] },
  },
  build: {
    target: "es2022",
    sourcemap: true,
  },
});
