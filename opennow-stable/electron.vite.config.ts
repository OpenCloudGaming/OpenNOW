import { resolve } from "node:path";

import { defineConfig, externalizeDepsPlugin } from "electron-vite";
import react from "@vitejs/plugin-react";

function readBuildMetadata(): Record<string, string> {
  return {
    __OPENNOW_BUILD_NUMBER__: JSON.stringify(
      process.env.OPENNOW_BUILD_NUMBER?.trim()
      || process.env.BUILD_NUMBER?.trim()
      || process.env.GITHUB_RUN_NUMBER?.trim()
      || "",
    ),
    __OPENNOW_BUILD_COMMIT__: JSON.stringify(
      process.env.OPENNOW_BUILD_COMMIT?.trim()
      || process.env.GITHUB_SHA?.trim()
      || "",
    ),
  };
}

export default defineConfig({
  main: {
    plugins: [externalizeDepsPlugin()],
    define: readBuildMetadata(),
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
    plugins: [react()],
    resolve: {
      alias: {
        "@shared": resolve("src/shared"),
      },
    },
  },
});
