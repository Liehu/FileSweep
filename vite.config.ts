import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { resolve } from "path";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [vue()],
  resolve: {
    alias: {
      "@": resolve(__dirname, "src"),
      "@plugins": resolve(__dirname, "src/plugins"),
    },
  },
  clearScreen: false,
  server: {
    // 5173 在 Windows 动态端口排除范围（Hyper-V/WSL 保留 5088-5187），
    // 改用 1420（Tauri 默认端口，不在任何排除段内）
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
