//! mihomo 内核进程守护。
//!
//! 职责：
//! * 生成运行配置并启动 sidecar 内核；
//! * 通过 `/version` 做启动健康检查；
//! * 收集 stdout/stderr 转发为界面日志；
//! * 进程意外退出时按退避策略自动重启（最多 5 次）；
//! * 主动停止时设置停止标志，避免"停止即重启"的竞态。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

use crate::clash_api::ClashClient;
use crate::error::{AppError, AppResult};
use crate::models::{KernelStatus, KernelStatusKind, LogEntry};
use crate::state::AppState;
use crate::storage;
use crate::subscription::{self, ProfileContent};
use crate::time::now_millis;

pub const EVENT_KERNEL_STATUS: &str = "clash://kernel-status";
const MAX_AUTO_RESTARTS: u32 = 5;
const HEALTH_CHECK_ATTEMPTS: u32 = 24;

/// 内核进程句柄与停止标志。
pub struct KernelSupervisor {
    child: Mutex<Option<CommandChild>>,
    stopping: AtomicBool,
}

impl KernelSupervisor {
    pub fn new() -> Self {
        Self {
            child: Mutex::new(None),
            stopping: AtomicBool::new(false),
        }
    }

    fn lock_child(&self) -> std::sync::MutexGuard<'_, Option<CommandChild>> {
        self.child.lock().unwrap_or_else(|error| error.into_inner())
    }

    pub fn set_child(&self, child: CommandChild) {
        *self.lock_child() = Some(child);
    }

    pub fn take_child(&self) -> Option<CommandChild> {
        self.lock_child().take()
    }

    pub fn has_child(&self) -> bool {
        self.lock_child().is_some()
    }

    pub fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::SeqCst)
    }

    pub fn set_stopping(&self, value: bool) {
        self.stopping.store(value, Ordering::SeqCst);
    }
}

impl Default for KernelSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

/* ------------------------------ 配置生成 ------------------------------ */

/// 读取当前订阅并生成 mihomo 配置写入磁盘。
pub fn write_config(app: &AppHandle) -> AppResult<String> {
    let state = app.state::<AppState>();
    let mut settings = state.settings_snapshot();
    state.paths.ensure()?;

    // TUN 需要管理员 / root 或 CAP_NET_ADMIN，Windows 还依赖 wintun.dll。
    // 条件不满足时把「本次运行」降级为不含 tun 段的配置（不改用户设置），
    // 避免内核带着 tun 段反复启动失败，同时把原因写进日志供设置页展示。
    if settings.tun_enabled {
        let privilege = crate::tun::detect_privilege(app);
        if let Err(reason) = crate::tun::ensure_ready(&privilege) {
            settings.tun_enabled = false;
            state.push_log(LogEntry::app(
                "warning",
                format!("TUN 前置条件不满足，本次以内核非 TUN 模式启动：{reason}"),
            ));
        }
    }

    let profile = load_current_profile(state.inner());
    let text = subscription::build_config(&settings, profile.as_ref())?;
    storage::write_atomic(&state.paths.config_file, &text)?;

    Ok(state.paths.config_file.to_string_lossy().to_string())
}

/// 读取当前订阅的本地副本（导入/更新时已落盘）。
fn load_current_profile(state: &AppState) -> Option<ProfileContent> {
    let subscription = state.current_subscription()?;
    let path = state.paths.profile_file(&subscription.id);
    let content = storage::read_to_string(&path)?;
    subscription::parse_profile(&content, subscription.user_info.clone()).ok()
}

/* ------------------------------ 启停控制 ------------------------------ */

