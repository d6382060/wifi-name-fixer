//! 桌面端入口薄壳 —— Tauri 2 官方结构：请勿在此添加逻辑，改 lib.rs。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    wifi_name_fixer_lib::run()
}
