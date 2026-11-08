import { defineConfig, mergeConfig } from "vitest/config";
import viteConfig from "./vite.config.ts";

export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: "jsdom",
      setupFiles: ["./src/test/setup.ts"],
      include: [
        "src/**/*.test.ts",
        "src/**/*.test.tsx",
        "e2e/support/**/*.test.ts",
      ],
    },
  }),
);
