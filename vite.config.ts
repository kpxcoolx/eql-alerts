import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "path";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [
    react(),
    {
      name: "eql-strip-overlay-crossorigin",
      transformIndexHtml: {
        order: "post",
        handler(html: string, ctx: { filename?: string; path?: string }) {
          const path = ctx.filename || ctx.path || "";
          if (!path.includes("overlay")) return html;
          return html.replace(/(\s)crossorigin(="[^"]*")?/g, "");
        },
      },
    },
  ],
  base: "./",
  build: {
    modulePreload: false,
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        overlay: resolve(__dirname, "overlay.html"),
      },
    },
  },
  clearScreen: false,
  server: {
    port: 1422,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1423,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
