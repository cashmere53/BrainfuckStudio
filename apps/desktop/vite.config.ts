import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// リポジトリルート(examples/ を ?raw で取り込めるよう dev サーバの参照を許可する)
const repoRoot = fileURLToPath(new URL("../..", import.meta.url));

export default defineConfig({
  plugins: [react()],
  server: {
    port: 1420,
    strictPort: true,
    fs: { allow: [repoRoot] },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    rolldownOptions: {
      output: {
        // デバッガ画面の組み立てで 1 チャンクが 500 kB を超えるため、依存ごとに分ける(キャッシュ効率も上がる)
        manualChunks(id: string) {
          if (id.includes("node_modules/@codemirror/") || id.includes("node_modules/@lezer/")) {
            return "codemirror";
          }
          if (id.includes("node_modules/react") || id.includes("node_modules/scheduler/")) {
            return "react";
          }
          return undefined;
        },
      },
    },
  },
});
