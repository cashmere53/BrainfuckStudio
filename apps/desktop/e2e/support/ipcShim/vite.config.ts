// IPC シムのビルド設定。製品の vite.config.ts とは独立し、React のプラグインを含めない。
// entry.ts を IIFE の 1 ファイル e2e/.cache/ipc-shim.js にする(配布物の dist には含めない。3.2)。
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const desktopRoot = fileURLToPath(new URL("../../..", import.meta.url));

export default defineConfig({
  root: desktopRoot,
  publicDir: false,
  build: {
    outDir: "e2e/.cache",
    emptyOutDir: false,
    lib: {
      entry: fileURLToPath(new URL("./entry.ts", import.meta.url)),
      name: "BfE2eIpcShim",
      formats: ["iife"],
      fileName: () => "ipc-shim.js",
    },
  },
});
