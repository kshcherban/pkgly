import { defineConfig } from "vitest/config";
import vue from "@vitejs/plugin-vue";
import path from "node:path";

export default defineConfig({
  plugins: [vue()] as any,
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
      "@vue/devtools-kit": path.resolve(__dirname, "./src/__mocks__/devtools-kit.ts"),
    },
  },
  ssr: {
    noExternal: ["vuetify"],
  },
  test: {
    environment: "jsdom",
    globals: true,
    include: ["src/**/__tests__/**/*.spec.ts"],
    setupFiles: ["./vitest.setup.ts"],
  },
});
