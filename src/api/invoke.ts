/**
 * Tauri IPC 的最小封装。
 *
 * 重构前每个组件直接写 `window.api.xxx()`，错误处理各写各的，
 * 而且 Electron 的 `ipcRenderer.invoke` 抛出的错误格式不统一。
 * 现在所有调用都经过这里，错误统一成 `ApiError`。
 */

import { invoke as tauriInvoke } from '@tauri-apps/api/core'

export class ApiError extends Error {
  constructor(
    message: string,
    /** 出错的命令名，便于定位 */
    readonly command: string,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

/**
 * 调用后端命令。
 *
 * 参数键名用 camelCase —— Tauri 会自动映射到 Rust 侧的 snake_case 形参，
 * 例如 JS 的 `dataUrl` 会绑到 Rust 的 `data_url`。
 */
export async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await tauriInvoke<T>(command, args)
  } catch (e) {
    const message =
      typeof e === 'string' ? e : e instanceof Error ? e.message : JSON.stringify(e)
    throw new ApiError(message || '未知错误', command)
  }
}
