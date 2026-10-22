/// <reference types="vite/client" />

declare module '*.vue' {
  import type { DefineComponent } from 'vue'
  const component: DefineComponent<Record<string, never>, Record<string, never>, unknown>
  export default component
}

declare module 'pdfjs-dist/build/pdf.worker.min.mjs'

// 注意：原来这里还声明了 `window.api` 的全局类型（由 Electron preload 注入）。
// Tauri 版没有这层注入，前端统一走 `@/api`，所以该声明已移除。
