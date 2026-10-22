<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useMessage, NModal, NSelect, NInput, NCheckbox, NSwitch, NButton, NTag } from 'naive-ui'
import { state } from '@/stores/app'
import { CATEGORIES, type Rule } from '@/types'
import { toPlain } from '@/utils/plain'
import { api } from '@/api'
import ExeBadge from '../components/ExeBadge.vue'

const message = useMessage()
const rules = ref<Rule[]>([])
const showEditor = ref(false)
const editingId = ref<string | null>(null)
const form = ref({ category: '动漫', extensions: '', exe: '', args: '{file}', enabled: true })

const CAT_OPTIONS = CATEGORIES.map((c) => ({ label: c, value: c }))

onMounted(async () => {
  rules.value = await api.getRules()
})

async function persist(): Promise<boolean> {
  const res = await api.saveRules(toPlain(rules.value))
  if (!res.ok) {
    message.error('规则保存失败: ' + (res.error || '未知错误'))
    return false
  }
  rules.value = res.rules ?? rules.value
  return true
}

function openCreate() {
  editingId.value = null
  form.value = { category: state.category, extensions: '', exe: '', args: '{file}', enabled: true }
  showEditor.value = true
}

function openEdit(r: Rule) {
  editingId.value = r.id
  form.value = { category: r.category, extensions: r.extensions.join(', '), exe: r.exe, args: r.args, enabled: r.enabled }
  showEditor.value = true
}

async function browseExe() {
  const p = await api.pickExe()
  if (p) form.value.exe = p
}

async function save() {
  const exts = form.value.extensions
    .split(/[,，\s]+/)
    .map((s) => s.trim().toLowerCase().replace(/^\./, ''))
    .filter(Boolean)
  if (!exts.length) {
    message.warning('请填写至少一个扩展名')
    return
  }
  const data: Rule = {
    id: editingId.value ?? 'r' + Date.now().toString(36),
    category: form.value.category,
    extensions: exts,
    exe: form.value.exe.trim(),
    args: form.value.args.trim() || '{file}',
    enabled: form.value.enabled,
  }
  if (editingId.value) {
    const i = rules.value.findIndex((r) => r.id === editingId.value)
    if (i >= 0) rules.value[i] = data
  } else {
    rules.value.push(data)
  }
  if (await persist()) {
    showEditor.value = false
    message.success('规则已保存')
  }
}

async function remove(r: Rule) {
  rules.value = rules.value.filter((x) => x.id !== r.id)
  await persist()
}

async function toggle(r: Rule) {
  r.enabled = !r.enabled
  await persist()
}
function matchCount(r: Rule): number {
  const lib = state.library
  if (!lib) return 0
  const cols = lib.categories[r.category]?.collections ?? []
  const exts = new Set(r.extensions)
  let n = 0
  for (const col of cols) for (const f of col.files) if (exts.has(f.ext.toLowerCase())) n++
  return n
}

const CAT_TAG: Record<string, string> = {
  动漫: 'info',
  漫画: 'warning',
  小说: 'success',
  图片: 'error',
}
</script>

