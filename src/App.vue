<script setup lang="ts">
import { NConfigProvider, NMessageProvider, NDialogProvider, darkTheme, zhCN, dateZhCN } from 'naive-ui'
import { state } from '@/stores/app'
import { useBootstrap } from '@/composables/useBootstrap'
import CategorySidebar from '@/components/CategorySidebar.vue'
import ContextMenu from '@/components/ContextMenu.vue'

const win11Overrides = {
  common: {
    primaryColor: '#0078d4',
    primaryColorHover: '#106ebe',
    primaryColorPressed: '#005a9e',
    primaryColorSuppl: '#106ebe',
    borderRadius: '8px',
  },
}

/**
 * 构成主义：主色用比海报正红稍暗、稍降饱和的红（#d42a1e），
 * 在白底上对比度足够、又不像纯 #f00 那样刺眼。
 * radius 统一收到 2px —— 构成主义的形是直角和斜角，没有圆角。
 */
const constructOverrides = {
  common: {
    primaryColor: '#d42a1e',
    primaryColorHover: '#e8473a',
    primaryColorPressed: '#a81f15',
    primaryColorSuppl: '#e8473a',
    borderRadius: '2px',
    borderColor: '#1a1a1a',
    textColorBase: '#1a1a1a',
  },
}

useBootstrap()
</script>

<template>
  <n-config-provider
    :theme="state.ui.theme === 'dark' || state.ui.theme === 'vscode' ? darkTheme : null"
    :theme-overrides="state.ui.theme === 'win11' ? win11Overrides : state.ui.theme === 'construct' ? constructOverrides : null"
    :locale="zhCN"
    :date-locale="dateZhCN"
  >
    <n-message-provider placement="bottom">
      <n-dialog-provider>
        <div class="shell" :class="state.ui.theme">
          <CategorySidebar />
          <router-view />
          <ContextMenu />
        </div>
      </n-dialog-provider>
    </n-message-provider>
  </n-config-provider>
</template>
