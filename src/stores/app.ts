/**
 * 应用级状态。
 *
 * 相比重构前（原 `store.ts`）的差异：
 * - **排序不在这里做了**。曾经用 `Intl.Collator('zh-CN', {numeric:true})`，
 *   那是 ICU 的中文规则、不是资源管理器的规则（ICU 把汉字排在拉丁字母之前），
 *   导致同一文件夹在两个窗口里的顺序对不上。
 *   现在由 Rust 侧用资源管理器自己的 `StrCmpLogicalW` 排好再下发（见 `sort.rs`），
 *   前端**拿到即用**——这里再排一次只会用错误的规则把顺序改回去。
 * - 类型来自 `@/types`，不再和 API 契约互相引用。
 */

import { reactive } from 'vue'
import { api } from '@/api'
import { CATEGORIES, type Library, type MediaFile, type ThemeName } from '@/types'

/**
 * 右键菜单的目标。
 *
 * 两种形态共用同一个菜单组件：
 * - `file`：对着单个文件右键
 * - `collection`：对着左侧作品集卡片右键（操作整组）
 * 用判别联合而不是可选字段，模板里 `target.kind === 'file'` 一判即可，
 * 不会出现「两个字段都空」这种非法状态。
 */
export type CtxTarget =
  | { kind: 'file'; file: MediaFile }
  | { kind: 'collection'; name: string; files: MediaFile[] }

export const state = reactive({
  library: null as Library | null,
  category: CATEGORIES[0] as string,
  collectionId: null as string | null,
  ctxMenu: null as { x: number; y: number; target: CtxTarget } | null,
  ui: {
    theme: 'dark' as ThemeName,
    scanning: false,
  },
})

/**
 * 已读文件路径集合。
 *
 * 用 Vue 的 `reactive` 包一个 `Set`：Vue 3 对 Set 的 `has`/`add`/`delete`
 * 做了响应式追踪，所以 `readSet.has(path)` 直接写在模板里就能自动更新，
 * 不需要额外的版本号或强制刷新。
 *
 * 单独放在 `state` 之外，是因为它更新频繁（每点一个文件变一次），
 * 混进大对象里会让依赖 `state` 的组件跟着一起重新求值。
 */
export const readSet = reactive(new Set<string>())

export function isRead(path: string): boolean {
  return readSet.has(path)
}

/** 从后端拉全量已读集合（启动时调用一次） */
export async function loadRead(): Promise<void> {
  try {
    const paths = await api.getRead()
    readSet.clear()
    for (const p of paths) readSet.add(p)
  } catch {
    // 读不到就当没有已读 —— 已读只是辅助标记，不该阻塞启动
  }
}

/** 切换单文件已读状态，乐观更新 + 失败回滚 */
export async function toggleRead(path: string): Promise<boolean> {
  const wasRead = readSet.has(path)
  // 乐观更新：点击必须立刻有反馈，不能等 IPC 往返
  if (wasRead) readSet.delete(path)
  else readSet.add(path)
  try {
    const nowRead = await api.toggleRead(path)
    // 以后端为准（防止并发点击导致前后端不一致）
    if (nowRead) readSet.add(path)
    else readSet.delete(path)
    return nowRead
  } catch {
    // 回滚
    if (wasRead) readSet.add(path)
    else readSet.delete(path)
    throw new Error('标记已读失败')
  }
}

/** 批量设置已读状态，乐观更新 + 失败回滚 */
export async function setReadBatch(paths: string[], read: boolean): Promise<void> {
  const before = paths.map((p) => [p, readSet.has(p)] as const)
  for (const p of paths) {
    if (read) readSet.add(p)
    else readSet.delete(p)
  }
  try {
    await api.setRead(paths, read)
  } catch {
    for (const [p, was] of before) {
      if (was) readSet.add(p)
      else readSet.delete(p)
    }
    throw new Error('批量标记失败')
  }
}

export function normalizeTheme(t: string | undefined): ThemeName {
  return t === 'light' || t === 'vscode' || t === 'win11' || t === 'construct' ? t : 'dark'
}

export async function loadLibrary(scan = false): Promise<void> {
  state.ui.scanning = true
  try {
    // 顺序直接用后端给的 —— 它已经按资源管理器的规则排好了（见文件头说明）
    const lib = scan ? await api.rescan() : await api.getLibrary()
    state.library = lib
    // 当前选中的作品集可能已经不存在了
    if (state.collectionId) {
      const col = lib.categories[state.category]?.collections.find(
        (c) => c.id === state.collectionId,
      )
      if (!col) state.collectionId = null
    }
  } finally {
    state.ui.scanning = false
  }
}

export async function loadTheme(): Promise<void> {
  try {
    const s = await api.getSettings()
    state.ui.theme = normalizeTheme(s.theme)
  } catch {
    state.ui.theme = 'dark'
  }
}
