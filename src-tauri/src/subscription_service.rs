//! 订阅业务逻辑：导入、刷新、切换、删除、定时更新。
//!
//! 订阅原文统一落地到 `<config_dir>/profiles/<id>.yaml`，
//! 生成运行配置时只读本地副本，因此内核启动不依赖网络。

use std::time::Duration;

use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::clash_api;
use crate::error::{AppError, AppResult};
use crate::mihomo_guard;
use crate::models::{LogEntry, Subscription, SubscriptionInput};
use crate::state::AppState;
use crate::storage;
use crate::subscription as profile_parser;
use crate::time::now_millis;

const USER_AGENT: &str = "clash-gui/0.1.0 (mihomo; Clash.Meta)";

/// 下载远程订阅，返回（正文, subscription-userinfo 头）。
pub async fn fetch_remote(url: &str) -> AppResult<(String, Option<String>)> {
    // 协议白名单：只接受 http/https。
    // 订阅地址直接来自用户输入，若不加限制，file://、data: 等 scheme 会让下载器
    // 访问非预期的本地资源；空地址也会退化成对 base URL 的请求。
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(AppError::Subscription("订阅地址不能为空".to_string()));
    }
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(AppError::Subscription(
            "订阅地址必须以 http:// 或 https:// 开头".to_string(),
        ));
    }
    let url = trimmed;

    // 订阅下载必须直连（no_proxy）：
    // 用户开启系统代理或 TUN 后，若沿用系统代理配置，请求会绕回本机内核端口形成回环，
    // 表现为订阅刷新长时间挂起直至超时；而订阅本身是更新配置的唯一来源，不能依赖被更新对象。
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(8))
        .user_agent(USER_AGENT)
        .no_proxy()
        .build()
        .map_err(|error| AppError::Subscription(format!("初始化下载客户端失败：{error}")))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| AppError::Subscription(format!("订阅下载失败：{error}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError::Subscription(format!(
            "订阅下载失败：HTTP {}",
            status.as_u16()
        )));
    }

    let user_info = response
        .headers()
        .get("subscription-userinfo")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());

    let text = response
        .text()
        .await
        .map_err(|error| AppError::Subscription(format!("读取订阅内容失败：{error}")))?;

    if text.trim().is_empty() {
        return Err(AppError::Subscription("订阅返回内容为空".to_string()));
    }

    Ok((text, user_info))
}

async fn load_source(input: &SubscriptionInput) -> AppResult<(String, Option<String>)> {
    if let Some(url) = input
        .url
        .as_ref()
        .map(|url| url.trim())
        .filter(|url| !url.is_empty())
    {
        return fetch_remote(url).await;
    }

    if let Some(path) = input
        .file_path
        .as_ref()
        .map(|path| path.trim())
        .filter(|path| !path.is_empty())
    {
        let content = std::fs::read_to_string(path)
            .map_err(|error| AppError::Subscription(format!("读取本地订阅文件失败：{error}")))?;
        return Ok((content, None));
    }

    Err(AppError::Subscription(
        "请提供订阅链接或选择本地 YAML 文件".to_string(),
    ))
}

/// 导入订阅（不自动切换，仅当当前没有订阅时自动设为当前）。
pub async fn import(app: &AppHandle, input: SubscriptionInput) -> AppResult<Subscription> {
    let state = app.state::<AppState>();
    state.paths.ensure()?;

    let (content, user_info) = load_source(&input).await?;
    let parsed = profile_parser::parse_profile(&content, user_info.clone())?;

    let id = Uuid::new_v4().to_string();
    storage::write_atomic(&state.paths.profile_file(&id), &content)?;

    let name = {
        let trimmed = input.name.trim();
        if trimmed.is_empty() {
            match input.url.as_ref() {
                Some(url) => host_of(url).unwrap_or_else(|| "远程订阅".to_string()),
                None => "本地订阅".to_string(),
            }
        } else {
            trimmed.to_string()
        }
    };

    let now = now_millis();
    let item = Subscription {
        id: id.clone(),
        name,
        url: input.url.clone(),
        file_path: input.file_path.clone(),
        node_count: parsed.node_count(),
        created_at: now,
        updated_at: now,
        user_info,
    };

    let mut list = state.subscriptions();
    list.push(item.clone());
    state.set_subscriptions(list.clone());
    storage::save_subscriptions(&state.paths, &list)?;

    state.push_log(LogEntry::app(
        "info",
        format!("已导入订阅「{}」，节点数 {}", item.name, item.node_count),
    ));

    let mut settings = state.settings_snapshot();
    if settings.current_subscription_id.is_none() {
        settings.current_subscription_id = Some(id);
        state.update_settings(settings.clone());
        storage::save_settings(&state.paths, &settings)?;
        apply_current(app).await?;
    }

    Ok(item)
}

