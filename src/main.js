// Wi-Fi 名称显示修复助手 —— 前端逻辑
// 所有数据一律用 textContent 渲染（SSID 是外部输入，防注入）

const invoke = window.__TAURI__.core.invoke;
const $ = (id) => document.getElementById(id);

let lastDiag = null;

// ---------- 通用弹窗（替代被 WebView 禁用的 alert/confirm） ----------
function showModalBase(title, text, withCancel) {
  return new Promise((resolve) => {
    $("modalTitle").textContent = title;
    $("modalText").textContent = text;
    $("modalCancel").classList.toggle("hidden", !withCancel);
    $("modalOverlay").classList.remove("hidden");
    const done = (val) => {
      $("modalOverlay").classList.add("hidden");
      $("modalOk").onclick = null;
      $("modalCancel").onclick = null;
      resolve(val);
    };
    $("modalOk").onclick = () => done(true);
    $("modalCancel").onclick = () => done(false);
  });
}
const showModal = (title, text) => showModalBase(title, text, false);
const confirmModal = (title, text) => showModalBase(title, text, true);

// ---------- 原生系统对话框（官方 dialog 插件） ----------
// 关键确认用系统原生弹窗（小白更信任）；插件不可用时退回 HTML 弹窗。
// 长文本说明仍用 HTML 弹窗（可滚动、排版好）。
function nativeAsk(title, text, kind) {
  const dlg = window.__TAURI__ && window.__TAURI__.dialog;
  if (dlg && typeof dlg.ask === "function") {
    return dlg.ask(text, { title, kind: kind || "info" });
  }
  return confirmModal(title, text);
}
function nativeMessage(title, text, kind) {
  const dlg = window.__TAURI__ && window.__TAURI__.dialog;
  if (dlg && typeof dlg.message === "function") {
    return dlg.message(text, { title, kind: kind || "info" });
  }
  return showModal(title, text);
}

// ---------- 启动 ----------
window.addEventListener("DOMContentLoaded", async () => {
  $("btnScan").onclick = startScan;
  $("btnFix").onclick = oneClickFix;
  $("btnOpenRegion").onclick = openRegion;
  $("btnSteps").onclick = showSteps;
  $("btnReport").onclick = saveReport;

  try {
    const info = await invoke("get_startup_info");
    $("verText").textContent = "Wi-Fi 名称显示修复助手 v" + info.version;
    if (info.isAdmin) {
      const b = $("adminBadge");
      b.textContent = "已获得管理员权限";
      b.classList.add("on");
    }
    if (info.autoScan) startScan(); // 提权重启后的实例自动开始检测
  } catch (e) {
    nativeMessage("提示", "初始化失败：" + e, "error");
  }
});

// ---------- 检测 ----------
function setBusy(busy) {
  $("btnScan").disabled = busy;
  $("busyBox").classList.toggle("hidden", !busy);
  $("firstHint").classList.toggle("hidden", busy || lastDiag !== null);
}

async function startScan() {
  setBusy(true);
  try {
    lastDiag = await invoke("run_diagnosis", { triggerScan: true });
    render(lastDiag);
  } catch (e) {
    nativeMessage("检测失败", String(e), "error");
  }
  setBusy(false);
  $("btnScan").textContent = "🔍 重新检测";
}

