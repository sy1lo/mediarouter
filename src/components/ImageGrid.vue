<script setup lang="ts">
/**
 * 图片分类的三级菜单 —— 平铺填满的 justify 布局。
 *
 * ## 布局规则
 *
 * 一行一行地摆：往当前行里塞图，直到「按目标行高摆放时的总宽度」够得上容器宽度，
 * 就把这一行**整体缩放**到刚好铺满（所以每张图的宽度随其宽高比自然变化，
 * 而行内高度统一）。最后一行不满时**不拉伸**，保持目标行高、右边留空 ——
 * 这是照片墙的标准做法，硬把末行也铺满会让那几张图被拉得惨不忍睹。
 *
 * ## 为什么需要后端给宽高
 *
 * 这个算法的输入是每张图的宽高比。如果拿不到，就只能先按占位比例排一遍、
 * 等图片加载完再重算 —— 而重算就是整面墙重排。用户在字体问题上已经抱怨过
 * 一次「界面跳一下」，这里不能再犯。
 *
 * 所以扫描阶段就把宽高读出来（`src-tauri/src/imagedims.rs`），
 * 布局**一次算准、零重排**。少数解析不出尺寸的图退回兜底比例。
 *
 * ## 像素取整
 *
 * 每格宽度先按浮点算、再向下取整，然后把这一行累计的取整误差补给**最后一张**。
 * 这样 `Σ宽 + 间隙` 严格等于容器宽度，行尾不会因为累积误差露出一条缝。
 * 若直接交给 flex 用小数像素排，Chromium 在小数边界上偶尔会画出 1px 的亮线。
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { MediaFile } from '@/types'
import { createLazyLoader, provideLazyLoader } from '@/composables/useLazyLoader'
import ImageTile from './ImageTile.vue'

const props = withDefaults(
  defineProps<{
    files: MediaFile[]
    /** 目标行高：决定一屏能排几行。容器越宽，同样的行高会让每行的图更多 */
    targetRowHeight?: number
    /** 格与格的间隙 */
    gap?: number
  }>(),
  { targetRowHeight: 188, gap: 6 },
)

const emit = defineEmits<{ open: [f: MediaFile]; ctx: [f: MediaFile, x: number, y: number] }>()

/** 拿不到宽高时的兜底比例。3:2 是最常见的照片比例，比 1:1 更接近实际观感 */
const FALLBACK_RATIO = 3 / 2

/**
 * 整面墙共用一个懒加载观察者，下发给每个格子。
 *
 * 一个照片库动辄上千张，每格一个 `IntersectionObserver` 是 Chromium
 * 明确警告过的写法（每个观察者都要参与布局后的交叉计算）。
 * 共享一个之后，每格的行为不变，开销降一个量级。
 */
provideLazyLoader(createLazyLoader('400px 0px'))

/**
 * 分批渲染：先只挂前若干行，滚到底部附近再追加。
 *
 * 为什么必须做：图墙是**整列一次性建 DOM** 的，一个 5000 张的目录会立刻
 * 产生 5000 个 `<figure>` + `<img>` 节点，首帧就卡住；
 * 每个格子即使懒加载也要先注册观察者，几千个回调同样是开销。
 *
 * 注意这不是虚拟滚动（没有回收已渲染的行）—— 滚过一遍之后 DOM 还是会全建出来。
 * 但配合"只加载视口附近的缩略图"，内存峰值由**解码后的位图**决定，
 * 而那些只存在于视口附近，所以回收行并不必要。
 * 真正的长列表（上万条）才需要上虚拟滚动。
 */
const INITIAL_ROWS = 6
const GROW_STEP = 6

const shownRows = ref(INITIAL_ROWS)
const sentinel = ref<HTMLElement | null>(null)
let growIO: IntersectionObserver | null = null

const visibleRows = computed(() => rows.value.slice(0, shownRows.value))

// 换了一批文件（切作品集/搜索），展示上限从头开始
watch(
  () => props.files,
  () => {
    shownRows.value = INITIAL_ROWS
  },
)

const host = ref<HTMLElement | null>(null)
const width = ref(0)
let ro: ResizeObserver | null = null

function ratioOf(f: MediaFile): number {
  const w = f.width
  const h = f.height
  if (!w || !h) return FALLBACK_RATIO
  // 极端长条会把行高算崩，夹一下
  return Math.min(10, Math.max(0.1, w / h))
}

interface Placed {
  file: MediaFile
  w: number
}
interface Row {
  height: number
  items: Placed[]
}