/// 启动内核（已在运行则直接返回）。
pub async fn start_kernel(app: &AppHandle) -> AppResult<()> {
    {
        let supervisor = app.state::<KernelSupervisor>();
        if supervisor.has_child() {
            return Ok(());
        }
        supervisor.set_stopping(false);
    }

    let state = app.state::<AppState>();
    let settings = state.settings_snapshot();
    let paths = state.paths.clone();

    // 先写配置，失败直接把错误抛给前端，不留下半启动状态。
    if let Err(error) = write_config(app) {
        let status = KernelStatus::failed(format!("生成配置失败：{error}"));
        state.set_kernel_status(status);
        emit_status(app);
        return Err(error);
    }

    state.set_kernel_status(KernelStatus::starting());
    emit_status(app);

    let program = settings
        .kernel_path
        .as_ref()
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty());

    let command = match program {
        Some(path) => {
            if !std::path::Path::new(&path).exists() {
                let message = format!("未找到指定的内核文件：{path}");
                state.set_kernel_status(KernelStatus::failed(message.clone()));
                emit_status(app);
                return Err(AppError::KernelMissing(path));
            }
            app.shell().command(path)
        }
        None => app.shell().sidecar("mihomo").map_err(|error| {
            let message = format!(
                "随包内核不可用（请把 mihomo 二进制放到 src-tauri/binaries 并带目标三元组后缀）：{error}"
            );
            state.set_kernel_status(KernelStatus::failed(message.clone()));
            emit_status(app);
            AppError::KernelMissing(message)
        })?,
    };

    let command = command.args(vec![
        "-d".to_string(),
        paths.data_dir.to_string_lossy().to_string(),
        "-f".to_string(),
        paths.config_file.to_string_lossy().to_string(),
    ]);

    let (receiver, child) = match command.spawn() {
        Ok(pair) => pair,
        Err(error) => {
            let message = format!("启动内核进程失败：{error}");
            state.set_kernel_status(KernelStatus::failed(message.clone()));
            emit_status(app);
            return Err(AppError::KernelStart(message));
        }
    };

    let pid = child.pid();
    app.state::<KernelSupervisor>().set_child(child);
    state.push_log(LogEntry::app(
        "info",
        format!("内核进程已拉起，PID {pid}，工作目录 {}", paths.data_display()),
    ));

    // 健康检查：轮询 /version 直到内核可用或进程先退出。
    let client = ClashClient::new(settings.controller_port, &settings.controller_secret)?;
    let mut version: Option<String> = None;

    for _ in 0..HEALTH_CHECK_ATTEMPTS {
        if !app.state::<KernelSupervisor>().has_child() {
            break;
        }
        if let Ok(value) = client.version().await {
            version = value
                .get("version")
                .and_then(|item| item.as_str())
                .map(|item| item.to_string());
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    if version.is_none() {
        if let Some(child) = app.state::<KernelSupervisor>().take_child() {
            let _ = child.kill();
        }
        let message = "健康检查未通过：内核未能提供 API 服务，请查看内核日志与配置".to_string();
        state.set_kernel_status(KernelStatus::failed(message.clone()));
        state.push_log(LogEntry::app("error", message.clone()));
        emit_status(app);
        // 仍需消费事件流，避免管道阻塞（进程已被 kill，流很快结束）。
        drain_events(app.clone(), receiver, true);
        return Err(AppError::KernelStart(message));
    }

    // 健康检查通过说明本次启动是成功的，清零自动重启计数：
    // 否则计数跨会话只增不减，撞满 5 次后内核再也不会有自动重启机会。
    state.reset_restart_count();

    let mut status = KernelStatus::running(pid, now_millis());
    status.version = version.clone();
    state.set_kernel_status(status);
    state.push_log(LogEntry::app(
        "info",
        format!(
            "内核启动成功：{}，混合端口 {}，控制端口 {}",
            version.unwrap_or_else(|| "未知版本".to_string()),
            settings.mixed_port,
            settings.controller_port
        ),
    ));
    emit_status(app);

    drain_events(app.clone(), receiver, false);

    Ok(())
}

/// 消费内核输出流并在进程退出后做重启判定。
fn drain_events(app: AppHandle, mut receiver: tokio::sync::mpsc::Receiver<CommandEvent>, passive: bool) {
    tauri::async_runtime::spawn(async move {
        while let Some(event) = receiver.recv().await {
            match event {
                CommandEvent::Stdout(bytes) => forward_log(&app, "info", &bytes),
                CommandEvent::Stderr(bytes) => forward_log(&app, "warning", &bytes),
                CommandEvent::Terminated(payload) => {
                    forward_log(
                        &app,
                        "warning",
                        format!("内核进程退出：code={:?} signal={:?}", payload.code, payload.signal)
                            .as_bytes(),
                    );
                }
                CommandEvent::Error(error) => forward_log(&app, "error", error.as_bytes()),
                _ => {}
            }
        }
        if !passive {
            handle_unexpected_exit(&app).await;
        }
    });
}

/// 主动停止内核。
pub async fn stop_kernel(app: &AppHandle) -> AppResult<()> {
    {
        let supervisor = app.state::<KernelSupervisor>();
        supervisor.set_stopping(true);
        if let Some(child) = supervisor.take_child() {
            let _ = child.kill();
        }
    }

    // 给进程一点退出时间，保证端口被释放。
    tokio::time::sleep(Duration::from_millis(350)).await;

    let state = app.state::<AppState>();
    state.set_kernel_status(KernelStatus::default());
    state.push_log(LogEntry::app("info", "内核已停止"));
    emit_status(app);

    Ok(())
}

/// 同步停止内核，供退出流程调用（`RunEvent::Exit` 与托盘"退出"不能 await）。
///
/// 只做三件事：标记为主动停止、kill 子进程、复位内核状态。
/// 不发事件（此时前端可能已销毁），可重复调用。
pub fn shutdown_sync(app: &AppHandle) {
    {
        let supervisor = app.state::<KernelSupervisor>();
        supervisor.set_stopping(true);
        if let Some(child) = supervisor.take_child() {
            let _ = child.kill();
        }
    }
    // 给内核进程一点退出时间，避免应用刚退出时端口仍被占用。
    std::thread::sleep(Duration::from_millis(300));
    app.state::<AppState>()
        .set_kernel_status(KernelStatus::default());
}

/// 重启内核（保留重启计数）。
pub async fn restart_kernel(app: &AppHandle) -> AppResult<()> {
    stop_kernel(app).await?;
    wait_port_released(app).await;
    start_kernel(app).await
}

/// 等待控制器端口释放，避免"Address already in use"。
async fn wait_port_released(app: &AppHandle) {
    let state = app.state::<AppState>();
    let port = state.settings_snapshot().controller_port;
    for _ in 0..20 {
        let in_use = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.is_ok();
        if !in_use {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn handle_unexpected_exit(app: &AppHandle) {
    let (stopping, had_child) = {
        let supervisor = app.state::<KernelSupervisor>();
        let had = supervisor.take_child().is_some();
        (supervisor.is_stopping(), had)
    };

    if stopping || !had_child {
        return;
    }

    let state = app.state::<AppState>();
    let count = state.bump_restart_count();

    if count > MAX_AUTO_RESTARTS {
        let status = KernelStatus::failed(format!(
            "内核连续异常退出 {count} 次，已停止自动重启，请检查配置或内核日志"
        ));
        state.set_kernel_status(status);
        state.push_log(LogEntry::app("error", "已停止自动重启内核"));
        emit_status(app);
        return;
    }

    let mut status = KernelStatus::default();
    status.kind = KernelStatusKind::Restarting;
    status.message = Some(format!("内核异常退出，正在进行第 {count} 次自动重启"));
    state.set_kernel_status(status);
    emit_status(app);

    let delay = std::cmp::min(2u64 * count as u64, 10);
    state.push_log(LogEntry::app(
        "warning",
        format!("内核异常退出，{delay} 秒后自动重启（第 {count}/{MAX_AUTO_RESTARTS} 次）"),
    ));

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(delay)).await;
        if let Err(error) = start_kernel(&app_handle).await {
            app_handle
                .state::<AppState>()
                .push_log(LogEntry::app("error", format!("自动重启失败：{error}")));
        }
    });
}

/* ------------------------------ 工具函数 ------------------------------ */

fn forward_log(app: &AppHandle, level: &str, bytes: &[u8]) {
    let text = String::from_utf8_lossy(bytes).trim().to_string();
    if text.is_empty() {
        return;
    }
    let level = detect_level(&text).unwrap_or_else(|| level.to_string());
    let entry = LogEntry::kernel(level, text);
    {
        let state = app.state::<AppState>();
        state.push_log(entry.clone());
    }
    let _ = app.emit(crate::clash_api::EVENT_LOG, entry);
}

/// 从 mihomo 日志行中识别等级：`time="..." level=info msg="..."`。
fn detect_level(line: &str) -> Option<String> {
    let index = line.find("level=")?;
    let rest = &line[index + "level=".len()..];
    let raw: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_lowercase();
    if raw.is_empty() {
        return None;
    }
    Some(match raw.as_str() {
        "warning" | "warn" => "warning",
        "error" | "fatal" => "error",
        "debug" => "debug",
        _ => "info",
    }
    .to_string())
}

pub fn emit_status(app: &AppHandle) {
    let status = app.state::<AppState>().kernel_status();
    let _ = app.emit(EVENT_KERNEL_STATUS, status);
}

/// 把最近的内核日志写入文件（便于崩溃后排查），失败静默忽略。
pub fn persist_log_tail(app: &AppHandle) {
    let state = app.state::<AppState>();
    let logs = state.recent_logs(200);
    let content = logs
        .iter()
        .map(|entry| format!("[{}][{}][{}] {}", entry.time, entry.source, entry.level, entry.message))
        .collect::<Vec<_>>()
        .join("\n");
    let path = state.paths.data_dir.join("last-session.log");
    let _ = storage::write_atomic(&path, &content);
}
