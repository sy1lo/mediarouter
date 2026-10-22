/**
 * PDF 首屏缩略图渲染 —— 走 pdf.js 的按需区间读取。
 *
 * ## 为什么不用「整份读」
 *
 * 旧做法是 `readFileData` 把整个 PDF 读进内存再 `getDocument({ data })`。
 * 一个 90MB 的 PDF 意味着峰值约 270MB（Rust `Vec` + wry 复制 + JS `ArrayBuffer`），
 * 而且几十个并发时直接把机器压垮。
 *
 * ## 为什么不用「自定义协议 + Range」
 *
 * 看起来更"标准"，但对 pdf.js 无效：`getDocument({ url })` 的**首次请求不带 `Range` 头**，
 * 而且它把响应的 `Content-Length` 当作整个文档长度 —— 所以第一次仍会整包拉满 90MB，
 * 而 wry 又必须把响应体整体物化。白搭。
 *
 * ## 正解：`PDFDataRangeTransport`
 *
 * 这条路 pdf.js **永不发全量 GET**（`getFullReader()` 只消费我们给的 `initialData`），
 * 只按 chunk 粒度要数据。渲染 90MB PDF 的第一页通常总共只需几 MB。
 * 而且不需要新自定义协议、不改 CSP、不碰 CORS —— 只加一个 IPC 命令。
 *
 * ## pdf.js 契约（违反任意一条都会挂死或抛异常，均在源码里核实过）
 *
 * 1. `progressiveDone` **必须传 `true`**：传 `false` 时全量 reader 不会结束，
 *    会一直等一个我们不会提供的「渐进流」，且 `_done` 为真时 `_enqueue` 直接丢弃数据。
 * 2. 必须用 pdf.js 传进来的 **`begin` 原样回数据**，不能自己造 offset
 *    （`PDFDataTransportStream._onReceiveData` 里有 `assert`）。
 * 3. **绝不能在 `requestDataRange` 里同步回调 `onDataRange`** ——
 *    `getRangeReader` 是先调 `requestDataRange`、再把 reader 压入 `_rangeReaders`。
 * 4. `end` 是**排他**上界。
 * 5. **短包是致命的**：worker 按 `begin + byteLength` 记账，缺失的 chunk 永不重发。
 * 6. **没有 error 通道** —— 失败必须靠 `fatal` + `Promise.race` + `destroy()` 兜。
 */

import * as pdfjs from 'pdfjs-dist'
import { api } from '@/api'
import type { MediaFile } from '@/types'

pdfjs.GlobalWorkerOptions.workerSrc = new URL(
  'pdfjs-dist/build/pdf.worker.min.mjs',
  import.meta.url,
).toString()

/**
 * 小于此体积直接整份读。
 *
 * 注意这**不是** pdf.js 的约束 —— pdf.js 那条「文件 ≤ 2×chunkSize 不启用 range」
 * 的规则只在 `PDFFetchStream` / `PDFNetworkStream`（`url:` 路径）里生效，
 * 我们这条 `range:` 路径根本不经过它。这里是纯粹的经济性选择：
 * 一次 IPC 读完比「取文件头 + 若干次区间」更简单，而 4MiB×3×并发 2 ≈ 24MiB 峰值可以接受。
 */
const FULL_READ_THRESHOLD = 4 * 1024 * 1024

/**
 * 预先交给 pdf.js 的文件头长度。
 *
 * 实测（192 MiB 图片型 PDF + 5.2 MiB 文字型 PDF，chunk 固定 4 KiB）：
 *
 * | 预置头部 | 大漫画真实总量 | 文字型真实总量 |
 * |---|---|---|
 * | 2048 KiB | 2.63 MiB | 2.21 MiB |
 * |  512 KiB | 1.13 MiB | 0.71 MiB |
 * |  192 KiB | 0.93 MiB | 0.40 MiB |
 * |   64 KiB | 0.93 MiB | 0.27 MiB |  ← 选它
 * |   16 KiB | 0.93 MiB | 0.25 MiB |
 *
 * 结论：**超过 ~192 KiB 就是纯过度预读**（2 MiB 的预读量已超过整页所需），
 * 而 16–192 KiB 区间内大文件的总量几乎相同（多预读就少发请求，互相抵消）。
 * 64 KiB 取的是两者都好的那个点。
 */
const INITIAL_DATA_BYTES = 64 * 1024

