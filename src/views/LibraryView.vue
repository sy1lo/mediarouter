<script setup lang="ts">
import { computed, ref } from 'vue'
import { useMessage } from 'naive-ui'
import { state, loadLibrary } from '@/stores/app'
import type { Collection, MediaFile } from '@/types'
import { fmtSize } from '@/utils/format'
import { toPlain } from '@/utils/plain'
import { api } from '@/api'
import CollectionCard from '../components/CollectionCard.vue'
import FileCard from '../components/FileCard.vue'
import ImageGrid from '../components/ImageGrid.vue'

const message = useMessage()
const query = ref('')
const collQuery = ref('')

const collections = computed(() => state.library?.categories[state.category]?.collections ?? [])

/**
 * 竖版封面的分类（漫画/小说）—— 卡片用 3:4 比例、`contain` 摆放。
 * 「图片」不在此列：它的封面是横版缩略图，与动漫一致。
 */
const isPortrait = computed(() => state.category === '漫画' || state.category === '小说')

/**
 * 图片分类走平铺填满的图墙（`ImageGrid`），而不是卡片网格。
 * 其余三个分类仍然是卡片 —— 它们要显示文件名、体积、时长这些文字信息，
 * 平铺会把这些挤掉。
 */
const isImage = computed(() => state.category === '图片')

const filteredCollections = computed(() => {
  const q = collQuery.value.trim().toLowerCase()
  if (!q) return collections.value
  return collections.value.filter((c) => c.name.toLowerCase().includes(q))
})

const current = computed<Collection | null>(() => {
  const col = collections.value.find((c) => c.id === state.collectionId)
  return col ?? null
})

const filtered = computed(() => {
  if (!current.value) return []
  const q = query.value.trim().toLowerCase()
  if (!q) return current.value.files
  return current.value.files.filter((f) => f.name.toLowerCase().includes(q))
})

function select(col: Collection) {
  state.collectionId = col.id
}

async function addCollection() {
  const dir = await api.pickDirectory()
  if (!dir) return
  const settings = await api.getSettings()
  const name = dir.split(/[\\/]/).pop() || dir
  settings.extraCollections[state.category] = settings.extraCollections[state.category] || []
  settings.extraCollections[state.category].push({ name, path: dir })
  await api.saveSettings(settings)
  await loadLibrary(true)
  state.collectionId = 'extra:' + dir
  message.success(`已添加作品集: ${name}`)
}

async function openFile(file: MediaFile) {
  try {
    const res = await api.openFile(toPlain(file))
    if (res.ok && !res.fallback) message.success(`已启动: ${res.exe}`)
    else if (res.ok) message.warning(`未匹配规则，已用系统默认程序打开${res.reason ? '（' + res.reason + '）' : ''}`)
    else message.error(`打开失败: ${res.reason}`)
  } catch (e) {
    message.error('打开失败: ' + (e instanceof Error ? e.message : String(e)))
  }
}

function showCtx(file: MediaFile, x: number, y: number) {
  state.ctxMenu = { x, y, target: { kind: 'file', file: toPlain(file) } }
}

function showCollCtx(col: Collection, x: number, y: number) {
  state.ctxMenu = {
    x,
    y,
    target: { kind: 'collection', name: col.name, files: col.files.map((f) => toPlain(f)) },
  }
}
</script>

