//! WiFi 扫描与 SSID 编码判定。
//!
//! 关键点：通过 Native WiFi API 拿到 SSID 的**原始字节**（DOT11_SSID），
//! 而不是 netsh 输出的、已被系统按错误代码页解码过的乱码文本。
//! 全部为只读操作，不修改任何系统状态。

use serde::Serialize;
use std::collections::HashMap;
use windows::core::GUID;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::NetworkManagement::WiFi::{
    WlanCloseHandle, WlanEnumInterfaces, WlanFreeMemory, WlanGetAvailableNetworkList,
    WlanOpenHandle, WlanScan, WLAN_AVAILABLE_NETWORK_LIST, WLAN_INTERFACE_INFO_LIST,
};

/// WlanOpenHandle 在 WLAN 服务未运行时返回的 Win32 错误码
const ERROR_SERVICE_NOT_ACTIVE: u32 = 1062;

#[derive(Clone, Serialize)]
pub struct WifiNetwork {
    /// 按判定出的编码解出的真实名称
    pub real_name: String,
    /// 模拟当前系统设置下，系统 WiFi 面板里显示的样子（可能是乱码）
    pub system_shows: String,
    /// 名称编码类型（给用户看的中文描述）
    pub encoding: String,
    /// 在当前系统设置下，系统面板显示是否和真名不一致（= 用户看到乱码）
    pub garbled: bool,
    /// 是否属于"老式编码"（非 ASCII 且非 UTF-8：GBK/Big5/Shift-JIS/未知）
    pub legacy: bool,
    /// 信号质量 0-100
    pub signal: u32,
    /// 加密方式（中文描述）
    pub auth: String,
    pub secured: bool,
    pub connected: bool,
    /// SSID 原始字节的十六进制表示（诊断报告用）
    pub hex: String,
}

