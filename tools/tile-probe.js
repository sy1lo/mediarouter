/**
 * 「平铺填满」布局探针 —— 验证图片分类的图墙是否真的铺满了、有没有变形。
 *
 * 用法：
 *   node tools/inject-mock.mjs --tile
 *   # ⚠️ 窗口高度要给够：末行的图在视口外就不会加载，比例校验会被跳过，
 *   #    上一版正是因此漏掉了末行被拉伸的 bug。见下方判据 2。
 *   Edge --headless --window-size=1400,2400 --dump-dom "...?case=image" > dom.html
 *   读 #tile-out
 *
 * 四项判据（都用数字，不靠眼睛）：
 *   1. **铺满**：非末行的 `Σ格宽 + 间隙` 必须等于容器宽度（允许 1px 取整误差）。
 *      这是「平铺填满」的字面含义 —— 差一点点就会在行尾留一条缝。
 *   2. **末行不被拉伸**：末行总宽必须 **≤** 容器宽度。
 *      ⚠️ 这条是补一个真实的漏检：早先版本把末行的 fillDiff 直接记为 0
 *      （理由"末行本来就不该铺满"），于是"末行被撑到满宽"反而蒙混过关 ——
 *      而那正是用户截图里"右下角那张只显示了一部分"的真因。
 *   3. **行内等高**：同一行的格子高度必须完全一致。
 *   4. **不变形**：每格的渲染宽高比必须等于图片自身宽高比
 *      （比 `img.naturalWidth/naturalHeight`，也就是真实图像的形状）。
 *      比例对不上、配合 `object-fit: cover` 就表现为裁切（用 `contain` 则表现为留边）。
 *      **末行必须一起校验** —— 它就是判据 2 那个 bug 的现场。
 */
