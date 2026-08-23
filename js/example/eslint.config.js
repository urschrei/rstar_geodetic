import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist/", "node_modules/", "public/"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["src/**/*.ts"],
    languageOptions: {
      globals: {
        window: "readonly",
        document: "readonly",
        console: "readonly",
        performance: "readonly",
        requestAnimationFrame: "readonly",
        setTimeout: "readonly",
        fetch: "readonly",
        Intl: "readonly",
        ResizeObserver: "readonly",
        OffscreenCanvas: "readonly",
        ImageData: "readonly",
      },
    },
  },
  {
    files: ["scripts/**/*.mjs", "vite.config.ts"],
    languageOptions: {
      globals: {
        process: "readonly",
        console: "readonly",
        fetch: "readonly",
        URL: "readonly",
        Buffer: "readonly",
      },
    },
  },
);
