import { resolve } from "node:path";

import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    alias: {
      "@shared": resolve(__dirname, "src/shared"),
    },
  },
  test: {
    include: ["test/**/*.test.ts"],
    environment: "node",
    environmentMatchGlobs: [["test/renderer/**/*.test.ts", "jsdom"]],
    restoreMocks: true,
    clearMocks: true,
  },
});
