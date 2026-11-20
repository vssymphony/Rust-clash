//! 统一错误类型。
//!
//! 所有 Tauri command 均返回 `Result<T, AppError>`，`AppError` 序列化为字符串，
//! 前端可直接展示 `toErrorMessage(error)` 的结果。

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("内核未运行")]
    KernelNotRunning,

    #[error("内核启动失败：{0}")]
    KernelStart(String),

    #[error("内核未找到：{0}")]
    KernelMissing(String),

    #[error("Clash API 请求失败：{0}")]
    Api(String),

    #[error("配置错误：{0}")]
    Config(String),

    #[error("订阅错误：{0}")]
    Subscription(String),

    #[error("系统代理设置失败：{0}")]
    SysProxy(String),

    #[error("没有可用的代理节点，请先导入订阅")]
    NoProxies,

    #[error("文件操作失败：{0}")]
    Io(#[from] std::io::Error),

    #[error("JSON 解析失败：{0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML 解析失败：{0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("网络请求失败：{0}")]
    Http(#[from] reqwest::Error),

    #[error("Tauri 运行时错误：{0}")]
    Tauri(#[from] tauri::Error),

    #[error("插件错误：{0}")]
    Plugin(String),

    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn api(message: impl Into<String>) -> Self {
        AppError::Api(message.into())
    }

    pub fn config(message: impl Into<String>) -> Self {
        AppError::Config(message.into())
    }

    pub fn other(message: impl Into<String>) -> Self {
        AppError::Other(message.into())
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<tauri_plugin_shell::Error> for AppError {
    fn from(error: tauri_plugin_shell::Error) -> Self {
        AppError::Plugin(error.to_string())
    }
}

/// 让 `Result<_, String>` 的帮助函数（如 TUN 前置条件校验）可以直接 `?` 上抛。
impl From<String> for AppError {
    fn from(message: String) -> Self {
        AppError::Other(message)
    }
}

impl From<&str> for AppError {
    fn from(message: &str) -> Self {
        AppError::Other(message.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

/* ================================ 测试 ================================ */

/// 错误类型的两条对外契约：
/// 1. `Display` 文案直接展示给用户，不能退化成 `Debug`；
/// 2. 序列化为**纯字符串**（前端 `invoke` 的 reject 值），不是结构化对象。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_uses_chinese_prefix_and_payload() {
        assert_eq!(AppError::KernelNotRunning.to_string(), "内核未运行");
        assert_eq!(
            AppError::Config("missing port".to_string()).to_string(),
            "配置错误：missing port",
        );
        assert_eq!(
            AppError::NoProxies.to_string(),
            "没有可用的代理节点，请先导入订阅"
        );
    }

    #[test]
    fn serializes_as_plain_string() {
        let encoded =
            serde_json::to_string(&AppError::Api("timeout".to_string())).expect("错误应可序列化");
        assert_eq!(encoded, "\"Clash API 请求失败：timeout\"");
        assert!(
            !encoded.contains('{'),
            "错误不得序列化为对象，否则前端 toErrorMessage 无法处理"
        );
    }

    #[test]
    fn from_string_maps_to_other_and_keeps_message() {
        let error = AppError::from("缺少 Kernel 二进制".to_string());
        assert!(matches!(error, AppError::Other(_)));
        assert_eq!(error.to_string(), "缺少 Kernel 二进制");

        let from_str: AppError = "tun 前置条件不满足".into();
        assert!(matches!(from_str, AppError::Other(_)));
        assert_eq!(from_str.to_string(), "tun 前置条件不满足");
    }

    #[test]
    fn io_error_conversion_keeps_kind_and_message() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "settings.json 丢失");
        let error = AppError::from(io);
        assert!(matches!(error, AppError::Io(_)), "应保留 Io 变体以区分类型");
        let text = error.to_string();
        assert!(text.starts_with("文件操作失败："), "文案应为 {text}");
        assert!(text.contains("settings.json 丢失"));
    }

    #[test]
    fn json_and_yaml_errors_are_wrapped() {
        let json_error = serde_json::from_str::<u8>("not-json").expect_err("应解析失败");
        let wrapped = AppError::from(json_error);
        assert!(matches!(wrapped, AppError::Json(_)));
        assert!(wrapped.to_string().starts_with("JSON 解析失败："));

        let yaml_error = serde_yaml::from_str::<u8>("[").expect_err("应解析失败");
        let wrapped = AppError::from(yaml_error);
        assert!(matches!(wrapped, AppError::Yaml(_)));
        assert!(wrapped.to_string().starts_with("YAML 解析失败："));
    }
}
