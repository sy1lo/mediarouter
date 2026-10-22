/**
 * 无头验证用的 Tauri IPC 模拟层（源文件，不参与 vite 构建）。
 *
 * 用法：
 *   1) vite build
 *   2) node tools/inject-mock.mjs   （把本文件拷进 dist/ 并在 index.html 里插入 script 标签）
 *   3) python -m http.server（在 dist 里起服务）
 *   4) Edge --headless --screenshot
 *
 * 数据形状严格对齐 src/types.ts。`?case=` 控制场景：
 *   novel / comic / anime —— 对应分类，主作品集前 3 个已标记
 *   jump —— 全部未标记，自动右键标记第一个文件（观察布局是否跳动）
 *   vary —— **复刻用户截图**：三个作品集，元信息文字长短不一、
 *           已读标签字数也不一（1/3、1/15、已读完），用来验证标签是否对齐
 *   image —— 图片分类的平铺填满布局。素材来自 `image-fixtures.mjs`，
 *            带真实的 width/height，且 `orig` 协议指向 `dist/mockimg/*.svg`
 */
;(function () {
  const q = new URLSearchParams(location.search)
  const scene = q.get('case') || 'novel'
  const anime = scene === 'anime'
  const isImage = scene === 'image'

  const CATEGORY = anime ? '动漫' : scene === 'comic' ? '漫画' : isImage ? '图片' : '小说'
  const EXT = isImage ? 'jpg' : anime ? 'mp4' : 'pdf'

  const NAMES_ANIME = [
    '86-不存在的战区- [01]',
    '86-不存在的战区- [02]',
    '雪女',
    '赛马娘 新时代之门',
    '咒术回战 第二季 [01]',
    '咒术回战 第二季 [02]',
    '葬送的芙莉莲 [01]',
    '葬送的芙莉莲 [02]',
  ]
  const NAMES_BOOK = [
    'Bleach 死神 第01卷',
    'Bleach 死神 第02卷',
    'Bleach 死神 第03卷',
    'Bleach 死神 第04卷',
    '海贼王 第1080话',
    '海贼王 第1081话',
    '进击的巨人 第01卷',
    '葬送的芙莉莲 第01卷',
  ]
  const NAMES = anime ? NAMES_ANIME : NAMES_BOOK
  /**
   * 要自动点开哪个作品集。
   *
   * 默认取 image 场景的第一个；`?col=<名字>` 可以指定 ——
   * 验证"末行只有一张图"这类特定排布时需要点到那个作品集去
   * （见 `image-fixtures.mjs` 里的「尾行」）。
   */
  const COLL_NAME = isImage
    ? (q.get('col') || window.__MOCK_IMAGES?.collections?.[0]?.name || '')
    : anime
      ? '86-不存在的战区-'
      : 'Bleach 死神'

  const mkFile = (i) => ({
    name: `${NAMES[i]}.${EXT}`,
    path: `D:\\Media\\${CATEGORY}\\${COLL_NAME}\\${NAMES[i]}.${EXT}`,
    ext: EXT,
    size: 2_400_000 + i * 611_233,
    mtime: 1759200000000 + i * 3600000,
    category: CATEGORY,
  })

  const files = NAMES.map((_, i) => mkFile(i))

  const colMain = {
    id: 'col-main',
    name: COLL_NAME,
    path: `D:\\Media\\${CATEGORY}\\${COLL_NAME}`,
    count: files.length,
    totalSize: files.reduce((n, f) => n + f.size, 0),
    files,
    loose: false,
    extra: false,
  }

  const files2 = NAMES.slice(0, 4).map((n, i) => ({
    name: `${n} (另一组).${EXT}`,
    path: `D:\\Media\\${CATEGORY}\\副组\\${n}.${EXT}`,
    ext: EXT,
    size: 1_800_000 + i * 400_000,
    mtime: 1759100000000 + i * 3600000,
    category: CATEGORY,
  }))
  const colSub = {
    id: 'col-sub',
    name: anime ? '咒术回战 第二季' : '海贼王',
    path: `D:\\Media\\${CATEGORY}\\副组`,
    count: files2.length,
    totalSize: files2.reduce((n, f) => n + f.size, 0),
    files: files2,
    loose: false,
    extra: false,
  }

  const empty = { roots: [], collections: [] }
  const others = {}
  for (const c of ['动漫', '漫画', '小说', '图片']) if (c !== CATEGORY) others[c] = empty

  const library = {
    categories: { [CATEGORY]: { roots: [`D:\\Media\\${CATEGORY}`], collections: [colMain, colSub] }, ...others },
    scannedAt: 1759200000000,
  }

  /**
   * `vary` 场景：严格复刻用户那张截图。
   * 三个作品集刻意做成：
   *   - 元信息文字宽度不同（"3 个文件 · 654.0 MB" / "15 个文件 · 2.2 GB" / "1 个文件 · 56.0 MB"）
   *   - 已读标签字数不同（"已读 1/3" / "已读 1/15" / "已读完"）
   * 修复前，这两处差异会让标签横向位置各卡一处；修复后应当三张卡的标签
   * **右边缘完全对齐在同一条竖线上**。验证时量 `.read-progress` 的 rect.right。
   */
  const mkCol = (id, name, fileCount, totalSize, marked, hue) => {
    const fs = Array.from({ length: fileCount }, (_, i) => ({
      name: `${name} 第${String(i + 1).padStart(2, '0')}话.${EXT}`,
      path: `D:\\Media\\${CATEGORY}\\${name}\\${i}.${EXT}`,
      ext: EXT,
      size: Math.round(totalSize / fileCount),
      mtime: 1759200000000 + i * 3600000,
      category: CATEGORY,
      _hue: hue,
    }))
    return {
      id,
      name,
      path: `D:\\Media\\${CATEGORY}\\${name}`,
      count: fileCount,
      totalSize,
      files: fs,
      loose: false,
      extra: false,
      _marked: marked,
    }
  }

  const varyCols = [
    mkCol('v1', '败给了性格恶劣的天才青梅，初体验全部被夺走', 3, 654_000_000, 1, 350),
    mkCol('v2', '柑橘味香气citrus', 15, 2_200_000_000, 1, 200),
    mkCol('v3', '金牌得主', 1, 56_000_000, 1, 45),
  ]

  const varyLibrary = {
    categories: {
      [CATEGORY]: {
        roots: [`D:\\Media\\${CATEGORY}`],
        collections: varyCols.map(({ _marked, files, ...c }) => ({
          ...c,
          files: files.map(({ _hue, ...f }) => f),
        })),
      },
      ...others,
    },
    scannedAt: 1759200000000,
  }

  /** vary 场景的已读集合：v1 标 1/3、v2 标 1/15、v3 全标（→ "已读完"） */
  const varyRead = new Set(
    varyCols.flatMap((c) => c.files.slice(0, c._marked).map((f) => f.path)),
  )

  /**
   * `image` 场景：图片分类的平铺填满布局。
   *
   * 素材来自 `image-fixtures.mjs`（由 inject-mock 注入成 `window.__MOCK_IMAGES`），
   * 每张都带**真实宽高**，模拟后端 `imagedims` 的输出。
   * 对应的可加载图片由 inject-mock 生成到 `dist/mockimg/<id>.svg`，
   * 所以在无头浏览器里这是一堵真图墙，而不是一堆占位块。
   */
  const IMG_BUNDLE = window.__MOCK_IMAGES || { collections: [], loose: [] }
  /** path → fixture，供 convertFileSrc 反查该用哪张图 */
  const IMG_BY_PATH = new Map()

  const mkImgFile = (f, colName, i, suffix = '') => {
    const path = `D:\\Media\\图片\\${colName}\\${f.name}${suffix}.jpg`
    IMG_BY_PATH.set(path, f)
    return {
      name: `${f.name}${suffix}.jpg`,
      path,
      ext: 'jpg',
      // 用宽高比反推一个像样的体积：约 1.2 字节/像素
      size: Math.round(f.w * f.h * 1.2),
      mtime: 1759200000000 + i * 1800000,
      category: '图片',
      width: f.w,
      height: f.h,
    }
  }

  /**
   * `?repeat=N` 把主作品集复制 N 份 —— 用来验证**大图库**下的分批渲染。
   * 不复制的话每个作品集只有十几张，跑不出"一次建几千个 DOM 节点"的场景。
   */
  const REPEAT = Math.max(1, parseInt(q.get('repeat') || '1', 10) || 1)

  const imageCols = IMG_BUNDLE.collections.map((c) => {
    const fs = []
    for (let r = 0; r < REPEAT; r++) {
      const suffix = REPEAT > 1 ? `-${r + 1}` : ''
      fs.push(...c.files.map((f, i) => mkImgFile(f, c.name, i, suffix)))
    }
    return {
      id: 'img:' + c.id,
      name: c.name,
      path: `D:\\Media\\图片\\${c.name}`,
      count: fs.length,
      totalSize: fs.reduce((n, f) => n + f.size, 0),
      files: fs,
      loose: false,
      extra: false,
    }
  })

  const looseFiles = IMG_BUNDLE.loose.map((f, i) => mkImgFile(f, '根目录', i))
  if (looseFiles.length) {
    imageCols.push({
      id: 'img:loose',
      name: '图片',
      path: 'D:\\Media\\图片',
      count: looseFiles.length,
      totalSize: looseFiles.reduce((n, f) => n + f.size, 0),
      files: looseFiles,
      loose: true,
      extra: false,
    })
  }

  const imageLibrary = {
    categories: {
      图片: { roots: ['D:\\Media\\图片'], collections: imageCols },
      ...others,
    },
    scannedAt: 1759200000000,
  }

  /** 图片场景的已读：主作品集前 4 张标上，看角标在真图上是否读得清 */
  const imageRead = new Set(
    imageCols[0] ? imageCols[0].files.slice(0, 4).map((f) => f.path) : [],
  )

  const lib = scene === 'vary' ? varyLibrary : isImage ? imageLibrary : library
  const initRead = isImage
    ? imageRead
    : scene === 'vary'
      ? varyRead
      : new Set(scene === 'jump' ? [] : files.slice(0, 3).map((f) => f.path))

  let readSet = initRead

  const settings = {
    categories: { [CATEGORY]: [`D:\\Media\\${CATEGORY}`] },
    extraCollections: {},
    ffmpegPath: '',
    // `?theme=` 可覆盖，用来核对各主题下标签配色（默认构成主义）
    theme: q.get('theme') || 'construct',
  }

  const ok = (v) => Promise.resolve(v)

  /**
   * 假缩略图。
   *
   * ⚠️ 必须**按调用方给的比例**画，不能固定成一张图。
   * 图墙的盒子是按 `width/height` 算出来的，探针又会拿盒子的宽高比去比
   * `img.naturalWidth/naturalHeight` —— 如果假缩略图是固定比例，
   * 探针就会报出莫名其妙的"变形"，而那是 mock 的错，不是布局的错。
   */
  const fakeThumb = (label, hue, w = 320, h = 200) => {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}">
      <defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="hsl(${hue},42%,42%)"/>
        <stop offset="1" stop-color="hsl(${hue},36%,22%)"/>
      </linearGradient></defs>
      <rect width="${w}" height="${h}" fill="url(#g)"/>
      <circle cx="${w * 0.8}" cy="${h * 0.26}" r="${Math.min(w, h) * 0.17}" fill="rgba(255,255,255,.12)"/>
      <rect x="16" y="${h - 60}" width="${w * 0.38}" height="8" fill="rgba(255,255,255,.25)"/>
      <rect x="16" y="${h - 44}" width="${w * 0.24}" height="6" fill="rgba(255,255,255,.18)"/>
      <text x="16" y="${Math.min(60, h * 0.3)}" font-family="Segoe UI,sans-serif" font-size="24" font-weight="700"
            fill="rgba(255,255,255,.9)">${label}</text>
    </svg>`
    return 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg)
  }

  /**
   * 图片场景的缩略图：按 fixture 的真实宽高比出一张等比图，
   * 长边压到 512 —— 与真实缩略图的行为一致（等比例缩小，不裁不拉）。
   */
  const thumbForFixture = (f) => {
    const scale = 512 / Math.max(f.w, f.h)
    const w = Math.max(2, Math.round(f.w * scale))
    const h = Math.max(2, Math.round(f.h * scale))
    return fakeThumb(`${f.w}x${f.h}`, (f.id.charCodeAt(1) * 47 + f.name.length * 31) % 360, w, h)
  }

  window.__TAURI_INTERNALS__ = {
    transformCallback: (cb) => {
      const id = Math.floor(Math.random() * 1e9)
      window[`_${id}`] = cb
      return id
    },
    unregisterCallback: () => {},
    convertFileSrc: (p) => p,
    invoke: (cmd, args) => {
      switch (cmd) {
        case 'library_get':
        case 'library_rescan':
          return ok(lib)
        case 'settings_get':
          return ok(settings)
        case 'exe_validate':
          // 规则页的「有效/无效」角标。返回 null 会一直停在"检测中…"
          return ok(true)
        case 'rules_get':
          // 几条像样的规则：路由页截图与"匹配数"统计都靠它
          return ok([
            { id: 'r1', category: '动漫', extensions: ['mp4', 'mkv', 'ts'], exe: 'D:\\Apps\\mpv\\mpv.exe', args: '{file}', enabled: true },
            { id: 'r2', category: '漫画', extensions: ['zip', 'cbz', 'cbr'], exe: 'D:\\Apps\\NeeView\\NeeView.exe', args: '"{file}"', enabled: true },
            { id: 'r3', category: '小说', extensions: ['epub', 'txt'], exe: 'D:\\Apps\\Readest\\Readest.exe', args: '"{file}"', enabled: true },
            { id: 'r4', category: '图片', extensions: ['jpg', 'png'], exe: 'D:\\Apps\\Honeyview\\Honeyview.exe', args: '"{file}"', enabled: true },
            { id: 'r5', category: '动漫', extensions: ['rmvb'], exe: 'D:\\Apps\\potplayer\\PotPlayerMini64.exe', args: '"{file}"', enabled: false },
          ])
        case 'read_get':
          return ok([...readSet])
        case 'read_toggle': {
          const p = args.path
          if (readSet.has(p)) readSet.delete(p)
          else readSet.add(p)
          return ok(readSet.has(p))
        }
        case 'read_set': {
          for (const p of args.paths) {
            if (args.read) readSet.add(p)
            else readSet.delete(p)
          }
          return ok(readSet.size)
        }
        case 'read_clear':
          readSet = new Set()
          return ok(0)
        case 'app_data_dir':
          return ok('D:\\MediaRouter\\data')
        case 'ffmpeg_status':
          return ok({ ready: false, dir: null, source: null, ffmpeg: null, ffprobe: null })
        case 'thumbs_count':
          return ok(0)
        case 'ui_ready':
          return ok(undefined)
        case 'file_thumb': {
          const p = String(args.path)
          // 图片场景：按该图真实比例出图（探针会拿它对盒子的宽高比）
          const fixture = IMG_BY_PATH.get(p)
          if (fixture) return ok(thumbForFixture(fixture))
          let h = 0
          for (const ch of p) h = (h * 31 + ch.charCodeAt(0)) % 360
          return ok(fakeThumb(p.slice(-16, -4), h))
        }
        case 'file_meta':
          return ok({ duration: 1440 })
        default:
          return ok(null)
      }
    },
  }

  window.__scene = scene
  // 布局探针模式（?probe=1）下不做任何自动交互，交给 jump-probe.js 自己控制时序
  if (q.get('probe') === '1') return

  /**
   * 直接以别的页面启动时（`#/settings`、`#/rules`）不要做自动交互。
   *
   * 原因：下面的自动流程会点侧边栏分类，而 `select()` 里有一句
   * 「不在首页就 push('/')」—— 于是刚打开的设置页会被立刻弹回媒体库，
   * 截图截到的还是首页（实测三张图 md5 完全相同才发现）。
   */
  const startPath = location.hash.replace(/^#/, '') || '/'
  if (startPath !== '/') return

  window.addEventListener('load', () => {
    // 切到目标分类（默认停在"动漫"）
    setTimeout(() => {
      for (const el of document.querySelectorAll('.nav-item')) {
        const label = el.querySelector('.label')
        if (label && label.textContent.trim() === CATEGORY) {
          el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
          break
        }
      }
    }, 700)

    // 点开主作品集（按名字匹配 —— 列表会按拼音重排，位置不固定）
    setTimeout(() => {
      for (const el of document.querySelectorAll('.coll-card')) {
        const nm = el.querySelector('.coll-name')
        if (nm && nm.textContent.includes(COLL_NAME)) {
          el.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
          break
        }
      }
    }, 1000)

    // 右键第一个文件。
    //   jump 场景 —— 再点「标记」（用来观察标记前后是否跳动）
    //   ?ctx=1    —— 只弹菜单不点（用来截图检查菜单项，图片分类应无"标记"项）
    setTimeout(() => {
      const wantMenu = q.get('ctx') === '1'
      if (scene !== 'jump' && !wantMenu) return
      // 图片分类的格子是 .tile，其余分类是 .file-card
      const cards = document.querySelectorAll('.file-card, .tile')
      if (!cards[0]) return
      cards[0].dispatchEvent(
        new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 500, clientY: 400 }),
      )
      if (!wantMenu) {
        setTimeout(() => {
          for (const it of document.querySelectorAll('.ctx-item')) {
            if (it.textContent && it.textContent.includes('标记')) {
              it.click()
              break
            }
          }
        }, 400)
      }
    }, 1600)
  })
})()
