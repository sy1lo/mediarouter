<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useMessage } from 'naive-ui'
import { isRead, setReadBatch, state, toggleRead } from '@/stores/app'
import { toPlain } from '@/utils/plain'
// 别名叫 readWordFor：下面有个同名 computed（模板里在用），直接 import 会撞名
import { readWord as readWordFor, supportsReadMark } from '@/utils/format'
import { api } from '@/api'

const message = useMessage()
const visible = computed(() => state.ctxMenu !== null)
const menuEl = ref<HTMLElement | null>(null)
const pos = ref({ x: 0, y: 0 })

const MENU_W = 180
// 单文件菜单 6 项 + 2 分隔线；作品集菜单 3 项 + 1 分隔线。
// 取较大者做夹取上界，宁可多留白也不让菜单被切掉。
const MENU_H = 196
const MENU_H_COLLECTION = 130

/** 当前目标是单个文件还是整个作品集 */
const target = computed(() => state.ctxMenu?.target ?? null)
const isFile = computed(() => target.value?.kind === 'file')

/** 当前作品集内已读 / 总数（仅作品集目标有意义） */
const collRead = computed(() => {
  const t = target.value
  if (t?.kind !== 'collection') return { read: 0, total: 0 }
  const read = t.files.reduce((n, f) => n + (isRead(f.path) ? 1 : 0), 0)
  return { read, total: t.files.length }
})

/** 当前右键目标文件是否已读 */
const currentRead = computed(() => {
  const t = target.value
  return t?.kind === 'file' ? isRead(t.file.path) : false
})

/** 当前右键目标所属的分类（取文件自带的；作品集取组内首个文件） */
const categoryOf = computed(() => {
  const t = target.value
  const cat =
    t?.kind === 'file'
      ? t.file.category
      : t?.kind === 'collection'
        ? t.files[0]?.category
        : undefined
  return cat ?? state.category
})

/**
 * 用词随分类变（动漫/图片 = 已看，漫画/小说 = 已读），判断集中在 `readWordFor`。
 * 取不到分类才退回全局当前分类。
 */
const readWord = computed(() => readWordFor(categoryOf.value))

/**
 * 该分类是否提供「已读/已看」操作。
 *
 * 图片分类**不提供**（见 `utils/format.ts`）：没有"看到第几张"的语义。
 * 模板里据此整段隐藏，而不是留一个点了没反应的按钮。
 */
const canMark = computed(() => supportsReadMark(categoryOf.value))

watch(visible, (v) => {
  if (v) clamp()
})

function close() {
  state.ctxMenu = null
}

function clamp() {
  const menu = state.ctxMenu
  if (!menu) return
  const h = menu.target.kind === 'collection' ? MENU_H_COLLECTION : MENU_H
  pos.value = {
    x: Math.min(menu.x, window.innerWidth - MENU_W - 8),
    y: Math.min(menu.y, window.innerHeight - h - 8),
  }
}

function onWindowClick(e: MouseEvent) {
  if (menuEl.value && !menuEl.value.contains(e.target as Node)) close()
}

async function toggleReadState() {
  const t = target.value
  if (t?.kind !== 'file') return
  const word = readWord.value
  close()
  try {
    const nowRead = await toggleRead(t.file.path)
    message.success(nowRead ? `已标记为${word}` : `已取消${word}`)
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  }
}

/** 整组标记 / 取消已读 */
async function markCollection(read: boolean) {
  const t = target.value
  if (t?.kind !== 'collection' || !t.files.length) return
  const paths = t.files.map((f) => f.path)
  const word = readWord.value
  close()
  try {
    await setReadBatch(paths, read)
    message.success(read ? `已把「${t.name}」全部标记为${word}` : `已取消「${t.name}」的${word}标记`)
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  }
}

async function open() {
  const t = target.value
  if (t?.kind !== 'file') return
  close()
  try {
    const res = await api.openFile(toPlain(t.file))
    if (res.ok && !res.fallback) message.success(`已启动: ${res.exe}`)
    else if (res.ok) message.warning(`未匹配规则，已用系统默认程序打开${res.reason ? '（' + res.reason + '）' : ''}`)
    else message.error(`打开失败: ${res.reason}`)
  } catch (e) {
    message.error('打开失败: ' + (e instanceof Error ? e.message : String(e)))
  }
}

