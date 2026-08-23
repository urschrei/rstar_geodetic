import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist/", "ts/wasm/", "node_modules/", "example/"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
);
