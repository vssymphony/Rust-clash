//! 系统托盘：显示主窗口、切换系统代理、重启内核、退出。

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use crate::error::{AppError, AppResult};
use crate::models::LogEntry;
use crate::state::AppState;
use crate::{commands, mihomo_guard};

pub fn create(app: &AppHandle) -> AppResult<()> {
    let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)
        .map_err(|error| AppError::other(format!("创建托盘菜单项失败：{error}")))?;
    let toggle_proxy = MenuItem::with_id(
        app,
        "toggle_proxy",
        "开启 / 关闭系统代理",
        true,
        None::<&str>,
    )
    .map_err(|error| AppError::other(format!("创建托盘菜单项失败：{error}")))?;
    let restart_kernel =
        MenuItem::with_id(app, "restart_kernel", "重启内核", true, None::<&str>)
            .map_err(|error| AppError::other(format!("创建托盘菜单项失败：{error}")))?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)
        .map_err(|error| AppError::other(format!("创建托盘菜单项失败：{error}")))?;
    // 分隔线必须各自独立实例：同一个 PredefinedMenuItem 只能挂到一个父菜单下，
    // 复用同一个对象加入两次会覆盖其 parent，导致菜单项丢失或位置错乱。
    let separator_top = PredefinedMenuItem::separator(app)
        .map_err(|error| AppError::other(format!("创建托盘分隔线失败：{error}")))?;
    let separator_bottom = PredefinedMenuItem::separator(app)
        .map_err(|error| AppError::other(format!("创建托盘分隔线失败：{error}")))?;

    let menu = Menu::with_items(
        app,
        &[
            &show,
            &separator_top,
            &toggle_proxy,
            &restart_kernel,
            &separator_bottom,
            &quit,
        ],
    )
    .map_err(|error| AppError::other(format!("创建托盘菜单失败：{error}")))?;

    let mut builder = TrayIconBuilder::with_id("clash-gui-tray")
        .tooltip("Clash GUI")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder
        .build(app)
        .map_err(|error| AppError::other(format!("创建托盘图标失败：{error}")))?;

    Ok(())
}

/// 从托盘或单实例唤起时把主窗口拉到前台。
pub fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        "show" => show_main_window(app),
        "toggle_proxy" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let enabled = {
                    let state = app.state::<AppState>();
                    state.system_proxy().enabled
                };
                if let Err(error) = commands::set_system_proxy(app.clone(), !enabled).await {
                    let entry = LogEntry::app("error", format!("切换系统代理失败：{error}"));
                    app.state::<AppState>().push_log(entry.clone());
                    let _ = app.emit(crate::clash_api::EVENT_LOG, entry);
                }
            });
        },
        "restart_kernel" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = mihomo_guard::restart_kernel(&app).await {
                    let entry = LogEntry::app("error", format!("重启内核失败：{error}"));
                    app.state::<AppState>().push_log(entry.clone());
                    // 与 toggle_proxy 保持一致：托盘触发的错误也要推给前端日志面板。
                    let _ = app.emit(crate::clash_api::EVENT_LOG, entry);
                }
            });
        },
        "quit" => crate::quit_app(app),
        _ => {},
    }
}
