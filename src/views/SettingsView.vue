<script setup lang="ts">
import { onMounted, ref, computed } from 'vue'
import { useMessage, useDialog, NButton, NSelect } from 'naive-ui'
import { state, loadLibrary, readSet } from '@/stores/app'
import { metaCache } from '@/stores/cache'
import { CATEGORIES, type ExtraCollection, type Settings, type ThemeName } from '@/types'
import { fmtTime } from '@/utils/format'
import { toPlain } from '@/utils/plain'
import { api } from '@/api'

const message = useMessage()
const dialog = useDialog()
const form = ref<Settings | null>(null)
const thumbCount = ref(0)
const ffStatus = ref<{
  ready: boolean
  dir: string | null
  source: 'settings' | 'auto' | 'path' | null
  ffmpeg: string | null
  ffprobe: string | null
} | null>(null)

/**
 * 兜底用的空设置。
 *
 * 分类清单直接从 `CATEGORIES` 生成 —— 以前这里手写了三遍分类名，
 * 加「图片」分类时得三处一起改，漏一处就会出现「有这一栏但字段是 undefined」的崩溃。
 */
function emptyByCategory<T>(make: () => T): Record<string, T> {
  const out: Record<string, T> = {}
  for (const c of CATEGORIES) out[c] = make()
  return out
}

const DEFAULT_SETTINGS: Settings = {
  categories: emptyByCategory<string[]>(() => []),
  extraCollections: emptyByCategory<ExtraCollection[]>(() => []),
  ffmpegPath: '',
  theme: 'dark',
}

onMounted(async () => {
  const s = await api.getSettings()
  form.value = {
    categories: { ...DEFAULT_SETTINGS.categories, ...s.categories },
    extraCollections: { ...DEFAULT_SETTINGS.extraCollections, ...s.extraCollections },
    ffmpegPath: s.ffmpegPath || '',
    theme: s.theme === 'light' ? 'light' : s.theme === 'vscode' ? 'vscode' : s.theme === 'win11' ? 'win11' : s.theme === 'construct' ? 'construct' : 'dark',
  }
  await refreshFfStatus()
  await refreshThumbCount()
})

async function refreshThumbCount() {
  thumbCount.value = await api.thumbCount()
}

async function clearThumbCache() {
  dialog.warning({
    title: '清空缩略图缓存',
    content: '将删除全部已生成的缩略图（当前 ' + thumbCount.value + ' 张），之后按需重新生成。确定继续吗？',
    positiveText: '清空',
    negativeText: '取消',
    onPositiveClick: async () => {
      await api.clearThumbs()
      metaCache.pdf.clear()
      metaCache.duration.clear()
      await refreshThumbCount()
      message.success('缩略图缓存已清空')
    },
  })
}

/** 已读数量直接读全局响应式集合，任何地方标记/取消都会让这里同步 */
const readCount = computed(() => readSet.size)

async function clearReadMarks() {
  if (readSet.size === 0) {
    message.info('当前没有已读标记')
    return
  }
  dialog.warning({
    title: '清空已读标记',
    content: `将取消全部 ${readSet.size} 个文件的已读/已看标记，所有文件恢复为未标记状态。确定继续吗？`,
    positiveText: '清空',
    negativeText: '取消',
    onPositiveClick: async () => {
      try {
        const n = await api.clearRead()
        readSet.clear()
        message.success(`已清空 ${n} 个已读标记`)
      } catch (e) {
        message.error('清空失败: ' + (e instanceof Error ? e.message : String(e)))
      }
    },
  })
}

async function refreshFfStatus() {
  ffStatus.value = await api.ffmpegStatus()
}

const ffSourceLabel = {
  settings: '手动指定',
  auto: '自动探测（程序目录）',
  path: '自动探测（系统 PATH）',
}

async function persist() {
  if (!form.value) return false
  const res = await api.saveSettings(toPlain(form.value))
  if (!res.ok) {
    message.error('设置保存失败: ' + (res.error || '未知错误'))
    return false
  }
  form.value = res.settings ?? form.value
  return true
}

async function addRoot(cat: string) {
  const dir = await api.pickDirectory()
  if (!dir || !form.value) return
  form.value.categories[cat].push(dir)
  await persist()
  await loadLibrary(true)
  message.success('已添加根目录并刷新媒体库')
}

async function removeRoot(cat: string, i: number) {
  if (!form.value) return
  form.value.categories[cat].splice(i, 1)
  await persist()
  await loadLibrary(true)
}

async function addExtra(cat: string) {
  const dir = await api.pickDirectory()
  if (!dir || !form.value) return
  const name = dir.split(/[\\/]/).pop() || dir
  form.value.extraCollections[cat].push({ name, path: dir })
  await persist()
  await loadLibrary(true)
  message.success('已添加手工作品集')
}

async function removeExtra(cat: string, i: number) {
  if (!form.value) return
  form.value.extraCollections[cat].splice(i, 1)
  await persist()
  await loadLibrary(true)
}

