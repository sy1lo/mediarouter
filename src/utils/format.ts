/**
 * 展示用格式化工具。
 *
 * `toPlain` 已迁到 `utils/plain.ts`（它是响应式工具，不是格式化工具）。
 */

export function fmtSize(n: number): string {
  if (n == null || n === 0) return '—'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let v = n
  let i = 0
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024
    i++
  }
  return (i === 0 ? String(Math.round(v)) : v.toFixed(1)) + ' ' + units[i]
}

export function fmtDuration(sec: number): string {
  const h = Math.floor(sec / 3600)
  const m = Math.floor((sec % 3600) / 60)
  const s = Math.floor(sec % 60)
  const mm = String(m).padStart(2, '0')
  const ss = String(s).padStart(2, '0')
  return h ? `${h}:${mm}:${ss}` : `${m}:${ss}`
}

export function fmtTime(ms: number): string {
  const d = new Date(ms)
  const p = (x: number) => String(x).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`
}

export const VIDEO_EXTS = new Set([
  'mp4',
  'mkv',
  'avi',
  'mov',
  'webm',
  'flv',
  'wmv',
  'm4v',
  'ts',
  'rmvb',
])

/**
 * 「已读」的用词随分类变：
 *
 * - 小说、漫画是**读**的（图文）→ 已读
 * - 动漫是**看**的（视频）、图片是**看**的（画面）→ 已看
 *
 * 提到这里是因为原先这段判断在 `FileCard` / `CollectionCard` / `ContextMenu`
 * 三处各写了一遍，加「图片」分类时四处都要改、漏一处就会出现
 * 「右键菜单写已看、卡片上写已读」这种自相矛盾。现在只有这一个定义。
 *
 * 认不出的分类一律回「已读」—— 与改动前的默认行为一致。
 */
export function readWord(category: string | undefined | null): '已读' | '已看' {
  return category === '动漫' || category === '图片' ? '已看' : '已读'
}

/**
 * 该分类是否使用「已读/已看」标记。
 *
 * **图片不用** —— 图片是拿来扫一眼、挑一张的，没有"看到第几页/第几话"这种
 * 进度语义；在一面图墙上给每张挂个"已看"角标只会让版面变花。
 * （用户明确提过这点。）
 *
 * 做成函数而不是到处写 `!== '图片'`，是为了将来若要再加"不需要标记"的分类，
 * 只改这一处。
 */
export function supportsReadMark(category: string | undefined | null): boolean {
  return category !== '图片'
}
