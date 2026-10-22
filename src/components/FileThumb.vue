<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from 'vue'
import type { MediaFile } from '@/types'
import { metaCache } from '@/stores/cache'
import { api } from '@/api'
import { renderPdfThumb } from '@/utils/pdfRange'
import { runPdfTask, type PdfTask } from '@/utils/pdfTaskQueue'

const props = withDefaults(defineProps<{ file: MediaFile; fit?: 'cover' | 'contain' }>(), {
  fit: 'cover',
})

const src = ref('')
const loading = ref(false)
const isSmall = ref(false)
const rootEl = ref<HTMLElement | null>(null)
const isPdf = props.file.ext.toLowerCase() === 'pdf'

/**
 * `metaCache.pdf` 的硬上限，兜底用。
 *
 * 它只是一份"本会话别重复渲染"的备忘，而渲染结果已经通过 `saveThumb` 落到磁盘、
 * 重挂载时 `file_thumb` 直接命中 —— 所以正常路径下渲染完就从内存删掉
 * （见 `renderPdf` 里 saveThumb 成功后的 delete）。
 *
 * 这个上限是防万一：存档一直失败（数据目录不可写等）时至少别无限涨。
 * 每张 512px JPEG 的 dataURL 是 ~100KB 字符串，JS 字符串按 UTF-16 存、实际翻倍。
 */
const PDF_CACHE_MAX = 24

function trimPdfCache(): void {
  while (metaCache.pdf.size > PDF_CACHE_MAX) {
    const oldest = metaCache.pdf.keys().next().value
    if (oldest === undefined) break
    metaCache.pdf.delete(oldest)
  }
}

let io: IntersectionObserver | null = null
let loaded = false
let disposed = false
let task: PdfTask<string | null> | null = null

async function load() {
  if (loaded) return
  loaded = true
  try {
    // 后端返回的是可直接使用的协议 URL（内部走 Tauri 的 convertFileSrc），
    // 不再需要前端自己拼 'thumb://' 前缀
    const url = await api.getThumb(props.file.path)
    if (url) {
      if (!disposed) src.value = url
      return
    }
    if (isPdf) await renderPdf()
  } catch {
    if (!disposed) src.value = ''
  }
}

async function renderPdf() {
  const cached = metaCache.pdf.get(props.file.path)
  if (cached) {
    if (!disposed) src.value = cached
    return
  }
  // 本会话里已经失败过：不重试。否则每次滚回视口都要白等一轮完整超时。
  if (metaCache.pdfFailed.has(props.file.path)) return

  loading.value = true
  const current = runPdfTask((signal) => renderPdfThumb(props.file, signal))
  task = current

  // 并发由 pdfTaskQueue 在**模块级**统一控制（上限 2）。
  // 这里绝不能再各自排队 —— 队列写在组件里会变成每实例一份，等于没有限流。
  const outcome = await current.outcome
  task = null
  loading.value = false

  if (outcome.status === 'aborted') return
  if (outcome.status === 'error' || !outcome.value) {
    metaCache.pdfFailed.add(props.file.path)
    return
  }

  // 缩略图已经产出。昂贵的那部分（getPage + render + toDataURL）钱已经付过了，
  // 所以**即便组件已卸载也照常落盘**：saveThumb 成功后后端会 unfail，
  // 下次滚到同一个文件时 file_thumb 直接命中磁盘缓存、零 PDF 工作 ——
  // 相当于把「快速滚动造成的瞬时工作」沉淀成「永久命中」。
  const dataUrl = outcome.value
  metaCache.pdf.set(props.file.path, dataUrl)
  trimPdfCache()
  void api
    .saveThumb(props.file.path, dataUrl)
    .then((r) => {
      /*
        落盘成功就把内存里这份丢掉。

        `metaCache.pdf` 原本的用途是"本会话别再重复渲染"，但**磁盘缓存已经完全
        承担了这件事** —— 重挂载时 `load()` 先问 `file_thumb`，命中磁盘就直接
        拿到 URL，根本走不到 renderPdf。

        留着它等于让每个看过的 PDF 都往内存里塞一个 ~100KB 的 base64 字符串
        （而 JS 字符串按 UTF-16 存，实际占用还要翻倍）。漫画/小说库动辄上千个
        PDF，这就是几百 MB 常驻内存 —— 正是"很吃内存"的一个主要来源。
      */
      if (r.ok) metaCache.pdf.delete(props.file.path)
    })
    .catch(() => {})
  if (!disposed) src.value = dataUrl
}