async function setTheme(v: ThemeName) {
  if (!form.value) return
  form.value.theme = v
  state.ui.theme = v
  await persist()
}

async function browseFfmpeg() {
  const dir = await api.pickDirectory()
  if (!dir || !form.value) return
  form.value.ffmpegPath = dir
  await persist()
  await refreshFfStatus()
}

async function rescan() {
  await loadLibrary(true)
  message.success('媒体库已刷新')
}
</script>

<template>
  <main class="page">
    <div class="page-head">
      <div>
        <h2>设置</h2>
        <p>媒体库根目录、手工作品集与路由相关配置。</p>
      </div>
      <n-button :loading="state.ui.scanning" type="primary" @click="rescan">重新扫描</n-button>
    </div>

    <template v-if="form">
      <section v-for="cat in CATEGORIES" :key="cat" class="card">
        <h3>{{ cat }} · 根目录</h3>
        <p class="hint">根目录下的每个子文件夹会被识别为一个作品集；直接放在根目录的文件归入「散件」。</p>
        <div v-if="form.categories[cat].length" class="row-list">
          <div v-for="(dir, i) in form.categories[cat]" :key="dir" class="row">
            <code class="path">{{ dir }}</code>
            <n-button quaternary size="tiny" type="error" @click="removeRoot(cat, i)">移除</n-button>
          </div>
        </div>
        <div v-else class="empty-line">尚未添加根目录</div>
        <div class="btn-line">
          <n-button size="small" @click="addRoot(cat)">+ 添加根目录</n-button>
        </div>

        <h3 class="sub-head">{{ cat }} · 手工作品集</h3>
        <p class="hint">将任意位置的文件夹手工挂载到本分类。</p>
        <div v-if="form.extraCollections[cat].length" class="row-list">
          <div v-for="(ex, i) in form.extraCollections[cat]" :key="ex.path" class="row">
            <b class="extra-name">{{ ex.name }}</b>
            <code class="path">{{ ex.path }}</code>
            <n-button quaternary size="tiny" type="error" @click="removeExtra(cat, i)">移除</n-button>
          </div>
        </div>
        <div v-else class="empty-line">尚未添加手工作品集</div>
        <div class="btn-line">
          <n-button size="small" @click="addExtra(cat)">+ 添加手工作品集</n-button>
        </div>
      </section>

      <section class="card">
        <h3>视频时长探测（可选）</h3>
        <div class="ff-status" :class="ffStatus?.ready ? 'ok' : 'warn'">
          <span class="ff-dot"></span>
          <b>{{ ffStatus ? (ffStatus.ready ? '已就绪' : '未检测到 ffprobe') : '检测中…' }}</b>
        </div>
        <p v-if="ffStatus?.ready" class="hint">
          使用目录：<code class="path inline">{{ ffStatus.dir }}</code>
          （{{ ffSourceLabel[ffStatus.source ?? 'path'] }}）— 视频缩略图将抽取真实帧，时长自动显示
        </p>
        <p v-else class="hint">
          未找到 ffmpeg：视频时长不会显示，缩略图降级为系统图标。将 ffmpeg 文件夹（以 ffmpeg- 开头）放在本程序同目录，或手动指定目录。
        </p>
        <div class="row">
          <code class="path">{{ form.ffmpegPath || '（留空则自动探测）' }}</code>
          <n-button quaternary size="tiny" @click="browseFfmpeg">选择目录</n-button>
          <n-button quaternary size="tiny" @click="refreshFfStatus">重新检测</n-button>
        </div>
      </section>

      <section class="card">
        <h3>外观</h3>
        <div class="row">
          <span>主题</span>
          <n-select
            style="width: 170px"
            :value="form.theme"
            :options="[
              { label: '暗色', value: 'dark' },
              { label: '亮色', value: 'light' },
              { label: 'VSCode 暗色', value: 'vscode' },
              { label: 'Windows 11 亮色', value: 'win11' },
              { label: '构成主义', value: 'construct' },
            ]"
            @update:value="setTheme($event as ThemeName)"
          />
        </div>
      </section>

      <section class="card">
        <h3>已读标记</h3>
        <p class="hint">
          标记后的文件会在缩略图左上角显示角标、卡片底边显示色条，方便区分看过与没看过的内容，画面本身不受影响。
          小说、漫画分类显示为"已读"，动漫分类显示为"已看"；图片分类不使用标记。
          标记保存在程序同目录的 <code class="path inline">read.json</code>，与文件路径绑定。
        </p>
        <div class="row">
          <span>当前已标记 {{ readCount }} 个文件</span>
          <n-button quaternary size="tiny" type="error" @click="clearReadMarks">清空已读标记</n-button>
        </div>
      </section>

      <section class="card">
        <h3>缩略图缓存</h3>
        <p class="hint">缩略图按需生成并缓存到 {{ 'MediaRouterData\\thumbs' }} 目录。若封面显示异常或更换文件后缩略图未更新，可手动清空重建。</p>
        <div class="row">
          <span>当前缓存 {{ thumbCount }} 张缩略图</span>
          <n-button quaternary size="tiny" type="error" @click="clearThumbCache">清空缩略图缓存</n-button>
        </div>
      </section>

      <section class="card">
        <h3>关于</h3>
        <p class="hint">
          最近扫描：{{ state.library ? fmtTime(state.library.scannedAt) : '—' }}。规则、设置与缩略图保存在程序同目录的
          <code>MediaRouterData</code> 文件夹；该目录不可写时自动改用 <code>%APPDATA%\media-launcher</code>。
        </p>
      </section>
    </template>
  </main>
