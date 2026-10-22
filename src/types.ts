/**
 * 全部对外类型定义。
 *
 * 重构前这些类型散在 `store.ts`（数据模型）和 `api.ts`（接口契约）两个文件里，
 * 且 `api.ts` 反向 import `store.ts` 的类型，形成环。现在集中到一处，
 * 只依赖它、不依赖任何运行时模块。
 *
 * 字段名与 Rust 侧的 serde 输出严格一致（Rust 用 `#[serde(rename)]` 转成 camelCase）。
 */

/**
 * 媒体库分类。顺序即侧边栏展示顺序，与 Rust 的 `model::CATEGORIES` 对应。
 *
 * 「图片」是第四个分类，它的三级菜单不用卡片网格，而是平铺填满的
 * justify 布局（见 `components/ImageGrid.vue`），因此 `MediaFile` 会额外带上
 * `width`/`height` —— 详见那两个字段的注释。
 */
export const CATEGORIES = ['动漫', '漫画', '小说', '图片'] as const

export type Category = (typeof CATEGORIES)[number]

export type ThemeName = 'dark' | 'light' | 'vscode' | 'win11' | 'construct'

export interface MediaFile {
  name: string
  /** 绝对路径 */
  path: string
  /** 小写扩展名，不含前导点 */
  ext: string
  size: number
  /** Unix 毫秒 */
  mtime: number
  category?: string
  /**
   * 图片原始宽高，**只有「图片」分类会带**（Rust 扫描时读文件头得到）。
   *
   * 平铺布局必须提前知道宽高比才能一次算准：否则只能先按占位比例排一遍、
   * 等图片加载完再重排，观感就是「打开文件夹后版面跳一下」。
   * 取不到时（少见格式/文件损坏）为 undefined，布局会退回兜底比例。
   */
  width?: number
  height?: number
}

export interface Collection {
  id: string
  name: string
  path: string
  count: number
  totalSize: number
  files: MediaFile[]
  /** 根目录下的散落文件集合 */
  loose?: boolean
  /** 用户手工挂载的作品集 */
  extra?: boolean
}

export interface CategoryData {
  roots: string[]
  collections: Collection[]
}

export interface Library {
  categories: Record<string, CategoryData>
  /** Unix 毫秒 */
  scannedAt: number
}

export interface Rule {
  id: string
  category: string
  extensions: string[]
  exe: string
  args: string
  enabled: boolean
}

export interface ExtraCollection {
  name: string
  path: string
}

export interface Settings {
  categories: Record<string, string[]>
  extraCollections: Record<string, ExtraCollection[]>
  ffmpegPath: string
  theme: ThemeName
}

/** 保存类操作的统一返回 */
export interface SaveResult<T> {
  ok: boolean
  error?: string
  settings?: T
  rules?: T
}

export interface OpenResult {
  ok: boolean
  fallback?: boolean
  reason?: string
  exe?: string
  error?: string
}

export interface FfmpegStatus {
  ready: boolean
  dir: string | null
  source: 'settings' | 'auto' | 'path' | null
  ffmpeg: string | null
  ffprobe: string | null
}

export interface VideoMeta {
  duration?: number | null
}
