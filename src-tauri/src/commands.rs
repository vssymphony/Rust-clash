//! 暴露给前端的 Tauri 命令层。
//!
//! 命名与 `src/lib/api.ts` 中的 `invoke` 调用一一对应，错误统一为
//! `AppError`（序列化为字符串），前端可直接 `toErrorMessage` 展示。

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

use crate::clash_api;
use crate::error::{AppError, AppResult};
use crate::mihomo_guard;
use crate::models::{
    AppSettings, AppStateSnapshot, KernelStatus, LogEntry, ProxyDelayResult, Subscription,
    SubscriptionInput, SystemProxyState, TunPrivilege, TunState,
};
use crate::state::AppState;
use crate::storage;
use crate::subscription_service;
use crate::system_proxy;
use crate::tun;

/* ------------------------------ 应用状态 ------------------------------ */

#[tauri::command]
pub fn get_app_state(app: AppHandle) -> AppResult<AppStateSnapshot> {
    let state = app.state::<AppState>();
    Ok(AppStateSnapshot {
        kernel: state.kernel_status(),
        settings: state.settings_snapshot(),
        system_proxy: state.system_proxy(),
        subscriptions: state.subscriptions(),
        app_version: app.package_info().version.to_string(),
        config_path: state.paths.config_display(),
        data_dir: state.paths.data_display(),
        platform: std::env::consts::OS.to_string(),
    })
}

#[tauri::command]
pub async fn update_settings(app: AppHandle, settings: AppSettings) -> AppResult<AppSettings> {
    if settings.mixed_port == 0 || settings.controller_port == 0 {
        return Err(AppError::config("端口不能为 0"));
    }
    if settings.mixed_port == settings.controller_port {
        return Err(AppError::config("混合端口与控制器端口不能相同"));
    }

    let (previous, paths) = {
        let state = app.state::<AppState>();
        (state.settings_snapshot(), state.paths.clone())
    };

    {
        let state = app.state::<AppState>();
        state.update_settings(settings.clone());
    }
    storage::save_settings(&paths, &settings)?;

    let restart_needed = previous.mixed_port != settings.mixed_port
        || previous.controller_port != settings.controller_port
        || previous.controller_secret != settings.controller_secret
        || previous.allow_lan != settings.allow_lan;

    let running = app.state::<AppState>().kernel_status().is_running();

    if restart_needed && running {
        mihomo_guard::restart_kernel(&app).await?;
    } else {
        mihomo_guard::write_config(&app)?;
        if running {
            if let Ok(client) = clash_api::client_from_state(app.state::<AppState>().inner()) {
                let patch = json!({
                    "mode": settings.mode.as_str(),
                    "log-level": settings.log_level.as_str(),
                });
                let _ = client.patch_configs(patch).await;
            }
        }
    }

    // 端口变化而系统代理处于开启状态时，需要把系统代理重新指向新端口。
    if settings.system_proxy && previous.mixed_port != settings.mixed_port {
        let _ = set_system_proxy(app.clone(), true).await;
    }

    Ok(settings)
}

/* ------------------------------ 内核控制 ------------------------------ */

#[tauri::command]
pub async fn kernel_start(app: AppHandle, kernel_path: Option<String>) -> AppResult<KernelStatus> {
    if let Some(path) = kernel_path.filter(|path| !path.trim().is_empty()) {
        let (settings, paths) = {
            let state = app.state::<AppState>();
            let mut settings = state.settings_snapshot();
            settings.kernel_path = Some(path);
            let paths = state.paths.clone();
            state.update_settings(settings.clone());
            (settings, paths)
        };
        storage::save_settings(&paths, &settings)?;
    }

    mihomo_guard::start_kernel(&app).await?;
    Ok(app.state::<AppState>().kernel_status())
}

#[tauri::command]
pub async fn kernel_stop(app: AppHandle) -> AppResult<KernelStatus> {
    mihomo_guard::stop_kernel(&app).await?;
    Ok(app.state::<AppState>().kernel_status())
}

#[tauri::command]
pub async fn kernel_restart(app: AppHandle) -> AppResult<KernelStatus> {
    mihomo_guard::restart_kernel(&app).await?;
    Ok(app.state::<AppState>().kernel_status())
}

