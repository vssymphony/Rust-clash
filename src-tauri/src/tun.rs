//! TUN（虚拟网卡）模式支撑。
//!
//! 职责划分：
//! * `subscription::build_config` 负责把 `AppSettings` 中的 TUN 选项渲染成 mihomo 的 `tun` 配置段；
//! * 本模块负责权限探测（Windows 管理员 / Linux root 或 CAP_NET_ADMIN）、
//!   Windows 侧 `wintun.dll` 依赖检查，以及把权限与内核状态合成前端可渲染的 `TunState`；
//! * `commands` 中的三个命令 `tun_status` / `tun_check_privilege` / `tun_toggle`
//!   是本模块对前端的唯一入口。
//!
//! 说明：TUN 的读写权限不在 Rust 侧"模拟提升"，而是如实探测并给出可操作指引，
//! 避免出现"开关打开了但内核起不来"的静默失败。

use tauri::{AppHandle, Manager};

use crate::clash_api;
use crate::models::{AppSettings, TunPrivilege, TunState};
use crate::state::AppState;

/// 平台默认 TUN 设备名。
pub fn default_device_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "Mihomo"
    } else {
        "mihomo"
    }
}

/// 实际生效的设备名（用户留空时回落到平台默认值）。
pub fn effective_device(settings: &AppSettings) -> String {
    settings
        .tun_device_name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
        .unwrap_or_else(|| default_device_name().to_string())
}

/// 探测当前进程是否具备 TUN 所需权限与依赖。
pub fn detect_privilege(app: &AppHandle) -> TunPrivilege {
    platform::detect(app)
}

/// 平台是否支持 TUN。
pub fn supported() -> bool {
    cfg!(any(target_os = "windows", target_os = "linux"))
}

/// Windows 下 TUN 依赖 `wintun.dll`；缺失时必须视为未就绪。
fn dependency_ready(privilege: &TunPrivilege) -> bool {
    if cfg!(target_os = "windows") {
        privilege.wintun_present != Some(false)
    } else {
        true
    }
}

/// 当前是否满足启用 TUN 的全部前置条件。
pub fn ready(privilege: &TunPrivilege) -> bool {
    privilege.supported && privilege.elevated && dependency_ready(privilege)
}

/// 是否满足启用条件；不满足时返回带指引的错误（供命令层直接抛给前端）。
pub fn ensure_ready(privilege: &TunPrivilege) -> Result<(), String> {
    if !privilege.supported {
        return Err(privilege
            .hint
            .clone()
            .unwrap_or_else(|| "当前平台暂不支持 TUN 模式".to_string()));
    }
    if !privilege.elevated {
        return Err(privilege
            .hint
            .clone()
            .unwrap_or_else(|| "TUN 模式需要更高权限，请以管理员 / root 身份运行".to_string()));
    }
    if !dependency_ready(privilege) {
        return Err(privilege.hint.clone().unwrap_or_else(|| {
            "缺少 wintun.dll，请从 https://www.wintun.net/ 下载对应架构的 wintun.dll 放到程序目录"
                .to_string()
        }));
    }
    Ok(())
}

/// 组装当前 TUN 状态（含内核实际生效情况）。
pub async fn current_state(app: &AppHandle) -> TunState {
    let (settings, running, client) = {
        let state = app.state::<AppState>();
        (
            state.settings_snapshot(),
            state.kernel_status().is_running(),
            clash_api::client_from_state(state.inner()).ok(),
        )
    };

    let privilege = detect_privilege(app);

    // 内核运行中时以 `/configs` 返回的实际值为准，避免界面状态与内核脱节。
    let kernel_enable = match (&client, running) {
        (Some(client), true) => client
            .get_configs()
            .await
            .ok()
            .and_then(|value| value.get("tun").cloned())
            .and_then(|tun| tun.get("enable").and_then(|item| item.as_bool())),
        _ => None,
    };

    let active =
        kernel_enable.unwrap_or_else(|| settings.tun_enabled && running && ready(&privilege));

    let message = build_message(&settings, &privilege, running, active);

    TunState {
        supported: privilege.supported,
        enabled: settings.tun_enabled,
        active,
        stack: settings.tun_stack,
        device_name: effective_device(&settings),
        privilege,
        message,
    }
}

