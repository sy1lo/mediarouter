/**
 * 对齐探针：量取左侧每个作品集卡片里「已读标签」的位置，
 * 验证它们是否落在**同一条右边缘竖线**上。
 *
 * 用法（在 inject-mock.mjs 之后、截图之前）：
 *   node tools/inject-mock.mjs --align
 *   Edge --headless --dump-dom "...?case=vary" > dom.html
 *   然后读 #align-out 的内容
 *
 * 输出格式：每个标签一行，附 rect.right / rect.bottom 与所属卡片。
 */
;(function () {
  function measure() {
    const out = []
    // 每个作品集卡片：找它内部的 .read-progress（标签已改到卡片右下角绝对定位）
    const cards = document.querySelectorAll('.coll-card')
    for (const card of cards) {
      const badge = card.querySelector('.read-progress')
      const nameEl = card.querySelector('.coll-name')
      if (!badge) continue

      const br = badge.getBoundingClientRect()
      const cr = card.getBoundingClientRect()
      const cs = getComputedStyle(badge)

      out.push({
        name: (nameEl?.textContent || '').trim().slice(0, 20),
        text: badge.textContent.trim(),
        hidden: badge.classList.contains('hidden'),
        // 关键指标：右边缘与下边缘（应当三张卡完全一致）
        right: +br.right.toFixed(2),
        bottom: +br.bottom.toFixed(2),
        // 与卡片右/下边缘的距离（定位是否锚定在固定 offset）
        gapRight: +(cr.right - br.right).toFixed(2),
        gapBottom: +(cr.bottom - br.bottom).toFixed(2),
        position: cs.position,
        cardRight: +cr.right.toFixed(2),
        cardBottom: +cr.bottom.toFixed(2),
      })
    }

    // 汇总判定
    const vis = out.filter((o) => !o.hidden)
    const rights = [...new Set(vis.map((o) => o.right))]
    const gapsR = [...new Set(vis.map((o) => o.gapRight))]
    const gapsB = [...new Set(vis.map((o) => o.gapBottom))]

    const verdict =
      vis.length === 0
        ? 'NO_TAGS'
        : gapsR.length === 1 && gapsB.length === 1
          ? 'ALIGNED'
          : `MISALIGNED (右侧间距 ${gapsR.length} 种, 下侧间距 ${gapsB.length} 种)`

    const el = document.createElement('div')
    el.id = 'align-out'
    el.textContent = JSON.stringify({ verdict, count: out.length, items: out }, null, 2)
    el.style.display = 'none'
    document.body.appendChild(el)
  }

  window.addEventListener('load', () => {
    // 等 mock 自动选分类 + 卡片渲染完成
    setTimeout(measure, 2600)
  })
})()
