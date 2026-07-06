# WiFiNameFixer · Wi-Fi 名称显示修复助手

一个面向普通 Windows 用户的轻量级图形工具，用于诊断并修复「WiFi 列表中文名显示乱码」问题。

> 症状：电脑一切正常、其他软件中文显示正常，唯独系统 WiFi 面板里的中文 WiFi 名全是乱码，导致找不到要连接的网络。

## 问题原理

WiFi 名称（SSID）在无线电波中只是**一串字节**（最长 32 字节），标准不规定编码：

- 新式路由器用 **UTF-8** 编码广播中文名 → 任何 Windows 都能正常显示；
- 老式路由器用 **GBK** 编码广播 → Windows 先按 UTF-8 解码，失败后回退到「系统 ANSI 代码页（ACP）」解码。

因此，当电脑出现以下两种设置之一时，GBK 编码的中文 WiFi 名就会变成乱码：

| 设置状态 | ACP | 后果 |
|---|---|---|
| 勾选了「Beta 版: 使用 Unicode UTF-8 提供全球语言支持」 | 65001 | GBK 字节按 UTF-8 解码失败 → 乱码/� |
| 「非 Unicode 程序的语言」不是简体中文 | 1252 等 | GBK 字节按西欧码表解码 → ÂÃ 类乱码 |

这两个设置都不影响现代 Unicode 软件——所以"其他软件都正常、唯独 WiFi 乱码"正是它们的特征指纹。

## 功能

**检测（纯只读，不改动系统）**
- 通过 Native WLAN API（`WlanGetAvailableNetworkList`）获取周围 WiFi 的 **SSID 原始字节**，逐个判定编码（ASCII / UTF-8 / GBK / Big5 / Shift-JIS）；
- 读取系统 ACP、「Beta UTF-8」选项状态、系统区域、WLAN 服务状态、无线网卡与驱动信息；
- 模拟"系统面板会显示成什么样"，生成 **乱码 ↔ 真实中文名对照表** —— 不修复系统也能立刻认出目标 WiFi；
- 用大白话给出诊断结论和建议。

**修复（保守、可逆、全程确认）**
- 一键安全修复 = 重启 WLAN AutoConfig 服务（刷新列表缓存，不删除任何已保存的 WiFi/密码）；
- 系统编码问题（Beta UTF-8 / 区域设置）→ **强引导手动修复**：一键打开 `intl.cpl` + 分步说明，工具不自动改系统区域设置；
- 驱动问题 → 只提示更新方法，不自动安装；
- 诊断报告 / 修复日志保存到桌面（UTF-8 BOM + CRLF，记事本直接打开）。

**安全原则**
默认不修改系统；检测无需管理员；修复前弹窗确认；启动时不弹 UAC，需要时才提权；不后台常驻、不开机自启、不联网、不收集任何信息。

## 技术栈

- Rust（edition 2024）+ Tauri 2，采用官方推荐工程结构：`lib.rs` 承载全部逻辑（`run()` + `mobile_entry_point`），`main.rs` 仅为桌面薄壳
- 前端为纯 HTML/CSS/JS（无 Node/npm 依赖），`cargo build` 直接产出单文件 exe
- **所有耗时命令均为 `async fn` + `tauri::async_runtime::spawn_blocking`**：Tauri 2 的同步命令在主线程执行，任何阻塞工作都会冻结界面，这是官方明确的规则
- 官方插件：`tauri-plugin-dialog`（原生系统确认框）、`tauri-plugin-opener`（资源管理器定位文件）
- `windows` crate：WLAN API / 代码页 / 服务状态 / 提权（UAC）/ 已知文件夹
- `encoding_rs`：GBK / Big5 / Shift-JIS 解码
- 运行依赖：Microsoft Edge WebView2 运行时（Win11 与近年 Win10 自带；缺失时启动会给出下载提示）

## 构建

```bash
# 需要：Rust (stable-msvc) + VS Build Tools（MSVC 链接器）
cd src-tauri
cargo build --release
# 产物: src-tauri/target/release/wifi-name-fixer.exe（单文件，绿色免安装）
```

## 命令行模式

```bash
WiFiNameFixer.exe --selftest [输出文件]   # 无界面自检，诊断报告写入文件（默认 selftest_report.txt）
WiFiNameFixer.exe --elevated              # 内部参数：提权重启后的实例自动开始检测
```

## 项目结构

```
wifi-name-fixer/
├─ src/                    # 前端（纯静态，编译时嵌入 exe）
│  ├─ index.html
│  ├─ style.css
│  └─ main.js              # 关键确认走 dialog 插件原生弹窗，长文本走 HTML 弹窗
├─ src-tauri/
│  ├─ src/
│  │  ├─ main.rs           # 桌面端薄壳（官方结构：不放逻辑）
│  │  ├─ lib.rs            # 应用入口 run()、Tauri 命令（async + spawn_blocking）、插件注册
│  │  ├─ diagnose.rs       # 诊断决策矩阵 + 大白话结论生成
│  │  ├─ wifi.rs           # WLAN API 扫描、SSID 原始字节编码判定、乱码模拟（含单元测试）
│  │  ├─ system.rs         # ACP/区域/版本/服务状态、重启服务、打开 intl.cpl
│  │  ├─ privilege.rs      # 管理员检测、UAC 提权重启
│  │  ├─ repair.rs         # 修复编排（低风险）+ 修复日志
│  │  └─ report.rs         # 诊断报告生成/保存桌面
│  ├─ tauri.conf.json
│  ├─ capabilities/default.json   # core:default + dialog:default
│  └─ icons/icon.ico
└─ README.md
```

## 诊断决策矩阵

| 检测到 | 结论 | 主修复路径 |
|---|---|---|
| WLAN 服务未运行 | WiFi 整体不可用（比乱码优先） | 一键修复：启动服务（需管理员） |
| 无无线网卡 | 提示检查飞行模式/设备管理器 | 无（编码检测结果仍然给出） |
| ACP = 65001 | Beta UTF-8 已勾选 → 乱码主因 | 引导手动取消（打开 intl.cpl + 步骤） |
| ACP ≠ 936 | 系统区域不是简体中文 → 乱码主因 | 引导手动改为中文(简体，中国) |
| ACP = 936 但存在 Big5/日文/未知编码 SSID | 路由器编码问题，非本机故障 | 对照表认出真名直接连接；建议路由器改名 |
| 一切正常 | 系统面板应显示正常 | 仍提供：重启 WLAN 服务刷新缓存、更新驱动指引 |
