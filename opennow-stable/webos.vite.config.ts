import { resolve } from "node:path";

import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  root: "src/renderer",
  base: "./",
  plugins: [react()],
  define: {
    "import.meta.env.VITE_OPENNOW_RUNTIME": JSON.stringify("webos"),
    "import.meta.env.VITE_OPENNOW_VERSION": JSON.stringify(process.env.npm_package_version ?? "0.0.0"),
  },
  build: {
    outDir: "../../dist-webos",
    emptyOutDir: true,
    target: "es2020",
    modulePreload: {
      polyfill: false,
    },
  },
  resolve: {
    alias: {
      "@shared": resolve("src/shared"),
    },
  },
});
