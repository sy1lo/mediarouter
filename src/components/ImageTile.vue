<script setup lang="ts">
/**
 * 图片网格里的单个格子。
 *
 * 与 `FileCard` 的区别：格子**没有固定的内部结构**（没有名字行、没有 meta 行），
 * 尺寸完全由 `ImageGrid` 算好的宽高决定，图片铺满整格。
 * 名字与大小只在 hover 时以一条浮层出现 —— 平铺浏览时，信息压在图上
 * 会让整面墙变花（用户此前对「乱」很敏感）。
 *
 * 定位：`ImageGrid` 负责算，这里只负责显示与懒加载。
 */
import { onBeforeUnmount, onMounted, ref } from 'vue'
import type { MediaFile } from '@/types'
import { fmtSize } from '@/utils/format'
import { useLazyLoader } from '@/composables/useLazyLoader'
import { api } from '@/api'

const props = defineProps<{ file: MediaFile; w: number; h: number }>()
const emit = defineEmits<{ open: []; ctx: [x: number, y: number] }>()

/** 整个图墙共用一个观察者（见 `useLazyLoader` 里关于"别每个格子建一个"的说明） */
const lazy = useLazyLoader()

const rootEl = ref<HTMLElement | null>(null)
const src = ref('')

let disposed = false
let started = false

/**
 * 进视口附近才开始加载。
 *
 * 这一步是**必须**的：直接给全部格子挂 `src`，打开一个几千张的图库会瞬间
 * 发起几千个请求 + 几千次位图解码。
 */
async function start() {
  if (started) return
  started = true
  try {
    // ⚠️ 用**缩略图**而不是原图。
    //
    // 这里曾经走的是原图（协议直读文件），理由是"图片浏览就该看原图"。
    // 但那是按「一张图」在想问题，按「一屏图」想就完全是另一回事：
    // 一张 4000×3000 的照片解码成位图是 **48MB**，一屏铺 20 张 ≈ 1GB，
    // 滚动时还要不断替换 —— 这正是用户反馈的"很吃内存"的主因。
    //
    // 缩略图长边 512px，铺满 188px 高的格子绰绰有余（甚至仍是过采样），
    // 解码后约 0.6MB，降了两个数量级；而且它本来就是**磁盘缓存**的，
    // 点开过的目录第二次几乎零成本。
    const url = await api.getThumb(props.file.path)
    if (!disposed) src.value = url ?? ''
  } catch {
    if (!disposed) src.value = ''
  }
}

onMounted(() => {
  if (lazy && rootEl.value) lazy.observe(rootEl.value, start)
  // 没有提供者时直接加载 —— 宁可少一层优化，也不能把图卡住不显示
  else void start()
})

onBeforeUnmount(() => {
  disposed = true
  if (lazy && rootEl.value) lazy.unobserve(rootEl.value)
})

let lastOpen = 0
function open() {
  const now = Date.now()
  if (now - lastOpen < 600) return
  lastOpen = now
  emit('open')
}
</script>

<template>
  <figure
    ref="rootEl"
    class="tile"
    :style="{ width: w + 'px', height: h + 'px' }"
    :title="file.name"
    @click="open"
    @dblclick="open"
    @contextmenu.prevent="emit('ctx', $event.clientX, $event.clientY)"
  >
    <img v-if="src" :src="src" alt="" draggable="false" decoding="async" @error="src = ''" />
    <div v-else class="ph">{{ file.ext.toUpperCase() }}</div>

    <!--
      这里**没有**「已看」角标。
      图片分类不使用已读/已看标记（见 `utils/format.ts` 的 `supportsReadMark`）：
      图墙是拿来扫一眼挑一张的，"看到第几张"没有意义，
      而每格挂个角标只会让画面变花。
    -->
    <figcaption class="cap">
      <span class="cap-name">{{ file.name }}</span>
      <span class="cap-meta">{{ fmtSize(file.size) }}</span>
    </figcaption>
  </figure>
</template>

<style scoped>
.tile {
  position: relative;
  margin: 0;
  overflow: hidden;
  background: #16161f;
  cursor: pointer;
  /* 直角。平铺布局本身就是把画面切成方块铺满，
     圆角会让相邻两格的缝隙形状不规则，一眼就显脏 */
  border-radius: 0;
}

.tile img {
  width: 100%;
  height: 100%;
  /*
    ⚠️ 必须是 `contain`，不能是 `cover`。

    `contain` 保证**整张图一定可见**：盒子和图片比例一致时（正常情况下
    由 `imagedims` 保证）两者的渲染结果完全相同；万一对不上（尺寸没解析出来、
    缩略图比例与原件有细微出入），`contain` 最多留一线背景色，而 `cover`
    会**直接把画面裁掉一块**。

    用户上一版反馈的"右下角这张没展示出全图，只有部分"就是 `cover` 干的 ——
    当时末行的取整误差把盒子横向撑大了一倍，`cover` 就横向裁掉一半。
    宁可看到一线底，也不要看到半张图。
  */
  object-fit: contain;
  display: block;
}

.ph {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 13px;
  font-weight: 700;
  letter-spacing: 0.08em;
  color: rgba(255, 255, 255, 0.16);
}

/* 悬停：不位移、不缩放（图墙上任何位移都会让整片画面"呼吸"），
   只把格子四周围一道色框 */
.tile::after {
  content: '';
  position: absolute;
  inset: 0;
  border: 2px solid transparent;
  pointer-events: none;
  transition: border-color 0.12s;
}

.tile:hover::after {
  border-color: #8b5cf6;
}

.cap {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  padding: 14px 7px 5px;
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
  /* 底部压一层渐变，保证白字在任何画面上都读得清 */
  background: linear-gradient(to top, rgba(0, 0, 0, 0.82), rgba(0, 0, 0, 0));
  color: #fff;
  opacity: 0;
  transition: opacity 0.12s;
  pointer-events: none;
}

.tile:hover .cap {
  opacity: 1;
}

.cap-name {
  font-size: 11px;
  line-height: 1.35;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.cap-meta {
  font-size: 10px;
  line-height: 1.3;
  color: rgba(255, 255, 255, 0.62);
  font-variant-numeric: tabular-nums;
}

/* ---- 各主题的底与悬停色 ---- */
.shell.light .tile {
  background: #eceef3;
}

.shell.light .tile:hover::after {
  border-color: #6d28d9;
}

.shell.vscode .tile {
  background: #1e1e1e;
}

.shell.vscode .tile:hover::after {
  border-color: #0078d4;
}

.shell.win11 .tile {
  background: #eef0f4;
}

.shell.win11 .tile:hover::after {
  border-color: #0078d4;
}

/* ---- 构成主义：直角、无圆角、悬停换成主题红 ---- */
.shell.construct .tile {
  background: #e7e3d9;
}

.shell.construct .tile:hover::after {
  border-color: #d42a1e;
}

.shell.construct .ph {
  color: rgba(26, 26, 26, 0.24);
}
</style>
