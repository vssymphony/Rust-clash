//! 前后端共享的数据结构定义。
//!
//! 所有结构体统一使用 `camelCase` 序列化，与 `src/lib/types.ts` 中的 TypeScript
//! 类型保持一一对应，避免前端再做字段名转换。

use serde::{Deserialize, Serialize};

/// 内核运行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KernelStatusKind {
    Stopped,
    Starting,
    Running,
    Restarting,
    Failed,
}

/// 内核状态快照，前端用于渲染状态灯与诊断信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KernelStatus {
    pub kind: KernelStatusKind,
    /// 内核进程 PID，未运行时为 None
    pub pid: Option<u32>,
    /// 状态说明 / 错误原因
    pub message: Option<String>,
    /// 自动重启次数（本次应用运行期间累计）
    pub restart_count: u32,
    /// 启动时间（Unix 毫秒）
    pub started_at: Option<i64>,
    /// mihomo 版本号，启动后由 /version 接口回填
    pub version: Option<String>,
}

impl Default for KernelStatus {
    fn default() -> Self {
        Self {
            kind: KernelStatusKind::Stopped,
            pid: None,
            message: None,
            restart_count: 0,
            started_at: None,
            version: None,
        }
    }
}

impl KernelStatus {
    pub fn starting() -> Self {
        Self {
            kind: KernelStatusKind::Starting,
            ..Default::default()
        }
    }

    pub fn running(pid: u32, started_at: i64) -> Self {
        Self {
            kind: KernelStatusKind::Running,
            pid: Some(pid),
            message: None,
            restart_count: 0,
            started_at: Some(started_at),
            version: None,
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            kind: KernelStatusKind::Failed,
            message: Some(message.into()),
            ..Default::default()
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.kind, KernelStatusKind::Running)
    }
}

/// 代理模式，与 mihomo 的 mode 取值一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    Rule,
    Global,
    Direct,
}

impl ProxyMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProxyMode::Rule => "rule",
            ProxyMode::Global => "global",
            ProxyMode::Direct => "direct",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "global" => ProxyMode::Global,
            "direct" => ProxyMode::Direct,
            _ => ProxyMode::Rule,
        }
    }
}

/// 日志等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Silent,
    Error,
    Warning,
    Info,
    Debug,
}

impl LogLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Silent => "silent",
            LogLevel::Error => "error",
            LogLevel::Warning => "warning",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
        }
    }
}

/// 应用设置，持久化到 `<config_dir>/settings.json`。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    /// 混合端口（HTTP + SOCKS 共用）
    pub mixed_port: u16,
    /// Clash 外部控制器端口
    pub controller_port: u16,
    pub controller_secret: String,
    pub mode: ProxyMode,
    pub log_level: LogLevel,
    pub allow_lan: bool,
    /// 期望的系统代理状态（用于崩溃后恢复/清理）
    pub system_proxy: bool,
    pub auto_start: bool,
    pub start_minimized: bool,
    /// 订阅自动更新周期（分钟），0 表示关闭
    pub sub_update_interval_minutes: u64,
    pub latency_test_url: String,
    pub current_subscription_id: Option<String>,
    /// 自定义内核二进制路径，为空则使用随包分发的 sidecar
    pub kernel_path: Option<String>,
    pub close_connections_on_switch: bool,
    /// 是否启用 TUN（虚拟网卡）模式
    pub tun_enabled: bool,
    /// TUN 协议栈实现：gvisor / system / mixed
    pub tun_stack: TunStack,
    /// TUN 设备名，None 表示使用系统默认（Windows: `Mihomo`，Linux: `mihomo`）
    pub tun_device_name: Option<String>,
    /// 自动接管系统路由
    pub tun_auto_route: bool,
    /// 自动探测并绑定出口网卡
    pub tun_auto_detect_interface: bool,
    /// 严格路由：阻止流量绕开 TUN 从物理网卡泄漏
    pub tun_strict_route: bool,
    /// DNS 劫持目标，交给 TUN 内置 DNS 处理
    pub tun_dns_hijack: Vec<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            mixed_port: 7890,
            controller_port: 9090,
            controller_secret: String::new(),
            mode: ProxyMode::Rule,
            log_level: LogLevel::Info,
            allow_lan: false,
            system_proxy: false,
            auto_start: false,
            start_minimized: false,
            sub_update_interval_minutes: 360,
            latency_test_url: "https://www.gstatic.com/generate_204".to_string(),
            current_subscription_id: None,
            kernel_path: None,
            close_connections_on_switch: true,
            tun_enabled: false,
            tun_stack: TunStack::Gvisor,
            tun_device_name: None,
            tun_auto_route: true,
            tun_auto_detect_interface: true,
            tun_strict_route: false,
            tun_dns_hijack: default_dns_hijack(),
        }
    }
}

