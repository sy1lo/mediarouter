/**
 * 启动握手。
 *
 * Rust 侧窗口配的是 `visible: false`（避免白屏闪一下），
 * 所以这里必须在挂载完成后**无条件**回调 `ui_ready`——
 * 哪怕加载设置或扫描失败也要显示窗口，否则用户面对的是一个永不出现的窗口，
 * 而且没有任何提示。
 */

import { onMounted } from 'vue'
import { api } from '@/api'
import { loadLibrary, loadRead, loadTheme, state } from '@/stores/app'

/**
 * 等字体就绪。
 *
 * ⚠️ 这一步**不能省**。WebView 首帧渲染时用的往往还是回退字体
 * （中文环境下退到 Arial），此时 `line-height: normal` 是按回退字体的
 * 度量算的（比 Segoe UI / 微软雅黑窄）。等真实字体就绪后浏览器会重排，
 * 行高从 ~1.15em 跳到 ~1.3em —— 表现为用户说的「标记一下，整个界面跳一下」。
 *
 * 因为窗口此刻还没 show（`visible: false`），在这里等字体是**零成本**的：
 * 等到的这段时间用户本来就看不到窗口，不会多等、也不会闪。
 *
 * `document.fonts.ready` 只等**已声明且已使用**的字体，不会傻等全部字重；
 * 再加一个 1.5s 上限兜底 —— 万一某个字体永不 resolve，也不能拖住窗口显示。
 */
async function waitForFonts(timeoutMs = 1500): Promise<void> {
  if (!document.fonts) return
  await Promise.race([
    document.fonts.ready.then(() => undefined),
    new Promise<void>((r) => setTimeout(r, timeoutMs)),
  ])
}

export function useBootstrap(): void {
  onMounted(async () => {
    try {
      await loadTheme()
      // 已读集合与媒体库并行拉 —— 两者互不依赖，串行会白白多等一个往返
      await Promise.all([loadRead(), loadLibrary()])
      // 数据齐了再等字体：DOM 已经按真实内容布局，字体一到就能一次性定稿
      await waitForFonts()
      console.log(
        '[renderer] ready, library:',
        state.library ? Object.keys(state.library.categories).join(',') : 'none',
      )
    } catch (err) {
      console.error('[renderer] 初始化失败:', err)
    } finally {
      try {
        await api.uiReady()
      } catch (err) {
        console.error('[renderer] 显示窗口失败:', err)
      }
    }
  })
}