#[tauri::command]
pub fn kernel_status(app: AppHandle) -> AppResult<KernelStatus> {
    Ok(app.state::<AppState>().kernel_status())
}

#[tauri::command]
pub fn kernel_logs(app: AppHandle, limit: Option<usize>) -> AppResult<Vec<LogEntry>> {
    Ok(app.state::<AppState>().recent_logs(limit.unwrap_or(300)))
}

#[tauri::command]
pub fn open_config_dir(app: AppHandle) -> AppResult<()> {
    let dir = {
        let state = app.state::<AppState>();
        state.paths.config_dir.clone()
    };
    std::fs::create_dir_all(&dir)?;

    let program = if cfg!(target_os = "windows") {
        "explorer"
    } else {
        "xdg-open"
    };

    std::process::Command::new(program)
        .arg(&dir)
        .spawn()
        .map_err(|error| AppError::config(format!("打开配置目录失败：{error}")))?;

    Ok(())
}

#[tauri::command]
pub async fn reload_config(app: AppHandle) -> AppResult<()> {
    subscription_service::apply_current(&app).await
}

/* ------------------------------ Clash REST ------------------------------ */

#[tauri::command]
pub async fn get_proxies(app: AppHandle) -> AppResult<Value> {
    let state = app.state::<AppState>();
    clash_api::client_from_state(state.inner())?.proxies().await
}

#[tauri::command]
pub async fn select_proxy(app: AppHandle, group: String, name: String) -> AppResult<()> {
    let state = app.state::<AppState>();
    clash_api::client_from_state(state.inner())?
        .select_proxy(&group, &name)
        .await
}

#[tauri::command]
pub async fn proxy_delay(
    app: AppHandle,
    name: String,
    url: Option<String>,
    timeout: Option<u64>,
) -> AppResult<ProxyDelayResult> {
    let (client, settings) = {
        let state = app.state::<AppState>();
        (
            clash_api::client_from_state(state.inner())?,
            state.settings_snapshot(),
        )
    };

    let test_url = url
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(settings.latency_test_url);
    let timeout_ms = timeout.unwrap_or(5000).clamp(1000, 30_000);

    match client.proxy_delay(&name, &test_url, timeout_ms).await {
        Ok(delay) => Ok(ProxyDelayResult {
            name,
            delay,
            error: None,
        }),
        Err(error) => Ok(ProxyDelayResult {
            name,
            delay: 0,
            error: Some(error.to_string()),
        }),
    }
}

#[tauri::command]
pub async fn proxy_group_delay(
    app: AppHandle,
    group: String,
    url: Option<String>,
    timeout: Option<u64>,
) -> AppResult<Vec<ProxyDelayResult>> {
    let (client, settings) = {
        let state = app.state::<AppState>();
        (
            clash_api::client_from_state(state.inner())?,
            state.settings_snapshot(),
        )
    };

    let test_url = url
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(settings.latency_test_url);
    let timeout_ms = timeout.unwrap_or(5000).clamp(1000, 30_000);

    let proxies = client.proxies().await?;
    let members: Vec<String> = proxies
        .get("proxies")
        .and_then(|value| value.get(&group))
        .and_then(|value| value.get("all"))
        .and_then(|value| value.as_array())
        .map(|array| {
            array
                .iter()
                .filter_map(|item| item.as_str().map(|item| item.to_string()))
                .collect()
        })
        .ok_or_else(|| AppError::api(format!("策略组「{group}」不存在或不支持测速")))?;

    // 并发上限 6，避免瞬间打出上百个探测请求。
    let semaphore = Arc::new(tokio::sync::Semaphore::new(6));
    let mut handles = Vec::with_capacity(members.len());

    for name in members {
        let client = client.clone();
        let semaphore = semaphore.clone();
        let test_url = test_url.clone();
        handles.push(tauri::async_runtime::spawn(async move {
            let _permit = semaphore.acquire().await;
            match client.proxy_delay(&name, &test_url, timeout_ms).await {
                Ok(delay) => ProxyDelayResult {
                    name,
                    delay,
                    error: None,
                },
                Err(error) => ProxyDelayResult {
                    name,
                    delay: 0,
                    error: Some(error.to_string()),
                },
            }
        }));
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        if let Ok(item) = handle.await {
            results.push(item);
        }
    }

    Ok(results)
}

