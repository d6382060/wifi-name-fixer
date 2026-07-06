//! 诊断报告与修复日志：生成、保存到桌面、在资源管理器中显示。

use crate::diagnose::Diagnosis;
use std::path::PathBuf;

pub fn build(d: &Diagnosis) -> String {
    let mut s = String::new();
    s.push_str("================================================\n");
    s.push_str("        Wi-Fi 名称显示修复助手 · 诊断报告\n");
    s.push_str("================================================\n");
    s.push_str(&format!("生成时间     : {}\n", now_string()));
    s.push_str(&format!("工具版本     : v{}\n", env!("CARGO_PKG_VERSION")));
    s.push_str(&format!("Windows 版本 : {}\n", d.win_version));
    s.push_str(&format!(
        "管理员权限   : {}\n",
        if d.is_admin { "是" } else { "否" }
    ));
    s.push_str("\n---------- 系统编码设置 ----------\n");
    s.push_str(&format!("ANSI 代码页 (ACP) : {}\n", d.acp_desc));
    s.push_str(&format!("OEM 代码页        : {}\n", d.oemcp));
    s.push_str(&format!(
        "Beta UTF-8 选项   : {}\n",
        if d.utf8_beta { "已勾选 ← 中文 WiFi 名乱码的常见原因" } else { "未勾选" }
    ));
    s.push_str(&format!("系统区域（非 Unicode 程序的语言）: {}\n", d.locale));
    s.push_str(&format!(
        "WLAN 服务         : {}\n",
        if d.service_ok { "运行中" } else { "未运行" }
    ));
    s.push_str("\n---------- WiFi 扫描 ----------\n");
    if !d.scan_error.is_empty() {
        s.push_str(&format!("扫描失败：{}\n", d.scan_error));
    } else {
        s.push_str(&format!("无线网卡 : {}\n", d.adapter));
        s.push_str(&format!(
            "共扫描到 {} 个网络，其中 {} 个在当前系统设置下会显示乱码\n\n",
            d.networks.len(),
            d.garbled_count
        ));
        for (i, n) in d.networks.iter().enumerate() {
            s.push_str(&format!(
                "[{}] 真实名称: {}\n     系统面板显示为: {}{}\n     编码: {} | 信号: {}% | 加密: {}{}\n     原始字节(HEX): {}\n",
                i + 1,
                n.real_name,
                n.system_shows,
                if n.garbled { "  ← 乱码" } else { "" },
                n.encoding,
                n.signal,
                n.auth,
                if n.connected { " | 当前已连接" } else { "" },
                n.hex
            ));
        }
    }
    s.push_str("\n---------- 诊断结论 ----------\n");
    s.push_str(&d.conclusion);
    s.push_str("\n\n---------- 建议 ----------\n");
    s.push_str(&d.advice);
    s.push_str("\n\n---------- 无线网卡驱动信息（netsh wlan show drivers 原文）----------\n");
    s.push_str(if d.driver_info.is_empty() { "（无）" } else { &d.driver_info });
    s.push_str("\n\n================ 报告结束 ================\n");
    s
}

fn now_string() -> String {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond
    )
}

/// 保存文本到桌面（UTF-8 带 BOM + CRLF，确保双击用记事本打开不乱码）。
/// 返回完整路径。
pub fn save_to_desktop(filename: &str, content: &str) -> Result<String, String> {
    let path = desktop_dir().join(filename);
    let text = format!("\u{FEFF}{}", content.replace('\n', "\r\n"));
    std::fs::write(&path, text.as_bytes()).map_err(|e| format!("写入失败：{}", e))?;
    Ok(path.display().to_string())
}

/// 桌面目录：优先 SHGetKnownFolderPath（正确处理 OneDrive 桌面重定向），失败退回 %USERPROFILE%\Desktop
fn desktop_dir() -> PathBuf {
    unsafe {
        use windows::Win32::System::Com::CoTaskMemFree;
        use windows::Win32::UI::Shell::{FOLDERID_Desktop, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
        if let Ok(p) = SHGetKnownFolderPath(&FOLDERID_Desktop, KF_FLAG_DEFAULT, None) {
            let s = p.to_string().unwrap_or_default();
            CoTaskMemFree(Some(p.0 as *const _));
            if !s.is_empty() {
                return PathBuf::from(s);
            }
        }
    }
    PathBuf::from(std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\".into())).join("Desktop")
}