<template>
  <div class="library">
    <aside class="collections-pane">
      <div class="pane-sticky">
        <div class="pane-head">
          <div class="pane-title">
            <b>{{ state.category }}</b>
            <span>作品集</span>
          </div>
          <button class="add-btn" @click="addCollection">+ 添加作品</button>
        </div>
        <div class="coll-search">
          <input v-model="collQuery" class="search" type="text" placeholder="搜索作品集…" />
        </div>
      </div>
      <div v-if="filteredCollections.length" class="coll-list">
        <CollectionCard
          v-for="col in filteredCollections"
          :key="col.id"
          :col="col"
          :active="col.id === state.collectionId"
          :portrait="isPortrait"
          @select="select(col)"
          @ctx="(x, y) => showCollCtx(col, x, y)"
        />
      </div>
      <div v-else class="empty-card">
        <div class="empty-icon"><i class="book-icon"></i></div>
        <div class="empty-title">{{ collections.length ? '没有匹配的作品集' : '暂无作品集' }}</div>
        <div class="empty-desc">{{ collections.length ? '换一个关键词试试' : '请在「设置」中添加根目录，或点击「+ 添加作品」手工挂载' }}</div>
      </div>
    </aside>

    <main class="files-pane">
      <template v-if="current">
        <div class="files-head">
          <div class="crumb">
            {{ state.category }} / <b>{{ current.name }}</b>
            <span v-if="current.loose" class="crumb-tag">根目录散件</span>
            <span v-if="current.extra" class="crumb-tag">手动添加</span>
            <span class="crumb-meta">{{ current.count }} 个文件 · {{ fmtSize(current.totalSize) }}</span>
          </div>
          <input v-model="query" class="search" type="text" placeholder="搜索文件…" />
        </div>
        <div v-if="filtered.length && isImage" class="image-host">
          <ImageGrid :files="filtered" @open="openFile" @ctx="showCtx" />
        </div>
        <div v-else-if="filtered.length" class="file-grid" :class="{ portrait: isPortrait }">
          <FileCard
            v-for="f in filtered"
            :key="f.path"
            :file="f"
            :portrait="isPortrait"
            @open="openFile(f)"
            @ctx="(x, y) => showCtx(f, x, y)"
          />
        </div>
        <div v-else class="empty-tip">没有匹配的文件</div>
      </template>
      <div v-else class="empty-card big">
        <div class="empty-icon"><i class="play-tri"></i></div>
        <div class="empty-title">选择左侧作品集查看文件</div>
        <div class="empty-desc">单击或双击文件，将按路由规则静默启动外部程序</div>
      </div>
    </main>
  </div>
</template>

<style scoped>
.library {
  flex: 1;
  display: flex;
  min-width: 0;
}

.collections-pane {
  width: 330px;
  min-width: 330px;
  border-right: 1px solid #232330;
  display: flex;
  flex-direction: column;
  background: #111118;
  overflow-y: auto;
}

.shell.light .collections-pane {
  background: #fafbfe;
  border-right-color: #e2e3ea;
}

.pane-sticky {
  position: sticky;
  top: 0;
  z-index: 2;
  background: #111118;
}

.shell.light .pane-sticky {
  background: #fafbfe;
}

.pane-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 14px 10px;
}

.coll-search {
  padding: 0 14px 12px;
}

.coll-search .search {
  width: 100%;
}

.pane-title b {
  font-size: 16px;
}

.pane-title span {
  margin-left: 6px;
  font-size: 12px;
  color: #7a7a90;
}

.add-btn {
  border: 1px solid rgba(139, 92, 246, 0.5);
  color: #d8ccff;
  background: rgba(139, 92, 246, 0.12);
  border-radius: 8px;
  padding: 5px 10px;
  font-size: 12px;
  cursor: pointer;
}

.add-btn:hover {
  background: rgba(139, 92, 246, 0.24);
}

.shell.light .add-btn {
  border-color: rgba(109, 40, 217, 0.45);
  color: #6d28d9;
  background: rgba(139, 92, 246, 0.1);
}

.shell.light .add-btn:hover {
  background: rgba(139, 92, 246, 0.18);
}

.shell.vscode .collections-pane {
  background: #252526;
  border-right-color: #3c3c3c;
}

.shell.vscode .pane-sticky {
  background: #252526;
}

.shell.vscode .pane-title span {
  color: #8a8a8a;
}

.shell.vscode .add-btn {
  background: #0e639c;
  border-color: #0e639c;
  color: #ffffff;
}

.shell.vscode .add-btn:hover {
  background: #1177bb;
}

.shell.vscode .files-head {
  background: #1e1e1e;
}

.shell.vscode .crumb {
  color: #8a8a8a;
}

.shell.vscode .crumb b {
  color: #e8e8e8;
}

.shell.vscode .crumb-meta {
  color: #7a7a7a;
}

.shell.vscode .search {
  background: #3c3c3c;
  border-color: #3c3c3c;
  color: #cccccc;
}

.shell.vscode .search:focus {
  border-color: #007fd4;
}

.shell.vscode .empty-card .empty-title {
  color: #cccccc;
}

.shell.vscode .empty-card .empty-desc {
  color: #8a8a8a;
}

.shell.vscode .empty-card .empty-icon {
  background: rgba(0, 122, 204, 0.12);
  border-color: rgba(0, 122, 204, 0.4);
}

.shell.vscode .play-tri {
  border-left-color: #0078d4;
}

.shell.vscode .book-icon {
  border-color: #0078d4;
}

.shell.vscode .book-icon::after {
  background: #0078d4;
}

