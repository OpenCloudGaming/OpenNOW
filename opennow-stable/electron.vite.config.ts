import { resolve } from "node:path";

import { defineConfig, externalizeDepsPlugin } from "electron-vite";
import react from "@vitejs/plugin-react";
import reactScan from "@react-scan/vite-plugin-react-scan";

export default defineConfig({
  main: {
    plugins: [externalizeDepsPlugin()],
    build: {
      outDir: "dist-electron/main",
    },
    resolve: {
      alias: {
        "@shared": resolve("src/shared"),
      },
    },
  },
  preload: {
    plugins: [externalizeDepsPlugin()],
    build: {
      outDir: "dist-electron/preload",
    },
    resolve: {
      alias: {
        "@shared": resolve("src/shared"),
      },
    },
  },
  renderer: {
    build: {
      outDir: "dist",
    },
    plugins: [
      react(),
      reactScan({
        enable: process.env.NODE_ENV === "development",
        autoDisplayNames: true,
      }),
    ],
    resolve: {
      alias: {
        "@shared": resolve("src/shared"),
      },
    },
  },
});
