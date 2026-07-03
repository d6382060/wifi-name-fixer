//! Windows 系统信息检测与低风险系统操作。

use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn get_acp() -> u32 {
    unsafe { windows::Win32::Globalization::GetACP() }
}

pub fn get_oemcp() -> u32 {
    unsafe { windows::Win32::Globalization::GetOEMCP() }
}

/// "Beta: 使用 Unicode UTF-8 提供全球语言支持" 勾选后，系统 ANSI 代码页变为 65001。
/// 该复选框没有独立的注册表开关，控制面板就是依据这一点显示勾选状态的。
pub fn utf8_beta_enabled() -> bool {
    get_acp() == 65001
}

/// 系统区域设置（"非 Unicode 程序的语言"）的名称，如 "zh-CN"
pub fn system_locale_name() -> String {
    let mut buf = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
    let n = unsafe { windows::Win32::Globalization::GetSystemDefaultLocaleName(&mut buf) };
    if n > 1 {
        String::from_utf16_lossy(&buf[..(n as usize - 1)])
    } else {
        "未知".into()
    }
}

/// 代码页的用户可读描述
pub fn codepage_desc(cp: u32) -> String {
    let name = match cp {
        936 => "简体中文 GBK —— 对中文 WiFi 名而言是正确设置",
        65001 => "UTF-8 —— 「Beta: Unicode UTF-8 全球语言支持」已开启",
        950 => "繁体中文 Big5",
        932 => "日文 Shift-JIS",
        949 => "韩文",
        1252 => "西欧语言（英语等）",
        1251 => "西里尔语",
        _ => "其他",
    };
    format!("{}（{}）", cp, name)
}

pub fn windows_version() -> String {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    match hklm.open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion") {
        Ok(k) => {
            let product: String = k.get_value("ProductName").unwrap_or_default();
            let disp: String = k.get_value("DisplayVersion").unwrap_or_default();
            let build: String = k.get_value("CurrentBuildNumber").unwrap_or_default();
            format!("{} {} (内部版本 {})", product, disp, build)
        }
        Err(_) => "未知".into(),
    }
}

pub enum ServiceState {
    Running,
    Stopped,
    NotFound,
}

/// 查询 WLAN 自动配置服务状态（只读，无需管理员）
pub fn wlan_service_state() -> ServiceState {
    use windows::core::w;
    use windows::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatus,
        SC_MANAGER_CONNECT, SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STATUS,
    };
    unsafe {
        let scm = match OpenSCManagerW(None, None, SC_MANAGER_CONNECT) {
            Ok(h) => h,
            Err(_) => return ServiceState::NotFound,
        };
        let state = match OpenServiceW(scm, w!("WlanSvc"), SERVICE_QUERY_STATUS) {
            Ok(svc) => {
                let mut st = SERVICE_STATUS::default();
                let ok = QueryServiceStatus(svc, &mut st).is_ok();
                let out = if ok && st.dwCurrentState == SERVICE_RUNNING {
                    ServiceState::Running
                } else {
                    ServiceState::Stopped
                };
                let _ = CloseServiceHandle(svc);
                out
            }
            Err(_) => ServiceState::NotFound,
        };
        let _ = CloseServiceHandle(scm);
        state
    }
}

/// 隐藏窗口执行命令并回收输出（输出尽力按 UTF-8 / OEM 代码页解码）
fn run_hidden(cmd: &str, args: &[&str]) -> (bool, String) {
    match Command::new(cmd)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(o) => {
            let mut text = decode_console(&o.stdout);
            let err = decode_console(&o.stderr);
            if !err.trim().is_empty() {
                text.push('\n');
                text.push_str(&err);
            }
            (o.status.success(), text.trim().to_string())
        }
        Err(e) => (false, format!("无法启动 {}：{}", cmd, e)),
    }
}

fn decode_console(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    let enc = match get_oemcp() {
        950 => encoding_rs::BIG5,
        932 => encoding_rs::SHIFT_JIS,
        _ => encoding_rs::GBK,
    };
    let (cow, _, _) = enc.decode(bytes);
    cow.into_owned()
}

/// 重启 WLAN 自动配置服务（低风险刷新手段；需要管理员权限，由调用方保证）。
/// 不改动服务的启动类型等任何配置，只是 stop + start。
pub fn restart_wlan_service() -> Result<String, String> {
    let mut log = String::new();
    // 服务本就停止时 stop 会报错，属正常情况，不视为失败
    let (_, out1) = run_hidden("net", &["stop", "wlansvc"]);
    if !out1.is_empty() {
        log.push_str(&out1);
        log.push('\n');
    }
    let (ok, out2) = run_hidden("net", &["start", "wlansvc"]);
    log.push_str(&out2);
    if ok || out2.contains("已经启动") {
        Ok(log)
    } else {
        Err(log)
    }
}

/// 打开控制面板的"区域"设置窗口（用户手动修复的入口）
pub fn open_region_settings() -> Result<(), String> {
    Command::new("control.exe")
        .arg("intl.cpl")
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("无法打开区域设置：{}", e))
}

/// netsh 输出的无线网卡驱动信息原文（放进报告和"技术详情"，不做脆弱的逐行解析）
pub fn wifi_driver_info() -> String {
    let (_, out) = run_hidden("netsh", &["wlan", "show", "drivers"]);
    out
}
