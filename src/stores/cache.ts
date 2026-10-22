/**
 * 会话级内存缓存。
 *
 * 从原 `store.ts` 里拆出来 —— 这是纯粹的"渲染层私有缓存"，
 * 与后端状态、UI 状态都没有关系：
 * - `duration`：ffprobe 查出来的时长，避免滚动列表时重复请求
 * - `pdf`：pdf.js 渲染出的封面 dataURL，避免重挂载后重渲染
 * - `pdfFailed`：本会话里渲染失败的 PDF 路径
 *
 * ⚠️ **`pdf` 这个缓存要特别当心内存。**
 * 一张 512px JPEG 的 dataURL 是 ~100KB 的字符串，而 JS 字符串按 UTF-16 存储，
 * 实际占用还要翻倍 —— 漫画/小说库动辄上千个 PDF，无节制地存就是几百 MB 常驻，
 * 这正是"很吃内存"的一个主要来源。
 *
 * 因此现在的策略是：**渲染结果落盘成功后立刻从内存删掉**（见 `FileThumb.renderPdf`）。
 * 内存这份只是"落盘还没完成"时的过渡；重挂载时 `file_thumb` 会命中磁盘缓存，
 * 根本不会重新渲染。另外 `FileThumb` 里还有一个硬上限兜底。
 *
 * 三者都在"清空缩略图"时一并清掉（见 SettingsView）。
 */

export const metaCache = {
  duration: new Map<string, number>(),
  pdf: new Map<string, string>(),
  /**
   * 渲染失败的 PDF。
   *
   * 防止一个病态文件每次滚回视口都重试完整超时（30 秒）—— 失败一次就在本次会话
   * 放弃、显示占位图。下次启动会重新尝试，因为文件可能已经换过了。
   */
  pdfFailed: new Set<string>(),
}

export function clearMetaCache(): void {
  metaCache.duration.clear()
  metaCache.pdf.clear()
  metaCache.pdfFailed.clear()
}
