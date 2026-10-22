/**
 * 全局 PDF 任务队列。
 *
 * ## 为什么需要它
 *
 * 原实现的队列写在 `FileThumb.vue` 的 `<script setup>` 里，
 * Vue 会把它编进 `setup()`，于是**每个组件实例各有一份** `running / MAX`。
 * 而每个实例一辈子只提交一个任务（`load()` 由 `loaded` 守卫只跑一次），
 * 所以那个 `MAX = 1` **从未真正限流过** ——
 * 打开一个含 70 个 PDF 的作品集时，有效并发度等于视口内触发的卡片数，可逼近 70。
 *
 * 这里的 `running` / `queue` 是**模块级**的，全局唯一，才真正限流。
 *
 * ## 四个必须成立的性质
 *
 * 1. **并发峰值恒 ≤ `PDF_TASK_CONCURRENCY`**
 * 2. **排队期被取消的任务不占名额**：它连 `fn` 都不会被调用
 * 3. **不饥饿**：取消项就地丢弃后 `continue`（而非 `return`），后面的任务能立刻补位
 * 4. **运行期取消**：名额在 `fn` 收敛后释放。所以 `fn` 必须自带超时，
 *    否则一个卡死的任务会长期占位 —— 这是有意的权衡，
 *    避免在 cancel 瞬间超订并发（宁可慢一点，也不要重新变成 70 路并发）
 *
 * 不依赖 Vue / DOM，因此可以用 `node --experimental-strip-types` 直接验证
 * （见 `scripts/verify-pdf-queue.ts`）。
 */

/** 全局并发上限。每个任务在 pdf.js 的 worker 里会按文件全长分配缓冲，所以这个数直接乘文件体积。 */
export const PDF_TASK_CONCURRENCY = 2

export type PdfOutcome<T> =
  | { status: 'ok'; value: T }
  | { status: 'aborted' }
  | { status: 'error'; error: unknown }

export interface PdfTask<T> {
  /** 任务终局。排队期就被取消的，`fn` 从未被调用，结果直接是 `aborted`。 */
  readonly outcome: Promise<PdfOutcome<T>>
  /** 幂等 */
  cancel(): void
}

interface Entry {
  fn: (signal: AbortSignal) => Promise<unknown>
  controller: AbortController
  settle: (outcome: PdfOutcome<unknown>) => void
}

const queue: Entry[] = []
let running = 0

/**
 * 从队列里捞出可开工的任务。
 *
 * 用 `continue` 而不是 `return`：否则一个被取消的队首会把整个队列堵住（饥饿）。
 * 取消项就地结算，`running` 不动 —— 它从未占用过名额。
 */
function pump(): void {
  while (running < PDF_TASK_CONCURRENCY && queue.length > 0) {
    const entry = queue.shift()
    if (!entry) break

    if (entry.controller.signal.aborted) {
      entry.settle({ status: 'aborted' })
      continue
    }

    running++
    void execute(entry)
  }
}

async function execute(entry: Entry): Promise<void> {
  const { signal } = entry.controller
  let outcome: PdfOutcome<unknown>

  try {
    const value = await entry.fn(signal)
    // fn 结束时才被取消的，也按 aborted 结算 —— 结果已无意义
    outcome = signal.aborted ? { status: 'aborted' } : { status: 'ok', value }
  } catch (error) {
    outcome = signal.aborted ? { status: 'aborted' } : { status: 'error', error }
  }

  running--
  entry.settle(outcome)
  pump()
}

/**
 * 提交一个 PDF 任务。
 *
 * `fn` 会拿到一个 `AbortSignal`，应当在耗时步骤前后检查它，并给自身加超时。
 */
export function runPdfTask<T>(fn: (signal: AbortSignal) => Promise<T>): PdfTask<T> {
  const controller = new AbortController()

  let settle!: (outcome: PdfOutcome<T>) => void
  const outcome = new Promise<PdfOutcome<T>>((resolve) => {
    settle = resolve
  })

  queue.push({
    fn: fn as (signal: AbortSignal) => Promise<unknown>,
    controller,
    settle: settle as (outcome: PdfOutcome<unknown>) => void,
  })
  pump()

  return {
    outcome,
    cancel() {
      controller.abort()
      // 立刻驱动一次：若它还在排队，这一步就把它丢掉并让后面的补位
      pump()
    },
  }
}

/** 仅供验证脚本使用：当前正在运行的任务数 */
export function activePdfTaskCount(): number {
  return running
}

/** 仅供验证脚本使用：当前排队中的任务数 */
export function queuedPdfTaskCount(): number {
  return queue.length
}
