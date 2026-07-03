//! 诊断引擎：汇总所有检测项，产出面向小白用户的结论与建议。

use crate::{privilege, system, wifi};
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct Diagnosis {
    pub win_version: String,
    pub acp: u32,
    pub acp_desc: String,
    pub oemcp: u32,
    pub utf8_beta: bool,
    pub locale: String,
    pub is_admin: bool,
    pub service_ok: bool,
    pub adapter: String,
    pub scan_error: String,
    pub networks: Vec<wifi::WifiNetwork>,
    pub garbled_count: usize,
    pub legacy_count: usize,
    /// utf8_beta | wrong_locale | service_down | no_adapter | router_encoding | looks_fine
    pub problem: String,
    /// bad | warn | ok —— 前端结论卡配色
    pub severity: String,
    pub conclusion: String,
    pub advice: String,
    /// 前端主修复按钮语义：open_region | restart_service | none
    pub main_action: String,
    pub driver_info: String,
}

pub fn run(trigger_scan: bool) -> Diagnosis {
    let acp = system::get_acp();
    let oemcp = system::get_oemcp();
    let utf8_beta = system::utf8_beta_enabled();
    let locale = system::system_locale_name();
    let win_version = system::windows_version();
    let is_admin = privilege::is_admin();
    let service_ok = matches!(system::wlan_service_state(), system::ServiceState::Running);

    let (networks, adapter, scan_error) = match wifi::scan(acp, trigger_scan) {
        Ok((nets, desc)) => (nets, desc, String::new()),
        Err(e) => (Vec::new(), String::new(), e.message()),
    };

    let garbled_count = networks.iter().filter(|n| n.garbled).count();
    let legacy_count = networks.iter().filter(|n| n.legacy).count();
    let gbk_count = networks
        .iter()
        .filter(|n| n.encoding.starts_with("GBK"))
        .count();
    let total = networks.len();

    // ---------- 诊断决策矩阵 ----------
    // 优先级：WLAN 服务/网卡问题（WiFi 整体不可用） > 系统代码页问题（乱码主因） > 路由器编码 > 正常
    let (problem, severity, conclusion, advice, main_action);

    if !scan_error.is_empty() && !service_ok {
        problem = "service_down";
        severity = "bad";
        conclusion = "检测到一个比乱码更优先的问题：Windows 的「WLAN 自动配置」服务（WlanSvc）没有运行。\n\
                      这个服务是 WiFi 功能的总开关，它不运行时整个 WiFi 都不可用，也无法扫描网络。"
            .to_string();
        advice = "请点击下方「一键安全修复」，工具会尝试启动该服务（需要管理员权限）。\n\
                  如果修复后很快又失效，多半是某些“系统优化/加速”软件把它禁用了——请在优化软件里恢复，\n\
                  或按 Win+R 输入 services.msc 回车，找到「WLAN AutoConfig」，右键属性，把启动类型改为「自动」。"
            .to_string();
        main_action = "restart_service";
    } else if !scan_error.is_empty() {
        problem = "no_adapter";
        severity = "warn";
        conclusion = format!(
            "没有扫描到无线网卡（{}）。\n\
             可能的情况：这是一台没装无线网卡的台式机；或无线网卡被禁用；或笔记本的物理 WiFi 开关/飞行模式把 WiFi 关掉了。",
            scan_error
        );
        advice = format!(
            "系统编码方面的检查结果依然有效：{}\n\
             若这台电脑本应有 WiFi：请检查飞行模式，或右键「此电脑」→「管理」→「设备管理器」→「网络适配器」，\n\
             看无线网卡是否被禁用（图标带向下箭头，右键选“启用设备”）。",
            if acp == 936 {
                "您的系统编码设置正常（简体中文 GBK），不是乱码的成因。".to_string()
            } else {
                format!(
                    "您的系统编码设置（{}）不是简体中文，这本身就会导致中文 WiFi 名乱码，建议一并按「查看手动修复步骤」处理。",
                    system::codepage_desc(acp)
                )
            }
        );
        main_action = if acp != 936 { "open_region" } else { "none" };
    } else if acp == 65001 {
        problem = "utf8_beta";
        severity = "bad";
        let mut c = String::from(
            "✅ 找到乱码的原因了！\n\n\
             您的电脑开启了一项试验性功能：「Beta 版: 使用 Unicode UTF-8 提供全球语言支持」。\n\
             开启它之后，Windows 无法正确读懂老式路由器广播的中文 WiFi 名（GBK 编码），WiFi 列表里就会出现乱码。\n\
             这项设置不影响正规软件的中文显示，所以其他地方一切正常——这正是您这台电脑症状的由来。",
        );
        if legacy_count > 0 {
            c.push_str(&format!(
                "\n\n本次扫描发现 {} 个受影响的 WiFi，它们的真实名字已解析出来，见下方对照表——现在就能认出您要连的那个。",
                legacy_count
            ));
        }
        conclusion = c;
        advice = manual_fix_steps(true, &locale);
        main_action = "open_region";
    } else if acp != 936 {
        problem = "wrong_locale";
        severity = "bad";
        let mut c = format!(
            "✅ 找到乱码的原因了！\n\n\
             您电脑的「非 Unicode 程序的语言」目前对应的是「{}」（区域：{}），不是「中文(简体，中国)」。\n\
             老式路由器广播的中文 WiFi 名需要按简体中文（GBK）解读，当前设置下 Windows 解读错误，于是显示乱码。\n\
             正规软件走 Unicode 机制、不受影响，所以只有 WiFi 列表出问题。",
            system::codepage_desc(acp),
            locale
        );
        if legacy_count > 0 {
            c.push_str(&format!(
                "\n\n本次扫描发现 {} 个受影响的 WiFi，真实名字见下方对照表。",
                legacy_count
            ));
        }
        conclusion = c;
        advice = manual_fix_steps(utf8_beta, &locale);
        main_action = "open_region";
    } else if networks.iter().any(|n| n.garbled) {
        problem = "router_encoding";
        severity = "warn";
        let names: Vec<String> = networks
            .iter()
            .filter(|n| n.garbled)
            .map(|n| format!("「{}」", n.real_name))
            .collect();
        conclusion = format!(
            "您电脑的系统编码设置是正常的（简体中文 GBK）。\n\
             但有 {} 个 WiFi 用了少见的编码（繁体 Big5 / 日文等）广播名字：{}。\n\
             这属于路由器那边的问题，Windows 本身无法正常显示它们，并非您电脑的故障。",
            garbled_count,
            names.join("、")
        );
        advice = "好消息：它们的真实名字已经解析出来（见下方对照表）。\n\
                  您可以对照「系统面板显示为」那一列，在系统 WiFi 列表里认出目标网络、正常输入密码连接——连接和使用完全不受乱码影响。\n\
                  长久之计：请路由器主人登录路由器管理页面，把 WiFi 名称重新保存一次（新固件一般会改用通用的 UTF-8 编码），或者改用英文名。"
            .to_string();
        main_action = "none";
    } else {
        problem = "looks_fine";
        severity = "ok";
        if gbk_count > 0 {
            conclusion = format!(
                "您电脑的系统编码设置完全正常（简体中文 GBK）。\n\
                 本次扫描到 {} 个 WiFi，其中 {} 个是老式 GBK 编码的中文名——按当前系统设置，它们在系统面板里都应当显示正常。",
                total, gbk_count
            );
            advice = "如果您在系统 WiFi 面板里看到的仍然是乱码，请依次尝试：\n\
                      1. 点击下方「一键安全修复」——重启 WLAN 服务、刷新网络列表缓存；\n\
                      2. 若无效，多半是无线网卡驱动过旧对中文处理有缺陷——请到电脑品牌官网下载最新无线网卡驱动（本工具遵循安全原则，不代替您安装驱动）；\n\
                      3. 点「保存诊断报告」，把桌面上生成的报告发给帮您处理问题的人。"
                .to_string();
        } else {
            conclusion = format!(
                "您电脑的系统编码设置完全正常，本次扫描到的 {} 个 WiFi 名称也都是通用编码（英文或标准 UTF-8），当前环境里不存在会乱码的 WiFi 名。",
                total
            );
            advice = "如果您要找的中文 WiFi 没出现在列表里，请靠近路由器一些，再点一次「开始检测」。\n\
                      如果在别的地点遇到乱码，把本工具带过去检测即可。"
                .to_string();
        }
        main_action = if total > 0 { "restart_service" } else { "none" };
    }

    Diagnosis {
        win_version,
        acp,
        acp_desc: system::codepage_desc(acp),
        oemcp,
        utf8_beta,
        locale,
        is_admin,
        service_ok,
        adapter,
        scan_error,
        networks,
        garbled_count,
        legacy_count,
        problem: problem.to_string(),
        severity: severity.to_string(),
        conclusion,
        advice,
        main_action: main_action.to_string(),
        driver_info: system::wifi_driver_info(),
    }
}

