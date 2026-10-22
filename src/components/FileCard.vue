<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import type { MediaFile } from '@/types'
import { metaCache } from '@/stores/cache'
import { isRead, state } from '@/stores/app'
import { fmtDuration, fmtSize, VIDEO_EXTS, readWord } from '@/utils/format'
import { toPlain } from '@/utils/plain'
import { api } from '@/api'
import FileThumb from './FileThumb.vue'

const props = withDefaults(defineProps<{ file: MediaFile; portrait?: boolean }>(), {
  portrait: false,
})
const emit = defineEmits<{ open: []; ctx: [x: number, y: number] }>()

const isVideo = VIDEO_EXTS.has(props.file.ext.toLowerCase())
const duration = ref<string | null>(null)
let lastOpen = 0

/** 已读状态直接读全局响应式集合 —— 在任意地方切换都会让本卡片自动重渲染 */
const read = computed(() => isRead(props.file.path))

/**
 * 「已读」的用词：动漫（视频）与图片（画面）是**看**的 → 已看；
 * 漫画、小说是**读**的 → 已读。判断集中在 `readWord`，四处调用共用一份。
 *
 * 取 `props.file.category`（扫描时就带上），拿不到时退回全局当前分类。
 */
const readLabel = computed(() => readWord(props.file.category ?? state.category))

const durationText = computed(() => {
  if (!isVideo) return null
  return duration.value ?? '…'
})

/** 展示用文件名：剥掉结尾的扩展名（扩展名单独显示在卡片右下角） */
const stem = computed(() => {
  const n = props.file.name
  const e = props.file.ext
  if (!e) return n
  const suffix = '.' + e
  // 只在确实以该后缀结尾时才剥离 —— 名字里恰好含点的文件不会被误伤
  if (n.length > suffix.length && n.toLowerCase().endsWith(suffix.toLowerCase())) {
    return n.slice(0, -suffix.length)
  }
  return n
})

function open() {
  const now = Date.now()
  if (now - lastOpen < 600) return
  lastOpen = now
  emit('open')
}

onMounted(async () => {
  if (!isVideo) return
  const cached = metaCache.duration.get(props.file.path)
  if (cached != null) {
    duration.value = fmtDuration(cached)
    return
  }
  try {
    const meta = await api.getVideoMeta(toPlain(props.file))
    if (meta.duration != null) {
      metaCache.duration.set(props.file.path, meta.duration)
      duration.value = fmtDuration(meta.duration)
    } else {
      duration.value = ''
    }
  } catch {
    duration.value = ''
  }
})
</script>

<template>
  <div
    class="file-card"
    :class="{ portrait, read }"
    @click="open"
    @dblclick="open"
    @contextmenu.prevent="emit('ctx', $event.clientX, $event.clientY)"
  >
    <div class="thumb-wrap">
      <FileThumb :file="file" :fit="portrait ? 'contain' : 'cover'" />
      <span v-if="isVideo && durationText" class="dur-badge">{{ durationText }}</span>
      <!-- 已读角标：绝对定位在缩略图左上角，不参与布局 -->
      <span v-if="read" class="read-badge" :title="readLabel">
        <span class="read-tick">✓</span>
        <span class="read-text">{{ readLabel }}</span>
      </span>
    </div>
    <div class="file-name" :title="file.name">{{ stem }}</div>
    <div class="file-meta">
      <span class="size">{{ fmtSize(file.size) }}</span>
      <span class="ext-badge">{{ file.ext }}</span>
    </div>
    <!--
      已读色条用**伪元素**画，不用真实节点。
      原因：真实节点即便 `position:absolute`，一旦它出现在 flex 子元素列表里，
      就会参与这个 flex 容器的"基线/行高协商"，实测让 .file-card 高度
      从 257px 变成 258.59px —— 网格整排跟着重排，就是用户说的"UI 跳一下"。
      伪元素完全在盒子模型之外，不可能影响布局。
    -->
  </div>
</template>

<style scoped>
.file-card {
  border: 1px solid #232330;
  border-radius: 10px;
  overflow: hidden;
  background: #16161f;
  cursor: pointer;
  transition: border-color 0.15s, transform 0.15s;
  /* 纵向 flex：网格会把同一排卡片拉成等高，靠下面的 margin-top:auto
     把「大小 + 后缀」那一行钉到卡片底部，名字 1 行还是 3 行都落在同一水平线上 */
  display: flex;
  flex-direction: column;
  /* 已读色条（.file-card.read::after）要贴在这个盒子的底边上 */
  position: relative;
}

