/**
 * pdf.js 区间读取契约验证（纯 Node，不需要 GUI，不需要 canvas）。
 *
 * 运行：
 *   node scripts/verify-range-transport.mjs "D:/某本.pdf"
 *
 * 为什么需要它：本机环境无法启动 GUI 进程，所以 WebView2 里的真实表现测不了。
 * 但整个方案依赖的每一条 pdf.js 契约都可以在这里证伪或证实 —— 包括
 * **「只拉取渲染第一页所需的那几 MB，而不是整个文件」** 这个最核心的论点。
 *
 * 用 legacy 构建 + `getOperatorList()`（渲染需要 canvas，无 GUI 环境拿不到；
 * operator list 同样会触发区间读取，正好覆盖目标路径）。
 */

import * as pdfjs from 'pdfjs-dist/legacy/build/pdf.mjs'
import fs from 'node:fs'
import path from 'node:path'

const target = process.argv[2]
if (!target) {
  console.error('用法: node scripts/verify-range-transport.mjs <文件.pdf>')
  process.exit(2)
}

// 第三个参数可选：预置的文件头字节数（默认 64 KiB）
const HEAD_BYTES = Number(process.argv[3]) || 64 * 1024

let pass = 0
let fail = 0
function check(name, cond, extra = '') {
  if (cond) {
    pass++
    console.log(`  PASS  ${name}`)
  } else {
    fail++
    console.log(`  FAIL  ${name}${extra ? '  → ' + extra : ''}`)
  }
}
const sleep = (ms) => new Promise((r) => setTimeout(r, ms))
const mib = (n) => (n / 1024 / 1024).toFixed(2)

const fd = await fs.promises.open(target, 'r')
const size = (await fd.stat()).size
console.log(`\n目标: ${path.basename(target)}`)
console.log(`大小: ${mib(size)} MiB (${size} 字节)\n`)

// ---------------------------------------------------------------------------
// 一个「读本地文件」的区间 transport —— 与 src/utils/pdfRange.ts 的 RangeTransport 同构
// ---------------------------------------------------------------------------
function makeTransport({ head, corruptBegin = false, log }) {
  class T extends pdfjs.PDFDataRangeTransport {
    constructor() {
      // 契约第 1 条：progressiveDone 必须 true
      super(size, head, true)
      this.aborted = false
      this.threwOnWrongBegin = false
      this.fatal = new Promise((_, reject) => {
        this._reject = reject
      })
      this.fatal.catch(() => {})
    }

    requestDataRange(begin, end) {
      if (this.aborted) return
      const len = end - begin
      if (log) log.push([begin, end])

      const buf = Buffer.alloc(len)
      fd.read(buf, 0, len, begin).then(
        ({ bytesRead }) => {
          if (this.aborted) return
          if (bytesRead !== len) {
            // 契约第 5 条：短包是致命的
            this._fail(new Error(`短包 ${bytesRead} != ${len}`))
            return
          }
          // 契约第 2 条：必须原样回传 pdf.js 给的 begin。
          // corruptBegin 模式故意偏 1，用来证明这个 assert 真实存在。
          const echoBegin = corruptBegin ? begin + 1 : begin
          try {
            this.onDataRange(echoBegin, new Uint8Array(buf))
          } catch (e) {
            this.threwOnWrongBegin = true
            this._fail(e)
          }
        },
        (e) => this._fail(e),
      )
    }

    _fail(e) {
      if (this.aborted) return
      this.aborted = true
      this._reject(e)
    }

    abort() {
      if (this.aborted) return
      this.aborted = true
      this._reject(new Error('aborted'))
    }
  }
  return new T()
}

async function withTimeout(promise, ms, label) {
  let timer
  try {
    return await Promise.race([
      promise,
      new Promise((_, rej) => {
        timer = setTimeout(() => rej(new Error(`${label} 超时（${ms}ms）`)), ms)
      }),
    ])
  } finally {
    clearTimeout(timer)
  }
}

async function readHead() {
  const buf = Buffer.alloc(Math.min(size, HEAD_BYTES))
  await fd.read(buf, 0, buf.length, 0)
  return new Uint8Array(buf)
}

// ---------------------------------------------------------------------------
// 场景 1：区间粒度对比
//
// 关键洞察：解析器发起的是**零散的对象请求**，而每次请求都会被向上取整到
// chunk 边界。所以 chunkSize 越大，「为了一个 1KB 的对象读进 256KB」的浪费越严重。
// 这里把几种粒度都跑一遍，用实测数字决定取值 —— 而不是凭感觉"减少 IPC 轮次"。
// ---------------------------------------------------------------------------
console.log('[1] 区间粒度对比')

const CHUNK_CONFIGS = [
  { label: '256 KiB', size: 256 * 1024 },
  { label: '64 KiB（默认）', size: 64 * 1024 },
  { label: '16 KiB', size: 16 * 1024 },
  { label: '8 KiB', size: 8 * 1024 },
  { label: '4 KiB', size: 4 * 1024 },
]

