//! 修复流程编排。
//!
//! 安全原则：
//! - 第一版只做低风险修复：重启 WLAN 自动配置服务（刷新网络列表缓存）；
//! - 不删除任何已保存的 WiFi 配置（那与乱码无关，且会丢失用户密码）；
//! - 不自动修改系统区域设置 / 注册表 / 驱动——这些只给出手动指引；
//! - 每次修复都在桌面留下日志。

use crate::{privilege, report, system};

/// 前端识别这个特殊错误串后，引导用户走提权重启流程
pub const NEED_ADMIN: &str = "NEED_ADMIN";

pub fn one_click_fix() -> Result<String, String> {
    if !privilege::is_admin() {
        return Err(NEED_ADMIN.into());
    }
    let result = system::restart_wlan_service();

    let log_body = match &result {
        Ok(out) => format!(
            "修复动作：重启 WLAN 自动配置服务（net stop/start wlansvc）\n结果：成功\n命令输出：\n{}",
            out
        ),
        Err(out) => format!(
            "修复动作：重启 WLAN 自动配置服务（net stop/start wlansvc）\n结果：失败\n命令输出：\n{}",
            out
        ),
    };
    let _ = report::save_to_desktop("WiFiNameFixer_修复日志.txt", &log_body);

    match result {
        Ok(_) => Ok(
            "已重新启动「WLAN 自动配置」服务，网络列表缓存已刷新。\n\n\
             请点击「开始检测」重新扫描一次，然后到系统右下角的 WiFi 面板看看效果。\n\
             （修复日志已保存到桌面「WiFiNameFixer_修复日志.txt」）"
                .into(),
        ),
        Err(out) => Err(format!(
            "服务重启没有成功。命令输出：\n{}\n\n\
             建议：按 Win+R 输入 services.msc 回车，找到「WLAN AutoConfig」，\n\
             右键「属性」，把启动类型设为「自动」，再点「启动」。",
            out
        )),
    }
}
