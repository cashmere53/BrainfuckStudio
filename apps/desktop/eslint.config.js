import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: [
      "dist",
      "node_modules",
      "src-tauri",
      "test-results",
      "playwright-report",
      "blob-report",
      "e2e/.cache",
    ],
  },
  js.configs.recommended,
  tseslint.configs.recommended,
  {
    files: ["**/*.{ts,tsx}"],
    rules: {
      "@typescript-eslint/no-explicit-any": "error",
    },
  },
  {
    // Playwright のフィクスチャの `async ({}, use) => ...` の形のため(e2e に限る)
    files: ["e2e/**/*.{ts,tsx}"],
    rules: {
      "no-empty-pattern": "off",
    },
  },
);
