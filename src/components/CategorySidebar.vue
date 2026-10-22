<script setup lang="ts">
import { computed } from 'vue'
import { useRouter } from 'vue-router'
import { useMessage } from 'naive-ui'
import { state, loadLibrary } from '@/stores/app'
import { CATEGORIES } from '@/types'
import { fmtTime } from '@/utils/format'

const message = useMessage()
const router = useRouter()

const CAT_DOT: Record<string, string> = {
  动漫: '#f472b6',
  漫画: '#fb923c',
  小说: '#38bdf8',
  // 图片用翠绿：与其余三个（粉/橙/蓝）在色相上拉开，缩到 9px 也能一眼分辨
  图片: '#34d399',
}

const counts = computed(() => {
  const out: Record<string, number> = {}
  for (const c of CATEGORIES) {
    const cats = state.library?.categories[c]
    out[c] = cats ? cats.collections.length : 0
  }
  return out
})

const scannedAt = computed(() => (state.library ? fmtTime(state.library.scannedAt) : '—'))

function select(c: string) {
  state.category = c
  state.collectionId = null
  if (router.currentRoute.value.path !== '/') router.push('/')
}

async function rescan() {
  await loadLibrary(true)
  message.success('媒体库已刷新')
}
</script>

<template>
  <aside class="sidebar">
    <div class="brand">
      <div class="logo">M</div>
      <div class="brand-text">
        <b>MediaRouter</b>
        <span>本地媒体路由平台</span>
      </div>
    </div>

    <div class="sec-title">媒体库</div>
    <nav class="nav">
      <button
        v-for="c in CATEGORIES"
        :key="c"
        class="nav-item"
        :class="{ active: state.category === c }"
        @click="select(c)"
      >
        <i class="dot" :style="{ background: CAT_DOT[c] }"></i>
        <span class="label">{{ c }}</span>
        <span class="count">{{ counts[c] }}</span>
      </button>
    </nav>

    <div class="sec-title">管理</div>
    <nav class="nav">
      <router-link class="nav-item" to="/rules">
        <i class="dot gray">⚙</i>
        <span class="label">路由</span>
      </router-link>
      <router-link class="nav-item" to="/settings">
        <i class="dot gray">⚙</i>
        <span class="label">设置</span>
      </router-link>
    </nav>

    <div class="sidebar-footer">
      <div class="scan-info">最近扫描 {{ scannedAt }}</div>
      <button class="rescan-btn" :disabled="state.ui.scanning" @click="rescan">
        {{ state.ui.scanning ? '扫描中…' : '重新扫描' }}
      </button>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  width: 180px;
  min-width: 180px;
  display: flex;
  flex-direction: column;
  background: #14141c;
  border-right: 1px solid #232330;
  padding: 14px 10px;
}

.shell.light .sidebar {
  background: #ffffff;
  border-right-color: #e2e3ea;
}

.shell.light .brand-text span {
  color: #9a9aad;
}

.shell.light .sec-title {
  color: #9a9aad;
}

.shell.light .nav-item {
  color: #4a4a5a;
}

.shell.light .nav-item:hover {
  background: rgba(0, 0, 0, 0.05);
}

.shell.light .nav-item.active {
  background: rgba(139, 92, 246, 0.12);
  color: #6d28d9;
}

.shell.light .dot.gray {
  color: #9a9aad;
}

.shell.light .count {
  color: #6d6d82;
  background: rgba(0, 0, 0, 0.06);
}

.shell.light .scan-info {
  color: #9a9aad;
}

.shell.light .rescan-btn {
  border-color: #d8d9e0;
  color: #4a4a5a;
}

.shell.light .rescan-btn:hover:not(:disabled) {
  border-color: #8b5cf6;
  color: #6d28d9;
}

.shell.vscode .sidebar {
  background: #252526;
  border-right-color: #3c3c3c;
}

.shell.vscode .logo {
  background: linear-gradient(160deg, #0e639c, #0078d4);
}

.shell.vscode .brand-text span,
.shell.vscode .sec-title {
  color: #8a8a8a;
}

.shell.vscode .nav-item {
  color: #cccccc;
}

.shell.vscode .nav-item:hover {
  background: #2a2d2e;
}

.shell.vscode .nav-item.active {
  background: rgba(0, 122, 204, 0.22);
  color: #ffffff;
}

.shell.vscode .dot.gray {
  color: #8a8a8a;
}

.shell.vscode .count {
  color: #8a8a8a;
  background: #3c3c3c;
}

.shell.vscode .scan-info {
  color: #8a8a8a;
}

.shell.vscode .rescan-btn {
  border-color: #3c3c3c;
  color: #cccccc;
}

.shell.vscode .rescan-btn:hover:not(:disabled) {
  border-color: #0078d4;
  color: #ffffff;
}

.shell.win11 .sidebar {
  background: rgba(255, 255, 255, 0.72);
  backdrop-filter: blur(28px) saturate(150%);
  border: 1px solid rgba(0, 0, 0, 0.06);
  border-radius: 14px;
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.05);
}