.shell.win11 .library {
  gap: 12px;
}

.shell.win11 .collections-pane {
  background: rgba(255, 255, 255, 0.62);
  backdrop-filter: blur(28px) saturate(150%);
  border: 1px solid rgba(0, 0, 0, 0.06);
  border-radius: 14px;
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.05);
}

.shell.win11 .pane-sticky {
  background: rgba(255, 255, 255, 0.55);
  backdrop-filter: blur(24px);
  border-radius: 14px 14px 0 0;
}

.shell.win11 .pane-title b {
  font-weight: 600;
}

.shell.win11 .pane-title span {
  color: #8a8a90;
}

.shell.win11 .add-btn {
  background: #0078d4;
  border-color: #0078d4;
  color: #ffffff;
  border-radius: 8px;
  box-shadow: 0 2px 8px rgba(0, 120, 212, 0.3);
}

.shell.win11 .add-btn:hover {
  background: #106ebe;
}

.shell.win11 .files-pane {
  background: rgba(255, 255, 255, 0.55);
  backdrop-filter: blur(28px) saturate(150%);
  border: 1px solid rgba(0, 0, 0, 0.06);
  border-radius: 14px;
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.05);
}

.shell.win11 .files-head {
  background: rgba(246, 248, 251, 0.7);
  backdrop-filter: blur(20px);
  border-radius: 14px 14px 0 0;
}

.shell.win11 .crumb {
  color: #616161;
}

.shell.win11 .crumb b {
  color: #1b1b1f;
}

.shell.win11 .crumb-meta {
  color: #8a8a90;
}

.shell.win11 .search {
  background: #ffffff;
  border-color: rgba(0, 0, 0, 0.1);
  color: #1b1b1f;
  border-radius: 8px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.03);
}

.shell.win11 .search:focus {
  border-color: #0078d4;
  box-shadow: 0 0 0 3px rgba(0, 120, 212, 0.15);
}

.shell.win11 .empty-card .empty-title {
  color: #1b1b1f;
}

.shell.win11 .empty-card .empty-desc {
  color: #8a8a90;
}

.shell.win11 .empty-card .empty-icon {
  background: rgba(0, 120, 212, 0.1);
  border-color: rgba(0, 120, 212, 0.3);
}

.shell.win11 .play-tri {
  border-left-color: #0078d4;
}

.shell.win11 .book-icon {
  border-color: #0078d4;
}

.shell.win11 .book-icon::after {
  background: #0078d4;
}

/* ---- 构成主义 ---- */
.shell.construct .library {
  /* 与侧边栏的 3px 黑界呼应，留一条窄缝让纸面色透出来 */
  gap: 0;
}

.shell.construct .collections-pane {
  background: #f3f0e9;
  /* 中缝：同样是硬黑线 */
  border-right: 3px solid #1a1a1a;
}

.shell.construct .pane-sticky {
  background: #f3f0e9;
  /* 吸顶区底部压一道黑线，滚动时内容从线下穿过 */
  border-bottom: 2px solid #1a1a1a;
}

.shell.construct .pane-title b {
  font-weight: 800;
  letter-spacing: 0.02em;
  color: #1a1a1a;
}

.shell.construct .pane-title span {
  color: #8a8579;
  text-transform: uppercase;
  letter-spacing: 0.14em;
  font-weight: 700;
  font-size: 10px;
}

/* 主按钮：实心红块，直角，无阴影 —— 构成主义的按钮就是一块红的形状 */
.shell.construct .add-btn {
  background: #d42a1e;
  border: 2px solid #d42a1e;
  border-radius: 0;
  color: #f3f0e9;
  font-weight: 700;
  letter-spacing: 0.03em;
}

.shell.construct .add-btn:hover {
  background: #1a1a1a;
  border-color: #1a1a1a;
}

.shell.construct .files-pane {
  background: #f3f0e9;
}

.shell.construct .files-head {
  background: #f3f0e9;
  border-bottom: 2px solid #1a1a1a;
}

.shell.construct .crumb {
  color: #8a8579;
  letter-spacing: 0.02em;
}

.shell.construct .crumb b {
  color: #1a1a1a;
  font-weight: 800;
}

.shell.construct .crumb-meta {
  color: #8a8579;
}