fn build_message(
    settings: &AppSettings,
    privilege: &TunPrivilege,
    running: bool,
    active: bool,
) -> Option<String> {
    if !privilege.supported {
        return Some(
            privilege
                .hint
                .clone()
                .unwrap_or_else(|| "当前平台暂不支持 TUN 模式".to_string()),
        );
    }
    if !settings.tun_enabled {
        return None;
    }
    if !ready(privilege) {
        return Some(
            privilege
                .hint
                .clone()
                .unwrap_or_else(|| "TUN 前置条件不满足，本次已降级为系统代理模式".to_string()),
        );
    }
    if active {
        return Some(format!(
            "TUN 已生效：stack={}，设备={}",
            settings.tun_stack.as_str(),
            effective_device(settings)
        ));
    }
    if !running {
        return Some("TUN 已配置，等待内核启动后接管流量".to_string());
    }
    // 内核在运行、配置里也启用了 TUN，但 `/configs` 未回报 tun.enable=true，
    // 说明内核侧其实没接管流量（权限被回收 / 驱动加载失败 / 重载未生效）。
    // 这里不能静默返回 None，否则界面会显示"已启用"却完全没有说明。
    Some("TUN 已启用但内核未接管流量，请重新开启 TUN 或重启内核后查看日志".to_string())
}

/* ========================= 平台无关的解析工具 ========================= */

/// Linux `/proc/self/status` 的解析结果。
///
/// 抽成平台无关的纯函数，是为了让「权限解析」这条最容易出错的逻辑
/// 能在任意平台（含 Windows 开发机与 CI）上被单元测试覆盖 ——
/// `detect` 本身需要真的跑在 Linux 上才能验证。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProcIdentity {
    /// `Uid:` 行第 1 列（real uid）。
    pub uid: Option<u32>,
    /// `Uid:` 行第 2 列（effective uid）；内核在做权限判定时用的是它。
    /// 若该列缺失（非标准输出），回落到 real uid。
    pub euid: Option<u32>,
    /// `CapEff:` 的十六进制位掩码；行缺失或解析失败时为 `None`。
    pub cap_eff: Option<u64>,
}

impl ProcIdentity {
    /// 是否具备 CAP_NET_ADMIN（Linux capability 位 12）。
    pub fn has_cap_net_admin(&self, bit: u32) -> bool {
        match self.cap_eff {
            Some(mask) => mask & (1u64 << bit) != 0,
            None => false,
        }
    }

    /// 是否为 root（effective uid 为 0）。
    pub fn is_root(&self) -> bool {
        self.euid == Some(0)
    }
}

/// 解析 `/proc/self/status` 文本。
///
/// 只关心两行：
/// * `Uid:\t<real>\t<effective>\t<saved>\t<fs>`
/// * `CapEff:\t<16 进制位掩码>`
///
/// 容错策略：任何一行缺失或格式异常都只让对应字段变成 `None`，
/// 不 panic、不返回 Err —— 权限探测失败应当降级为"无权限"，
/// 而不是让整个状态查询命令失败。
pub fn parse_proc_status(content: &str) -> ProcIdentity {
    let mut identity = ProcIdentity::default();

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            let mut fields = rest.split_whitespace();
            let real = fields.next().and_then(|value| value.parse::<u32>().ok());
            let effective = fields.next().and_then(|value| value.parse::<u32>().ok());
            identity.uid = real;
            identity.euid = effective.or(real);
        } else if let Some(rest) = line.strip_prefix("CapEff:") {
            identity.cap_eff = u64::from_str_radix(rest.trim(), 16).ok();
        }
    }

    identity
}

/* ============================== Windows ============================== */

