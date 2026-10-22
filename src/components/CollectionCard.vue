<script setup lang="ts">
import { computed } from 'vue'
import type { Collection } from '@/types'
import { fmtSize, supportsReadMark, readWord } from '@/utils/format'
import { readSet, state } from '@/stores/app'
import FileThumb from './FileThumb.vue'

const props = withDefaults(defineProps<{ col: Collection; active: boolean; portrait?: boolean }>(), {
  portrait: false,
})
const emit = defineEmits<{ select: []; ctx: [x: number, y: number] }>()

const coverFile = props.col.files[0] ?? null

/** 已读进度：读 readSet 里的响应式条目，任何地方标记都会让这里同步刷新 */
const readCount = computed(() => props.col.files.reduce((n, f) => n + (readSet.has(f.path) ? 1 : 0), 0))

const allRead = computed(() => props.col.files.length > 0 && readCount.value === props.col.files.length)

/**
 * 用词随分类变（动漫/图片 = 已看，漫画/小说 = 已读），判断集中在 `readWord`。
 *
 * 数据源优先取「组内第一个文件的 category」而不是全局 `state.category` ——
 * 作品集卡片是按当前分类渲染的，两者通常一致；但用文件自带的 category
 * 更贴近数据本身，也不会在切换分类的过渡帧里显示错词。
 */
const categoryOf = computed(() => props.col.files[0]?.category ?? state.category)

const readLabel = computed(() => readWord(categoryOf.value))

const doneLabel = computed(() => (readLabel.value === '已看' ? '已看完' : '已读完'))

/**
 * 图片分类**不使用**已读/已看标记（见 `utils/format.ts`）。
 * 不给它渲染进度标签 —— 图墙是拿来扫一眼挑一张的，没有"看到第几张"的语义。
 * 这里额外的作用是：把右侧的 `padding-right` 也一并去掉，
 * 否则那行文字会凭空留出一块空白。
 */
const showReadMark = computed(() => supportsReadMark(categoryOf.value))
</script>

<template>
  <div
    class="coll-card"
    :class="{ active, portrait }"
    @click="emit('select')"
    @dblclick="emit('select')"
    @contextmenu.prevent="emit('ctx', $event.clientX, $event.clientY)"
  >
    <div class="cover">
      <FileThumb v-if="coverFile" :file="coverFile" :fit="portrait ? 'contain' : 'cover'" />
      <div v-else class="cover-ph">{{ col.name.slice(0, 1) }}</div>
      <span v-if="col.loose" class="tag-loose">根目录散件</span>
    </div>
    <div class="coll-info">
      <div class="coll-name" :title="col.name">
        {{ col.name }}
        <span v-if="col.extra" class="tag-extra">手动</span>
      </div>
      <div class="coll-meta" :class="{ 'no-mark': !showReadMark }">
        <span class="meta-files">{{ col.count }} 个文件 · {{ fmtSize(col.totalSize) }}</span>
      </div>
    </div>
    <!--
      ⚠️ 已读标签**必须**在 `.coll-info` 之外、且用绝对定位钉在卡片右下角。
      历史教训（用户反馈"很乱"，见截图）：
        之前它放在 `.coll-meta` 里，靠 `justify-content: space-between` 挤到右边。
        但左边那串"3 个文件 · 654.0 MB"宽度是变的 —— 文字短标签就贴过来，
        文字长就被推到最右；再加上"已读 1/3"与"已读 1/15"字数不同，
        标签的横向位置**每张卡都不一样**，三个卡片看下来就是参差不齐。
      现在改成 `position: absolute; right/bottom` 锚定右下角，
      左侧文件信息无论多长都影响不到它，所有卡片标签**永远对齐在同一点**。

      另外仍保留「常驻节点 + visibility 切换」：
      用 `v-if` 会让本行高度变化、整列重排（这就是更早那次"UI 跳一下"的原因）。
      绝对定位后它已完全脱离布局，但常驻写法的语义更保险，继续留着。

      `.coll-meta` 右侧预留了 padding（见样式），
      免得"15 个文件 · 2.2 GB"这串文字钻到标签底下。
    -->
    <span
      v-if="showReadMark"
      class="read-progress"
      :class="{ hidden: readCount === 0, done: allRead }"
      :title="readCount ? `${readLabel} ${readCount} / ${col.files.length}` : ''"
    >
      {{ allRead ? doneLabel : `${readLabel} ${readCount}/${col.files.length}` }}
    </span>
  </div>
