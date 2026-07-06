//! Wi-Fi 名称显示修复助手 —— 应用库入口（Tauri 2 官方推荐结构）。
//!
//! main.rs 仅是桌面端薄壳，全部逻辑集中在这里的 run()。
//! 所有耗时命令均为 async + spawn_blocking：同步命令会在主线程执行并冻结界面。
//!
//! 运行模式：
//! - 双击运行：图形界面（默认不弹 UAC）
//! - WiFiNameFixer.exe --selftest [输出文件]：无界面自检，报告写入文件后退出
//! - 内部参数 --elevated：提权重启后的实例，启动即自动开始检测

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

/// 完整诊断：内含约 4 秒的 WLAN 扫描等待和外部命令调用，属阻塞型工作，
/// 必须放入 spawn_blocking 线程池执行，主线程保持响应
#[tauri::command]
async fn run_diagnosis(
    trigger_scan: bool,
    state: State<'_, LastDiag>,
) -> Result<diagnose::Diagnosis, String> {
    let d = tauri::async_runtime::spawn_blocking(move || diagnose::run(trigger_scan))
        .await
        .map_err(|e| format!("诊断任务异常结束：{}", e))?;
    *state.0.lock().unwrap() = Some(d.clone());
    Ok(d)
}

/// 一键安全修复（重启 WLAN 服务，net stop/start 有数秒阻塞等待）
#[tauri::command]
async fn one_click_fix() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(repair::one_click_fix)
        .await
        .map_err(|e| format!("修复任务异常结束：{}", e))?
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
async fn save_report(state: State<'_, LastDiag>) -> Result<String, String> {
    // MutexGuard 不能跨 await：先克隆取出，再进入阻塞任务
    let d = state
        .0
        .lock()
        .unwrap()
        .clone()
        .ok_or("请先点击「开始检测」完成一次检测")?;
    tauri::async_runtime::spawn_blocking(move || {
        let text = report::build(&d);
        report::save_to_desktop("WiFiNameFixer_诊断报告.txt", &text)
    })
    .await
    .map_err(|e| format!("保存任务异常结束：{}", e))?
}

/// 在资源管理器中定位到报告文件（官方 opener 插件）
#[tauri::command]
fn reveal_report(path: String) -> Result<(), String> {
    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
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