const rows = computed<Row[]>(() => {
  const W = Math.floor(width.value)
  if (W <= 0 || props.files.length === 0) return []

  const gap = props.gap
  const target = props.targetRowHeight
  const out: Row[] = []
  let cur: MediaFile[] = []
  let sum = 0

  const flush = (isLast: boolean) => {
    if (cur.length === 0) return
    const avail = W - gap * (cur.length - 1)
    // 末行不铺满（除非它本来就超宽），其余行严格铺满
    const raw = avail / sum
    const h = isLast ? Math.min(target, raw) : raw
    const height = Math.max(24, Math.round(h))

    const exact = cur.map((f) => height * ratioOf(f))
    const widths = exact.map((x) => Math.max(1, Math.floor(x)))

    /*
      ⚠️ **只有"必须铺满"的行才补取整误差。**

      末行本来就该留白，在这里补误差等于把最后一格**横向撑大** ——
      盒子的宽高比不再等于图片比例，于是 `object-fit` 会把这张图裁掉一块。
      用户截图里右下角那张"只显示了一部分"就是这个原因：
      末行只有 1 张图（比例 1.5、目标行高 188 → 宽 282），
      结果被补了 536px 的误差、撑成 818×188（比例 4.35 vs 真实 1.5），
      画面被裁掉一大半。

      顺带记一笔：上一版的铺满探针**没抓到**这个 bug —— 它把末行的
      fillDiff 直接记 0（"末行不该铺满"），又恰好因为那张图在视口外
      没加载、比例校验被跳过。判据必须覆盖"本该留白"的那一行。
    */
    if (!isLast) {
      let deficit = avail - widths.reduce((a, b) => a + b, 0)
      if (deficit > 0) {
        /*
          把取整误差按小数部分「从大到小」摊给各格，而不是一股脑塞给最后一格。

          全塞给最后一格时，那一格可能被加 4px —— 对一张 120px 宽的小图
          就是 3% 的变形（实测探针报 3.42%）。摊开之后**每格最多差 1px**，
          肉眼看不出，`contain` 的留边也只是一条亚像素级的缝。
        */
        const byFrac = exact
          .map((x, i) => ({ i, frac: x - Math.floor(x) }))
          .sort((a, b) => b.frac - a.frac)
        for (let k = 0; deficit > 0; k++, deficit--) {
          widths[byFrac[k % byFrac.length].i] += 1
        }
      }
    }

    out.push({ height, items: cur.map((f, i) => ({ file: f, w: widths[i] })) })
    cur = []
    sum = 0
  }

  for (const f of props.files) {
    cur.push(f)
    sum += ratioOf(f)
    // 按目标行高摆，够宽就收行
    if (sum * target >= W - gap * (cur.length - 1)) flush(false)
  }
  flush(true)

  return out
})

/**
 * 量容器宽度。
 *
 * ⚠️ 不能只靠 ResizeObserver。观察者的首次回调要等一个完整的渲染帧才会派发，
 * 而 `rows` 是完全由宽高算出来的 —— 宽度还是 0 的那一刻，整面图墙就是**空白**。
 * 在无头截图/虚拟时间这类"帧不一定照常推进"的环境里，这个空白会被固化下来
 * （实测就是这么暴露的：`.igrid` 在、一行都没有）。
 *
 * 所以挂载后**立刻同步量一次**（`getBoundingClientRect` 会强制一次布局，
 * 拿到的是真值），ResizeObserver 只负责后续的窗口缩放。
 */
function measure() {
  const el = host.value
  if (!el) return
  const w = Math.floor(el.getBoundingClientRect().width)
  if (w > 0 && w !== width.value) width.value = w
}

onMounted(() => {
  measure()
  ro = new ResizeObserver(measure)
  if (host.value) ro.observe(host.value)

  // 底部哨兵：进入预加载范围就追加一批。
  // 哨兵始终挂在最后一行之后，所以追加之后它自动下移；
  // 若它仍在范围内（容器比首屏的 6 行还高），观察者会立刻再触发一次，
  // 一直追加到填满视口为止 —— 这正好是我们想要的。
  growIO = new IntersectionObserver(
    (entries) => {
      if (!entries.some((e) => e.isIntersecting)) return
      if (shownRows.value >= rows.value.length) return
      shownRows.value = Math.min(rows.value.length, shownRows.value + GROW_STEP)
    },
    { rootMargin: '400px 0px' },
  )
  if (sentinel.value) growIO.observe(sentinel.value)
})

onBeforeUnmount(() => {
  ro?.disconnect()
  ro = null
  growIO?.disconnect()
  growIO = null
})
</script>

<template>
  <div ref="host" class="igrid" :style="{ rowGap: gap + 'px' }">
    <div v-for="(row, ri) in visibleRows" :key="ri" class="igrow" :style="{ gap: gap + 'px' }">
      <ImageTile
        v-for="it in row.items"
        :key="it.file.path"
        :file="it.file"
        :w="it.w"
        :h="row.height"
        @open="emit('open', it.file)"
        @ctx="(x, y) => emit('ctx', it.file, x, y)"
      />
    </div>
    <!-- 追加批次的触发点。1px 高，肉眼不可见，但必须占据布局位置才能被观察到 -->
    <div ref="sentinel" class="isentinel"></div>
  </div>
</template>

<style scoped>
.igrid {
  /* 块级、占满父容器宽度 —— ResizeObserver 量的就是它的 contentRect */
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
}

.igrow {
  display: flex;
  /* 行内每格的宽度已经算好了，不允许 flex 再伸缩，
     否则会把刚算准的像素宽度重新揉匀 */
  flex-wrap: nowrap;
  align-items: stretch;
}

.isentinel {
  height: 1px;
  /* 只是给 IntersectionObserver 一个锚点，不参与视觉 */
  visibility: hidden;
}
</style>
