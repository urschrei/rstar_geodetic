/**
 * Node entry point, selected by the `node` export condition. Node's `fetch` does not
 * read `file:` URLs, so the default source is the binary read from disk.
 */

import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { WASM_URL, initOnce, type InitInput } from "./init.js";

export * from "./geometry.js";
export * from "./tree.js";
export type { InitInput };

/**
 * Instantiates the wasm module. Must complete before any tree is built. Pass `source`
 * to load the binary from somewhere other than the package's own copy.
 */
export function init(source?: InitInput | Promise<InitInput>): Promise<void> {
  return initOnce(source, () => readFile(fileURLToPath(WASM_URL)));
}