/**
 * 区间粒度。**这是整条路径上最关键的参数，而且直觉是反的。**
 *
 * 最初的实现选了 256 KiB，理由是"减少 IPC 轮次"。实测证明这是纯粹的反向优化：
 * 解析器发起的是**零散的对象请求**，而每笔都会按 chunk 向上取整，
 * 所以 `读取量 ≈ 请求数 × chunkSize` —— chunk 越大，为一个小对象读进的废数据越多。
 *
 * 192 MiB 图片型 PDF 实测（预置头部 64 KiB）：
 *
 * | 粒度 | 请求 | 读取 | 占比 |
 * |---|---|---|---|
 * | 256 KiB | 161 | 40.10 MiB | 20.87% |
 * |  64 KiB | 161 | 10.04 MiB |  5.22% |
 * |  16 KiB | 163 |  2.61 MiB |  1.36% |  ← 选它
 * |   8 KiB | 164 |  1.37 MiB |  0.71% |
 * |   4 KiB | 165 |  0.74 MiB |  0.39% |
 *
 * 对照：整份读是 192 MiB ⇒ 现在的方案是 **~74 倍**。
 *
 * 为什么不再往下取 4 KiB：另一本**纯文字型** PDF 上，4 KiB 会把请求数从
 * 16 KiB 的 16 次推到 54 次（对象更零散），而收益仅 0.04 MiB。
 * 16 KiB 是在「字节数」与「IPC 轮次」之间两个指标都不吃亏的那个点。
 */
const RANGE_CHUNK_SIZE = 16 * 1024

/** 单个文档内并发发出的区间请求数（与文档级并发 2 是两个维度） */
const FETCH_CONCURRENCY = 2

/** 单次区间请求超时 */
const RANGE_TIMEOUT_MS = 15_000

/** 整个 PDF 加载的硬超时。兜底用 —— 防止任何未预料的挂起把队列名额永久占住。 */
const LOAD_TIMEOUT_MS = 30_000

/** 输出缩略图的边长 */
const THUMB_TARGET = 512

/** 给 promise 加超时，避免任何一次读取无响应就永久挂住 */
function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T> {
  return Promise.race([
    promise,
    new Promise<T>((_, reject) => {
      setTimeout(() => reject(new Error(`区间读取超时（${ms}ms）`)), ms)
    }),
  ])
}

/**
 * 按需区间读取的 transport。
 *
 * pdf.js 会在需要某段字节时调 `requestDataRange(begin, end)`，
 * 我们把拿到的数据用**同一个 `begin`** 交回去。
 */
class RangeTransport extends pdfjs.PDFDataRangeTransport {
  private readonly path: string
  private aborted = false
  /** 文档内的区间请求并发闸门 */
  private inflight = 0
  private waiters: Array<() => void> = []
  private rejectFatal!: (reason: unknown) => void

  /**
   * 致命错误的出口。
   *
   * pdf.js 的 `PDFDataRangeTransport` **没有 error 通道**：某次区间读取失败后
   * 若什么都不回，那个 range reader 会永远 pending，`getPage()` 随之永久挂起。
   * 所以必须由这里把失败抛给上层，让上层 `Promise.race` 接住并 `destroy()`。
   */
  readonly fatal: Promise<never>

  constructor(path: string, length: number, initialData: Uint8Array) {
    // 参数顺序：(length, initialData, progressiveDone, contentDispositionFilename)
    // 第三个**必须是 true**，见文件头契约第 1 条。第四个省略即用默认的 null。
    super(length, initialData, true)

    this.path = path
    this.fatal = new Promise<never>((_, reject) => {
      this.rejectFatal = reject
    })
    // 这个 rejection 可能先于 race 被别处消费，挂个空 catch 避免 unhandledRejection 噪音
    this.fatal.catch(() => {})
  }

  override requestDataRange(begin: number, end: number): void {
    if (this.aborted) return

    // pdf.js 已把 end clamp 到 length，这里再兜一层
    const safeEnd = Math.min(end, this.length)
    if (!(begin >= 0) || !(safeEnd > begin)) {
      this.fail(new Error(`非法的区间请求 ${begin}-${end}`))
      return
    }

    // ⚠️ 必须异步派发（契约第 3 条）：此时 pdf.js 还没把 range reader 压入
    // _rangeReaders，同步回调 onDataRange 会命中 assert。
    void this.deliver(begin, safeEnd)
  }

  private async deliver(begin: number, end: number): Promise<void> {
    await this.acquire()
    try {
      if (this.aborted) return

      const chunk = await withTimeout(api.readFileRange(this.path, begin, end), RANGE_TIMEOUT_MS)

      if (this.aborted) return
      if (!chunk || chunk.byteLength !== end - begin) {
        // 契约第 5 条：短包会让 chunk 永久缺失，宁可直接判死
        throw new Error(`区间长度不符 ${begin}-${end}`)
      }

      // 契约第 2 条：原样回传 pdf.js 给的 begin
      this.onDataRange(begin, chunk)
    } catch (error) {
      if (!this.aborted) this.fail(error)
    } finally {
      this.release()
    }
  }

  private fail(error: unknown): void {
    if (this.aborted) return
    this.aborted = true
    this.rejectFatal(error)
    this.wakeAll()
  }