#[tauri::command]
pub async fn get_connections(app: AppHandle) -> AppResult<Value> {
    let state = app.state::<AppState>();
    clash_api::client_from_state(state.inner())?
        .connections()
        .await
}

#[tauri::command]
pub async fn close_connection(app: AppHandle, id: String) -> AppResult<()> {
    let state = app.state::<AppState>();
    clash_api::client_from_state(state.inner())?
        .close_connection(&id)
        .await
}

#[tauri::command]
pub async fn close_all_connections(app: AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    clash_api::client_from_state(state.inner())?
        .close_all_connections()
        .await
}

#[tauri::command]
pub async fn get_rules(app: AppHandle) -> AppResult<Value> {
    let state = app.state::<AppState>();
    clash_api::client_from_state(state.inner())?.rules().await
}

#[tauri::command]
pub async fn get_clash_config(app: AppHandle) -> AppResult<Value> {
    let state = app.state::<AppState>();
    clash_api::client_from_state(state.inner())?
        .get_configs()
        .await
}

#[tauri::command]
pub async fn patch_clash_config(app: AppHandle, patch: Value) -> AppResult<()> {
    let state = app.state::<AppState>();
    clash_api::client_from_state(state.inner())?
        .patch_configs(patch)
        .await
}

/* -------------------------------- 订阅 -------------------------------- */

#[tauri::command]
pub fn list_subscriptions(app: AppHandle) -> AppResult<Vec<Subscription>> {
    Ok(app.state::<AppState>().subscriptions())
}

#[tauri::command]
pub async fn add_subscription(app: AppHandle, input: SubscriptionInput) -> AppResult<Subscription> {
    subscription_service::import(&app, input).await
}

#[tauri::command]
pub async fn remove_subscription(app: AppHandle, id: String) -> AppResult<()> {
    subscription_service::remove(&app, &id).await
}

#[tauri::command]
pub async fn update_subscription(app: AppHandle, id: String) -> AppResult<Subscription> {
    subscription_service::refresh(&app, &id).await
}

#[tauri::command]
pub async fn update_all_subscriptions(app: AppHandle) -> AppResult<Vec<Subscription>> {
    let remote_ids: Vec<String> = app
        .state::<AppState>()
        .subscriptions()
        .into_iter()
        .filter(|item| item.url.is_some())
        .map(|item| item.id)
        .collect();

    for id in remote_ids {
        if let Err(error) = subscription_service::refresh(&app, &id).await {
            app.state::<AppState>()
                .push_log(LogEntry::app("warning", format!("订阅更新失败：{error}")));
        }
    }

    Ok(app.state::<AppState>().subscriptions())
}

#[tauri::command]
pub async fn set_active_subscription(app: AppHandle, id: String) -> AppResult<Subscription> {
    subscription_service::switch(&app, &id).await?;
    app.state::<AppState>()
        .subscriptions()
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::Subscription("订阅不存在".to_string()))
}

/* ------------------------------ 系统代理 ------------------------------ */

#[tauri::command]
pub fn get_system_proxy(app: AppHandle) -> AppResult<SystemProxyState> {
    Ok(app.state::<AppState>().system_proxy())
}

#[tauri::command]
pub async fn set_system_proxy(app: AppHandle, enabled: bool) -> AppResult<SystemProxyState> {
    let (paths, port) = {
        let state = app.state::<AppState>();
        (state.paths.clone(), state.settings_snapshot().mixed_port)
    };

    let result = if enabled {
        system_proxy::enable(&app, port)?
    } else {
        system_proxy::disable(&app)?
    };

    {
        let state = app.state::<AppState>();
        let mut settings = state.settings_snapshot();
        settings.system_proxy = result.enabled;
        state.update_settings(settings.clone());
        state.set_system_proxy(result.clone());
        storage::save_settings(&paths, &settings)?;
    }

    Ok(result)
}

