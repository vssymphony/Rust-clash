//! 系统代理（全局代理）设置。
//!
//! * Windows：注册表 `Internet Settings` + `InternetSetOption` 刷新 WinINet 配置。
//! * Linux：优先 GNOME（`gsettings`），其次 KDE（`kioslaverc`），都不可用时返回
//!   `Unsupported` 并在 `message` 中给出手动配置提示（属于明确的降级提示，不抛错）。
//!
//! 所有平台在开启前都会备份原值，正常退出 / 应用重启时恢复，避免"退出后仍然全局代理"。

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager};

#[allow(unused_imports)]
use crate::error::{AppError, AppResult};
use crate::models::{SystemProxyBackend, SystemProxyState};
use crate::state::AppState;

/// 本次运行是否由本应用写入过系统代理（用于退出时精确清理，避免误关用户原有代理）。
static MANAGED_BY_APP: AtomicBool = AtomicBool::new(false);

/// 开启系统代理，指向本地混合端口。
pub fn enable(app: &AppHandle, port: u16) -> AppResult<SystemProxyState> {
    let state = platform::enable(app, port)?;
    if state.enabled {
        MANAGED_BY_APP.store(true, Ordering::SeqCst);
    }
    Ok(state)
}

/// 关闭系统代理并尽可能恢复原始设置。
pub fn disable(app: &AppHandle) -> AppResult<SystemProxyState> {
    let state = platform::disable(app)?;
    MANAGED_BY_APP.store(false, Ordering::SeqCst);
    Ok(state)
}

/// 关闭系统代理，失败不向上抛（用于退出流程）。
pub fn disable_silently(app: &AppHandle) {
    // 没有改动过系统代理时直接跳过，避免把用户自己的代理设置关掉
    if !MANAGED_BY_APP.load(Ordering::SeqCst) && !platform::has_backup(app) {
        return;
    }
    if let Err(error) = disable(app) {
        app.state::<AppState>()
            .push_log(crate::models::LogEntry::app(
                "warning",
                format!("清理系统代理失败：{error}"),
            ));
    }
}

/// 启动时自愈：上次异常退出可能残留系统代理，这里按用户期望状态恢复。
pub fn recover_on_startup(app: &AppHandle) {
    if !platform::has_backup(app) {
        return;
    }
    let settings = app.state::<AppState>().settings_snapshot();
    if settings.system_proxy {
        let _ = platform::enable(app, settings.mixed_port);
    } else if let Err(error) = platform::disable(app) {
        app.state::<AppState>()
            .push_log(crate::models::LogEntry::app(
                "warning",
                format!("恢复系统代理原设置失败：{error}"),
            ));
    }
}

#[allow(dead_code)]
fn unsupported_state(port: u16) -> SystemProxyState {
    SystemProxyState {
        enabled: false,
        backend: SystemProxyBackend::Unsupported,
        message: Some(format!(
            "当前桌面环境不支持自动设置系统代理，请手动把 HTTP/SOCKS 代理指向 127.0.0.1:{port}"
        )),
    }
}

/* ============================== Windows ============================== */

#[cfg(target_os = "windows")]
mod platform {
    use std::path::PathBuf;

    use serde::{Deserialize, Serialize};
    use windows_sys::Win32::Networking::WinInet::{
        InternetSetOptionW, INTERNET_OPTION_REFRESH, INTERNET_OPTION_SETTINGS_CHANGED,
    };
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    use super::*;

    const INTERNET_SETTINGS: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    const BYPASS: &str =
        "localhost;127.*;10.*;172.16.*;172.17.*;172.18.*;172.19.*;172.20.*;172.21.*;172.22.*;172.23.*;172.24.*;172.25.*;172.26.*;172.27.*;172.28.*;172.29.*;172.30.*;172.31.*;192.168.*;<local>";

    #[derive(Debug, Clone, Serialize, Deserialize, Default)]
    #[serde(rename_all = "camelCase")]
    struct ProxyBackup {
        proxy_enable: u32,
        proxy_server: Option<String>,
        proxy_override: Option<String>,
    }