.file-card:hover {
  border-color: #8b5cf6;
  transform: translateY(-1px);
}

.shell.light .file-card {
  background: #fff;
  border-color: #e2e3ea;
}

.thumb-wrap {
  position: relative;
  aspect-ratio: 16 / 10;
  background: #101017;
  overflow: hidden;
  /* 高度由 aspect-ratio 决定，不参与 flex 伸缩 */
  flex: 0 0 auto;
}

.file-card.portrait .thumb-wrap {
  aspect-ratio: 3 / 4;
}

.dur-badge {
  position: absolute;
  right: 6px;
  bottom: 6px;
  background: rgba(0, 0, 0, 0.72);
  color: #fff;
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 6px;
}

.file-name {
  padding: 8px 10px 2px;
  font-size: 12.5px;
  line-height: 1.45;
  word-break: break-all;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.file-meta {
  padding: 2px 10px 9px;
  font-size: 11px;
  color: #7a7a90;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  /* 吃掉卡片余下的空白，使这一行的纵向位置与名字行数无关 */
  margin-top: auto;
}

.file-meta .size {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 扩展名：卡片右下角，与文件大小同一行 */
.ext-badge {
  flex-shrink: 0;
  font-size: 10px;
  line-height: 1;
  letter-spacing: 0.4px;
  text-transform: uppercase;
  padding: 3px 6px;
  border-radius: 5px;
  background: rgba(255, 255, 255, 0.07);
  color: #9a9ab0;
}

.shell.light .file-meta {
  color: #9a9aad;
}

.shell.light .ext-badge {
  background: rgba(0, 0, 0, 0.05);
  color: #6d6d82;
}

.shell.vscode .file-card {
  background: #252526;
  border-color: #3c3c3c;
}

.shell.vscode .file-card:hover {
  border-color: #0078d4;
}

.shell.vscode .file-meta {
  color: #8a8a8a;
}

.shell.vscode .ext-badge {
  background: #3c3c3c;
  color: #a8a8a8;
}

.shell.win11 .file-card {
  background: #ffffff;
  border-color: rgba(0, 0, 0, 0.07);
  border-radius: 12px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04), 0 4px 12px rgba(0, 0, 0, 0.05);
}

.shell.win11 .file-card:hover {
  border-color: rgba(0, 120, 212, 0.45);
  box-shadow: 0 4px 18px rgba(0, 0, 0, 0.1);
  transform: translateY(-2px);
}

.shell.win11 .thumb-wrap {
  background: #f0f2f5;
}

.shell.win11 .file-meta {
  color: #8a8a90;
}

.shell.win11 .ext-badge {
  background: rgba(0, 120, 212, 0.1);
  color: #005fb8;
}

/* 底边色条：伪元素，贴在卡片最底边。
   与角标同色系，给整排一个可扫视的"已读"信号。
   伪元素不参与盒子模型，绝不引起重排。
   ⚠️ 本规则必须写在下面各主题覆盖规则**之前**，否则主题色会被这里的默认值盖掉。 */
.file-card.read::after {
  content: '';
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  height: 2px;
  background: linear-gradient(90deg, #34d399, #10b981);
  pointer-events: none;
}

/* ---- 各主题的已读标记配色 ----
   默认(dark)用深底 + 绿勾 + 绿色条，在暗色缩略图上对比足够。
   亮色的 light / win11 反转为深底（缩略图本身偏暗，深底浅字更稳）。
   construct 走主题语言：直角、墨黑底、纸白字，色条用主题红。 */
.shell.light .read-badge {
  background: rgba(22, 22, 30, 0.82);
}

.shell.light .read-tick {
  color: #6ee7a8;
}

.shell.light .file-card.read::after {
  background: linear-gradient(90deg, #16a34a, #15803d);
}

.shell.win11 .read-badge {
  background: rgba(22, 22, 30, 0.8);
  backdrop-filter: blur(8px) saturate(150%);
}

.shell.win11 .read-tick {
  color: #6ee7a8;
}

.shell.win11 .file-card.read::after {
  background: linear-gradient(90deg, #107c41, #0f7b3f);
}

.shell.vscode .read-badge {
  background: rgba(24, 24, 24, 0.85);
}

.shell.vscode .read-tick {
  color: #4ec9b0;
}

.shell.vscode .file-card.read::after {
  background: linear-gradient(90deg, #4ec9b0, #3aa892);
}

/* 构成主义：直角角标 + 墨黑底 + 纸白字，色条用主题红（全主题里唯一用红的，
   因为红在这套语言里就是"标记"的意思） */
.shell.construct .read-badge {
  background: #1a1a1a;
  backdrop-filter: none;
  border-bottom-right-radius: 0;
  border-radius: 0;
  font-weight: 700;
  letter-spacing: 0.06em;
}

.shell.construct .read-tick {
  color: #f3f0e9;
}

.shell.construct .file-card.read::after {
  background: #d42a1e;
  height: 3px;
}

.shell.construct .file-card.read {
  border-color: #1a1a1a;
}

/* ---- 构成主义 ---- */
.shell.construct .file-card {
  background: #fbf9f4;
  /* 2px 黑框：构成主义的卡片是一块实板，不是浮起来的玻璃 */
  border: 2px solid #1a1a1a;
  border-radius: 0;
}

/* hover 不做位移、不做阴影 —— 只把黑框换成红框。克制，但有反应 */
.shell.construct .file-card:hover {
  border-color: #d42a1e;
  transform: none;
}

.shell.construct .thumb-wrap {
  background: #e7e3d9;
}

.shell.construct .dur-badge {
  border-radius: 0;
  background: #1a1a1a;
  color: #f3f0e9;
  font-weight: 600;
  letter-spacing: 0.04em;
}

.shell.construct .file-name {
  color: #1a1a1a;
  letter-spacing: 0.01em;
}

.shell.construct .file-meta {
  color: #8a8579;
}

/* 扩展名徽标：黑底米白字，全大写 —— 构成主义排版里的"标签" */
.shell.construct .ext-badge {
  background: #1a1a1a;
  color: #f3f0e9;
  border-radius: 0;
  font-weight: 700;
  letter-spacing: 0.08em;
}

/* ------------------------------------------------------------------
   已读态
   ------------------------------------------------------------------
   设计取向（第二版，据用户反馈重做）：
   第一版的做法是"让已读退后"——缩略图降透明度 + 文字转灰。
   用户反馈**变灰有碍观瞻**，因此弃用。

   现在改为**在画面之外加一个克制的标记**，画面本身一点不动：
     1) 缩略图左上角一个直角小标签（勾 + 「已读」/「已看」）
     2) 卡片底边一条 2px 色条（构成主义 3px 红）

   ⚠️ **硬约束：色条必须用伪元素，不能用真实节点。**
   实测（Edge 无头 + getBoundingClientRect 比对，见 tools/）：
     · 用 `<span class="read-strip">` 时，即便它 `position:absolute`，
       `.file-card` 的高度仍从 257px 变成 258.594px —— 因为 `.file-card`
       是 flex column，多出的那个子节点参与了**行高协商**，
       `.file-meta` 的 line-height 由 `normal` 变成继承来的 17.6px。
     · 高度一变，`.file-grid` 的行高跟着涨（257px → 258.594px），
       整排卡片下移 2px、第二排下移 4px —— 这就是用户说的"UI 跳一下"。
   改成 `.file-card.read::after` 伪元素后：盒子模型完全不参与，
     `.file-card` 高度在两个状态下**逐字节相等**。

   `.read-badge` 之所以能用真实节点，是因为它挂在 `.thumb-wrap` 上，
   而后者有 `aspect-ratio` + `flex: 0 0 auto`，高度是写死的，不受子节点影响。
------------------------------------------------------------------ */
.read-badge {
  position: absolute;
  left: 0;
  top: 0;
  display: flex;
  align-items: center;
  gap: 3px;
  padding: 2px 6px 2px 5px;
  border-bottom-right-radius: 7px;
  /* 半透明深底 + 高亮文字：压在任意缩略图上都读得清，
     又不像纯色块那样抢画面 */
  background: rgba(18, 18, 26, 0.82);
  backdrop-filter: blur(6px);
  font-size: 10px;
  line-height: 1.35;
  font-weight: 600;
  letter-spacing: 0.02em;
  color: #fff;
  pointer-events: none;
}

.read-tick {
  font-size: 10px;
  line-height: 1;
  font-weight: 700;
  color: #6ee7a8;
}

.read-text {
  white-space: nowrap;
}

/* 悬停时角标稍微压暗一点，表示"可交互" */
.file-card:hover .read-badge {
  background: rgba(18, 18, 26, 0.92);
}
</style>