/* ------------------------------ 开机自启 ------------------------------ */

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> AppResult<bool> {
    Ok(app.autolaunch().is_enabled().unwrap_or(false))
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> AppResult<bool> {
    let manager = app.autolaunch();
    let outcome = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    outcome.map_err(|error| AppError::config(format!("设置开机自启失败：{error}")))?;

    {
        let state = app.state::<AppState>();
        let mut settings = state.settings_snapshot();
        settings.auto_start = enabled;
        let paths = state.paths.clone();
        state.update_settings(settings.clone());
        storage::save_settings(&paths, &settings)?;
    }

    Ok(manager.is_enabled().unwrap_or(enabled))
}

/* ------------------------------ TUN 模式 ------------------------------ */

/// 查询权限状态与 TUN 运行状态（设置页进入/刷新时调用）。
#[tauri::command]
pub async fn tun_status(app: AppHandle) -> AppResult<TunState> {
    Ok(tun::current_state(&app).await)
}

/// 只做权限与驱动检测，不改配置也不动内核（设置页"重新检测"按钮）。
#[tauri::command]
pub fn tun_check_privilege(app: AppHandle) -> AppResult<TunPrivilege> {
    Ok(tun::detect_privilege(&app))
}

/// 开关 TUN：持久化设置 → 重写配置 → 运行中的内核重载配置 → 返回最新状态。
#[tauri::command]
pub async fn tun_toggle(app: AppHandle, enabled: bool) -> AppResult<TunState> {
    if enabled {
        // 权限不足直接拒绝，避免落盘一份永远起不来的配置
        tun::ensure_ready(&tun::detect_privilege(&app))?;
    }

    let (settings, paths, running) = {
        let state = app.state::<AppState>();
        let mut settings = state.settings_snapshot();
        settings.tun_enabled = enabled;
        let paths = state.paths.clone();
        let running = state.kernel_status().is_running();
        state.update_settings(settings.clone());
        (settings, paths, running)
    };
    storage::save_settings(&paths, &settings)?;

    if running {
        // tun 段的增删必须整份配置重载，force 让内核立即创建/销毁虚拟网卡
        let config_path = mihomo_guard::write_config(&app)?;
        if let Ok(client) = clash_api::client_from_state(app.state::<AppState>().inner()) {
            if let Err(error) = client.reload_config(&config_path).await {
                // 重载失败必须回滚已落盘的 tun_enabled：
                // 否则前端提示失败、磁盘配置却已开启（或关闭）TUN，
                // 下次启动的实际行为与界面显示不一致。
                let state = app.state::<AppState>();
                let mut rollback = state.settings_snapshot();
                rollback.tun_enabled = !enabled;
                state.update_settings(rollback.clone());
                if let Err(save_error) = storage::save_settings(&paths, &rollback) {
                    state.push_log(LogEntry::app(
                        "error",
                        format!("回滚 TUN 设置失败（{save_error}），请手动重启内核"),
                    ));
                }
                if let Err(write_error) = mihomo_guard::write_config(&app) {
                    state.push_log(LogEntry::app(
                        "error",
                        format!("回滚配置失败（{write_error}），请手动重启内核"),
                    ));
                }
                return Err(error);
            }
            // 关闭 TUN 必须清空 fake-ip 缓存：
            // 关闭前解析出的 198.18.0.0/16 假地址会残留，后续请求仍发往已销毁的虚拟网卡，
            // 表现为"关了 TUN 之后断网几分钟"。清理失败只记日志，不阻断关闭流程。
            if !enabled {
                if let Err(error) = client.flush_fakeip().await {
                    let entry = LogEntry::app(
                        "warning",
                        format!("清理 fake-ip 缓存失败（{error}），若出现短暂断网请重连网络"),
                    );
                    app.state::<AppState>().push_log(entry);
                }
            }
        }
    }

    Ok(tun::current_state(&app).await)
}

/* ---------------------------- 辅助：等待端口 ---------------------------- */

/// 供托盘"重启内核"等场景复用的短等待，避免端口抢占。
pub async fn settle() {
    tokio::time::sleep(Duration::from_millis(120)).await;
}
