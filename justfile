# Development recipes for rstar_geodetic. Run `just --list` for the full set.

# Build the C library (cdylib and staticlib) with the ffi and wgs84 features.
ffi-build:
    cargo rustc -p rstar_geodetic --release --features ffi,wgs84 --crate-type cdylib
    cargo rustc -p rstar_geodetic --release --features ffi,wgs84 --crate-type staticlib

# Regenerate the committed C header from the Rust source.
ffi-header:
    cbindgen --config cbindgen.toml --output include/rstar_geodetic.h .

# Fail if the committed header is out of step with the source (CI drift check).
ffi-check-header:
    mkdir -p target
    cbindgen --config cbindgen.toml --output target/rstar_geodetic.generated.h .
    diff -u include/rstar_geodetic.h target/rstar_geodetic.generated.h

# Compile and run the C smoke test against the freshly built cdylib.
ffi-smoke: ffi-build
    cc examples/c/smoke.c -I include -DRSG_HAVE_WGS84 -L target/release -lrstar_geodetic -o target/rsg_smoke
    DYLD_LIBRARY_PATH=target/release LD_LIBRARY_PATH=target/release target/rsg_smoke

# Build the wasm module into js/ts/wasm (size-optimised `wasm-release` profile).
js-build-wasm:
    cd js && npm run build:wasm

# Build the complete npm package into js/dist (wasm module, then TypeScript).
js-build: js-build-wasm
    cd js && npm run build:ts

# Typecheck, lint, and run the vitest suite against the built wasm module.
js-test: js-build-wasm
    cd js && npm run typecheck && npm run lint && npm test