function render(d) {
  // 结论卡
  const card = $("resultCard");
  card.classList.remove("hidden", "bad", "warn", "ok");
  card.classList.add(d.severity);
  $("resultTitle").textContent =
    d.severity === "bad" ? "❗ 检测结果：发现问题" :
    d.severity === "warn" ? "⚠️ 检测结果：请注意" :
    "✅ 检测结果：一切正常";
  $("conclusion").textContent = d.conclusion;
  $("advice").textContent = d.advice;

  // 主修复按钮编排
  $("btnOpenRegion").classList.toggle("hidden", d.main_action !== "open_region");
  const fixBtn = $("btnFix");
  fixBtn.classList.remove("hidden");
  if (d.problem === "service_down") {
    fixBtn.textContent = "🛠️ 一键安全修复（启动WLAN服务）";
    fixBtn.classList.add("primary");
  } else if (d.main_action === "restart_service") {
    fixBtn.textContent = "🛠️ 一键安全修复（重启WLAN服务·刷新列表）";
    fixBtn.classList.toggle("primary", d.problem === "looks_fine");
  } else if (d.main_action === "open_region") {
    // 代码页问题：主路径是手动改区域设置，服务重启仅作辅助
    fixBtn.textContent = "🛠️ 重启WLAN服务（辅助手段）";
    fixBtn.classList.remove("primary");
  } else {
    fixBtn.classList.add("hidden");
  }

  // WiFi 对照表
  const listCard = $("listCard");
  const tb = $("wifiBody");
  tb.innerHTML = "";
  if (d.networks.length > 0) {
    listCard.classList.remove("hidden");
    for (const n of d.networks) {
      const tr = document.createElement("tr");
      if (n.garbled) tr.className = "garbled";

      const tdName = document.createElement("td");
      tdName.className = "name";
      tdName.textContent = n.real_name;
      tdName.title = "原始字节: " + n.hex;

      const tdShows = document.createElement("td");
      tdShows.className = "shows";
      tdShows.textContent = n.system_shows;

      const tdSig = document.createElement("td");
      const bar = document.createElement("span");
      bar.className = "signal-bar";
      const fill = document.createElement("i");
      fill.style.width = n.signal + "%";
      bar.appendChild(fill);
      tdSig.appendChild(bar);
      tdSig.appendChild(document.createTextNode(n.signal + "%"));

      const tdAuth = document.createElement("td");
      tdAuth.textContent = n.secured ? n.auth : "无密码";

      const tdEnc = document.createElement("td");
      const encTag = document.createElement("span");
      encTag.className = "tag" + (n.legacy ? " warn" : "");
      encTag.textContent = n.encoding;
      tdEnc.appendChild(encTag);

      const tdState = document.createElement("td");
      const stTag = document.createElement("span");
      if (n.connected) {
        stTag.className = "tag ok";
        stTag.textContent = "当前已连接";
      } else if (n.garbled) {
        stTag.className = "tag warn";
        stTag.textContent = "会显示乱码";
      } else {
        stTag.className = "tag";
        stTag.textContent = "显示正常";
      }
      tdState.appendChild(stTag);

      tr.append(tdName, tdShows, tdSig, tdAuth, tdEnc, tdState);
      tb.appendChild(tr);
    }
  } else {
    listCard.classList.add("hidden");
  }

  // 技术详情
  $("techCard").classList.remove("hidden");
  $("techDetails").textContent = buildTechText(d);
}

function buildTechText(d) {
  const lines = [
    "Windows 版本 : " + d.win_version,
    "ANSI 代码页(ACP) : " + d.acp_desc,
    "OEM 代码页 : " + d.oemcp,
    "Beta UTF-8 选项 : " + (d.utf8_beta ? "已勾选" : "未勾选"),
    "系统区域(非Unicode程序语言) : " + d.locale,
    "WLAN 服务 : " + (d.service_ok ? "运行中" : "未运行"),
    "无线网卡 : " + (d.adapter || "（无）"),
    d.scan_error ? "扫描错误 : " + d.scan_error : "",
    "",
  ];
  for (const n of d.networks) {
    lines.push(
      "SSID HEX=" + n.hex + "  编码=" + n.encoding +
      "  真名=" + n.real_name + "  系统显示=" + n.system_shows
    );
  }
  if (d.driver_info) {
    lines.push("", "—— netsh wlan show drivers ——", d.driver_info);
  }
  return lines.filter((l) => l !== null).join("\n");
}