/// TUN 默认劫持的 DNS 端口（mihomo 的 `tun.dns-hijack` 默认值）。
fn default_dns_hijack() -> Vec<String> {
    vec!["any:53".to_string()]
}

/// 订阅条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscription {
    pub id: String,
    pub name: String,
    /// 远程订阅地址；本地文件订阅为 None
    pub url: Option<String>,
    /// 本地导入时的源文件路径
    pub file_path: Option<String>,
    /// 解析出的节点数量
    pub node_count: u32,
    /// 创建时间（Unix 毫秒）
    pub created_at: i64,
    /// 最近一次更新时间（Unix 毫秒）
    pub updated_at: i64,
    /// 订阅服务端返回的流量信息（subscription-userinfo 头，原样保存）
    pub user_info: Option<String>,
}

/// 订阅创建入参（来自前端）。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SubscriptionInput {
    pub name: String,
    pub url: Option<String>,
    pub file_path: Option<String>,
}

/// 系统代理实现后端。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SystemProxyBackend {
    Windows,
    Gnome,
    Kde,
    Unsupported,
}

/// 系统代理状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemProxyState {
    pub enabled: bool,
    pub backend: SystemProxyBackend,
    /// 降级或失败时的说明文案
    pub message: Option<String>,
}

impl Default for SystemProxyState {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: if cfg!(windows) {
                SystemProxyBackend::Windows
            } else {
                SystemProxyBackend::Unsupported
            },
            message: None,
        }
    }
}

/* --------------------------------- TUN --------------------------------- */

/// TUN 协议栈实现，与 mihomo 的 `tun.stack` 取值一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TunStack {
    /// 用户态协议栈，兼容性最好（默认）
    Gvisor,
    /// 系统协议栈，性能更高但依赖内核参数
    System,
    /// 混合：TCP 走 system，UDP 走 gvisor
    Mixed,
}

impl TunStack {
    pub fn as_str(&self) -> &'static str {
        match self {
            TunStack::Gvisor => "gvisor",
            TunStack::System => "system",
            TunStack::Mixed => "mixed",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "gvisor" => TunStack::Gvisor,
            "system" => TunStack::System,
            _ => TunStack::Mixed,
        }
    }
}

/// TUN 权限探测结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunPrivilege {
    /// 当前平台是否实现了 TUN 支持（Windows / Linux 为 true）
    pub supported: bool,
    /// 平台标识：`windows` / `linux` / 其他
    pub platform: String,
    /// 是否具备创建虚拟网卡所需权限
    pub elevated: bool,
    /// 权限来源：`administrator` / `root` / `cap_net_admin` / `unknown`
    pub mechanism: String,
    /// Windows 专用：是否在 sidecar 目录找到 wintun.dll
    pub wintun_present: Option<bool>,
    /// 探测到的具体信息（如 euid、CapEff），便于排障
    pub detail: Option<String>,
    /// 权限不足时的手动处理指引
    pub hint: Option<String>,
}

