/**
 * 把 Vue 的响应式代理还原成普通对象。
 *
 * 跨 IPC 传对象时用它兜一层。从 `utils/format.ts` 里拆出来 ——
 * 这是响应式相关工具，和"格式化"没有关系。
 */

export function toPlain<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T
}