#[cfg(target_os = "windows")]
mod platform {
    use std::path::PathBuf;
    use std::process::Command;

    use tauri::{AppHandle, Manager};

    use crate::models::TunPrivilege;
    use crate::state::AppState;

    const HINT_NO_ADMIN: &str = "TUN 模式需要管理员权限：请右键「以管理员身份运行」本程序，\
或关闭 TUN 改用系统代理模式。以管理员身份运行时，Windows 会弹出 UAC 确认框。";

    const HINT_NO_WINTUN: &str = "未找到 wintun.dll。mihomo 在 Windows 上启用 TUN 依赖该驱动库：\
请到 https://www.wintun.net/ 下载与系统架构一致（通常为 amd64）的压缩包，\
解压后把 wintun.dll 放到本程序可执行文件（clash-gui.exe）同目录，或放到数据目录后再开启 TUN。";

    /// 通过 PowerShell 读取当前进程的令牌提升状态。
    ///
    /// 不使用 `IsUserAnAdmin`（需额外启用 windows-sys feature，且该 API 已被标记弃用），
    /// 这里只依赖系统自带的 PowerShell，避免引入新的编译期依赖。
    fn is_elevated() -> Option<bool> {
        let script = "([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)";
        let output = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).to_lowercase();
        if text.contains("true") {
            Some(true)
        } else if text.contains("false") {
            Some(false)
        } else {
            None
        }
    }

    /// 允许放置 `wintun.dll` 的目录：仅 `exe` 目录与应用数据目录两级。
    ///
    /// 上轮审计遗留项：旧实现额外搜索 `%APPDATA%\<identifier>` 配置目录根，
    /// 该目录对同机低权限进程可写，等于给 DLL 劫持开了一个口子。
    /// 现在统一交给 `crate::wintun::candidate_files`：它只接受这两级，
    /// 并剔除 `%APPDATA%` 根、临时目录、系统目录与当前工作目录。
    fn candidate_files(app: &AppHandle) -> Vec<PathBuf> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|parent| parent.to_path_buf()));
        let data_dir = app.state::<AppState>().paths.data_dir.clone();
        crate::wintun::candidate_files(exe_dir.as_deref(), &data_dir)
    }

    /// `wintun.dll` 探测结果。
    enum WintunProbe {
        /// 找到并通过大小 / PE 头 / 架构 / 版本校验
        Ready(PathBuf, crate::wintun::ImageInfo),
        /// 两级目录里都没有该文件
        Missing,
        /// 文件存在但校验失败（消息里已带路径与原因）
        Invalid(String),
    }

    /// 逐个候选路径校验，返回第一个通过校验的文件；全部失败时保留最后一条错误。
    fn probe_wintun(app: &AppHandle) -> WintunProbe {
        let mut last_error: Option<String> = None;
        for path in candidate_files(app) {
            if !path.is_file() {
                continue;
            }
            match crate::wintun::validate_file(&path) {
                Ok(info) => return WintunProbe::Ready(path, info),
                Err(error) => {
                    last_error = Some(format!("{}：{}", path.display(), error.message()));
                },
            }
        }
        match last_error {
            Some(message) => WintunProbe::Invalid(message),
            None => WintunProbe::Missing,
        }
    }

    pub fn detect(app: &AppHandle) -> TunPrivilege {
        let elevated = is_elevated();
        let probe = probe_wintun(app);
        let wintun_present = matches!(probe, WintunProbe::Ready(..));

        let hint = if elevated != Some(true) {
            Some(HINT_NO_ADMIN.to_string())
        } else {
            match &probe {
                WintunProbe::Ready(..) => None,
                WintunProbe::Missing => Some(format!(
                    "{HINT_NO_WINTUN}\n已检查的目录：{}",
                    crate::wintun::describe_candidates(&candidate_files(app))
                )),
                WintunProbe::Invalid(detail) => Some(format!(
                    "wintun.dll 未通过校验：{detail}\n\
                     请从 https://www.wintun.net/ 重新下载与本程序架构一致的 wintun.dll，\
                     覆盖到程序目录或数据目录后重试。"
                )),
            }
        };

        let detail = match (&probe, elevated) {
            (WintunProbe::Ready(path, info), Some(true)) => Some(format!(
                "wintun.dll 校验通过：{}（架构 {}，版本 {}）；已具备管理员权限",
                path.display(),
                info.machine_text(),
                info.version_text()
            )),
            (WintunProbe::Ready(path, info), _) => Some(format!(
                "wintun.dll 校验通过：{}（架构 {}，版本 {}）；权限探测未通过（需要管理员）",
                path.display(),
                info.machine_text(),
                info.version_text()
            )),
            (WintunProbe::Invalid(message), _) => Some(format!("wintun.dll 校验失败：{message}")),
            (WintunProbe::Missing, Some(_)) => Some(format!(
                "未在程序目录 / 数据目录找到 wintun.dll；已检查：{}",
                crate::wintun::describe_candidates(&candidate_files(app))
            )),
            (WintunProbe::Missing, None) => Some(format!(
                "未找到 wintun.dll；权限探测失败（PowerShell 不可用）。已检查：{}",
                crate::wintun::describe_candidates(&candidate_files(app))
            )),
        };

        TunPrivilege {
            supported: true,
            platform: "windows".to_string(),
            elevated: elevated.unwrap_or(false),
            mechanism: "administrator".to_string(),
            wintun_present: Some(wintun_present),
            detail,
            hint,
        }
    }
}