impl TunPrivilege {
    pub fn unknown(platform: &str) -> Self {
        Self {
            supported: false,
            platform: platform.to_string(),
            elevated: false,
            mechanism: "unknown".to_string(),
            wintun_present: None,
            detail: None,
            hint: Some(format!(
                "当前平台（{platform}）暂未实现 TUN 模式，请改用系统代理模式"
            )),
        }
    }
}

/// TUN 运行状态快照（前端设置页展示）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TunState {
    /// 平台是否支持 TUN
    pub supported: bool,
    /// 配置中是否启用 TUN（用户意图）
    pub enabled: bool,
    /// 内核当前是否真的跑在 TUN 模式（配置启用 + 内核运行 + 权限满足）
    pub active: bool,
    pub stack: TunStack,
    /// 实际生效的设备名
    pub device_name: String,
    /// 权限与依赖探测结果
    pub privilege: TunPrivilege,
    /// 面向用户的综合说明（正常 / 降级 / 失败原因）
    pub message: Option<String>,
}

/// 应用初始化快照，前端启动时一次拉取。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStateSnapshot {
    pub kernel: KernelStatus,
    pub settings: AppSettings,
    pub system_proxy: SystemProxyState,
    pub subscriptions: Vec<Subscription>,
    pub app_version: String,
    pub config_path: String,
    pub data_dir: String,
    pub platform: String,
}

/// 流量采样。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrafficSample {
    /// 上行速率（字节/秒）
    pub up: u64,
    /// 下行速率（字节/秒）
    pub down: u64,
    /// 累计上行（字节）
    pub up_total: u64,
    /// 累计下行（字节）
    pub down_total: u64,
}

/// 单节点延迟测试结果。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyDelayResult {
    pub name: String,
    pub delay: u64,
    pub error: Option<String>,
}

/// 日志条目。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    /// Unix 毫秒
    pub time: i64,
    pub level: String,
    pub message: String,
    /// "kernel" 或 "app"
    pub source: String,
}

impl LogEntry {
    pub fn new(level: &str, message: impl Into<String>, source: &str) -> Self {
        Self {
            time: crate::time::now_millis(),
            level: level.to_string(),
            message: message.into(),
            source: source.to_string(),
        }
    }

    pub fn app(level: &str, message: impl Into<String>) -> Self {
        Self::new(level, message, "app")
    }

    pub fn kernel(level: &str, message: impl Into<String>) -> Self {
        Self::new(level, message, "kernel")
    }
}

/* ================================ 测试 ================================ */

