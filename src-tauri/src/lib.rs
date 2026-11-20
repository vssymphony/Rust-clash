//! clash-gui 应用入口（lib 形式，便于未来扩展移动端或单元测试）。
//!
//! 启动流程：
//! 1. 解析配置/数据目录，加载设置与订阅清单；
//! 2. 注册系统托盘、单实例锁、开机自启插件；
//! 3. 拉起 WebSocket 实时数据订阅与订阅定时更新任务；
//! 4. 若存在可用订阅则自动启动内核；
//! 5. 退出时停止内核并恢复系统代理。

// 模块全部 `pub`：`tests/` 下的集成测试以外部 crate 形式编译，
// 只能访问 lib 的公开项；同时避免"仅测试使用"的辅助函数触发 dead_code。
pub mod clash_api;
pub mod commands;
pub mod error;
pub mod mihomo_guard;
pub mod models;
pub mod redact;
pub mod state;
pub mod storage;
pub mod subscription;
pub mod subscription_service;
pub mod system_proxy;
pub mod time;
pub mod tray;
pub mod tun;
pub mod wintun;

use tauri::{AppHandle, Manager, RunEvent, WindowEvent};

use crate::models::LogEntry;
use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 单实例锁：重复启动时唤出已有窗口
        .plugin(tauri_plugin_single_instance::init(
            |app: &AppHandle, _argv: Vec<String>, _cwd: String| {
                tray::show_main_window(app);
            },
        ))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .invoke_handler(tauri::generate_handler![
            commands::get_app_state,
            commands::update_settings,
            commands::kernel_start,
            commands::kernel_stop,
            commands::kernel_restart,
            commands::kernel_status,
            commands::kernel_logs,
            commands::open_config_dir,
            commands::reload_config,
            commands::get_proxies,
            commands::select_proxy,
            commands::proxy_delay,
            commands::proxy_group_delay,
            commands::get_connections,
            commands::close_connection,
            commands::close_all_connections,
            commands::get_rules,
            commands::get_clash_config,
            commands::patch_clash_config,
            commands::list_subscriptions,
            commands::add_subscription,
            commands::remove_subscription,
            commands::update_subscription,
            commands::update_all_subscriptions,
            commands::set_active_subscription,
            commands::get_system_proxy,
            commands::set_system_proxy,
            commands::get_autostart,
            commands::set_autostart,
            commands::tun_status,
            commands::tun_check_privilege,
            commands::tun_toggle,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            let paths = storage::AppPaths::resolve(&handle)?;
            paths.ensure()?;

            let settings = storage::load_settings(&paths);
            let subscriptions = storage::load_subscriptions(&paths);

            app.manage(AppState::new(paths, settings.clone(), subscriptions));
            app.manage(mihomo_guard::KernelSupervisor::new());

            // 托盘失败不阻塞启动，只记录日志
            if let Err(error) = tray::create(&handle) {
                handle
                    .state::<AppState>()
                    .push_log(LogEntry::app("warning", format!("创建托盘失败：{error}")));
            }

            // 上次异常退出可能残留系统代理设置，先做一次恢复/清理
            system_proxy::recover_on_startup(&handle);

            // 实时数据订阅（内核未运行时任务会自行等待）
            clash_api::spawn_ws_task(handle.clone(), "/traffic");
            clash_api::spawn_ws_task(handle.clone(), "/connections");
            clash_api::spawn_ws_task(handle.clone(), "/logs");

            // 订阅定时更新
            subscription_service::spawn_auto_update(handle.clone());

            // 开机自启时以最小化方式启动
            let started_by_autostart = std::env::args().any(|arg| arg == "--minimized");
            if settings.start_minimized || started_by_autostart {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            // 有当前订阅或自定义内核路径时自动拉起内核
            if settings.current_subscription_id.is_some() || settings.kernel_path.is_some() {
                let boot_handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    match mihomo_guard::start_kernel(&boot_handle).await {
                        Ok(()) => {
                            let (enabled, port) = {
                                let state = boot_handle.state::<AppState>();
                                let snapshot = state.settings_snapshot();
                                (snapshot.system_proxy, snapshot.mixed_port)
                            };
                            if enabled {
                                if let Err(error) = system_proxy::enable(&boot_handle, port) {
                                    boot_handle.state::<AppState>().push_log(LogEntry::app(
                                        "warning",
                                        format!("恢复系统代理失败：{error}"),
                                    ));
                                }
                            }
                        }
                        Err(error) => {
                            boot_handle.state::<AppState>().push_log(LogEntry::app(
                                "error",
                                format!("自动启动内核失败：{error}"),
                            ));
                        }
                    }
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // 关闭窗口只是收起到托盘，真正退出走托盘菜单或系统退出
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("启动 Tauri 应用失败")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                finalize(app);
            }
        });
}

/// 托盘菜单"退出"：先做清理再退出进程。
pub fn quit_app(app: &AppHandle) {
    finalize(app);
    app.exit(0);
}

/// 退出清理：停内核（否则残留 mihomo 进程占用端口）、还原系统代理、落盘日志。
/// 该函数可重复调用。
fn finalize(app: &AppHandle) {
    mihomo_guard::shutdown_sync(app);
    system_proxy::disable_silently(app);
    mihomo_guard::persist_log_tail(app);
}