;(function () {
  /**
   * 变形容差，单位是**像素**而不是百分比。
   *
   * 用百分比会误判：一张 120px 宽的小图差 4px 就是 3.3%，而一张 400px 的图
   * 差 4px 只有 1%。但人眼看到的是那 4px 的留边/裁切，与图的大小无关。
   * 布局的取整误差天然在 1px 内，所以阈值取 1.5px 既贴住物理事实又留一点余量。
   */
  const PX_TOLERANCE = 1.5

  /**
   * 基准比例的来源：**注入的素材表**，不是 `img.naturalWidth`。
   *
   * 一开始用的是 naturalWidth，结果在无头 dump-dom 下经常拿不到 ——
   * `decoding="async"` 的解码在虚拟时间里不一定跑完，于是所有比例校验被跳过，
   * 探针给出一个"看着通过、其实啥也没验"的结果（`loaded: 0`）。
   *
   * 素材表是确定的：它既是下发给前端的 `width/height` 的来源，
   * 也是 mock 生成缩略图比例的依据。拿它当基准才真正回答了
   * "盒子有没有按这张图的真实形状来排"。
   */
  function fixtureRatios() {
    const bundle = window.__MOCK_IMAGES
    const map = new Map()
    if (!bundle) return map
    for (const f of [...bundle.collections.flatMap((c) => c.files), ...bundle.loose]) {
      map.set(f.name, f.w / f.h)
    }
    return map
  }

  function run() {
    const emit = (verdict, extra) => {
      const el = document.createElement('div')
      el.id = 'tile-out'
      el.textContent = JSON.stringify({ verdict, ...(extra || {}) }, null, 1)
      el.style.display = 'none'
      document.body.appendChild(el)
    }

    const host = document.querySelector('.igrid')
    if (!host) return emit('NO_GRID')

    const hostW = host.getBoundingClientRect().width
    const rows = [...host.querySelectorAll('.igrow')]
    if (!rows.length) return emit('NO_ROWS')

    const ratios = fixtureRatios()
    const details = []
    let worstFill = 0 // 非末行与容器宽度的最大偏差
    let worstShift = 0 // 格宽相对"理想宽度"的最大偏离（px）
    let lastRowTotal = 0
    let imgsLoaded = 0
    let checked = 0
    let tileCount = 0

    rows.forEach((row, ri) => {
      const tiles = [...row.querySelectorAll('.tile')]
      tileCount += tiles.length
      const cs = getComputedStyle(row)
      const gap = parseFloat(cs.columnGap || cs.gap || '0') || 0

      const rects = tiles.map((t) => t.getBoundingClientRect())
      const total =
        rects.reduce((a, r) => a + r.width, 0) + gap * Math.max(0, tiles.length - 1)

      const heights = [...new Set(rects.map((r) => Math.round(r.height * 100) / 100))]
      const isLast = ri === rows.length - 1
      if (isLast) lastRowTotal = total

      // 末行不计入"铺满"偏差 —— 它本来就不该铺满，判据 2 单独管
      const fill = isLast ? 0 : Math.abs(total - hostW)
      if (fill > worstFill) worstFill = fill

      let rowWorstShift = 0
      let rowChecked = 0
      tiles.forEach((t) => {
        const img = t.querySelector('img')
        if (img && img.naturalWidth) imgsLoaded++

        // 用 title（文件名）反查素材表里的真实比例。
        // `?repeat=N` 会把同一个素材复制成 `名字-2.jpg`，所以先剥后缀再试一次。
        const title = t.getAttribute('title') || ''
        const stem = title.replace(/\.[^.]+$/, '')
        const truth = ratios.get(stem) ?? ratios.get(stem.replace(/-\d+$/, ''))
        if (!truth) return
        rowChecked++
        checked++
        const r = t.getBoundingClientRect()
        // 盒子的"理想宽度" = 行高 × 真实宽高比；偏差以像素计
        const shift = Math.abs(r.width - r.height * truth)
        if (shift > rowWorstShift) rowWorstShift = shift
      })
      if (rowWorstShift > worstShift) worstShift = rowWorstShift

      details.push({
        row: ri,
        tiles: tiles.length,
        isLast,
        total: +total.toFixed(2),
        hostWidth: +hostW.toFixed(2),
        fillDiff: +fill.toFixed(2),
        heights,
        uniformHeight: heights.length === 1,
        checked: rowChecked,
        worstPxShift: +rowWorstShift.toFixed(2),
      })
    })

    const problems = []

    const badFill = details.filter((d) => !d.isLast && d.fillDiff > 1).length
    if (badFill) problems.push(`${badFill} 行没铺满`)

    // 判据 2：末行被拉伸 = 总宽超过容器
    if (lastRowTotal > hostW + 1) {
      problems.push(`末行被拉伸到 ${lastRowTotal.toFixed(0)}px（容器 ${hostW.toFixed(0)}px）`)
    }

    const badHeight = details.filter((d) => !d.uniformHeight).length
    if (badHeight) problems.push(`${badHeight} 行高不一致`)

    if (worstShift > PX_TOLERANCE) {
      problems.push(`最大变形 ${worstShift.toFixed(1)}px（容差 ${PX_TOLERANCE}px）`)
    }
    if (checked < tileCount) {
      // 基准表没覆盖到某些格子会让上面两条失去意义，必须报出来
      problems.push(`只有 ${checked}/${tileCount} 格能对上基准比例`)
    }

    emit(problems.length ? 'BAD: ' + problems.join(' / ') : 'TILED', {
      hostWidth: +hostW.toFixed(2),
      rows: rows.length,
      tiles: tileCount,
      imgsLoaded,
      checked,
      lastRowTotal: +lastRowTotal.toFixed(2),
      worstFillDiff: +worstFill.toFixed(2),
      worstPxShift: +worstShift.toFixed(2),
      details,
    })
  }

  window.addEventListener('load', () => {
    // 等 mock 选分类 + 点开作品集 + 图片加载完成
    setTimeout(run, 3600)
  })
})()