/* =============================== Linux =============================== */

#[cfg(target_os = "linux")]
mod platform {
    use tauri::AppHandle;

    use crate::models::TunPrivilege;

    /// CAP_NET_ADMIN 在 capability 位图中的位置。
    const CAP_NET_ADMIN_BIT: u32 = 12;

    const HINT_NO_PRIVILEGE: &str = "TUN 模式需要 root 权限或 CAP_NET_ADMIN 能力，二者任选其一：\
① 临时验证：sudo ./clash-gui；\
② 长期方案（推荐）：sudo setcap cap_net_admin,cap_net_raw+eip $(which clash-gui)，\
以普通用户启动即可创建虚拟网卡；\
③ systemd 方案：在 service 单元中设置 AmbientCapabilities=CAP_NET_ADMIN CAP_NET_RAW，\
CapabilityBoundingSet=CAP_NET_ADMIN CAP_NET_RAW。";

    /// 从 `/proc/self/status` 读取 euid 与有效 capability 位图（纯 std，不依赖 libc）。
    fn read_identity() -> (Option<u32>, u64) {
        let content = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        // 解析逻辑抽到 crate::tun::parse_proc_status（纯函数、可在任意平台单测），
        // 这里只负责把结果映射回旧签名，调用方无需感知改动。
        let identity = crate::tun::parse_proc_status(&content);
        (identity.euid, identity.cap_eff.unwrap_or(0))
    }

    pub fn detect(_app: &AppHandle) -> TunPrivilege {
        let (euid, cap_eff) = read_identity();

        let is_root = euid == Some(0);
        let has_cap_net_admin = cap_eff & (1u64 << CAP_NET_ADMIN_BIT) != 0;
        let elevated = is_root || has_cap_net_admin;

        let mechanism = if is_root {
            "root"
        } else if has_cap_net_admin {
            "cap_net_admin"
        } else {
            "unknown"
        };

        let detail = Some(format!(
            "euid={}, CapEff=0x{:016x}（CAP_NET_ADMIN={}）",
            euid.map(|value| value.to_string())
                .unwrap_or_else(|| "未知".to_string()),
            cap_eff,
            if has_cap_net_admin { "有" } else { "无" }
        ));

        TunPrivilege {
            supported: true,
            platform: "linux".to_string(),
            elevated,
            mechanism: mechanism.to_string(),
            wintun_present: None,
            detail,
            hint: if elevated {
                None
            } else {
                Some(HINT_NO_PRIVILEGE.to_string())
            },
        }
    }
}

