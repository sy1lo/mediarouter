//! 规则匹配与文件启动。
//!
//! 排版对齐旧版 `electron/router.js`：
//! - 规则按顺序匹配，第一条命中的生效
//! - 扩展名比较忽略大小写与前导点
//! - 启动用分离进程，不阻塞、不弹控制台窗口

use std::path::Path;
use std::process::{Command, Stdio};

use crate::model::Rule;
use crate::platform::hide_console;

pub enum LaunchOutcome {
    /// 成功用规则里的程序拉起
    Launched { exe: String },
    /// 需要回退到系统默认程序
    Fallback { reason: String },
}

/// 命中的规则（第一条生效）
pub fn match_rule(ext: &str, category: Option<&str>, rules: &[Rule]) -> Option<Rule> {
    let ext = ext.to_lowercase();
    for r in rules {
        if !r.enabled {
            continue;
        }
        if !r.category.is_empty() && Some(r.category.as_str()) != category {
            continue;
        }
        let hit = r.extensions.iter().any(|e| {
            e.trim()
                .trim_start_matches('.')
                .to_lowercase()
                == ext
        });
        if hit {
            return Some(r.clone());
        }
    }
    None
}

/// 把参数模板拆成 argv，`{file}` 替换为真实路径
pub fn render_args(template: &str, file_path: &str) -> Vec<String> {
    let tpl = if template.is_empty() { "{file}" } else { template };
    tpl.split_whitespace()
        .filter(|s| !s.is_empty())
        .map(|s| s.replace("{file}", file_path))
        .collect()
}

/// 用规则里的程序启动文件
pub fn launch(rule: &Rule, file_path: &str) -> LaunchOutcome {
    if rule.exe.is_empty() {
        return LaunchOutcome::Fallback {
            reason: "未配置程序".to_string(),
        };
    }
    if !Path::new(&rule.exe).exists() {
        return LaunchOutcome::Fallback {
            reason: "程序不存在".to_string(),
        };
    }

    let mut cmd = Command::new(&rule.exe);
    cmd.args(render_args(&rule.args, file_path))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_console(&mut cmd);

    match cmd.spawn() {
        Ok(_child) => LaunchOutcome::Launched {
            exe: rule.exe.clone(),
        },
        Err(e) => LaunchOutcome::Fallback {
            reason: e.to_string(),
        },
    }
}
