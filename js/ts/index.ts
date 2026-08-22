/**
 * Browser (and bundler) entry point. The wasm binary is fetched from a URL next to
 * this module, which bundlers such as Vite and webpack rewrite to the emitted asset.
 */

import { WASM_URL, initOnce, type InitInput } from "./init.js";

export * from "./geometry.js";
export * from "./tree.js";
export type { InitInput };

/**
 * Instantiates the wasm module. Must complete before any tree is built. Pass `source`
 * to load the binary from somewhere other than the default URL, for example a
 * `Response` from your own `fetch`, or a `WebAssembly.Module` compiled ahead of time.
 */
export function init(source?: InitInput | Promise<InitInput>): Promise<void> {
  return initOnce(source, () => WASM_URL);
}