pub enum ScanError {
    /// WLAN 自动配置服务未运行
    ServiceDown,
    /// 没有无线网卡
    NoAdapter,
    /// 其他 API 错误
    Api(u32, &'static str),
}

impl ScanError {
    pub fn message(&self) -> String {
        match self {
            ScanError::ServiceDown => "Windows 的「WLAN 自动配置」服务(WlanSvc)未运行".into(),
            ScanError::NoAdapter => "未检测到无线网卡".into(),
            ScanError::Api(code, func) => {
                format!("无线接口查询失败（{} 返回错误码 {}）", func, code)
            }
        }
    }
}

/// 扫描周围 WiFi。`trigger_scan` 为 true 时主动触发一次无线扫描并等待约 4 秒，
/// 保证拿到的是新鲜列表而不是缓存。
/// 成功返回 (网络列表, 无线网卡描述)。
pub fn scan(acp: u32, trigger_scan: bool) -> Result<(Vec<WifiNetwork>, String), ScanError> {
    let mut ver = 0u32;
    let mut handle = HANDLE::default();
    let r = unsafe { WlanOpenHandle(2, None, &mut ver, &mut handle) };
    if r == ERROR_SERVICE_NOT_ACTIVE {
        return Err(ScanError::ServiceDown);
    }
    if r != 0 {
        return Err(ScanError::Api(r, "WlanOpenHandle"));
    }
    let result = scan_inner(handle, acp, trigger_scan);
    let _ = unsafe { WlanCloseHandle(handle, None) };
    result
}

/// 打开句柄后的扫描主体。FFI 调用集中在显式 unsafe 块中（edition 2024 风格）。
fn scan_inner(
    handle: HANDLE,
    acp: u32,
    trigger_scan: bool,
) -> Result<(Vec<WifiNetwork>, String), ScanError> {
    let mut ifaces: Vec<(GUID, String)> = Vec::new();
    unsafe {
        let mut if_list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        let r = WlanEnumInterfaces(handle, None, &mut if_list);
        if r != 0 {
            return Err(ScanError::Api(r, "WlanEnumInterfaces"));
        }
        let list = &*if_list;
        let first = list.InterfaceInfo.as_ptr();
        for i in 0..list.dwNumberOfItems as usize {
            let info = &*first.add(i);
            ifaces.push((info.InterfaceGuid, utf16_to_string(&info.strInterfaceDescription)));
        }
        WlanFreeMemory(if_list as *const core::ffi::c_void);
    }

    if ifaces.is_empty() {
        return Err(ScanError::NoAdapter);
    }

    if trigger_scan {
        for (guid, _) in &ifaces {
            // 触发失败（如网卡刚被禁用）不致命，仍可读取上次的扫描结果
            let _ = unsafe { WlanScan(handle, guid, None, None, None) };
        }
        std::thread::sleep(std::time::Duration::from_millis(4200));
    }

    // 同一 SSID 可能出现多条（已连接项 + 扫描项），按原始字节去重合并
    let mut merged: HashMap<Vec<u8>, WifiNetwork> = HashMap::new();
    for (guid, _) in &ifaces {
        unsafe {
            let mut nl: *mut WLAN_AVAILABLE_NETWORK_LIST = std::ptr::null_mut();
            let r = WlanGetAvailableNetworkList(handle, guid, 0, None, &mut nl);
            if r != 0 {
                continue;
            }
            let list = &*nl;
            let first = list.Network.as_ptr();
            for i in 0..list.dwNumberOfItems as usize {
                let nw = &*first.add(i);
                let len = (nw.dot11Ssid.uSSIDLength as usize).min(32);
                let raw = nw.dot11Ssid.ucSSID[..len].to_vec();
                let connected = nw.dwFlags & 1 != 0; // WLAN_AVAILABLE_NETWORK_CONNECTED
                let entry = build_entry(
                    &raw,
                    acp,
                    nw.wlanSignalQuality,
                    nw.bSecurityEnabled.as_bool(),
                    nw.dot11DefaultAuthAlgorithm.0,
                    connected,
                );
                merged
                    .entry(raw)
                    .and_modify(|e| {
                        if entry.signal > e.signal {
                            e.signal = entry.signal;
                        }
                        e.connected = e.connected || entry.connected;
                    })
                    .or_insert(entry);
            }
            WlanFreeMemory(nl as *const core::ffi::c_void);
        }
    }

    let mut nets: Vec<WifiNetwork> = merged.into_values().collect();
    // 会乱码的排最前（用户最关心），其余按信号从强到弱
    nets.sort_by(|a, b| b.garbled.cmp(&a.garbled).then(b.signal.cmp(&a.signal)));
    let desc = ifaces
        .iter()
        .map(|(_, d)| d.clone())
        .collect::<Vec<_>>()
        .join("; ");
    Ok((nets, desc))
}

fn build_entry(
    raw: &[u8],
    acp: u32,
    signal: u32,
    secured: bool,
    auth_algo: i32,
    connected: bool,
) -> WifiNetwork {
    let (encoding, real_name, legacy) = classify_ssid(raw);
    let system_shows = simulate_system_display(raw, acp);
    let garbled = !raw.is_empty() && system_shows != real_name;
    WifiNetwork {
        real_name,
        system_shows,
        encoding,
        garbled,
        legacy,
        signal,
        auth: auth_name(auth_algo).to_string(),
        secured,
        connected,
        hex: to_hex(raw),
    }
}

fn utf16_to_string(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// 尝试用指定编码"干净地"解码：不允许解码错误、控制字符、私用区字符。
/// 私用区（U+E000..U+F8FF）通常意味着码表虽然没报错但结果不可信。
fn clean_decode(bytes: &[u8], enc: &'static encoding_rs::Encoding) -> Option<String> {
    let (cow, _, had_errors) = enc.decode(bytes);
    if had_errors {
        return None;
    }
    let s = cow.into_owned();
    for ch in s.chars() {
        let c = ch as u32;
        if ch.is_control() || (0xE000..=0xF8FF).contains(&c) {
            return None;
        }
    }
    Some(s)
}

/// 判定 SSID 原始字节的编码。
/// 返回 (编码中文描述, 真实名称, 是否老式编码)。
/// 判定顺序与 Windows 的实际解码优先级一致：ASCII → UTF-8 → GBK → Big5 → Shift-JIS。
pub fn classify_ssid(bytes: &[u8]) -> (String, String, bool) {
    if bytes.is_empty() {
        return ("隐藏网络".into(), "(隐藏网络，无名称)".into(), false);
    }
    if bytes.iter().all(|b| *b < 0x80) {
        return (
            "英文/数字".into(),
            String::from_utf8_lossy(bytes).into_owned(),
            false,
        );
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        if s.chars().all(|c| !c.is_control()) {
            return ("UTF-8（通用）".into(), s.to_string(), false);
        }
    }
    if let Some(s) = clean_decode(bytes, encoding_rs::GBK) {
        return ("GBK（老式国标）".into(), s, true);
    }
    if let Some(s) = clean_decode(bytes, encoding_rs::BIG5) {
        return ("Big5（繁体）".into(), s, true);
    }
    if let Some(s) = clean_decode(bytes, encoding_rs::SHIFT_JIS) {
        return ("Shift-JIS（日文）".into(), s, true);
    }
    (
        "无法识别".into(),
        format!("(无法解码，原始字节: {})", to_hex(bytes)),
        true,
    )
}

/// 模拟当前 Windows 在 WiFi 面板里的显示效果：
/// Windows 先按 UTF-8 严格解码，失败则回退到系统 ANSI 代码页（ACP）宽松解码。
pub fn simulate_system_display(bytes: &[u8], acp: u32) -> String {
    if bytes.is_empty() {
        return "(隐藏网络，无名称)".into();
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return sanitize(s);
    }
    let enc = match acp {
        936 => encoding_rs::GBK,
        950 => encoding_rs::BIG5,
        932 => encoding_rs::SHIFT_JIS,
        949 => encoding_rs::EUC_KR,
        65001 => encoding_rs::UTF_8,
        1251 => encoding_rs::WINDOWS_1251,
        _ => encoding_rs::WINDOWS_1252,
    };
    let (cow, _, _) = enc.decode(bytes);
    sanitize(&cow)
}

fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_control() { '?' } else { c }).collect()
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02X}", b)).collect()
}

