//! 文件名排序 —— 与 Windows 资源管理器的「按名称排序」完全一致。
//!
//! ## 为什么不能用 ICU / `Intl.Collator`
//!
//! 前端原先用 `Intl.Collator('zh-CN', { numeric: true })` 排序。它对"中文按拼音"
//! 是合理的，**但那不是资源管理器的规则**：ICU 的中文排序把汉字放在拉丁字母
//! **之前**（代码点顺序恰好相反，`千` = U+5343 远大于 `B` = U+0042），于是：
//!
//! ```text
//! ICU（旧行为）                         资源管理器（用户期望）
//! 【黑白繁体高清版】[千年血战篇] 55      【黑白繁体高清版】[Bleach 死神] 01
//! 【黑白繁体高清版】[Bleach 死神] 01     【黑白繁体高清版】[Bleach 死神] 02
//!                                       【黑白繁体高清版】[千年血战篇] 01
//! ```
//!
//! 用户把两个窗口并排一放就发现了。**"与应用内的排序一致"这种需求，
//! 只要参考实现不是同一个函数，就永远会有下一个边角对不上。**
//!
//! ## 做法：直接调资源管理器自己用的那个函数
//!
//! Windows 的「按名称」排序就是 `shlwapi.dll` 的 `StrCmpLogicalW` ——
//! 它把数字段按**数值**比较（所以 `第2话` 在 `第10话` 之前），其余按码点。
//!
//! 不自己用 JS 去"模仿"它：数字前导零、全角数字、标点权重、繁简汉字、
//! 拉丁与 CJK 的相对顺序……边界多到不可能穷举对齐。直接调它，
//! 两边就**不可能**不一致 —— 因为它们是同一个实现。
//!
//! ⚠️ 这个函数在 `shlwapi.dll` 里，属于 `Win32_UI_Shell` 特性（已在 Cargo.toml 启用）。
//! 非 Windows 平台没有它，退回一套近似规则（见文件末尾），仅为可移植编译。

use std::cmp::Ordering;

/// 转成 NUL 结尾的 UTF-16 —— `StrCmpLogicalW` 要的就是这个形式
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 按资源管理器规则比较两个名字
pub fn compare(a: &str, b: &str) -> Ordering {
    compare_wide(&to_wide(a), &to_wide(b))
}

/// 就地按名字排序。
///
/// 用「先算键、再排序」而不是在比较器里现转 UTF-16：
/// 排序要做 O(n log n) 次比较，每次都转两遍字符串是无谓的浪费。
/// 对几千个文件来说差别不大，但先算键是免费的，没必要省这一点。
pub fn sort_by_name<T>(items: &mut Vec<T>, name_of: impl Fn(&T) -> String) {
    let mut keyed: Vec<(Vec<u16>, T)> = items
        .drain(..)
        .map(|it| (to_wide(&name_of(&it)), it))
        .collect();
    keyed.sort_by(|a, b| compare_wide(&a.0, &b.0));
    items.extend(keyed.into_iter().map(|(_, it)| it));
}

#[cfg(windows)]
fn compare_wide(a: &[u16], b: &[u16]) -> Ordering {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::StrCmpLogicalW;

    // SAFETY: 两个指针都来自存活的 `Vec<u16>`，且都以 NUL 结尾
    // （`to_wide` 保证了这一点），符合函数对 PCWSTR 的要求。
    let r = unsafe { StrCmpLogicalW(PCWSTR(a.as_ptr()), PCWSTR(b.as_ptr())) };
    // 文档只说返回负数/0/正数，不保证就是 -1/0/1
    r.cmp(&0)
}

