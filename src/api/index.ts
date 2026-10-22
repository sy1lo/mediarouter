/**
 * 后端 API 门面。
 *
 * 这是前端唯一允许碰后端的地方 —— 组件里不应该再出现 `invoke(...)`。
 * 相比重构前，这里额外吸收了三件"本该藏起来"的事：
 *
 * 1. **缩略图 URL 组装**：Rust 返回的是缩略图文件的绝对路径，
 *    这里用 Tauri 官方的 `convertFileSrc` 转成平台正确的协议 URL。
 *    以前这个拼接散在 `FileThumb.vue` 里（`'thumb://' + id`），
 *    而 Electron 的私有协议名一旦换掉就会静默失效。
 * 2. **原始字节的解包**：`read_file_data` 走的是 `ipc::Response`，
 *    前端拿到 ArrayBuffer，这里统一成 `Uint8Array` 给 pdf.js 用。
 * 3. **失败语义**：读不到文件返回 `null` 而不是抛错（对齐旧版行为）。
 */

import { convertFileSrc } from '@tauri-apps/api/core'
import { call } from './invoke'
import type {
  FfmpegStatus,
  Library,
  MediaFile,
  OpenResult,
  Rule,
  SaveResult,
  Settings,
  VideoMeta,
} from '@/types'

/** 自定义资源协议名，与 Rust 侧 `register_uri_scheme_protocol("thumb", ...)` 对应 */
const THUMB_PROTOCOL = 'thumb'

export const api = {
  // --- 媒体库 ---
  getLibrary: () => call<Library>('library_get'),
  rescan: () => call<Library>('library_rescan'),

  // --- 设置与规则 ---
  getSettings: () => call<Settings>('settings_get'),
  saveSettings: (settings: Settings) => call<SaveResult<Settings>>('settings_save', { settings }),
  getRules: () => call<Rule[]>('rules_get'),
  saveRules: (rules: Rule[]) => call<SaveResult<Rule[]>>('rules_save', { rules }),

  // --- 已读标记 ---
  /** 全量已读路径集合（启动时拉一次） */
  getRead: () => call<string[]>('read_get'),
  /** 切换单文件已读状态，返回切换后的新状态 */
  toggleRead: (path: string) => call<boolean>('read_toggle', { path }),
  /** 批量设置已读状态，返回设置后的总条数 */
  setRead: (paths: string[], read: boolean) => call<number>('read_set', { paths, read }),
  /** 清空全部已读标记，返回清掉的条数 */
  clearRead: () => call<number>('read_clear'),

  // --- 文件操作 ---
  openFile: (file: MediaFile) => call<OpenResult>('file_open', { file }),
  openWithDefault: (file: MediaFile) => call<OpenResult>('file_open_default', { file }),
  reveal: (path: string) => call<boolean>('file_reveal', { path }),
  copyText: (text: string) => call<boolean>('file_copy', { text }),
  validateExe: (path: string) => call<boolean>('exe_validate', { path }),

  // --- 对话框 ---
  pickDirectory: () => call<string | null>('dialog_pick_directory'),
  pickExe: () => call<string | null>('dialog_pick_exe'),

  // --- 缩略图 ---
  /**
   * 返回可直接塞进 `<img src>` 的 URL；`null` 表示后端生成不了（pdf 走这里）。
   *
   * 图墙与卡片都走这一条 —— 长边 512px 且**落在磁盘缓存上**。
   * 不要为了"更清楚"改成读原图：一张 40MP 照片解码后是 48MB，
   * 一屏铺二十张就是一个 GB（详见 `src-tauri/src/lib.rs` 里的那段说明）。
   */
  async getThumb(path: string): Promise<string | null> {
    const file = await call<string | null>('file_thumb', { path })
    return file ? convertFileSrc(file, THUMB_PROTOCOL) : null
  },
  saveThumb: (path: string, dataUrl: string) =>
    call<SaveResult<never>>('file_save_thumb', { path, dataUrl }),
  clearThumbs: () => call<SaveResult<never>>('thumbs_clear'),
  thumbCount: () => call<number>('thumbs_count'),

  // --- 视频元数据 ---
  getVideoMeta: (file: MediaFile) =>
    call<VideoMeta>('file_meta', { file: { path: file.path, ext: file.ext } }),
  ffmpegStatus: () => call<FfmpegStatus>('ffmpeg_status'),

  // --- 原始字节（pdf.js 用） ---
  async readFileData(path: string): Promise<Uint8Array | null> {
    try {
      const raw = await call<ArrayBuffer | number[] | null>('read_file_data', { path })
      if (!raw) return null
      return raw instanceof ArrayBuffer ? new Uint8Array(raw) : new Uint8Array(raw)
    } catch {
      // 文件不存在 / 过大 / 无权限 / 不在媒体库根目录内 —— 与旧版 fs.readFileSync 失败返回 null 对齐
      return null
    }
  },

  /**
   * 按区间读取原始字节，供 pdf.js 的 range transport 使用。
   *
   * `begin` / `end` 是字节偏移，区间为 **[begin, end)（右开）** ——
   * 与 pdf.js `ChunkedStreamManager` 的记账方式严格一致，改了会解析错乱。
   *
   * 返回 `null` 表示失败。**调用方必须把它判定为致命错误**：
   * 短包或空包会让 pdf.js 的对应 chunk 永久缺失且不重发，导致加载永久挂起。
   */
  async readFileRange(path: string, begin: number, end: number): Promise<Uint8Array | null> {
    try {
      const raw = await call<ArrayBuffer | number[] | null>('read_file_range', {
        path,
        begin,
        end,
      })
      if (!raw) return null
      const bytes = raw instanceof ArrayBuffer ? new Uint8Array(raw) : new Uint8Array(raw)
      return bytes.byteLength === end - begin ? bytes : null
    } catch {
      return null
    }
  },

  // --- 应用握手 ---
  /** 前端挂载完成后调用，让 Rust 侧把窗口显示出来 */
  uiReady: () => call<void>('ui_ready'),
  /** 数据目录（设置页展示） */
  dataDir: () => call<string>('app_data_dir'),
}

export type Api = typeof api