/// 手动修复步骤（系统区域设置）。本工具遵循安全原则，不自动改动这项系统设置。
fn manual_fix_steps(beta_checked: bool, locale: &str) -> String {
    let step4 = if beta_checked {
        "4. 把「Beta 版: 使用 Unicode UTF-8 提供全球语言支持」前面的勾【去掉】；"
    } else {
        "4. （若「Beta 版: 使用 Unicode UTF-8 提供全球语言支持」有勾，也一并去掉）；"
    };
    format!(
        "修复方法（动手约 1 分钟，改完需要重启电脑）：\n\
         1. 点击下方「打开系统区域设置」按钮（也可以：按 Win+R，输入 intl.cpl，回车）；\n\
         2. 在弹出的「区域」窗口顶部，切换到「管理」选项卡；\n\
         3. 点击「更改系统区域设置...」（需要管理员权限）；\n\
         {}\n\
         5. 把「当前系统区域设置」下拉框选为「中文(简体，中国)」（当前是：{}）；\n\
         6. 点「确定」，按提示重启电脑。重启后打开 WiFi 列表，乱码即恢复正常。\n\n\
         说明：本工具遵循安全原则，不会自动改动这项系统设置，请按上面步骤手动完成；\n\
         这是 Windows 的标准设置项，随时可以按同样路径改回去，不会损坏系统。\n\
         （等不及重启？先用下方对照表认出目标 WiFi，直接在系统面板连它，一样能上网。）",
        step4, locale
    )
}