.shell.construct .crumb-tag {
  border: 1px solid #1a1a1a;
  border-radius: 0;
  color: #1a1a1a;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

/* 搜索框：只有下边框的黑线，或一个完整的黑方框 */
.shell.construct .search {
  background: #fbf9f4;
  border: 2px solid #1a1a1a;
  border-radius: 0;
  color: #1a1a1a;
}

.shell.construct .search::placeholder {
  color: #a8a297;
}

.shell.construct .search:focus {
  border-color: #d42a1e;
  /* 不发光，只把框变红 —— 光晕不属于这套语言 */
  box-shadow: none;
}

.shell.construct .empty-card .empty-title {
  color: #1a1a1a;
  font-weight: 700;
  letter-spacing: 0.02em;
}

.shell.construct .empty-card .empty-desc {
  color: #8a8579;
}

/* 空状态图标：黑圆环 + 红方块，构成主义的两大母题 */
.shell.construct .empty-card .empty-icon {
  border-radius: 0;
  background: transparent;
  border: 2px solid #1a1a1a;
}

.shell.construct .play-tri {
  border-left-color: #d42a1e;
}

.shell.construct .book-icon {
  border-color: #1a1a1a;
}

.shell.construct .book-icon::after {
  background: #d42a1e;
}

.coll-list {
  padding: 4px 8px 14px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.files-pane {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
}

.files-head {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 16px 20px 12px;
  position: sticky;
  top: 0;
  background: #0e0e13;
  z-index: 2;
}

.shell.light .files-head {
  background: #f2f3f7;
}

.crumb {
  font-size: 13px;
  color: #9a9ab0;
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.crumb b {
  color: #e8e8f2;
}

.shell.light .crumb b {
  color: #2c2c38;
}

.crumb-tag {
  font-size: 10px;
  color: #d8ccff;
  border: 1px solid rgba(139, 92, 246, 0.4);
  border-radius: 4px;
  padding: 0 5px;
  margin-left: 6px;
  vertical-align: 1px;
}

.crumb-meta {
  margin-left: 10px;
  font-size: 11px;
  color: #6d6d82;
}

.search {
  width: 190px;
  background: #1a1a24;
  border: 1px solid #2c2c3d;
  color: #d6d6e0;
  border-radius: 8px;
  padding: 7px 11px;
  font-size: 12.5px;
  outline: none;
}

.search:focus {
  border-color: #8b5cf6;
}

.shell.light .search {
  background: #fff;
  border-color: #d8d9e0;
  color: #2c2c38;
}

.file-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(168px, 1fr));
  gap: 14px;
  padding: 6px 20px 24px;
  align-content: start;
}

.file-grid.portrait {
  grid-template-columns: repeat(auto-fill, minmax(132px, 1fr));
}

/* 图片分类的图墙：不用 grid —— 行高与每格宽度都由 ImageGrid 精算好，
   外层只负责给内边距。padding 与 .file-grid 保持一致，切换分类时左边缘不跳。 */
.image-host {
  padding: 6px 20px 24px;
}

.empty-tip.big {
  font-size: 15px;
  color: #9a9ab0;
}

.empty-tip .sub {
  display: block;
  margin-top: 8px;
  font-size: 12px;
  color: #6d6d82;
}

.empty-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  text-align: center;
  padding: 24px;
  gap: 8px;
}

.empty-card .empty-icon {
  width: 60px;
  height: 60px;
  border-radius: 50%;
  background: rgba(139, 92, 246, 0.1);
  border: 1px solid rgba(139, 92, 246, 0.28);
  display: flex;
  align-items: center;
  justify-content: center;
  margin-bottom: 8px;
}

.empty-card .empty-title {
  font-size: 15px;
  font-weight: 600;
  color: #d6d6e0;
}

.empty-card .empty-desc {
  font-size: 12px;
  color: #7a7a90;
  line-height: 1.8;
  max-width: 380px;
}

.empty-card.big {
  height: 100%;
}

.play-tri {
  width: 0;
  height: 0;
  border-left: 16px solid #8b5cf6;
  border-top: 10px solid transparent;
  border-bottom: 10px solid transparent;
  margin-left: 5px;
}

.book-icon {
  width: 26px;
  height: 20px;
  border: 2px solid #8b5cf6;
  border-radius: 3px;
  position: relative;
}

.book-icon::after {
  content: '';
  position: absolute;
  left: 7px;
  top: -2px;
  bottom: -2px;
  width: 2px;
  background: #8b5cf6;
  opacity: 0.55;
}

.shell.light .empty-card .empty-title {
  color: #2c2c38;
}

.shell.light .empty-card .empty-desc {
  color: #8a8a9d;
}
</style>