/// 刷新订阅内容并（若为当前订阅）热重载内核。
pub async fn refresh(app: &AppHandle, id: &str) -> AppResult<Subscription> {
    let state = app.state::<AppState>();
    let existing = state
        .subscriptions()
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::Subscription("订阅不存在".to_string()))?;

    let (content, user_info) = match existing.url.as_ref() {
        Some(url) => fetch_remote(url).await?,
        None => match existing.file_path.as_ref() {
            Some(path) => {
                let content = std::fs::read_to_string(path).map_err(|error| {
                    AppError::Subscription(format!("读取本地订阅文件失败：{error}"))
                })?;
                (content, existing.user_info.clone())
            },
            None => {
                return Err(AppError::Subscription(
                    "该订阅没有可用的更新来源".to_string(),
                ))
            },
        },
    };

    let parsed = profile_parser::parse_profile(&content, user_info.clone())?;
    storage::write_atomic(&state.paths.profile_file(id), &content)?;

    let updated = Subscription {
        node_count: parsed.node_count(),
        updated_at: now_millis(),
        user_info,
        ..existing
    };

    let mut list = state.subscriptions();
    if let Some(slot) = list.iter_mut().find(|item| item.id == id) {
        *slot = updated.clone();
    }
    state.set_subscriptions(list.clone());
    storage::save_subscriptions(&state.paths, &list)?;

    state.push_log(LogEntry::app(
        "info",
        format!(
            "订阅「{}」已更新，节点数 {}",
            updated.name, updated.node_count
        ),
    ));

    if state.settings_snapshot().current_subscription_id.as_deref() == Some(id) {
        apply_current(app).await?;
    }

    Ok(updated)
}

/// 切换当前订阅。
pub async fn switch(app: &AppHandle, id: &str) -> AppResult<()> {
    let state = app.state::<AppState>();
    let target = state
        .subscriptions()
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::Subscription("订阅不存在".to_string()))?;

    let mut settings = state.settings_snapshot();
    settings.current_subscription_id = Some(id.to_string());
    state.update_settings(settings.clone());
    storage::save_settings(&state.paths, &settings)?;

    let running = state.kernel_status().is_running();
    if running && settings.close_connections_on_switch {
        if let Ok(client) = clash_api::client_from_state(state.inner()) {
            let _ = client.close_all_connections().await;
        }
    }

    state.push_log(LogEntry::app(
        "info",
        format!("已切换到订阅「{}」", target.name),
    ));

    apply_current(app).await
}

/// 删除订阅（同时清理订阅原文副本）。
pub async fn remove(app: &AppHandle, id: &str) -> AppResult<()> {
    let state = app.state::<AppState>();
    let mut list = state.subscriptions();
    let before = list.len();
    list.retain(|item| item.id != id);
    if list.len() == before {
        return Err(AppError::Subscription("订阅不存在".to_string()));
    }

    state.set_subscriptions(list.clone());
    storage::save_subscriptions(&state.paths, &list)?;
    let _ = std::fs::remove_file(state.paths.profile_file(id));

    let mut settings = state.settings_snapshot();
    if settings.current_subscription_id.as_deref() == Some(id) {
        settings.current_subscription_id = list.first().map(|item| item.id.clone());
        state.update_settings(settings.clone());
        storage::save_settings(&state.paths, &settings)?;
        apply_current(app).await?;
    }

    Ok(())
}

/// 重新生成配置并让内核生效（未运行时仅写文件）。
pub async fn apply_current(app: &AppHandle) -> AppResult<()> {
    let path = mihomo_guard::write_config(app)?;
    let state = app.state::<AppState>();

    if !state.kernel_status().is_running() {
        return Ok(());
    }

    match clash_api::client_from_state(state.inner()) {
        Ok(client) => match client.reload_config(&path).await {
            Ok(()) => {
                state.push_log(LogEntry::app("info", "内核配置已热重载"));
            },
            Err(error) => {
                state.push_log(LogEntry::app(
                    "warning",
                    format!("热重载失败（{error}），改为重启内核"),
                ));
                mihomo_guard::restart_kernel(app).await?;
            },
        },
        Err(error) => {
            state.push_log(LogEntry::app(
                "warning",
                format!("构造 API 客户端失败（{error}），改为重启内核"),
            ));
            mihomo_guard::restart_kernel(app).await?;
        },
    }

    Ok(())
}

/// 后台定时更新订阅（每 60 秒检查一次到期项）。
pub fn spawn_auto_update(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;

            let (interval_minutes, pending) = {
                let state = app.state::<AppState>();
                let settings = state.settings_snapshot();
                if settings.sub_update_interval_minutes == 0 {
                    continue;
                }
                let interval_ms = settings.sub_update_interval_minutes as i64 * 60_000;
                let now = now_millis();
                let pending: Vec<String> = state
                    .subscriptions()
                    .into_iter()
                    .filter(|item| item.url.is_some())
                    .filter(|item| now.saturating_sub(item.updated_at) >= interval_ms)
                    .map(|item| item.id)
                    .collect();
                (settings.sub_update_interval_minutes, pending)
            };

            if pending.is_empty() {
                let _ = interval_minutes;
                continue;
            }

            for id in pending {
                if let Err(error) = refresh(&app, &id).await {
                    app.state::<AppState>().push_log(LogEntry::app(
                        "warning",
                        format!("订阅自动更新失败：{error}"),
                    ));
                }
            }
        }
    });
}

fn host_of(url: &str) -> Option<String> {
    let without_scheme = url.split("://").nth(1).unwrap_or(url);
    let host = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme);
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}