async function openDefault() {
  const t = target.value
  if (t?.kind !== 'file') return
  close()
  try {
    const res = await api.openWithDefault(toPlain(t.file))
    if (!res.ok) message.error(`打开失败: ${res.error}`)
  } catch (e) {
    message.error('打开失败: ' + (e instanceof Error ? e.message : String(e)))
  }
}

async function copy() {
  const t = target.value
  if (t?.kind !== 'file') return
  close()
  await api.copyText(t.file.path)
  message.success('路径已复制')
}

function reveal() {
  const t = target.value
  if (t?.kind !== 'file') return
  close()
  api.reveal(t.file.path)
}

onMounted(() => {
  window.addEventListener('mousedown', onWindowClick)
  window.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') close()
  })
})

onBeforeUnmount(() => window.removeEventListener('mousedown', onWindowClick))
</script>

<template>
  <div
    v-if="visible"
    ref="menuEl"
    class="ctx-menu"
    :style="{ left: pos.x + 'px', top: pos.y + 'px' }"
    @mousedown.stop
  >
    <!-- 单文件菜单 -->
    <template v-if="isFile">
      <button class="ctx-item" @click="open">按规则打开</button>
      <button class="ctx-item" @click="openDefault">系统默认程序打开</button>
      <!-- 图片分类不提供已读/已看标记（见 isMarkable 的说明） -->
      <template v-if="canMark">
        <div class="ctx-divider"></div>
        <button class="ctx-item" :class="{ 'is-on': currentRead }" @click="toggleReadState">
          <span class="tick" aria-hidden="true">{{ currentRead ? '✓' : '' }}</span>
          {{ currentRead ? `取消${readWord}标记` : `标记${readWord}` }}
        </button>
      </template>
      <div class="ctx-divider"></div>
      <button class="ctx-item" @click="copy">复制文件路径</button>
      <button class="ctx-item" @click="reveal">在资源管理器中显示</button>
    </template>

    <!-- 作品集菜单：整组操作 -->
    <template v-else-if="target && target.kind === 'collection'">
      <div class="ctx-head" :title="target.name">{{ target.name }}</div>
      <div class="ctx-divider"></div>
      <template v-if="canMark">
        <button
          class="ctx-item"
          :disabled="collRead.total === 0 || collRead.read === collRead.total"
          @click="markCollection(true)"
        >
          <span class="tick" aria-hidden="true"></span>
          全部标记{{ readWord }}（{{ collRead.read }}/{{ collRead.total }}）
        </button>
        <button
          class="ctx-item"
          :disabled="collRead.read === 0"
          @click="markCollection(false)"
        >
          <span class="tick" aria-hidden="true"></span>
          取消全部{{ readWord }}
        </button>
      </template>
      <!--
        图片分类的整个作品集菜单只有"标记"这一类操作，隐藏之后会剩一个空壳。
        与其显示空菜单（看着像坏了），不如说明一句为什么没有操作。
      -->
      <div v-else class="ctx-note">图片分类无需标记</div>
    </template>
  </div>
</template>

<style scoped>
.ctx-menu {
  position: fixed;
  z-index: 3000;
  width: 180px;
  background: #1c1c27;
  border: 1px solid #2c2c3d;
  border-radius: 10px;
  padding: 5px;
  box-shadow: 0 8px 28px rgba(0, 0, 0, 0.5);
}

.shell.light .ctx-menu {
  background: #fff;
  border-color: #e2e3ea;
  box-shadow: 0 8px 28px rgba(0, 0, 0, 0.12);
}

.shell.light .ctx-item {
  color: #33333f;
}

.shell.light .ctx-item:hover {
  background: rgba(139, 92, 246, 0.12);
  color: #6d28d9;
}

.shell.light .ctx-divider {
  background: #e2e3ea;
}

.shell.vscode .ctx-menu {
  background: #252526;
  border-color: #3c3c3c;
  box-shadow: 0 8px 28px rgba(0, 0, 0, 0.55);
}

.shell.vscode .ctx-item {
  color: #cccccc;
}

.shell.vscode .ctx-item:hover {
  background: #04395e;
  color: #ffffff;
}

.shell.vscode .ctx-divider {
  background: #3c3c3c;
}

.shell.win11 .ctx-menu {
  background: rgba(255, 255, 255, 0.85);
  backdrop-filter: blur(32px) saturate(160%);
  border-color: rgba(0, 0, 0, 0.06);
  border-radius: 12px;
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.14), 0 2px 6px rgba(0, 0, 0, 0.08);
}