.shell.win11 .logo {
  background: linear-gradient(135deg, #0067c0 0%, #2b88d8 55%, #4cc2ff 100%);
  box-shadow: 0 3px 10px rgba(0, 103, 192, 0.35);
}

.shell.win11 .brand-text b {
  font-weight: 600;
}

.shell.win11 .brand-text span,
.shell.win11 .sec-title {
  color: #616161;
}

.shell.win11 .nav-item {
  color: #333336;
  border-radius: 8px;
}

.shell.win11 .nav-item:hover {
  background: rgba(0, 0, 0, 0.05);
}

.shell.win11 .nav-item.active {
  background: #e8f1fb;
  color: #005fb8;
}

.shell.win11 .dot.gray {
  color: #8a8a90;
}

.shell.win11 .count {
  color: #616161;
  background: rgba(0, 0, 0, 0.06);
}

.shell.win11 .scan-info {
  color: #8a8a90;
}

.shell.win11 .rescan-btn {
  border-color: rgba(0, 0, 0, 0.12);
  color: #333336;
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.6);
}

.shell.win11 .rescan-btn:hover:not(:disabled) {
  border-color: #0078d4;
  color: #005fb8;
  background: rgba(0, 120, 212, 0.07);
}

/* ---- 构成主义：纸白面 + 粗黑右边界，像一张贴在墙上的海报 ---- */
.shell.construct .sidebar {
  background: #f3f0e9;
  /* 3px 实心黑界 —— 构成主义的分隔就是一根硬线，不做 1px 灰描边 */
  border-right: 3px solid #1a1a1a;
}

/* Logo：黑方块打底，红色方块斜切一角 —— 经典构成主义几何。
   用 SVG 式的伪元素摆位：红色块固定 16px 见方、贴右下角旋转 45°，
   露出一块明确的红色三角，而不是被 overflow 裁掉的碎角 */
.shell.construct .logo {
  position: relative;
  background: #1a1a1a;
  border-radius: 0;
  color: #f3f0e9;
  overflow: hidden;
  letter-spacing: 0;
}

.shell.construct .logo::after {
  content: '';
  position: absolute;
  right: -8px;
  bottom: -8px;
  width: 16px;
  height: 16px;
  background: #d42a1e;
  transform: rotate(45deg);
}

.shell.construct .brand-text b {
  font-weight: 700;
  letter-spacing: 0.02em;
  color: #1a1a1a;
}

.shell.construct .brand-text span {
  color: #8a8579;
}

.shell.construct .sec-title {
  color: #8a8579;
  letter-spacing: 0.18em;
  font-weight: 700;
  text-transform: uppercase;
}

.shell.construct .nav-item {
  color: #3a3a35;
  border-radius: 0;
  position: relative;
}

.shell.construct .nav-item:hover {
  background: rgba(26, 26, 26, 0.06);
}

/* 选中：不是染色，是左侧一道红色实心条 + 极淡红底 */
.shell.construct .nav-item.active {
  background: rgba(212, 42, 30, 0.08);
  color: #1a1a1a;
  font-weight: 700;
}

.shell.construct .nav-item.active::before {
  content: '';
  position: absolute;
  left: -10px;
  top: 0;
  bottom: 0;
  width: 3px;
  background: #d42a1e;
}

.shell.construct .dot.gray {
  color: #8a8579;
}

.shell.construct .count {
  color: #1a1a1a;
  background: transparent;
  border: 1px solid rgba(26, 26, 26, 0.3);
  border-radius: 0;
  font-weight: 600;
  padding: 0 5px;
}

.shell.construct .scan-info {
  color: #8a8579;
}

.shell.construct .rescan-btn {
  border: 2px solid #1a1a1a;
  border-radius: 0;
  color: #1a1a1a;
  font-weight: 700;
  letter-spacing: 0.04em;
  background: transparent;
}

.shell.construct .rescan-btn:hover:not(:disabled) {
  border-color: #d42a1e;
  background: #d42a1e;
  color: #f3f0e9;
}

.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 4px 6px 16px;
}

.logo {
  width: 38px;
  height: 38px;
  border-radius: 10px;
  background: linear-gradient(160deg, #6d5ae6, #8b5cf6);
  color: #fff;
  font-weight: 800;
  font-size: 18px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.brand-text b {
  display: block;
  font-size: 14px;
}

.brand-text span {
  display: block;
  font-size: 11px;
  color: #8a8a9d;
}

.sec-title {
  font-size: 11px;
  color: #6d6d82;
  padding: 10px 8px 6px;
  letter-spacing: 1px;
}

.nav {
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 9px;
  width: 100%;
  padding: 9px 10px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: #b9b9c9;
  font-size: 13px;
  text-align: left;
  cursor: pointer;
  text-decoration: none;
}

.nav-item:hover {
  background: rgba(255, 255, 255, 0.05);
}

.nav-item.active {
  background: rgba(139, 92, 246, 0.16);
  color: #d8ccff;
}

.dot {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  flex-shrink: 0;
  font-style: normal;
  font-size: 11px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.dot.gray {
  background: transparent;
  color: #8a8a9d;
}

.label {
  flex: 1;
}

.count {
  font-size: 11px;
  color: #6d6d82;
  background: rgba(255, 255, 255, 0.06);
  border-radius: 9px;
  padding: 1px 7px;
}

.sidebar-footer {
  margin-top: auto;
  padding-top: 10px;
}

.scan-info {
  font-size: 11px;
  color: #6d6d82;
  padding: 0 4px 8px;
}

.rescan-btn {
  width: 100%;
  padding: 8px;
  border: 1px solid #2c2c3d;
  border-radius: 8px;
  background: transparent;
  color: #b9b9c9;
  cursor: pointer;
  font-size: 12px;
}

.rescan-btn:hover:not(:disabled) {
  border-color: #8b5cf6;
  color: #d8ccff;
}

.rescan-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
