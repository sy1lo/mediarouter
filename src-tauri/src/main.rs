// 发布版不挂控制台窗口（debug 版保留，方便看 println 日志）
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    mediarouter_lib::run()
}