</template>

<style scoped>
.coll-card {
  position: relative; /* 已读标签的定位基准 */
  border: 1px solid transparent;
  border-radius: 10px;
  padding: 8px;
  display: flex;
  gap: 10px;
  cursor: pointer;
  transition: background 0.15s;
}

.coll-card:hover {
  background: rgba(255, 255, 255, 0.04);
}

.coll-card.active {
  background: rgba(139, 92, 246, 0.14);
  border-color: rgba(139, 92, 246, 0.35);
}

.cover {
  position: relative;
  width: 76px;
  height: 48px;
  border-radius: 7px;
  overflow: hidden;
  background: #101017;
  flex-shrink: 0;
}

.coll-card.portrait .cover {
  width: 52px;
  height: 72px;
}

.cover-ph {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 20px;
  font-weight: 700;
  color: rgba(255, 255, 255, 0.15);
}

.tag-loose {
  position: absolute;
  left: 4px;
  bottom: 4px;
  font-size: 10px;
  background: rgba(0, 0, 0, 0.66);
  color: #d8ccff;
  padding: 0 5px;
  border-radius: 4px;
}

.coll-info {
  min-width: 0;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 4px;
}

.coll-name {
  font-size: 13px;
  line-height: 1.4;
  word-break: break-all;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.tag-extra {
  font-size: 10px;
  color: #fb923c;
  border: 1px solid rgba(251, 146, 60, 0.4);
  border-radius: 4px;
  padding: 0 4px;
  vertical-align: 1px;
}

.coll-meta {
  font-size: 11px;
  color: #7a7a90;
  display: flex;
  align-items: center;
  /* 右侧给绝对定位的已读标签让出位置，避免文字钻到标签底下 */
  padding-right: 62px;
}

/* 不用已读标记的分类（图片）：把那块预留的空白也收掉 */
.coll-meta.no-mark {
  padding-right: 0;
}

.meta-files {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/*
  已读进度：卡片**右下角**一个直角小标签（不是进度条）。
  颜色即信号 —— 有标记就上色，全部标记则用实心底强调。

  ⚠️ 必须绝对定位。
  之前它靠 `justify-content: space-between` 排在 `.coll-meta` 行尾，
  而左边"3 个文件 · 654.0 MB"的宽度因卡片而异 —— 标签位置随之左右浮动，
  不同卡片参差不齐（用户："很乱"）。
  锚定 `right` + `bottom` 后，所有卡片标签落在完全相同的坐标上。

  定位基准是 `.coll-card`（`position: relative`），`right/bottom: 8px`
  与卡片 padding 等值，让标签四周形成一圈均匀的呼吸空间。

  ⚠️ `z-index` 必须加上。
  竖版（portrait）模式下封面高 72px、卡片内容高度也在 72px 左右，
  标签的垂直区间会与封面**重叠**（`bottom: 8px` 落在封面下沿内）。
  封面是 `.cover { position: relative }`，属于定位元素，
  在 DOM 中又排在标签**之前** —— 不加 z-index 的话同样定位层级下
  后来的标签本应压在上面，但一旦将来给封面加 transform/filter 就会翻转层级。
  显式抬到 1，稳定压住封面。
*/
.read-progress {
  position: absolute;
  right: 8px;
  bottom: 8px;
  z-index: 1;
  flex-shrink: 0;
  font-variant-numeric: tabular-nums;
  font-size: 10px;
  line-height: 1.5;
  padding: 0 5px;
  border: 1px solid rgba(52, 211, 153, 0.45);
  border-radius: 4px;
  color: #5eead4;
}

/* 占位但不可见：保留布局高度，不参与视觉 */
.read-progress.hidden {
  visibility: hidden;
}

.read-progress.done {
  background: #34d399;
  border-color: #34d399;
  color: #0b1a14;
  font-weight: 700;
}


.shell.light .coll-meta {
  color: #9a9aad;
}

.shell.vscode .coll-card:hover {
  background: #2a2d2e;
}

.shell.vscode .coll-card.active {
  background: rgba(0, 122, 204, 0.18);
  border-color: #0078d4;
}

.shell.vscode .coll-meta {
  color: #8a8a8a;
}

.shell.vscode .tag-extra {
  color: #d7ba7d;
  border-color: rgba(215, 186, 125, 0.5);
}

.shell.win11 .coll-card {
  border-radius: 12px;
}

.shell.win11 .coll-card:hover {
  background: rgba(0, 0, 0, 0.035);
}

.shell.win11 .coll-card.active {
  background: #e8f1fb;
  border-color: rgba(0, 120, 212, 0.35);
}

.shell.win11 .cover {
  border-radius: 8px;
  background: #f0f2f5;
}

.shell.win11 .coll-meta {
  color: #8a8a90;
}

.shell.win11 .tag-extra {
  color: #c19a00;
  border-color: rgba(193, 154, 0, 0.45);
}

/* ---- 构成主义 ---- */
.shell.construct .coll-card {
  border-radius: 0;
  /* 透明边框占位，避免 hover 时因为加边框而跳版 */
  border: 1px solid transparent;
  padding: 8px;
}

.shell.construct .coll-card:hover {
  background: rgba(26, 26, 26, 0.055);
  border-color: rgba(26, 26, 26, 0.18);
}

/* 选中：黑框 + 淡红底，边界是硬的 */
.shell.construct .coll-card.active {
  background: rgba(212, 42, 30, 0.09);
  border-color: #1a1a1a;
}

.shell.construct .cover {
  border-radius: 0;
  background: #e7e3d9;
  /* 封面压一道黑边，像贴在版面上的图版 */
  outline: 1px solid rgba(26, 26, 26, 0.22);
  outline-offset: -1px;
}

.shell.construct .cover-ph {
  color: rgba(26, 26, 26, 0.22);
  font-weight: 800;
}

.shell.construct .tag-loose {
  border-radius: 0;
  background: #1a1a1a;
  color: #f3f0e9;
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.shell.construct .coll-name {
  color: #1a1a1a;
  font-weight: 600;
}

.shell.construct .coll-meta {
  color: #8a8579;
  letter-spacing: 0.02em;
}

.shell.construct .tag-extra {
  color: #d42a1e;
  border: 1px solid #d42a1e;
  border-radius: 0;
  font-weight: 700;
}

/* ---- 已读进度标签：各主题配色 ---- */
.shell.light .read-progress {
  border-color: rgba(22, 163, 74, 0.4);
  color: #15803d;
}

.shell.light .read-progress.done {
  background: #16a34a;
  border-color: #16a34a;
  color: #ffffff;
}

.shell.vscode .read-progress {
  border-color: rgba(78, 201, 176, 0.45);
  color: #4ec9b0;
}

.shell.vscode .read-progress.done {
  background: #4ec9b0;
  border-color: #4ec9b0;
  color: #1e1e1e;
}

.shell.win11 .read-progress {
  border-color: rgba(15, 123, 63, 0.35);
  color: #0f7b3f;
}

.shell.win11 .read-progress.done {
  background: #0f7b3f;
  border-color: #0f7b3f;
  color: #ffffff;
}

/* 构成主义：直角标签，用主题红 —— 与右侧文件卡的底边红条同一套语言 */
.shell.construct .read-progress {
  border-radius: 0;
  border: 1px solid #d42a1e;
  color: #d42a1e;
  font-weight: 700;
  letter-spacing: 0.04em;
}

.shell.construct .read-progress.done {
  background: #d42a1e;
  border-color: #d42a1e;
  color: #f3f0e9;
}

</style>