// ---------- 修复动作 ----------
async function oneClickFix() {
  const ok = await nativeAsk(
    "一键安全修复",
    "将重新启动 Windows 的「WLAN 自动配置」服务，用来刷新 WiFi 列表缓存。\n\n" +
    "· 执行期间 WiFi 会断开几秒钟，之后自动恢复\n" +
    "· 不会删除、不会改动你保存过的任何 WiFi 和密码\n" +
    "· 需要管理员权限\n\n确定继续吗？",
    "warning"
  );
  if (!ok) return;
  try {
    const msg = await invoke("one_click_fix");
    await showModal("修复完成", msg);
  } catch (e) {
    const err = String(e);
    if (err.includes("NEED_ADMIN")) {
      const go = await nativeAsk(
        "需要管理员权限",
        "重启系统服务需要管理员权限。\n\n点「是」后本工具会重新打开一次，" +
        "并弹出 Windows 蓝色的授权确认框（UAC），请在那里选择「是」。",
        "warning"
      );
      if (go) {
        try { await invoke("relaunch_admin"); }
        catch (e2) { nativeMessage("提示", String(e2), "error"); }
      }
    } else {
      showModal("修复未成功", err);
    }
  }
}

async function openRegion() {
  try {
    await invoke("open_region_settings");
    showModal(
      "系统的「区域」窗口已打开",
      "接下来在那个窗口里：\n\n" +
      "1. 切到顶部的「管理」选项卡\n" +
      "2. 点「更改系统区域设置...」\n" +
      "3. 若「Beta 版: 使用 Unicode UTF-8 提供全球语言支持」有勾 → 把勾去掉\n" +
      "4. 「当前系统区域设置」选为「中文(简体，中国)」\n" +
      "5. 点确定，按提示重启电脑\n\n" +
      "重启后再打开 WiFi 列表看看，乱码就应该消失了。"
    );
  } catch (e) {
    nativeMessage("提示", String(e), "error");
  }
}

function showSteps() {
  const beta = lastDiag ? lastDiag.utf8_beta : false;
  showModal(
    "手动修复步骤（约 1 分钟）",
    "◆ 修复系统编码设置（针对中文 WiFi 名乱码的根本原因）：\n" +
    "1. 按 Win+R，输入 intl.cpl，回车（或点工具里的「打开系统区域设置」）\n" +
    "2. 切到「管理」选项卡 → 点「更改系统区域设置...」\n" +
    (beta
      ? "3. 把「Beta 版: 使用 Unicode UTF-8 提供全球语言支持」的勾去掉 ←你的电脑当前勾着它\n"
      : "3. 若「Beta 版: 使用 Unicode UTF-8 提供全球语言支持」有勾，去掉\n") +
    "4. 「当前系统区域设置」选「中文(简体，中国)」\n" +
    "5. 确定 → 重启电脑\n\n" +
    "◆ 如果上面的设置本来就是对的，WiFi 还是乱码：\n" +
    "1. 点「一键安全修复」重启 WLAN 服务刷新缓存\n" +
    "2. 更新无线网卡驱动（设备管理器 → 网络适配器 → 右键无线网卡 → 更新驱动程序，" +
    "或到电脑品牌官网下载）\n" +
    "3. 若只有个别 WiFi 乱码而其他中文 WiFi 正常：是那台路由器用了少见编码，" +
    "让路由器主人重新保存一次 WiFi 名称即可\n\n" +
    "以上操作都是 Windows 的常规设置，随时可以原路改回，不会损坏系统。"
  );
}

async function saveReport() {
  try {
    const path = await invoke("save_report");
    const open = await nativeAsk(
      "报告已保存",
      "诊断报告已保存到：\n" + path + "\n\n要打开它所在的文件夹吗？",
      "info"
    );
    if (open) await invoke("reveal_report", { path });
  } catch (e) {
    nativeMessage("提示", String(e), "error");
  }
}