/// 非 Windows 的近似实现：数字段按数值、其余按码点。
///
/// ⚠️ **不保证与资源管理器一致** —— 那需要在 Windows 上跑。
/// 留在这里只是为了让 crate 在非 Windows 平台仍能编译（本项目只发布 Windows 版）。
#[cfg(not(windows))]
fn compare_wide(a: &[u16], b: &[u16]) -> Ordering {
    fn is_digit(c: u16) -> bool {
        (b'0' as u16..=b'9' as u16).contains(&c)
    }
    fn trim_zeros(s: &[u16]) -> &[u16] {
        let k = s.iter().position(|&c| c != b'0' as u16).unwrap_or(s.len());
        &s[k..]
    }

    let (mut i, mut j) = (0usize, 0usize);
    while i < a.len() && j < b.len() {
        if is_digit(a[i]) && is_digit(b[j]) {
            let (i0, j0) = (i, j);
            while i < a.len() && is_digit(a[i]) {
                i += 1;
            }
            while j < b.len() && is_digit(b[j]) {
                j += 1;
            }
            // 去掉前导零再比：先比位数（等价于比数值大小），位数相同再逐位比
            let na = trim_zeros(&a[i0..i]);
            let nb = trim_zeros(&b[j0..j]);
            let ord = na.len().cmp(&nb.len()).then_with(|| na.cmp(nb));
            if ord != Ordering::Equal {
                return ord;
            }
        } else {
            let ord = a[i].cmp(&b[j]);
            if ord != Ordering::Equal {
                return ord;
            }
            i += 1;
            j += 1;
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

#[cfg(test)]
mod tests {
    use super::compare;
    use std::cmp::Ordering;

    #[test]
    fn numeric_runs_compare_as_numbers() {
        // 数字段按数值 —— 这是"按名称排序"最直观的一条
        assert_eq!(compare("第2话", "第10话"), Ordering::Less);
        assert_eq!(compare("第10话", "第2话"), Ordering::Greater);
        assert_eq!(compare("第2话", "第2话"), Ordering::Equal);
    }

    #[test]
    fn sorts_a_real_comic_folder_like_explorer() {
        // 用户实际反馈的那一组。ICU 会把「千年血战篇」排到「Bleach」之前，
        // 资源管理器的顺序是拉丁字母在前。
        let mut v = vec![
            "【黑白繁体高清版】[千年血战篇] 55.pdf",
            "【黑白繁体高清版】[Bleach 死神] 02.pdf",
            "【黑白繁体高清版】[Bleach 死神] 01.pdf",
            "【黑白繁体高清版】[千年血战篇] 01.pdf",
        ];
        super::sort_by_name(&mut v, |s| s.to_string());
        assert_eq!(
            v,
            vec![
                "【黑白繁体高清版】[Bleach 死神] 01.pdf",
                "【黑白繁体高清版】[Bleach 死神] 02.pdf",
                "【黑白繁体高清版】[千年血战篇] 01.pdf",
                "【黑白繁体高清版】[千年血战篇] 55.pdf",
            ]
        );
    }

    #[test]
    fn is_consistent_with_itself() {
        // 排序必须是全序：sort_by 的实现依赖这一点。
        // 只验证反对称性与自反性，不假设具体顺序（那是平台的规则）。
        let names = ["a", "B", "b", "1", "10", "2", "第1话", "第10话", "第2话", "あ", "啊"];
        for x in names {
            assert_eq!(compare(x, x), Ordering::Equal, "{x} 与自身必须相等");
            for y in names {
                // x 与 y 的比较结果必须与 y 与 x 相反
                assert_eq!(
                    compare(x, y),
                    compare(y, x).reverse(),
                    "{x} vs {y} 与反向不一致"
                );
            }
        }
    }

    #[test]
    fn sorts_in_place_without_losing_items() {
        let mut v: Vec<String> = (1..=50).map(|i| format!("卷{i}.cbz")).collect();
        let before = v.len();
        super::sort_by_name(&mut v, |s| s.clone());
        assert_eq!(v.len(), before, "排序不能丢条目");
        assert_eq!(v[0], "卷1.cbz");
        assert_eq!(v[1], "卷2.cbz");
        assert_eq!(v[9], "卷10.cbz", "卷10 应排在卷2 之后");
    }
}
