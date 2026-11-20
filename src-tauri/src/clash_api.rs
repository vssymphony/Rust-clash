//! Clash / mihomo REST API 客户端与 WebSocket 实时数据订阅。
//!
//! * REST 走 `reqwest`，固定访问 `127.0.0.1`，并显式禁用系统代理，
//!   避免"代理客户端自身请求被自己代理"的死循环。
//! * 实时数据（流量 / 连接）走 WebSocket 长连接，断线后自动重连。

use std::time::Duration;

use futures_util::StreamExt;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

use crate::error::{AppError, AppResult};
use crate::models::{LogEntry, TrafficSample};
use crate::state::AppState;

pub const EVENT_TRAFFIC: &str = "clash://traffic";
pub const EVENT_CONNECTIONS: &str = "clash://connections";
pub const EVENT_LOG: &str = "clash://log";

#[derive(Clone)]
pub struct ClashClient {
    client: reqwest::Client,
    base: String,
    secret: String,
}

impl ClashClient {
    pub fn new(port: u16, secret: &str) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(12))
            .connect_timeout(Duration::from_secs(4))
            .no_proxy()
            .build()
            .map_err(|error| AppError::api(format!("初始化 HTTP 客户端失败：{error}")))?;

        Ok(Self {
            client,
            base: format!("http://127.0.0.1:{port}"),
            secret: secret.to_string(),
        })
    }

    fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let builder = self
            .client
            .request(method, format!("{}{}", self.base, path));
        if self.secret.is_empty() {
            builder
        } else {
            builder.bearer_auth(&self.secret)
        }
    }

    async fn send(&self, builder: reqwest::RequestBuilder) -> AppResult<Value> {
        let response = builder.send().await?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();

        if !status.is_success() {
            let detail = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|value| {
                    value
                        .get("message")
                        .and_then(|message| message.as_str())
                        .map(|message| message.to_string())
                })
                .unwrap_or_else(|| {
                    let trimmed = text.trim();
                    if trimmed.is_empty() {
                        "无响应内容".to_string()
                    } else {
                        trimmed.chars().take(200).collect()
                    }
                });
            return Err(AppError::api(format!(
                "Clash API 请求失败（HTTP {}）：{}",
                status.as_u16(),
                detail
            )));
        }

        if text.trim().is_empty() {
            return Ok(Value::Null);
        }

        serde_json::from_str::<Value>(&text)
            .map_err(|error| AppError::api(format!("响应不是合法 JSON：{error}")))
    }

    pub async fn version(&self) -> AppResult<Value> {
        self.send(self.request(reqwest::Method::GET, "/version"))
            .await
    }

    pub async fn proxies(&self) -> AppResult<Value> {
        self.send(self.request(reqwest::Method::GET, "/proxies"))
            .await
    }

    pub async fn rules(&self) -> AppResult<Value> {
        self.send(self.request(reqwest::Method::GET, "/rules"))
            .await
    }

    pub async fn connections(&self) -> AppResult<Value> {
        self.send(self.request(reqwest::Method::GET, "/connections"))
            .await
    }

    pub async fn close_connection(&self, id: &str) -> AppResult<()> {
        let path = format!("/connections/{}", urlencode(id));
        self.send(self.request(reqwest::Method::DELETE, &path))
            .await
            .map(|_| ())
    }

    pub async fn close_all_connections(&self) -> AppResult<()> {
        self.send(self.request(reqwest::Method::DELETE, "/connections"))
            .await
            .map(|_| ())
    }

    /// 切换策略组选中的节点。
    pub async fn select_proxy(&self, group: &str, name: &str) -> AppResult<()> {
        let path = format!("/proxies/{}", urlencode(group));
        self.send(
            self.request(reqwest::Method::PUT, &path)
                .json(&json!({ "name": name })),
        )
        .await
        .map(|_| ())
    }

    /// 单节点延迟测试。
    pub async fn proxy_delay(&self, name: &str, test_url: &str, timeout_ms: u64) -> AppResult<u64> {
        let path = format!(
            "/proxies/{}/delay?url={}&timeout={}",
            urlencode(name),
            urlencode(test_url),
            timeout_ms
        );
        let value = self.send(self.request(reqwest::Method::GET, &path)).await?;
        value
            .get("delay")
            .and_then(|delay| delay.as_u64())
            .ok_or_else(|| AppError::api("延迟测试响应缺少 delay 字段"))
    }

    /// 整个策略组批量测速，返回 `名称 -> 延迟(毫秒)`。
    pub async fn group_delay(
        &self,
        group: &str,
        test_url: &str,
        timeout_ms: u64,
    ) -> AppResult<Vec<(String, u64)>> {
        let path = format!(
            "/group/{}/delay?url={}&timeout={}",
            urlencode(group),
            urlencode(test_url),
            timeout_ms
        );
        let value = self.send(self.request(reqwest::Method::GET, &path)).await?;
        let mut result = Vec::new();
        if let Some(map) = value.as_object() {
            for (name, delay) in map {
                if let Some(delay) = delay.as_u64() {
                    result.push((name.clone(), delay));
                }
            }
        }
        Ok(result)
    }

    pub async fn get_configs(&self) -> AppResult<Value> {
        self.send(self.request(reqwest::Method::GET, "/configs"))
            .await
    }

    pub async fn patch_configs(&self, patch: Value) -> AppResult<()> {
        self.send(
            self.request(reqwest::Method::PATCH, "/configs")
                .json(&patch),
        )
        .await
        .map(|_| ())
    }

    /// 让内核重新加载配置文件。
    pub async fn reload_config(&self, path: &str) -> AppResult<()> {
        self.send(
            self.request(reqwest::Method::PUT, "/configs?force=true")
                .json(&json!({ "path": path })),
        )
        .await
        .map(|_| ())
    }

    pub async fn flush_fakeip(&self) -> AppResult<()> {
        self.send(self.request(reqwest::Method::POST, "/cache/fakeip/flush"))
            .await
            .map(|_| ())
    }
}

