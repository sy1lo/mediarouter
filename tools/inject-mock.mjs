/**
 * 把 dev mock（以及可选的探针）注入到 dist/index.html，用于 Edge 无头验证。
 *
 * vite build 会清空 dist，所以每次构建后都要重跑这个脚本。
 *
 * 用法：
 *   node tools/inject-mock.mjs           只注入 mock
 *   node tools/inject-mock.mjs --probe   额外注入布局跳动探针
 *   node tools/inject-mock.mjs --align   额外注入「已读标签右边缘对齐」探针
 *   node tools/inject-mock.mjs --tile    额外注入「平铺填满」布局探针
 *
 * 无论哪个模式，都会顺带把图片素材清单注入成 `window.__MOCK_IMAGES`。
 * 图片分类的验证要求「下发的宽高」与「真实图片形状」严格一致，
 * 两者都取自 `image-fixtures.mjs`，单一数据源 —— 否则布局算出来和实际
 * 渲染的图对不上，验证结论就不可信了。
 */
import { readFileSync, writeFileSync, existsSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { COLLECTIONS, LOOSE_FILES } from './image-fixtures.mjs'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const dist = join(root, 'dist')
const htmlPath = join(dist, 'index.html')
const withProbe = process.argv.includes('--probe')
const withAlign = process.argv.includes('--align')
const withTile = process.argv.includes('--tile')

if (!existsSync(dist)) {
  console.error('dist/ 不存在，先运行 vite build')
  process.exit(1)
}

const scripts = ['mock-bridge.js']
if (withProbe) scripts.push('jump-probe.js')
if (withAlign) scripts.push('align-probe.js')
if (withTile) scripts.push('tile-probe.js')

// 落地脚本。`__MOCK_IMAGES` 必须在 mock 之前定义，所以直接拼在同一个
// <script> 里（排 IIFE 之前），而不是另起一个标签。
for (const s of scripts) {
  let code = readFileSync(join(root, 'tools', s), 'utf8')
  if (s === 'mock-bridge.js') {
    // 带上作品集分组，mock 才能照着列表建出多级的媒体库
    const slim = (f) => ({ id: f.id, name: f.name, w: f.w, h: f.h })
    const bundle = {
      collections: COLLECTIONS.map((c) => ({
        id: c.id,
        name: c.name,
        files: c.files.map(slim),
      })),
      loose: LOOSE_FILES.map(slim),
    }
    code = `window.__MOCK_IMAGES = ${JSON.stringify(bundle)};\n${code}`
  }
  writeFileSync(join(dist, s), code)
}

let html = readFileSync(htmlPath, 'utf8')
// 先清掉上一轮注入的（vite 可能保留了 index.html 的改写）
html = html.replace(
  /\s*<script src="\.\/(mock-bridge|jump-probe|align-probe|tile-probe)\.js"><\/script>/g,
  '',
)

const tags = scripts.map((s) => `    <script src="./${s}"></script>`).join('\n')
// 必须插在主 module 脚本之前
html = html.replace(/(\s*)(<script type="module")/, `\n${tags}$1$2`)
writeFileSync(htmlPath, html)

console.log(`已注入: ${scripts.join(', ')}`)
