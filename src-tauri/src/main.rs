//! Wi-Fi 名称显示修复助手 —— 主入口。
//!
//! 运行模式：
//! - 双击运行：图形界面（默认不弹 UAC）
//! - WiFiNameFixer.exe --selftest [输出文件]：无界面自检，把诊断报告写到文件后退出
//! - 内部参数 --elevated：提权重启后的实例，启动即自动检测

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod diagnose;
mod privilege;
mod repair;
mod report;
mod system;
mod wifi;

use std::sync::Mutex;
use tauri::State;

/// 缓存最近一次诊断结果，供"保存诊断报告"使用
struct LastDiag(Mutex<Option<diagnose::Diagnosis>>);

#[tauri::command]
fn get_startup_info() -> serde_json::Value {
    serde_json::json!({
        "isAdmin": privilege::is_admin(),
        "autoScan": std::env::args().any(|a| a == "--elevated"),
        "version": env!("CARGO_PKG_VERSION"),
    })
}

/// 完整诊断（阻塞数秒；Tauri 同步命令自动在工作线程执行，不会卡界面）
#[tauri::command]
fn run_diagnosis(trigger_scan: bool, state: State<LastDiag>) -> diagnose::Diagnosis {
    let d = diagnose::run(trigger_scan);
    *state.0.lock().unwrap() = Some(d.clone());
    d
}

#[tauri::command]
fn one_click_fix() -> Result<String, String> {
    repair::one_click_fix()
}

#[tauri::command]
fn relaunch_admin(app: tauri::AppHandle) -> Result<(), String> {
    privilege::relaunch_as_admin()?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
fn open_region_settings() -> Result<(), String> {
    system::open_region_settings()
}

#[tauri::command]
fn save_report(state: State<LastDiag>) -> Result<String, String> {
    let guard = state.0.lock().unwrap();
    let d = guard.as_ref().ok_or("请先点击「开始检测」完成一次检测")?;
    let text = report::build(d);
    report::save_to_desktop("WiFiNameFixer_诊断报告.txt", &text)
}

#[tauri::command]
fn reveal_report(path: String) {
    report::reveal_in_explorer(&path);
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // 无界面自检模式（开发验证 / 高级用户远程协助用）
    if let Some(i) = args.iter().position(|a| a == "--selftest") {
        let d = diagnose::run(true);
        let text = report::build(&d);
        let out = args
            .get(i + 1)
            .cloned()
            .unwrap_or_else(|| "selftest_report.txt".into());
        let _ = std::fs::write(&out, format!("\u{FEFF}{}", text.replace('\n', "\r\n")));
        return;
    }

    if !webview2_installed() {
        warn_webview2_missing();
        // 不强行退出：注册表探测可能有遗漏，仍尝试启动
    }

    tauri::Builder::default()
        .manage(LastDiag(Mutex::new(None)))
        .invoke_handler(tauri::generate_handler![
            get_startup_info,
            run_diagnosis,
            one_click_fix,
            relaunch_admin,
            open_region_settings,
            save_report,
            reveal_report
        ])
        .run(tauri::generate_context!())
        .expect("Tauri 启动失败");
}

/// 检测 WebView2 运行时（Tauri 的渲染依赖；Win11 与近年 Win10 都自带）
fn webview2_installed() -> bool {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;
    const GUID_KEY: &str = r"\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    let candidates = [
        (HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\WOW6432Node{}", GUID_KEY)),
        (HKEY_LOCAL_MACHINE, format!(r"SOFTWARE{}", GUID_KEY)),
        (HKEY_CURRENT_USER, format!(r"SOFTWARE{}", GUID_KEY)),
    ];
    for (hive, key) in candidates.iter() {
        if let Ok(k) = RegKey::predef(*hive).open_subkey(key) {
            if let Ok(v) = k.get_value::<String, _>("pv") {
                if !v.is_empty() && v != "0.0.0.0" {
                    return true;
                }
            }
        }
    }
    false
}

fn warn_webview2_missing() {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK};
    let title = HSTRING::from("Wi-Fi 名称显示修复助手");
    let text = HSTRING::from(
        "本工具的界面需要「Microsoft Edge WebView2 运行时」支持。\n\n\
         多数 Windows 10/11 电脑已经自带；如果接下来窗口没能打开，\n\
         请在浏览器访问下面的网址，下载安装“常青版引导程序”后重试：\n\n\
         https://developer.microsoft.com/microsoft-edge/webview2/",
    );
    unsafe {
        MessageBoxW(None, &text, &title, MB_OK | MB_ICONWARNING);
    }
}