fn auth_name(algo: i32) -> &'static str {
    match algo {
        1 => "开放",
        2 => "WEP",
        3 => "WPA-企业",
        4 => "WPA-个人",
        5 => "WPA-None",
        6 => "WPA2-企业",
        7 => "WPA2-个人",
        8 => "WPA3-企业192",
        9 => "WPA3-个人",
        10 => "开放(OWE)",
        11 => "WPA3-企业",
        _ => "未知",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gbk_bytes(s: &str) -> Vec<u8> {
        let (out, _, _) = encoding_rs::GBK.encode(s);
        out.into_owned()
    }

    #[test]
    fn ascii_ssid_never_garbles() {
        let (enc, name, legacy) = classify_ssid(b"HomeWifi-5G");
        assert_eq!(enc, "英文/数字");
        assert_eq!(name, "HomeWifi-5G");
        assert!(!legacy);
        assert_eq!(simulate_system_display(b"HomeWifi-5G", 65001), "HomeWifi-5G");
        assert_eq!(simulate_system_display(b"HomeWifi-5G", 1252), "HomeWifi-5G");
    }

    #[test]
    fn utf8_chinese_ssid_ok_on_any_acp() {
        let raw = "新小区宽带".as_bytes();
        let (enc, name, legacy) = classify_ssid(raw);
        assert_eq!(enc, "UTF-8（通用）");
        assert_eq!(name, "新小区宽带");
        assert!(!legacy);
        // Windows 对任何 ACP 都先按 UTF-8 解，UTF-8 SSID 永远显示正常
        assert_eq!(simulate_system_display(raw, 936), "新小区宽带");
        assert_eq!(simulate_system_display(raw, 65001), "新小区宽带");
        assert_eq!(simulate_system_display(raw, 1252), "新小区宽带");
    }

    #[test]
    fn gbk_ssid_garbles_exactly_when_acp_is_wrong() {
        let raw = gbk_bytes("办公室专用");
        let (enc, name, legacy) = classify_ssid(&raw);
        assert_eq!(enc, "GBK（老式国标）");
        assert_eq!(name, "办公室专用");
        assert!(legacy);
        // 正常中文系统（ACP=936）：显示正确
        assert_eq!(simulate_system_display(&raw, 936), "办公室专用");
        // 勾选 Beta UTF-8（ACP=65001）：乱码
        assert_ne!(simulate_system_display(&raw, 65001), "办公室专用");
        // 英文区域（ACP=1252）：乱码
        assert_ne!(simulate_system_display(&raw, 1252), "办公室专用");
    }

    #[test]
    fn hidden_ssid_is_not_a_problem() {
        let (enc, _, legacy) = classify_ssid(b"");
        assert_eq!(enc, "隐藏网络");
        assert!(!legacy);
    }

    #[test]
    fn big5_ssid_recognized() {
        let (out, _, _) = encoding_rs::BIG5.encode("繁體網路");
        let raw = out.into_owned();
        let (enc, name, legacy) = classify_ssid(&raw);
        // Big5 字节有可能恰好也是合法 GBK（两码表重叠），两种判定都可接受，
        // 关键是必须识别为"老式编码"且解出可读文本
        assert!(enc.starts_with("Big5") || enc.starts_with("GBK"), "enc={}", enc);
        assert!(legacy);
        assert!(!name.is_empty());
    }
}