/* ============================ 其他平台兜底 ============================ */

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod platform {
    use tauri::AppHandle;

    use crate::models::TunPrivilege;

    pub fn detect(_app: &AppHandle) -> TunPrivilege {
        TunPrivilege::unknown(std::env::consts::OS)
    }
}

/// 平台无关逻辑的单元测试。
///
/// 说明：`detect` 系列函数依赖 `AppHandle` 与真实 OS 环境，属于集成测试范畴，
/// 这里只覆盖可以在任意平台确定性验证的纯逻辑。
#[cfg(test)]
mod tests {
    use super::*;

    /// Linux CAP_NET_ADMIN 的 capability 位号（与 Linux platform 模块内常量一致）。
    const CAP_NET_ADMIN_BIT: u32 = 12;

    #[test]
    fn parses_typical_proc_status() {
        let text = "Name:\tclash-gui\nUid:\t1000\t1000\t1000\t1000\nCapEff:\t0000000000000000\n";
        let identity = parse_proc_status(text);
        assert_eq!(identity.uid, Some(1000));
        assert_eq!(identity.euid, Some(1000));
        assert_eq!(identity.cap_eff, Some(0));
        assert!(!identity.is_root());
        assert!(!identity.has_cap_net_admin(CAP_NET_ADMIN_BIT));
    }

    #[test]
    fn effective_uid_drives_root_detection() {
        // real=1000 / effective=0（sudo 场景）：应以 effective 为准
        let text = "Uid:\t1000\t0\t0\t0\nCapEff:\t0000000000001000\n";
        let identity = parse_proc_status(text);
        assert_eq!(identity.uid, Some(1000));
        assert_eq!(identity.euid, Some(0));
        assert!(identity.is_root());
        assert!(identity.has_cap_net_admin(CAP_NET_ADMIN_BIT));
        assert!(!identity.has_cap_net_admin(CAP_NET_ADMIN_BIT + 1));
    }

    #[test]
    fn falls_back_to_real_uid_when_effective_missing() {
        let identity = parse_proc_status("Uid:\t1001\n");
        assert_eq!(identity.uid, Some(1001));
        assert_eq!(identity.euid, Some(1001));
        assert_eq!(identity.cap_eff, None);
        assert!(!identity.has_cap_net_admin(CAP_NET_ADMIN_BIT));
    }

    #[test]
    fn tolerates_garbage_and_crlf() {
        let identity = parse_proc_status("Uid:\tabc\tdef\r\nCapEff:\tZZZ\r\n");
        assert_eq!(identity.uid, None);
        assert_eq!(identity.euid, None);
        assert_eq!(identity.cap_eff, None);

        let crlf = parse_proc_status("Uid:\t0\t0\t0\t0\r\nCapEff:\t7\r\n");
        assert_eq!(crlf.euid, Some(0));
        assert_eq!(crlf.cap_eff, Some(7));
    }

    #[test]
    fn empty_status_degrades_to_unknown() {
        let identity = parse_proc_status("");
        assert_eq!(identity, ProcIdentity::default());
        assert!(!identity.is_root());
    }

    #[test]
    fn device_name_falls_back_when_blank() {
        let blank = crate::models::AppSettings {
            tun_device_name: Some("   ".to_string()),
            ..crate::models::AppSettings::default()
        };
        assert_eq!(effective_device(&blank), default_device_name());

        let unset = crate::models::AppSettings {
            tun_device_name: None,
            ..crate::models::AppSettings::default()
        };
        assert_eq!(effective_device(&unset), default_device_name());
    }

    #[test]
    fn device_name_is_trimmed() {
        let custom = crate::models::AppSettings {
            tun_device_name: Some("  wg0  ".to_string()),
            ..crate::models::AppSettings::default()
        };
        assert_eq!(effective_device(&custom), "wg0");
    }
}