function onImgLoad(e: Event) {
  const img = e.target as HTMLImageElement
  isSmall.value = img.naturalWidth > 0 && img.naturalWidth < 160
}

function onImgError() {
  src.value = ''
}

onMounted(() => {
  io = new IntersectionObserver(
    (entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        io?.disconnect()
        load()
      }
    },
    { rootMargin: '400px 0px' },
  )
  if (rootEl.value) io.observe(rootEl.value)
})

onBeforeUnmount(() => {
  // 顺序不能变：
  //   1) 先标记已卸载，之后所有 DOM 写入都被挡掉
  //   2) 断开观察者
  //   3) 取消任务 —— 还在排队的会被就地丢弃（连 fn 都不会被调用），
  //      正在跑的会通过 AbortSignal 让 pdf.js 那边尽快收敛
  disposed = true
  io?.disconnect()
  task?.cancel()
})
</script>

<template>
  <div ref="rootEl" class="thumb-box" :class="{ 'has-small': isSmall }">
    <img
      v-if="src"
      :src="src"
      alt=""
      draggable="false"
      :style="{ objectFit: fit }"
      :class="{ sm: isSmall }"
      @load="onImgLoad"
      @error="onImgError"
    />
    <div v-else class="ph">
      <!-- 渲染 PDF 期间给个转动指示：以前这里只是一行静态灰字，
           几十张卡片排队时看起来就像整个界面卡死了 -->
      <span v-if="loading" class="ph-spin" />
      <template v-else>{{ file.ext.toUpperCase() }}</template>
    </div>
  </div>
</template>

<style scoped>
.thumb-box {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #101017;
}

.thumb-box.has-small {
  background: linear-gradient(160deg, #1e1e2e 0%, #101017 55%, #1b1630 100%);
}

.thumb-box img {
  width: 100%;
  height: 100%;
}

.thumb-box img.sm {
  width: auto;
  height: auto;
  max-width: 45%;
  max-height: 50%;
  object-fit: contain;
  filter: drop-shadow(0 4px 12px rgba(0, 0, 0, 0.55));
}

.ph {
  font-size: 22px;
  font-weight: 700;
  letter-spacing: 1px;
  color: rgba(255, 255, 255, 0.12);
}

.ph-spin {
  display: inline-block;
  width: 20px;
  height: 20px;
  border: 2px solid rgba(255, 255, 255, 0.16);
  border-top-color: rgba(255, 255, 255, 0.6);
  border-radius: 50%;
  animation: ph-spin 0.9s linear infinite;
}

@keyframes ph-spin {
  to {
    transform: rotate(360deg);
  }
}

.shell.vscode .thumb-box {
  background: #1e1e1e;
}

.shell.vscode .thumb-box.has-small {
  background: linear-gradient(160deg, #1f2a33 0%, #1e1e1e 55%, #1a2430 100%);
}

.shell.win11 .thumb-box {
  background: #f0f2f5;
}

.shell.win11 .thumb-box.has-small {
  background: linear-gradient(160deg, #e9eef5 0%, #f0f2f5 55%, #e5ebf3 100%);
}

.shell.win11 .ph {
  color: rgba(0, 0, 0, 0.14);
}

.shell.win11 .ph-spin {
  border-color: rgba(0, 0, 0, 0.12);
  border-top-color: rgba(0, 0, 0, 0.45);
}

/* ---- 构成主义 ---- */
.shell.construct .thumb-box {
  background: #e7e3d9;
}

/* 图小于版面时（比如竖版漫画封面被 contain 撑不满）：
   铺一层构成主义式的角标 —— 左上角一个小红三角 + 一道细黑对角线。
   刻意做小：这层是衬托缩略图的底，不能跟封面抢视线 */
.shell.construct .thumb-box.has-small {
  background: #e7e3d9;
  background-image:
    linear-gradient(135deg, transparent calc(50% - 0.5px), rgba(26, 26, 26, 0.13) calc(50% - 0.5px) calc(50% + 0.5px), transparent calc(50% + 0.5px)),
    linear-gradient(135deg, rgba(212, 42, 30, 0.92) 0 11%, transparent 11%);
}

.shell.construct .ph {
  color: rgba(26, 26, 26, 0.26);
  letter-spacing: 0.1em;
}

.shell.construct .ph-spin {
  border: 2px solid rgba(26, 26, 26, 0.15);
  border-top-color: #d42a1e;
  border-radius: 0;
  /* 直角旋转方 —— 构成主义连加载指示器都该是方的 */
  animation: ph-spin 0.9s steps(8, end) infinite;
}
</style>