</template>

<style scoped>
.page {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  padding: 22px 26px;
}

.page-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  margin-bottom: 18px;
}

.page-head h2 {
  margin: 0 0 6px;
  font-size: 18px;
}

.page-head p {
  margin: 0;
  color: #7a7a90;
  font-size: 12.5px;
}

.card {
  background: #16161f;
  border: 1px solid #232330;
  border-radius: 12px;
  padding: 16px 18px;
  margin-bottom: 14px;
  max-width: 860px;
}

.shell.light .card {
  background: #fff;
  border-color: #e2e3ea;
}

.card h3 {
  margin: 0 0 6px;
  font-size: 14px;
}

.card h3.sub-head {
  margin-top: 18px;
}

.hint {
  margin: 0 0 12px;
  font-size: 12px;
  color: #7a7a90;
}

.empty-line {
  font-size: 12px;
  color: #6d6d82;
  padding: 6px 0 8px;
}

.row-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 10px;
}

.row {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12.5px;
}

.row > span {
  color: #9a9ab0;
  flex-shrink: 0;
}

.path {
  flex: 1;
  font-family: Consolas, monospace;
  font-size: 12px;
  color: #c9c9db;
  background: rgba(255, 255, 255, 0.04);
  padding: 4px 9px;
  border-radius: 6px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.shell.light .path {
  background: #f0f1f6;
  color: #4a4a5a;
}

.extra-name {
  flex-shrink: 0;
  color: #d8ccff;
}

.shell.vscode .card {
  background: #252526;
  border-color: #3c3c3c;
}

.shell.vscode .path {
  background: #1e1e1e;
  color: #d4d4d4;
}

.shell.vscode .hint {
  color: #8a8a8a;
}

.shell.vscode .empty-line {
  color: #8a8a8a;
}

.shell.vscode .extra-name {
  color: #8ccbff;
}

.shell.win11 .card {
  background: rgba(255, 255, 255, 0.85);
  backdrop-filter: blur(24px) saturate(150%);
  border-color: rgba(0, 0, 0, 0.07);
  border-radius: 12px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04), 0 4px 12px rgba(0, 0, 0, 0.05);
}

.shell.win11 .path {
  background: #f0f2f5;
  color: #333336;
  border-radius: 6px;
}

.shell.win11 .hint {
  color: #8a8a90;
}

.shell.win11 .empty-line {
  color: #8a8a90;
}

.shell.win11 .extra-name {
  color: #005fb8;
}

/* ---- 构成主义 ---- */
.shell.construct .card {
  background: #fbf9f4;
  border: 2px solid #1a1a1a;
  border-radius: 0;
}

.shell.construct .card h3 {
  font-weight: 800;
  letter-spacing: 0.02em;
  color: #1a1a1a;
}

.shell.construct .hint {
  color: #8a8579;
}

.shell.construct .empty-line {
  color: #a8a297;
}

/* 路径：像一段被框起来的电传码 */
.shell.construct .path {
  background: rgba(26, 26, 26, 0.05);
  border: 1px solid rgba(26, 26, 26, 0.16);
  border-radius: 0;
  color: #1a1a1a;
}

.shell.construct .row > span {
  color: #8a8579;
  font-weight: 600;
}

.shell.construct .extra-name {
  color: #1a1a1a;
  font-weight: 700;
}

/* 状态灯：方点，不是圆点 */
.shell.construct .ff-dot {
  border-radius: 0;
}

.shell.construct .ff-status.ok .ff-dot {
  background: #1a1a1a;
  box-shadow: none;
}

.shell.construct .ff-status.ok b {
  color: #1a1a1a;
}

.shell.construct .ff-status.warn .ff-dot {
  background: #d42a1e;
  box-shadow: none;
}

.shell.construct .ff-status.warn b {
  color: #d42a1e;
}

.btn-line {
  display: flex;
  gap: 8px;
}

.ff-status {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  margin-bottom: 8px;
}

.ff-dot {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  flex-shrink: 0;
}

.ff-status.ok .ff-dot {
  background: #4ade80;
  box-shadow: 0 0 6px rgba(74, 222, 128, 0.6);
}

.ff-status.ok b {
  color: #4ade80;
}

.ff-status.warn .ff-dot {
  background: #fbbf24;
  box-shadow: 0 0 6px rgba(251, 191, 36, 0.6);
}

.ff-status.warn b {
  color: #fbbf24;
}

.path.inline {
  display: inline;
  padding: 2px 6px;
}
</style>