<template>
  <main class="page">
    <div class="page-head">
      <div>
        <h2>路由规则</h2>
        <p>按「媒体类别 + 扩展名」匹配第三方程序，双击文件时静默启动。未匹配的文件回退到系统默认程序。</p>
      </div>
      <n-button type="primary" @click="openCreate">新建规则</n-button>
    </div>

    <div v-if="rules.length" class="rule-grid">
      <div v-for="r in rules" :key="r.id" class="rule-card">
        <div class="rule-top">
          <n-tag :type="(CAT_TAG[r.category] as any) || 'default'" size="small">{{ r.category }}</n-tag>
          <div class="rule-exts">
            <span v-for="e in r.extensions" :key="e" class="ext-chip">{{ e }}</span>
          </div>
          <div class="rule-actions">
            <n-switch :value="r.enabled" size="small" @update:value="toggle(r)" />
            <n-button quaternary size="tiny" @click="openEdit(r)">编辑</n-button>
            <n-button quaternary size="tiny" type="error" @click="remove(r)">删除</n-button>
          </div>
        </div>
        <div class="rule-row">
          <span class="k">程序</span>
          <code class="exe">{{ r.exe || '（未配置 → 系统默认）' }}</code>
          <ExeBadge :path="r.exe" />
        </div>
        <div class="rule-row">
          <span class="k">参数</span>
          <code>{{ r.args }}</code>
          <span class="k match">当前匹配 {{ matchCount(r) }} 个文件</span>
        </div>
      </div>
    </div>
    <div v-else class="empty-card">
      <div class="empty-icon"><i class="arrow-icon">→</i></div>
      <div class="empty-title">暂无路由规则</div>
      <div class="empty-desc">未匹配规则的文件将回退到系统默认程序打开</div>
      <div class="empty-examples">
        <div class="example-row"><span class="example-cat">动漫</span><code>+ .mkv → mpv.exe</code></div>
        <div class="example-row"><span class="example-cat">漫画</span><code>+ .epub → NeeView.exe</code></div>
        <div class="example-row"><span class="example-cat">小说</span><code>+ .epub → Readest.exe</code></div>
        <div class="example-row"><span class="example-cat">图片</span><code>+ .jpg → Honeyview.exe</code></div>
      </div>
      <div class="empty-desc foot">点击右上角「新建规则」创建，规则保存于用户数据目录 rules.json</div>
    </div>

    <n-modal v-model:show="showEditor" preset="card" :title="editingId ? '编辑规则' : '新建规则'" style="width: 460px">
      <div class="form">
        <div class="form-row">
          <label>媒体类别</label>
          <n-select v-model:value="form.category" :options="CAT_OPTIONS" />
        </div>
        <div class="form-row">
          <label>扩展名</label>
          <n-input v-model:value="form.extensions" placeholder="mp4, mkv, avi（逗号分隔）" />
        </div>
        <div class="form-row">
          <label>目标程序</label>
          <div class="exe-picker">
            <n-input v-model:value="form.exe" placeholder="如 D:\Tools\mpv.exe（留空则用系统默认）" />
            <n-button @click="browseExe">浏览…</n-button>
          </div>
        </div>
        <div class="form-row">
          <label>启动参数</label>
          <n-input v-model:value="form.args" placeholder="{file} 表示文件路径占位符" />
        </div>
        <div class="form-row">
          <label>启用</label>
          <n-checkbox v-model:checked="form.enabled">该规则生效</n-checkbox>
        </div>
        <div class="form-foot">
          <n-button type="primary" @click="save">保存</n-button>
        </div>
      </div>
    </n-modal>
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

.rule-grid {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.rule-card {
  background: #16161f;
  border: 1px solid #232330;
  border-radius: 12px;
  padding: 14px 16px;
}

.shell.light .rule-card {
  background: #fff;
  border-color: #e2e3ea;
}

.rule-top {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 10px;
}

.rule-exts {
  display: flex;
  gap: 5px;
  flex-wrap: wrap;
}

.ext-chip {
  font-size: 11px;
  background: rgba(139, 92, 246, 0.14);
  color: #d8ccff;
  border: 1px solid rgba(139, 92, 246, 0.3);
  border-radius: 5px;
  padding: 1px 7px;
}

.rule-actions {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 2px;
}

.rule-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
  margin-top: 6px;
}

.k {
  color: #7a7a90;
  flex-shrink: 0;
}

.k.match {
  margin-left: auto;
}

code {
  font-family: Consolas, monospace;
  font-size: 12px;
  color: #c9c9db;
  background: rgba(255, 255, 255, 0.04);
  padding: 3px 8px;
  border-radius: 6px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}

.exe {
  flex: 1;
}

.shell.light code {
  background: #f0f1f6;
  color: #4a4a5a;
}

.empty-tip .sub {
  display: block;
  margin-top: 8px;
  font-size: 12px;
  color: #6d6d82;
  max-width: 520px;
  line-height: 1.7;
}

.empty-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  padding: 48px 24px;
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

.empty-card .arrow-icon {
  font-style: normal;
  font-size: 24px;
  color: #8b5cf6;
  line-height: 1;
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
}

.empty-card .empty-desc.foot {
  margin-top: 10px;
}

.empty-card .empty-examples {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-top: 14px;
  background: rgba(255, 255, 255, 0.04);
  border: 1px solid #232330;
  border-radius: 10px;
  padding: 14px 22px;
}

.empty-card .example-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
}

.empty-card .example-cat {
  font-size: 11px;
  color: #d8ccff;
  border: 1px solid rgba(139, 92, 246, 0.35);
  border-radius: 5px;
  padding: 1px 8px;
  flex-shrink: 0;
}