  /**
   * pdf.js 的 `cancelAllRequests()` 会调用它（`doc.destroy()` 的链路上），
   * 我们自己也主动调。**幂等**，且不反向调用 `destroy()`（防重入）。
   */
  override abort(): void {
    if (this.aborted) return
    this.aborted = true
    this.rejectFatal(new DOMException('aborted', 'AbortError'))
    this.wakeAll()
  }

  private acquire(): Promise<void> {
    if (this.inflight < FETCH_CONCURRENCY) {
      this.inflight++
      return Promise.resolve()
    }
    return new Promise<void>((resolve) => {
      this.waiters.push(() => {
        this.inflight++
        resolve()
      })
    })
  }

  private release(): void {
    this.inflight--
    this.waiters.shift()?.()
  }

  private wakeAll(): void {
    const pending = this.waiters
    this.waiters = []
    for (const wake of pending) wake()
  }
}

/**
 * 渲染 PDF 第一页为 JPEG dataURL。
 *
 * 失败或超时返回 `null`（不抛错）—— 调用方据此显示占位图。
 */
export async function renderPdfThumb(file: MediaFile, signal: AbortSignal): Promise<string | null> {
  if (signal.aborted) return null

  // size <= 0 表示扫描时没拿到大小，也走整份读（range 需要准确的总长）
  if (file.size <= FULL_READ_THRESHOLD) {
    const data = await api.readFileData(file.path)
    if (!data || signal.aborted) return null
    return drawFirstPage(() => pdfjs.getDocument({ data, disableAutoFetch: true }), signal)
  }

  // 大文件：transport 的构造是同步的，所以 initialData 必须先取好
  const headLen = Math.min(file.size, INITIAL_DATA_BYTES)
  const head = await api.readFileRange(file.path, 0, headLen)
  if (!head || signal.aborted) return null

  const transport = new RangeTransport(file.path, file.size, head)
  return drawFirstPage(
    () =>
      pdfjs.getDocument({
        range: transport,
        // 双保险：即便将来 pdf.js 的 isStreamingSupported 语义变了，也绝不触发全量抓取
        disableAutoFetch: true,
        rangeChunkSize: RANGE_CHUNK_SIZE,
      }),
    signal,
    transport,
  )
}

/**
 * 加载文档、渲染第一页、编码成 dataURL。
 *
 * 用 `Promise.race` 把四路失败出口都接住：渲染本身、transport 的致命错误、
 * 外部取消信号、硬超时。这是「pdf.js 没有 error 通道」的解法 ——
 * 少了任何一路都可能永久挂起。
 */
async function drawFirstPage(
  makeTask: () => pdfjs.PDFDocumentLoadingTask,
  signal: AbortSignal,
  transport?: RangeTransport,
): Promise<string | null> {
  const task = makeTask()

  let timer: ReturnType<typeof setTimeout> | undefined
  const hardTimeout = new Promise<never>((_, reject) => {
    timer = setTimeout(() => reject(new Error(`PDF 加载超时（${LOAD_TIMEOUT_MS}ms）`)), LOAD_TIMEOUT_MS)
  })
  const abortSignal = new Promise<never>((_, reject) => {
    const rejectAbort = () => reject(new DOMException('aborted', 'AbortError'))
    if (signal.aborted) {
      rejectAbort()
      return
    }
    signal.addEventListener('abort', rejectAbort, { once: true })
  })

  let doc: pdfjs.PDFDocumentProxy | null = null
  try {
    return await Promise.race([
      (async () => {
        doc = await task.promise
        // getPage(1) 这一句本身就会触发区间读取，所以必须在 race 保护范围内
        const page = await doc.getPage(1)
        const base = page.getViewport({ scale: 1 })
        const scale = Math.min(1.6, THUMB_TARGET / base.width)
        const viewport = page.getViewport({ scale })

        const canvas = document.createElement('canvas')
        canvas.width = Math.floor(viewport.width)
        canvas.height = Math.floor(viewport.height)
        const ctx = canvas.getContext('2d')
        if (!ctx) return null

        await page.render({ canvasContext: ctx, viewport }).promise
        return canvas.toDataURL('image/jpeg', 0.8)
      })(),
      transport ? transport.fatal : new Promise<never>(() => {}),
      abortSignal,
      hardTimeout,
    ])
  } catch {
    return null
  } finally {
    if (timer !== undefined) clearTimeout(timer)
    // 顺序固定：先停发新的区间请求，再 destroy。
    // （destroy 会先等 page.render 结束才 cancelAllRequests，先 abort 能让它快速失败。）
    transport?.abort()
    const target = doc ?? task
    try {
      await target.destroy()
    } catch {
      // 已被销毁或销毁失败都无所谓
    }
  }
}