async function measure(chunkSize, { corruptBegin = false } = {}) {
  const log = []
  const head = await readHead()
  const transport = makeTransport({ head, log, corruptBegin })
  let doc = null
  let ok = false
  const startedAt = Date.now()

  try {
    const task = pdfjs.getDocument({
      range: transport,
      disableAutoFetch: true,
      rangeChunkSize: chunkSize,
    })
    doc = await withTimeout(Promise.race([task.promise, transport.fatal]), 40_000, 'document 加载')
    const page = await withTimeout(
      Promise.race([doc.getPage(1), transport.fatal]),
      40_000,
      'getPage(1)',
    )
    try {
      await withTimeout(
        Promise.race([page.getOperatorList(), transport.fatal]),
        40_000,
        'getOperatorList',
      )
    } catch {
      await withTimeout(Promise.race([page.getTextContent(), transport.fatal]), 40_000, 'getTextContent')
    }
    ok = true
  } catch {
    ok = false
  } finally {
    transport.abort()
    if (doc) await doc.destroy().catch(() => {})
  }

  return {
    ok,
    elapsed: Date.now() - startedAt,
    count: log.length,
    // 关键：预置头部也是真实发生的读取，必须计入总量。
    // （最初的版本漏算了它，导致「总量」被低估、并得出「头部越大越好」的反向结论。）
    bytes: log.reduce((s, [b, e]) => s + (e - b), 0) + head.byteLength,
    max: log.reduce((m, [b, e]) => Math.max(m, e - b), 0),
    log,
    threwOnWrongBegin: transport.threwOnWrongBegin,
  }
}

let best = null
let bestLabel = ''
for (const cfg of CHUNK_CONFIGS) {
  const r = await measure(cfg.size)
  if (!r.ok) {
    console.log(`      ${cfg.label.padEnd(20)} 加载失败/超时`)
    continue
  }
  const pct = ((r.bytes / size) * 100).toFixed(2)
  console.log(
    `      ${cfg.label.padEnd(20)} 请求 ${String(r.count).padStart(4)} 次` +
      `   读取 ${mib(r.bytes).padStart(7)} MiB   ${pct.padStart(6)}%` +
      `   耗时 ${String(r.elapsed).padStart(5)} ms`,
  )
  if (!best || r.bytes < best.bytes) {
    best = r
    bestLabel = cfg.label
  }
}

const fetched = best ? best.bytes : 0
const readCount = best ? best.count : 0
const maxRead = best ? best.max : 0

if (best) {
  console.log(
    `\n      最优粒度: ${bestLabel} —— 读取 ${mib(fetched)} MiB，` +
      `${((fetched / size) * 100).toFixed(2)}%，共 ${readCount} 次请求\n`,
  )
  check('加载过程发生了区间读取', readCount > 0, `readCount=${readCount}`)
  check('读取量远小于文件体积（< 20%）', fetched < size * 0.2, `${mib(fetched)} MiB`)
  check(
    '所有区间合法：0 <= begin < end <= size',
    best.log.every(([b, e]) => Number.isInteger(b) && b >= 0 && e > b && e <= size),
  )
  check('没有发生整份读取', maxRead < size, `单次最大 ${mib(maxRead)} MiB`)
  if (size > 4 * 1024 * 1024) {
    check(`收益达到一个数量级以上（< 10%）`, fetched < size * 0.1, `${((fetched / size) * 100).toFixed(2)}%`)
  }
} else {
  check('至少有一种粒度能成功加载', false)
}

// ---------------------------------------------------------------------------
// 场景 2：反例 —— begin 回错必须抛异常（证明契约第 2 条真实存在）
// ---------------------------------------------------------------------------
console.log('\n[2] 反例：故意用 begin+1 回数据')
{
  const r = await measure(64 * 1024, { corruptBegin: true })
  check(
    'begin 回错会触发 pdf.js 的 assert（证明必须原样回传）',
    r.threwOnWrongBegin,
    '未观察到 assert —— 需要重新核对契约第 2 条',
  )
}

// ---------------------------------------------------------------------------
// 场景 3：反例 —— progressiveDone=false 的后果（只观察，不作硬断言）
// ---------------------------------------------------------------------------
console.log('\n[3] 反例：progressiveDone=false（观察项）')
{
  const head = await readHead()

  class TFalse extends pdfjs.PDFDataRangeTransport {
    constructor() {
      super(size, head, false) // ← 故意传 false
      this.aborted = false
      this.count = 0
    }
    requestDataRange(begin, end) {
      this.count++
      const len = end - begin
      const buf = Buffer.alloc(len)
      fd.read(buf, 0, len, begin).then(({ bytesRead }) => {
        if (bytesRead === len) this.onDataRange(begin, new Uint8Array(buf))
      })
    }
    abort() {
      this.aborted = true
    }
  }

  const transport = new TFalse()
  const started = Date.now()
  let loaded = false
  try {
    const task = pdfjs.getDocument({ range: transport, rangeChunkSize: 64 * 1024 })
    const doc = await withTimeout(task.promise, 15_000, '加载')
    await doc.getPage(1)
    await sleep(200)
    loaded = true
    await doc.destroy().catch(() => {})
  } catch {
    loaded = false
  } finally {
    transport.abort()
  }
  console.log(
    `      progressiveDone=false 时加载${loaded ? '仍能成功' : '失败/超时'}，` +
      `耗时 ${Date.now() - started}ms，请求 ${transport.count} 次`,
  )
  console.log('      （只作观察：结论是我们必须传 true，而不是依赖它的失败模式）')
}

// ---------------------------------------------------------------------------
await fd.close()
console.log(`\nPASS ${pass}   FAIL ${fail}`)
console.log(
  `\n核心数字：${mib(size)} MiB 的 PDF，渲染第一页读了 ${mib(fetched)} MiB` +
    `（${((fetched / size) * 100).toFixed(2)}%，${readCount} 次区间请求，粒度 ${bestLabel}）\n`,
)

if (fail > 0) process.exit(1)