    fn backup_file(app: &AppHandle) -> PathBuf {
        app.state::<AppState>()
            .paths
            .config_dir
            .join("system_proxy_backup.json")
    }

    fn read_backup(app: &AppHandle) -> Option<ProxyBackup> {
        let content = std::fs::read_to_string(backup_file(app)).ok()?;
        serde_json::from_str(&content).ok()
    }

    fn write_backup(app: &AppHandle, backup: &ProxyBackup) -> AppResult<()> {
        let content = serde_json::to_string_pretty(backup)?;
        crate::storage::write_atomic(&backup_file(app), &content)
    }

    fn remove_backup(app: &AppHandle) {
        let _ = std::fs::remove_file(backup_file(app));
    }

    fn open_key() -> AppResult<RegKey> {
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(INTERNET_SETTINGS, KEY_READ | KEY_WRITE)
            .map_err(|error| {
                AppError::SysProxy(format!("打开注册表 Internet Settings 失败：{error}"))
            })
    }

    /// 通知 WinINet 立即重新读取注册表，否则部分程序要等注销才生效。
    ///
    /// 这是全项目唯一的 `unsafe` 调用点（项目级 lint 把它设为 warn，此处显式豁免）：
    /// 两次 `InternetSetOptionW` 都只传空指针与整型常量，不涉及任何由本程序持有的
    /// 指针、缓冲区或生命周期，因此不存在未定义行为的前提条件。
    #[allow(unsafe_code)]
    fn refresh_internet_options() {
        // SAFETY: 见上，参数全为 null 指针 + 固定 option 常量，WinINet 不写回任何内存。
        unsafe {
            InternetSetOptionW(
                std::ptr::null_mut(),
                INTERNET_OPTION_SETTINGS_CHANGED,
                std::ptr::null(),
                0,
            );
            InternetSetOptionW(
                std::ptr::null_mut(),
                INTERNET_OPTION_REFRESH,
                std::ptr::null(),
                0,
            );
        }
    }

    pub fn has_backup(app: &AppHandle) -> bool {
        backup_file(app).exists()
    }

    pub fn enable(app: &AppHandle, port: u16) -> AppResult<SystemProxyState> {
        let key = open_key()?;

        if read_backup(app).is_none() {
            let backup = ProxyBackup {
                proxy_enable: key.get_value("ProxyEnable").unwrap_or(0u32),
                proxy_server: key.get_value("ProxyServer").ok(),
                proxy_override: key.get_value("ProxyOverride").ok(),
            };
            write_backup(app, &backup)?;
        }

        let server = format!("127.0.0.1:{port}");
        key.set_value("ProxyServer", &server)
            .map_err(|error| AppError::SysProxy(format!("写入 ProxyServer 失败：{error}")))?;
        key.set_value("ProxyOverride", &BYPASS.to_string())
            .map_err(|error| AppError::SysProxy(format!("写入 ProxyOverride 失败：{error}")))?;
        key.set_value("ProxyEnable", &1u32)
            .map_err(|error| AppError::SysProxy(format!("开启 ProxyEnable 失败：{error}")))?;

        refresh_internet_options();

        Ok(SystemProxyState {
            enabled: true,
            backend: SystemProxyBackend::Windows,
            message: Some(format!("系统代理已指向 {server}")),
        })
    }

    pub fn disable(app: &AppHandle) -> AppResult<SystemProxyState> {
        let key = open_key()?;

        match read_backup(app) {
            Some(backup) => {
                match backup.proxy_server {
                    Some(server) => {
                        let _ = key.set_value("ProxyServer", &server);
                        let _ = key.set_value(
                            "ProxyOverride",
                            &backup
                                .proxy_override
                                .unwrap_or_else(|| "<local>".to_string()),
                        );
                    },
                    None => {
                        let _ = key.delete_value("ProxyServer");
                        let _ = key.delete_value("ProxyOverride");
                    },
                }
                let _ = key.set_value("ProxyEnable", &backup.proxy_enable);
                remove_backup(app);
            },
            None => {
                let _ = key.set_value("ProxyEnable", &0u32);
            },
        }

        refresh_internet_options();

        Ok(SystemProxyState {
            enabled: false,
            backend: SystemProxyBackend::Windows,
            message: None,
        })
    }
}

