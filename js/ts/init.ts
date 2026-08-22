/**
 * Shared instantiation logic for the browser and Node entry points.
 */

import wasmInit, { type InitInput } from "./wasm/rstar_geodetic_wasm.js";

export type { InitInput };

/** The wasm binary, resolved relative to the built JavaScript. */
export const WASM_URL = new URL(
  "./wasm/rstar_geodetic_wasm_bg.wasm",
  import.meta.url,
);

let ready: Promise<void> | undefined;

/**
 * Instantiates the wasm module once; later calls return the same promise. `source`
 * is passed to the wasm-bindgen loader, so it may be a URL, a `Response`, bytes, or a
 * compiled `WebAssembly.Module`. `defaultSource` supplies it when the caller gives
 * none.
 */
export function initOnce(
  source: InitInput | Promise<InitInput> | undefined,
  defaultSource: () => InitInput | Promise<InitInput>,
): Promise<void> {
  if (ready === undefined) {
    ready = wasmInit({ module_or_path: source ?? defaultSource() }).then(
      () => undefined,
      (error: unknown) => {
        ready = undefined;
        throw error;
      },
    );
  }
  return ready;
}
