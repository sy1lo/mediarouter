<script setup lang="ts">
import { ref, watch } from 'vue'
import { api } from '@/api'

const props = defineProps<{ path: string }>()

const ok = ref<boolean | null>(null)

watch(
  () => props.path,
  async (p) => {
    ok.value = null
    if (!p) return
    ok.value = await api.validateExe(p)
  },
  { immediate: true },
)
</script>

<template>
  <span v-if="path" class="exe-badge" :class="ok === null ? '' : ok ? 'ok' : 'bad'">
    {{ ok === null ? '检测中…' : ok ? '有效' : '无效' }}
  </span>
</template>

<style scoped>
.exe-badge {
  font-size: 10px;
  border-radius: 4px;
  padding: 0 5px;
  margin-left: 6px;
  flex-shrink: 0;
}

.exe-badge.ok {
  color: #4ade80;
  border: 1px solid rgba(74, 222, 128, 0.4);
}

.exe-badge.bad {
  color: #f87171;
  border: 1px solid rgba(248, 113, 113, 0.4);
}

.exe-badge:not(.ok):not(.bad) {
  color: #9a9ab0;
  border: 1px solid rgba(154, 154, 176, 0.35);
}

/* ---- 构成主义：方形令牌，字距拉开 ---- */
.shell.construct .exe-badge {
  border-radius: 0;
  font-weight: 700;
  letter-spacing: 0.06em;
}

.shell.construct .exe-badge.ok {
  color: #1a1a1a;
  border-color: #1a1a1a;
  background: rgba(26, 26, 26, 0.06);
}

.shell.construct .exe-badge.bad {
  color: #d42a1e;
  border-color: #d42a1e;
}

.shell.construct .exe-badge:not(.ok):not(.bad) {
  color: #8a8579;
  border-color: rgba(138, 133, 121, 0.5);
}
</style>
