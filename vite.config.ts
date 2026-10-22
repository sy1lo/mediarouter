import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { fileURLToPath, URL } from 'node:url'

// Tauri 在 dev 模式下会注入 TAURI_DEV_HOST（仅移动端调试用）
const host = process.env.TAURI_DEV_HOST

export default defineConfig({
  plugins: [vue()],
  // Tauri 从 tauri://localhost 加载资源，必须用相对路径
  base: './',
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
    },
  },
  // 让 rustc 的报错不被 vite 清屏冲掉
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: {
      // src-tauri 由 cargo 自己 watch，vite 不必跟
      ignored: ['**/src-tauri/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    outDir: 'dist',
    // WebView2 是 Chromium 内核，可以放心用较新的语法
    target: 'chrome110',
    minify: 'esbuild',
    sourcemap: false,
  },
})