.shell.win11 .ctx-item {
  color: #1b1b1f;
  border-radius: 8px;
}

.shell.win11 .ctx-item:hover {
  background: rgba(0, 0, 0, 0.06);
  color: #005fb8;
}

.shell.win11 .ctx-divider {
  background: rgba(0, 0, 0, 0.08);
}

/* ---- 构成主义：菜单是一块硬边的黑框板 ---- */
.shell.construct .ctx-menu {
  background: #fbf9f4;
  border: 2px solid #1a1a1a;
  border-radius: 0;
  /* 偏移硬阴影，模拟油墨套印的错位 —— 不用柔和的浮起阴影 */
  box-shadow: 4px 4px 0 rgba(26, 26, 26, 0.9);
}

.shell.construct .ctx-item {
  color: #1a1a1a;
  border-radius: 0;
}

.shell.construct .ctx-item:hover {
  background: #d42a1e;
  color: #f3f0e9;
}

.shell.construct .ctx-divider {
  background: #1a1a1a;
  height: 1px;
  margin: 4px 6px;
}

/* 各主题下对勾的颜色：默认绿在白底/深底上都能读，但两个亮主题要压深一点 */
.shell.light .ctx-item .tick {
  color: #16a34a;
}

.shell.win11 .ctx-item .tick {
  color: #0f7b3f;
}

.shell.vscode .ctx-item .tick {
  color: #4ec9b0;
}

.shell.construct .ctx-item .tick {
  color: #d42a1e;
}

.shell.construct .ctx-item.is-on {
  color: #8a8579;
}

/* 各主题下 ctx-head 与禁用态的配色 */
.shell.light .ctx-head {
  color: #9a9aad;
}

.shell.light .ctx-item:disabled:hover {
  background: transparent;
  color: #33333f;
}

.shell.vscode .ctx-head {
  color: #8a8a8a;
}

.shell.vscode .ctx-item:disabled:hover {
  background: transparent;
  color: #cccccc;
}

.shell.win11 .ctx-head {
  color: #8a8a90;
}

.shell.win11 .ctx-item:disabled:hover {
  background: transparent;
  color: #1b1b1f;
}

.shell.construct .ctx-head {
  color: #8a8579;
  font-weight: 700;
  letter-spacing: 0.02em;
}

.shell.construct .ctx-item:disabled:hover {
  background: transparent;
  color: #1a1a1a;
}

.ctx-item {
  display: block;
  width: 100%;
  text-align: left;
  padding: 7px 10px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: #d6d6e0;
  font-size: 12.5px;
  cursor: pointer;
}

.ctx-item:hover {
  background: rgba(139, 92, 246, 0.2);
  color: #d8ccff;
}

/* 「标记已读」左边的对勾位：固定宽度占位，保证已读/未读两态文字左对齐 */
.ctx-item .tick {
  display: inline-block;
  width: 14px;
  margin-right: 2px;
  text-align: center;
  font-weight: 700;
  color: #7ee0a8;
}

/* 已读态：整条略微"退后"，与未读区分 */
.ctx-item.is-on {
  color: #9a9ab0;
}

/* 作品集菜单的标题行：不是按钮，纯展示 */
.ctx-head {
  padding: 5px 10px 6px;
  font-size: 11px;
  font-weight: 600;
  color: #8a8a9d;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/*
  说明性文字（非按钮）。
  用于"这个目标没有可执行操作"的场合 —— 比如图片分类的作品集。
  比留一个空菜单好：空菜单看着像坏了。
*/
.ctx-note {
  padding: 6px 10px 7px;
  font-size: 11.5px;
  line-height: 1.5;
  color: #6d6d82;
}

.shell.light .ctx-note {
  color: #9a9aad;
}

.shell.vscode .ctx-note {
  color: #8a8a8a;
}

.shell.win11 .ctx-note {
  color: #8a8a90;
}

.shell.construct .ctx-note {
  color: #8a8579;
}

/* 禁用条目：整组已读完时「全部标记已读」变灰，给出状态反馈 */
.ctx-item:disabled {
  opacity: 0.38;
  cursor: default;
}

.ctx-item:disabled:hover {
  background: transparent;
  color: #d6d6e0;
}

.ctx-divider {
  height: 1px;
  background: #2c2c3d;
  margin: 4px 6px;
}
</style>
