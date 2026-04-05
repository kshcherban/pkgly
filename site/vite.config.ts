import { fileURLToPath, URL } from "node:url";

import { defineConfig, type PluginOption, type UserConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import vueJsx from "@vitejs/plugin-vue-jsx";
import vuetify from "vite-plugin-vuetify";
import fs from "fs";
import browserslistToEsbuild from "browserslist-to-esbuild";
import { ViteEjsPlugin } from "vite-plugin-ejs";
import { stripLegacyMdiFontSourcesPlugin } from "./vite/plugins/stripLegacyMdiFontSources";

export default defineConfig(async ({ command }): Promise<UserConfig> => {
  const isTest = process.env.VITEST === "true";
  const enableDevTools =
    !isTest && command === "serve" && process.env.VITE_DEVTOOLS !== "false";
  const hasWindow =
    typeof globalThis !== "undefined" &&
    typeof (globalThis as { window?: unknown }).window !== "undefined";
  const plugins: PluginOption[] = [
    stripLegacyMdiFontSourcesPlugin(),
    vue(),
    vueJsx(),
    vuetify({ autoImport: true }),
    ViteEjsPlugin(),
  ];

  if (enableDevTools && hasWindow) {
    const { default: vueDevTools } = await import("vite-plugin-vue-devtools");
    plugins.push(vueDevTools());
  }

  plugins.push({
      name: "copy-routes",
      apply: "build",

      closeBundle() {
        console.log("Copying routes.json to dist/assets");
        fs.copyFile(
          fileURLToPath(new URL("./src/router/routes.json", import.meta.url)),
          fileURLToPath(new URL("./dist/routes.json", import.meta.url)),
          (err) => {
            if (err) {
              console.error(err);
            } else {
              console.log("routes.json copied successfully");
            }
          },
        );
      },
    });

  return {
    build: {
      target: browserslistToEsbuild(undefined, {
        path: ".browserlistrc",
      }),
    },
    plugins,
    css: {
      preprocessorOptions: {
        scss: {
          api: "modern-compiler",
        },
      },
      devSourcemap: true,
    },
    resolve: {
      alias: {
        "@": fileURLToPath(new URL("./src", import.meta.url)),
      },
    },
  };
});