.empty-card .example-row code {
  font-family: Consolas, monospace;
  font-size: 12px;
  color: #c9c9db;
}

.shell.light .empty-card .empty-title {
  color: #2c2c38;
}

.shell.light .empty-card .empty-desc {
  color: #8a8a9d;
}

.shell.light .empty-card .empty-examples {
  background: #f0f1f6;
  border-color: #e2e3ea;
}

.shell.light .empty-card .example-row code {
  color: #4a4a5a;
}

.shell.vscode .rule-card {
  background: #252526;
  border-color: #3c3c3c;
}

.shell.vscode code {
  background: #1e1e1e;
  color: #d4d4d4;
}

.shell.vscode .k {
  color: #8a8a8a;
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

.shell.vscode .empty-card .arrow-icon {
  color: #0078d4;
}

.shell.vscode .empty-card .empty-examples {
  background: #252526;
  border-color: #3c3c3c;
}

.shell.vscode .empty-card .example-row code {
  color: #d4d4d4;
}

.shell.vscode .empty-card .example-cat {
  color: #8ccbff;
  border-color: rgba(56, 148, 255, 0.45);
}

.shell.win11 .rule-card {
  background: #ffffff;
  border-color: rgba(0, 0, 0, 0.07);
  border-radius: 12px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.04), 0 4px 12px rgba(0, 0, 0, 0.05);
}

.shell.win11 code {
  background: #f0f2f5;
  color: #333336;
  border-radius: 6px;
}

.shell.win11 .k {
  color: #8a8a90;
}

.shell.win11 .ext-chip {
  background: rgba(0, 120, 212, 0.09);
  color: #005fb8;
  border-color: rgba(0, 120, 212, 0.3);
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

.shell.win11 .empty-card .arrow-icon {
  color: #0078d4;
}

.shell.win11 .empty-card .empty-examples {
  background: rgba(255, 255, 255, 0.8);
  border-color: rgba(0, 0, 0, 0.07);
  border-radius: 12px;
}

.shell.win11 .empty-card .example-row code {
  color: #333336;
}

.shell.win11 .empty-card .example-cat {
  color: #005fb8;
  border-color: rgba(0, 95, 184, 0.4);
}

/* ---- 构成主义 ---- */
.shell.construct .rule-card {
  background: #fbf9f4;
  border: 2px solid #1a1a1a;
  border-radius: 0;
}

.shell.construct .ext-chip {
  background: #1a1a1a;
  border: 1px solid #1a1a1a;
  border-radius: 0;
  color: #f3f0e9;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.shell.construct .k {
  color: #8a8579;
  font-weight: 600;
}

.shell.construct code {
  background: rgba(26, 26, 26, 0.05);
  border: 1px solid rgba(26, 26, 26, 0.16);
  border-radius: 0;
  color: #1a1a1a;
}

.shell.construct .empty-card .empty-icon {
  border-radius: 0;
  background: transparent;
  border: 2px solid #1a1a1a;
}

.shell.construct .empty-card .arrow-icon {
  color: #d42a1e;
  font-weight: 700;
}

.shell.construct .empty-card .empty-title {
  color: #1a1a1a;
  font-weight: 700;
  letter-spacing: 0.02em;
}

.shell.construct .empty-card .empty-desc {
  color: #8a8579;
}

.shell.construct .empty-card .empty-examples {
  background: #fbf9f4;
  border: 2px solid #1a1a1a;
  border-radius: 0;
}

.shell.construct .empty-card .example-cat {
  color: #f3f0e9;
  background: #d42a1e;
  border: 1px solid #d42a1e;
  border-radius: 0;
  font-weight: 700;
  letter-spacing: 0.06em;
}

.shell.construct .empty-card .example-row code {
  border: none;
  background: transparent;
  color: #1a1a1a;
}

.shell.construct .form-row label {
  color: #1a1a1a;
  font-weight: 700;
  letter-spacing: 0.03em;
}

.form {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding-top: 4px;
}

.form-row {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.form-row label {
  font-size: 12px;
  color: #9a9ab0;
}

.exe-picker {
  display: flex;
  gap: 8px;
}

.form-foot {
  display: flex;
  justify-content: flex-end;
  margin-top: 4px;
}
</style>
