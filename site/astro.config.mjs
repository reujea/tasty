// @ts-check
import { fileURLToPath } from "node:url";
import { defineConfig } from "astro/config";
import react from "@astrojs/react";
import { satteri } from "@astrojs/markdown-satteri";
import { createGuideLinkPlugin } from "./src/lib/satteri-guide-links.mjs";
import { createDiagramPlugin } from "./src/lib/satteri-diagrams.mjs";
import { createTableWrapPlugin } from "./src/lib/satteri-table-wrap.mjs";
import { BASE, SITE } from "./src/lib/base.mjs";

export default defineConfig({
  site: SITE,
  base: BASE,
  integrations: [react()],
  vite: {
    plugins: [{
      name: "tasty-isolate-dependency-cache",
      config(_config, { command, mode }) {
        // A build may run while the preview dev server is still open.
        // React's development JSX runtime must not reuse production prebundles.
        return { cacheDir: fileURLToPath(new URL(`./node_modules/.vite-tasty/${command}-${mode}/`, import.meta.url)) };
      },
    }],
  },
  markdown: {
    processor: satteri({
      hastPlugins: [
        createGuideLinkPlugin(fileURLToPath(new URL("content", import.meta.url)), fileURLToPath(new URL("..", import.meta.url))),
        // The English tree is `content/en/`; everything else is the Korean source,
        // which also serves untranslated English pages.
        createTableWrapPlugin(),
        createDiagramPlugin((url) => (url && fileURLToPath(url).includes("/content/en/") ? "en" : "ko"), BASE),
      ],
    }),
  },
  outDir: "./dist",
  build: { format: "directory" },
  devToolbar: { enabled: false },
});
