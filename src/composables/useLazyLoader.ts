/**
 * 图墙的懒加载调度器。
 *
 * ## 为什么要共享一个 IntersectionObserver
 *
 * 最直觉的写法是每个格子各建一个观察者。在几十个格子的规模上没问题，
 * 但 `ImageGrid` 面对的是**照片库** —— 几百到几千张图很正常，
 * 而 Chromium 明确警告过「每个元素一个 IntersectionObserver」的写法
 * 在大列表上开销显著（每个观察者都要参与每次布局后的交叉计算）。
 *
 * 所以由一个观察者统管：它拿 `entry.target` 反查回调，命中一次就解除观察。
 * 每个格子仍然只在自己接近视口时才挂 `src`，行为不变，开销降一个量级。
 *
 * 用 `provide`/`inject` 传递（而不是 props）是因为这属于"渲染基础设施"，
 * 不是业务数据 —— 不想让每一层组件都多一个透传的 prop。
 */

import { inject, provide, type InjectionKey } from 'vue'

export interface LazyLoader {
  /** 元素进入预加载范围时调用一次 `cb`，随后自动解除观察 */
  observe(el: Element, cb: () => void): void
  /** 元素提前销毁时解除观察（避免观察者拖住已卸载的 DOM） */
  unobserve(el: Element): void
}

const LAZY_KEY: InjectionKey<LazyLoader> = Symbol('media-router:lazy-loader')

export function provideLazyLoader(loader: LazyLoader): void {
  provide(LAZY_KEY, loader)
}

/** 拿不到提供者时返回 `null` —— 调用方应当直接加载，而不是把图卡住不显示 */
export function useLazyLoader(): LazyLoader | null {
  return inject(LAZY_KEY, null)
}

/**
 * 建一个"进入预加载范围即回调"的观察者。
 *
 * `rootMargin` 给一屏左右的提前量：滚动时图片已经在解了，
 * 而不是等它真的露出来才开始下载 —— 那种观感是"边滚边刷白框"。
 */
export function createLazyLoader(rootMargin = '600px 0px'): LazyLoader {
  const callbacks = new WeakMap<Element, () => void>()
  let io: IntersectionObserver | null = null

  const ensure = () => {
    if (io) return io
    io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (!e.isIntersecting) continue
          const cb = callbacks.get(e.target)
          // 先解除观察再回调：回调里可能触发重排，此时不该再收到通知
          io?.unobserve(e.target)
          callbacks.delete(e.target)
          cb?.()
        }
      },
      { rootMargin },
    )
    return io
  }

  return {
    observe(el, cb) {
      callbacks.set(el, cb)
      ensure().observe(el)
    },
    unobserve(el) {
      callbacks.delete(el)
      io?.unobserve(el)
    },
  }
}