/// 极简路径/查询参数编码，只处理会破坏 URL 的字符。
fn urlencode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            },
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

/* --------------------------- WebSocket 实时订阅 --------------------------- */

fn ws_url(port: u16, path: &str) -> String {
    format!("ws://127.0.0.1:{port}{path}")
}

/// 启动一条 WebSocket 订阅任务（断线后按指数退避重连，30 秒封顶）。
pub fn spawn_ws_task(app: AppHandle, path: &'static str) {
    tauri::async_runtime::spawn(async move {
        // 退避：2 → 4 → 8 → 16 → 30 秒封顶，连接成功后立即复位。
        // 固定 2 秒重连在内核端口不可达时会形成连接风暴（三条任务每 2 秒各试一次），
        // 既浪费 CPU，也会把环形日志缓冲刷满、掩盖真正的错误。
        let mut backoff_secs: u64 = 2;
        let mut failures: u32 = 0;
        loop {
            let (port, secret, running) = {
                let state = app.state::<AppState>();
                let settings = state.settings_snapshot();
                let running = state.kernel_status().is_running();
                (
                    settings.controller_port,
                    settings.controller_secret,
                    running,
                )
            };

            if !running {
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }

            let url = ws_url(port, path);
            let request = match url.as_str().into_client_request() {
                Ok(mut request) => {
                    if !secret.is_empty() {
                        if let Ok(value) = HeaderValue::from_str(&format!("Bearer {secret}")) {
                            request.headers_mut().insert(AUTHORIZATION, value);
                        }
                    }
                    request
                },
                Err(_) => {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                },
            };

            match tokio_tungstenite::connect_async(request).await {
                Ok((mut socket, _response)) => {
                    backoff_secs = 2;
                    failures = 0;
                    loop {
                        match socket.next().await {
                            Some(Ok(Message::Text(text))) => {
                                dispatch(&app, path, text.to_string());
                            },
                            Some(Ok(Message::Binary(bytes))) => {
                                dispatch(&app, path, String::from_utf8_lossy(&bytes).to_string());
                            },
                            Some(Ok(Message::Close(_))) => break,
                            Some(Ok(_)) => {},
                            Some(Err(_)) => break,
                            None => break,
                        }
                    }
                },
                Err(error) => {
                    failures = failures.saturating_add(1);
                    // 只在首次失败与每 10 次失败时记录，避免重连噪声淹没真正的错误。
                    if failures == 1 || failures.is_multiple_of(10) {
                        let state = app.state::<AppState>();
                        state.push_log(LogEntry::app(
                            "debug",
                            format!(
                                "WebSocket {path} 连接失败（第 {failures} 次，{backoff_secs} 秒后重试）：{error}"
                            ),
                        ));
                    }
                },
            }

            tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
            backoff_secs = std::cmp::min(backoff_secs.saturating_mul(2), 30);
        }
    });
}

fn dispatch(app: &AppHandle, path: &str, payload: String) {
    match path {
        "/traffic" => {
            let value: Value = match serde_json::from_str(&payload) {
                Ok(value) => value,
                Err(_) => return,
            };
            let up = value.get("up").and_then(|item| item.as_u64()).unwrap_or(0);
            let down = value
                .get("down")
                .and_then(|item| item.as_u64())
                .unwrap_or(0);

            let sample = {
                let state = app.state::<AppState>();
                state.accumulate_traffic(up, down)
            };

            let _ = app.emit(EVENT_TRAFFIC, sample);
        },
        "/connections" => {
            if let Ok(value) = serde_json::from_str::<Value>(&payload) {
                let _ = app.emit(EVENT_CONNECTIONS, value);
            }
        },
        "/logs" => {
            let value: Value = match serde_json::from_str(&payload) {
                Ok(value) => value,
                Err(_) => return,
            };
            let level = value
                .get("type")
                .and_then(|item| item.as_str())
                .unwrap_or("info")
                .to_string();
            let message = value
                .get("payload")
                .and_then(|item| item.as_str())
                .unwrap_or("")
                .to_string();
            if message.is_empty() {
                return;
            }
            let entry = LogEntry::kernel(&level, message);
            {
                let state = app.state::<AppState>();
                state.push_log(entry.clone());
            }
            let _ = app.emit(EVENT_LOG, entry);
        },
        _ => {},
    }
}

/// 便捷方法：用当前设置构造客户端。
pub fn client_from_state(state: &AppState) -> AppResult<ClashClient> {
    let settings = state.settings_snapshot();
    ClashClient::new(settings.controller_port, &settings.controller_secret)
}

/// 流量累计值的复位（切换订阅 / 重启内核时调用）。
pub fn reset_traffic_totals(state: &AppState) -> TrafficSample {
    state.reset_traffic()
}