/* =============================== Linux =============================== */

#[cfg(target_os = "linux")]
mod platform {
    use std::path::PathBuf;
    use std::process::Command;

    use super::*;

    fn home_dir() -> PathBuf {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/root"))
    }

    fn kioslaverc_path() -> PathBuf {
        home_dir().join(".config").join("kioslaverc")
    }

    fn gsettings_available() -> bool {
        Command::new("gsettings")
            .args(["get", "org.gnome.system.proxy", "mode"])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn desktop_is_kde() -> bool {
        std::env::var("XDG_CURRENT_DESKTOP")
            .map(|value| value.to_uppercase().contains("KDE"))
            .unwrap_or(false)
            || std::env::var("KDE_FULL_SESSION").is_ok()
    }

    fn detect_backend() -> SystemProxyBackend {
        if desktop_is_kde() && kioslaverc_path().exists() {
            return SystemProxyBackend::Kde;
        }
        if gsettings_available() {
            return SystemProxyBackend::Gnome;
        }
        if kioslaverc_path().exists() {
            return SystemProxyBackend::Kde;
        }
        SystemProxyBackend::Unsupported
    }

    fn gset(schema_key: &str, value: &str) -> Result<(), String> {
        let mut parts = schema_key.splitn(2, ' ');
        let schema = parts.next().unwrap_or_default();
        let key = parts.next().unwrap_or_default();
        let output = Command::new("gsettings")
            .args(["set", schema, key, value])
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }

    fn apply_gnome(port: u16) -> AppResult<SystemProxyState> {
        let port_text = port.to_string();
        let mut errors: Vec<String> = Vec::new();

        for scheme in ["http", "https"] {
            let host_key = format!("org.gnome.system.proxy.{scheme} host");
            let port_key = format!("org.gnome.system.proxy.{scheme} port");
            if let Err(error) = gset(&host_key, "127.0.0.1") {
                errors.push(format!("{host_key}: {error}"));
            }
            if let Err(error) = gset(&port_key, &port_text) {
                errors.push(format!("{port_key}: {error}"));
            }
        }

        if let Err(error) = gset("org.gnome.system.proxy.socks host", "127.0.0.1") {
            errors.push(format!("socks host: {error}"));
        }
        if let Err(error) = gset("org.gnome.system.proxy.socks port", &port_text) {
            errors.push(format!("socks port: {error}"));
        }
        if let Err(error) = gset(
            "org.gnome.system.proxy ignore-hosts",
            "['localhost', '127.0.0.0/8', '::1']",
        ) {
            errors.push(format!("ignore-hosts: {error}"));
        }
        if let Err(error) = gset("org.gnome.system.proxy mode", "manual") {
            errors.push(format!("mode: {error}"));
        }

        if !errors.is_empty() {
            return Err(AppError::SysProxy(format!(
                "gsettings 设置部分失败：{}",
                errors.join("; ")
            )));
        }

        Ok(SystemProxyState {
            enabled: true,
            backend: SystemProxyBackend::Gnome,
            message: Some(format!("GNOME 系统代理已指向 127.0.0.1:{port}")),
        })
    }

    fn disable_gnome() -> AppResult<SystemProxyState> {
        gset("org.gnome.system.proxy mode", "none")
            .map_err(|error| AppError::SysProxy(format!("关闭 GNOME 系统代理失败：{error}")))?;
        Ok(SystemProxyState {
            enabled: false,
            backend: SystemProxyBackend::Gnome,
            message: None,
        })
    }

    /// 在 ini 文件中更新/追加指定 section 的键值，保留其他内容。
    fn upsert_ini(path: &PathBuf, section: &str, entries: &[(&str, String)]) -> AppResult<()> {
        let existing = std::fs::read_to_string(path).unwrap_or_default();
        let header = format!("[{section}]");

        let mut lines: Vec<String> = existing.lines().map(|line| line.to_string()).collect();
        let mut section_start = None;
        let mut section_end = lines.len();

        for (index, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed == header {
                section_start = Some(index);
                continue;
            }
            if section_start.is_some() && trimmed.starts_with('[') {
                section_end = index;
                break;
            }
        }

        match section_start {
            Some(start) => {
                for (key, value) in entries {
                    let mut found = false;
                    // 注意：strip_prefix / starts_with 的实参是泛型 Pattern，
                    // 直接传 &String 不会被自动解引用为 &str，必须显式 as_str()。
                    let prefix = format!("{key}=");
                    for line in lines.iter_mut().take(section_end).skip(start + 1) {
                        if line.trim_start().starts_with(prefix.as_str()) {
                            *line = format!("{key}={value}");
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        lines.insert(section_end, format!("{key}={value}"));
                        section_end += 1;
                    }
                }
            },
            None => {
                lines.push(header);
                for (key, value) in entries {
                    lines.push(format!("{key}={value}"));
                }
            },
        }

        let mut content = lines.join("\n");
        content.push('\n');
        crate::storage::write_atomic(path, &content)
    }

    fn apply_kde(port: u16) -> AppResult<SystemProxyState> {
        let path = kioslaverc_path();
        let proxy = format!("http://127.0.0.1:{port}");
        let socks = format!("socks://127.0.0.1:{port}");
        let entries = vec![
            ("ProxyType", "1".to_string()),
            ("httpProxy", proxy.clone()),
            ("httpsProxy", proxy),
            ("socksProxy", socks),
            ("ftpProxy", String::new()),
            ("NoProxyFor", "localhost,127.0.0.1,::1".to_string()),
        ];
        upsert_ini(&path, "Proxy Settings", &entries)?;

        Ok(SystemProxyState {
            enabled: true,
            backend: SystemProxyBackend::Kde,
            message: Some(format!(
                "KDE 代理配置已写入 {}，部分应用需重新登录后生效",
                path.display()
            )),
        })
    }

    fn disable_kde() -> AppResult<SystemProxyState> {
        let path = kioslaverc_path();
        if !path.exists() {
            return Ok(SystemProxyState {
                enabled: false,
                backend: SystemProxyBackend::Kde,
                message: None,
            });
        }
        let entries = vec![("ProxyType", "0".to_string())];
        upsert_ini(&path, "Proxy Settings", &entries)?;
        Ok(SystemProxyState {
            enabled: false,
            backend: SystemProxyBackend::Kde,
            message: None,
        })
    }

    pub fn has_backup(_app: &AppHandle) -> bool {
        // Linux 侧不落备份文件：关闭时统一把模式设为 none / ProxyType=0。
        false
    }

    pub fn enable(app: &AppHandle, port: u16) -> AppResult<SystemProxyState> {
        let _ = app;
        match detect_backend() {
            SystemProxyBackend::Gnome => apply_gnome(port),
            SystemProxyBackend::Kde => apply_kde(port),
            _ => Ok(unsupported_state(port)),
        }
    }

    pub fn disable(app: &AppHandle) -> AppResult<SystemProxyState> {
        let _ = app;
        match detect_backend() {
            SystemProxyBackend::Gnome => disable_gnome(),
            SystemProxyBackend::Kde => disable_kde(),
            _ => Ok(SystemProxyState {
                enabled: false,
                backend: SystemProxyBackend::Unsupported,
                message: Some("当前桌面环境未检测到可用的系统代理接口".to_string()),
            }),
        }
    }
}

/* ============================ 其他平台 ============================ */

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod platform {
    use super::*;

    pub fn has_backup(_app: &AppHandle) -> bool {
        false
    }

    pub fn enable(_app: &AppHandle, port: u16) -> AppResult<SystemProxyState> {
        Ok(unsupported_state(port))
    }

    pub fn disable(_app: &AppHandle) -> AppResult<SystemProxyState> {
        Ok(SystemProxyState {
            enabled: false,
            backend: SystemProxyBackend::Unsupported,
            message: Some("当前系统不支持自动设置系统代理".to_string()),
        })
    }
}
