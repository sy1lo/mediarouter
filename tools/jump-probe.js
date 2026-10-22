/**
 * 布局跳动探针（源文件，不参与 vite 构建）。
 *
 * 用途：验证「标记已读」前后，卡片/网格的尺寸是否发生任何变化。
 * 这是修掉用户反馈"标记后 UI 跳一下"的验收工具。
 *
 * 用法：
 *   node tools/inject-mock.mjs --probe   （把本文件也注入 dist）
 *   Edge --dump-dom "…?case=jump&probe=1" | grep jump3-out
 *
 * 判定标准：before 与 after 两棵 JSON 应当**完全一致**（除 strip 相关的新增节点）。
 */
;(function () {
  const pick = (el, props) => {
    const cs = getComputedStyle(el)
    const o = {}
    for (const p of props) o[p] = cs[p]
    o._h = +el.getBoundingClientRect().height.toFixed(2)
    o._y = +el.getBoundingClientRect().y.toFixed(2)
    return o
  }

  const CARD_PROPS = [
    'display', 'flexDirection', 'alignItems', 'justifyContent',
    'height', 'minHeight', 'overflow', 'position', 'gap', 'rowGap',
    'borderTopWidth', 'borderBottomWidth', 'paddingTop', 'paddingBottom',
  ]
  const META_PROPS = ['display', 'alignItems', 'height', 'minHeight', 'lineHeight', 'fontSize', 'marginTop', 'paddingTop', 'paddingBottom', 'gap']
  const NAME_PROPS = ['display', 'height', 'lineHeight', 'fontSize', 'webkitLineClamp', 'paddingTop', 'paddingBottom']

  const grab = () => {
    const card = document.querySelector('.file-card')
    const meta = document.querySelector('.file-card .file-meta')
    const name = document.querySelector('.file-card .file-name')
    const after = getComputedStyle(card, '::after')
    return {
      card: { ...pick(card, CARD_PROPS), scrollH: card.scrollHeight, clientH: card.clientHeight, offsetH: card.offsetHeight },
      meta: pick(meta, META_PROPS),
      name: pick(name, NAME_PROPS),
      afterContent: after.content,
      afterHeight: after.height,
      // 所有卡片的坐标（判断是否整体位移）
      allCards: [...document.querySelectorAll('.file-card, .coll-card')].map((el) => {
        const r = el.getBoundingClientRect()
        return { n: (el.querySelector('.file-name, .coll-name') || {}).textContent?.trim() || '', x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height * 100) / 100 }
      }),
      gridH: +document.querySelector('.file-grid').getBoundingClientRect().height.toFixed(2),
      gridRows: getComputedStyle(document.querySelector('.file-grid')).gridTemplateRows,
    }
  }

  let before = null
  const q = new URLSearchParams(location.search)
  if (q.get('probe') !== '1') return

  window.addEventListener('load', () => {
    setTimeout(() => {
      for (const el of document.querySelectorAll('.nav-item')) {
        const l = el.querySelector('.label')
        if (l && l.textContent.trim() === '小说') { el.dispatchEvent(new MouseEvent('click', { bubbles: true })); break }
      }
    }, 600)
    setTimeout(() => {
      for (const el of document.querySelectorAll('.coll-card')) {
        const nm = el.querySelector('.coll-name')
        if (nm && nm.textContent.includes('Bleach')) { el.dispatchEvent(new MouseEvent('click', { bubbles: true })); break }
      }
    }, 900)
    setTimeout(() => {
      before = grab()
      document.querySelector('.file-card').dispatchEvent(
        new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 500, clientY: 400 }),
      )
    }, 1600)
    setTimeout(() => {
      for (const it of document.querySelectorAll('.ctx-item')) {
        if (it.textContent && it.textContent.includes('标记')) { it.click(); break }
      }
    }, 2100)
    setTimeout(() => {
      const pre = document.createElement('pre')
      pre.id = 'jump3-out'
      pre.textContent = JSON.stringify({ before, after: grab() })
      document.body.appendChild(pre)
    }, 2700)
  })
})()
