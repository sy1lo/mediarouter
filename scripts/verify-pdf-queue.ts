/**
 * 全局 PDF 任务队列的语义验证。
 *
 * 运行：
 *   node --experimental-strip-types scripts/verify-pdf-queue.ts
 *
 * `pdfTaskQueue.ts` 不依赖 Vue / DOM，所以可以直接被 Node 跑（Node 22 支持类型剥离）。
 * 这里断言的是四条硬性质：并发上限、排队期取消不占名额、不饥饿、运行期取消。
 */

import {
  runPdfTask,
  PDF_TASK_CONCURRENCY,
  activePdfTaskCount,
  queuedPdfTaskCount,
} from '../src/utils/pdfTaskQueue.ts'

let pass = 0
let fail = 0

function check(name: string, cond: boolean, extra = ''): void {
  if (cond) {
    pass++
    console.log(`  PASS  ${name}`)
  } else {
    fail++
    console.log(`  FAIL  ${name}${extra ? '  → ' + extra : ''}`)
  }
}

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms))

// ---------------------------------------------------------------------------
console.log('\n[1] 并发峰值必须 <= ' + PDF_TASK_CONCURRENCY)
{
  let peak = 0
  const tasks = Array.from({ length: 20 }, (_, i) =>
    runPdfTask(async () => {
      peak = Math.max(peak, activePdfTaskCount())
      await sleep(15)
      return i
    }),
  )
  const results = await Promise.all(tasks.map((t) => t.outcome))
  check(`峰值 ${peak} <= ${PDF_TASK_CONCURRENCY}`, peak <= PDF_TASK_CONCURRENCY)
  check('20 个任务全部成功', results.every((r) => r.status === 'ok'))
}

// ---------------------------------------------------------------------------
console.log('\n[2] 排队期取消：fn 不得被调用，且不占名额')
{
  // 注意：前 2 个会在 runPdfTask 内同步开工（并发上限就是 2），
  // 所以「排队期取消」只能断言下标 >= 2 的那些。
  const called: number[] = []
  const tasks = Array.from({ length: 8 }, (_, i) =>
    runPdfTask(async () => {
      called.push(i)
      await sleep(20)
      return i
    }),
  )
  for (let i = 0; i < 6; i++) tasks[i].cancel()

  const results = await Promise.all(tasks.map((t) => t.outcome))
  const queuedButCancelled = [2, 3, 4, 5]

  check(
    '排队期被取消的（2..5）fn 从未被调用',
    !called.some((i) => queuedButCancelled.includes(i)),
    `called=[${called}]`,
  )
  check('未取消的（6,7）都开工了', called.includes(6) && called.includes(7), `called=[${called}]`)
  check(
    '被取消的 6 个结算为 aborted',
    results.slice(0, 6).every((r) => r.status === 'aborted'),
    JSON.stringify(results.slice(0, 6).map((r) => r.status)),
  )
  check(
    '未取消的 2 个结算为 ok',
    results.slice(6).every((r) => r.status === 'ok'),
  )
}

// ---------------------------------------------------------------------------
console.log('\n[3] 不饥饿：70 个里取消前 68 个，剩余 2 个必须立刻开工')
{
  const started: number[] = []
  const tasks = Array.from({ length: 70 }, (_, i) =>
    runPdfTask(async () => {
      started.push(i)
      await sleep(10)
      return i
    }),
  )
  for (let i = 0; i < 68; i++) tasks[i].cancel()

  const results = await Promise.all(tasks.map((t) => t.outcome))

  check(
    '排队期被取消的（2..67）一个都没开工',
    !started.some((i) => i >= 2 && i <= 67),
    `started=[${started}]`,
  )
  check('68 与 69 都开工了', started.includes(68) && started.includes(69), `started=[${started}]`)
  check(
    '68 与 69 结算为 ok',
    results[68].status === 'ok' && results[69].status === 'ok',
  )
  check('前 68 个全部 aborted', results.slice(0, 68).every((r) => r.status === 'aborted'))
}

// ---------------------------------------------------------------------------
console.log('\n[4] 运行期取消')
{
  const t = runPdfTask(async (signal) => {
    await sleep(30)
    return signal.aborted ? 'should-not-matter' : 'done'
  })
  await sleep(5)
  t.cancel()
  const r = await t.outcome
  check('结算为 aborted', r.status === 'aborted', JSON.stringify(r))
}

// ---------------------------------------------------------------------------
console.log('\n[5] 抛错的任务必须释放名额')
{
  const t1 = runPdfTask(async () => {
    throw new Error('boom')
  })
  const r1 = await t1.outcome
  check('结算为 error', r1.status === 'error')

  const r2 = await runPdfTask(async () => 'after-error').outcome
  check('名额已释放，后续任务可开工', r2.status === 'ok' && r2.value === 'after-error')

  check(
    '零残留：无排队、无运行中',
    queuedPdfTaskCount() === 0 && activePdfTaskCount() === 0,
    `queued=${queuedPdfTaskCount()} active=${activePdfTaskCount()}`,
  )
}

// ---------------------------------------------------------------------------
console.log(`\nPASS ${pass}   FAIL ${fail}\n`)
if (fail > 0) process.exit(1)