/// 设置模型的序列化契约与向后兼容性测试。
///
/// 这些用例锁定三类容易回归的行为：
/// 1. 磁盘/前端 JSON 使用 camelCase，重命名字段会破坏已存配置；
/// 2. `#[serde(default)]` 让旧版本配置文件缺少新字段时仍可加载（向后兼容）；
/// 3. TUN 相关枚举的取值范围与 mihomo 配置一致。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_serializes_as_camel_case() {
        let value = serde_json::to_value(AppSettings::default()).expect("设置应可序列化");
        assert_eq!(value["mixedPort"], serde_json::json!(7890));
        assert_eq!(value["controllerPort"], serde_json::json!(9090));
        assert_eq!(value["subUpdateIntervalMinutes"], serde_json::json!(360));
        assert_eq!(value["mode"], serde_json::json!("rule"));
        assert_eq!(value["logLevel"], serde_json::json!("info"));
        assert_eq!(value["tunEnabled"], serde_json::json!(false));
        assert_eq!(value["tunStack"], serde_json::json!("gvisor"));
        assert_eq!(value["tunDnsHijack"], serde_json::json!(["any:53"]));
        assert_eq!(value["tunAutoRoute"], serde_json::json!(true));
        assert_eq!(value["tunStrictRoute"], serde_json::json!(false));
        assert!(
            value.get("tunDeviceName").is_some(),
            "可空字段也应出现在 JSON 中（值为 null）"
        );
    }

    #[test]
    fn empty_object_loads_all_defaults() {
        let settings: AppSettings = serde_json::from_str("{}").expect("空对象应可加载");
        assert_eq!(settings.mixed_port, 7890);
        assert_eq!(settings.controller_port, 9090);
        assert!(settings.controller_secret.is_empty());
        assert_eq!(settings.mode, ProxyMode::Rule);
        assert_eq!(settings.log_level, LogLevel::Info);
        assert_eq!(settings.sub_update_interval_minutes, 360);
        assert_eq!(settings.tun_stack, TunStack::Gvisor);
        assert_eq!(settings.tun_dns_hijack, vec!["any:53".to_string()]);
    }

    #[test]
    fn legacy_settings_without_tun_fields_still_load() {
        // 模拟首版（不含 TUN）写入的配置文件。
        let legacy =
            r#"{"mixedPort":7891,"controllerPort":9091,"mode":"global","logLevel":"debug"}"#;
        let settings: AppSettings = serde_json::from_str(legacy).expect("旧配置应可加载");
        assert_eq!(settings.mixed_port, 7891);
        assert_eq!(settings.controller_port, 9091);
        assert_eq!(settings.mode, ProxyMode::Global);
        assert_eq!(settings.log_level, LogLevel::Debug);
        assert!(!settings.tun_enabled, "旧配置不应默认开启 TUN");
        assert_eq!(settings.tun_stack, TunStack::Gvisor);
        assert_eq!(settings.tun_dns_hijack, vec!["any:53".to_string()]);
        assert_eq!(
            settings.latency_test_url,
            "https://www.gstatic.com/generate_204"
        );
    }

    #[test]
    fn tun_stack_round_trips_through_json() {
        for (stack, expected) in [
            (TunStack::Gvisor, "gvisor"),
            (TunStack::System, "system"),
            (TunStack::Mixed, "mixed"),
        ] {
            let encoded = serde_json::to_value(stack).expect("枚举应可序列化");
            assert_eq!(encoded, serde_json::json!(expected));
            let decoded: TunStack =
                serde_json::from_str(&format!("\"{expected}\"")).expect("枚举应可反序列化");
            assert_eq!(decoded, stack);
            assert_eq!(stack.as_str(), expected);
            assert_eq!(TunStack::parse(expected), stack);
        }
    }

    #[test]
    fn unknown_tun_stack_value_is_rejected() {
        let result = serde_json::from_str::<AppSettings>(r#"{"tunStack":"bogus"}"#);
        assert!(result.is_err(), "未知 stack 应报错而非静默降级");
    }

    #[test]
    fn tun_stack_parse_falls_back_to_mixed() {
        // 与 mihomo 保持一致：无法识别的取值回落 mixed。注意 serde 侧是严格报错，
        // `parse` 用于来自命令行/前端的宽松文本。
        assert_eq!(TunStack::parse("  SYSTEM "), TunStack::System);
        assert_eq!(TunStack::parse("GVisor"), TunStack::Gvisor);
        assert_eq!(TunStack::parse(""), TunStack::Mixed);
    }

    #[test]
    fn settings_round_trip_preserves_sensitive_fields() {
        let settings = AppSettings {
            controller_secret: "top-secret".to_string(),
            tun_enabled: true,
            tun_stack: TunStack::System,
            ..AppSettings::default()
        };
        let encoded = serde_json::to_string(&settings).expect("序列化应成功");
        let decoded: AppSettings = serde_json::from_str(&encoded).expect("反序列化应成功");
        assert_eq!(decoded.controller_secret, "top-secret");
        assert!(decoded.tun_enabled);
        assert_eq!(decoded.tun_stack, TunStack::System);
        assert_eq!(decoded.mixed_port, settings.mixed_port);
        assert_eq!(decoded.tun_dns_hijack, settings.tun_dns_hijack);
    }
}
