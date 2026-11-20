//! 本地持久化：目录解析、设置读写、订阅清单读写。
//!
//! 目录布局（`app_config_dir`，Windows 下为 `%APPDATA%\/com.clashgui.desktop`）：
//!
//! ```text
//! <config_dir>/
//!   settings.json        应用设置
//!   subscriptions.json   订阅清单
//!   config.yaml          由本程序生成的 mihomo 运行配置
//!   data/                mihomo 工作目录（-d 参数，缓存/GeoIP 等）
//!   profiles/            订阅原始内容副本
//! ```

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::models::{AppSettings, Subscription};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub profiles_dir: PathBuf,
    pub config_file: PathBuf,
    pub settings_file: PathBuf,
    pub subscriptions_file: PathBuf,
}

impl AppPaths {
    pub fn resolve(app: &AppHandle) -> AppResult<Self> {
        let config_dir = app
            .path()
            .app_config_dir()
            .map_err(|error| AppError::config(format!("无法定位应用配置目录：{error}")))?;

        let data_dir = config_dir.join("data");
        let profiles_dir = config_dir.join("profiles");

        Ok(Self {
            config_file: config_dir.join("config.yaml"),
            settings_file: config_dir.join("settings.json"),
            subscriptions_file: config_dir.join("subscriptions.json"),
            config_dir,
            data_dir,
            profiles_dir,
        })
    }

    /// 创建所有必需目录。
    pub fn ensure(&self) -> AppResult<()> {
        for dir in [&self.config_dir, &self.data_dir, &self.profiles_dir] {
            fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    /// 订阅原始文件保存路径。
    ///
    /// `subscription_id` 可能来自前端或导入文件，属于不可信输入：
    /// 原样拼接时 `../../settings.json` 这类 id 能把文件写出 profiles 目录之外。
    /// 这里统一经 [`sanitize_id`] 清洗后再拼路径。
    pub fn profile_file(&self, subscription_id: &str) -> PathBuf {
        self.profiles_dir.join(format!("{}.yaml", sanitize_id(subscription_id)))
    }

    /// 用于展示的配置路径（转义为 `\` 以便 Windows 前端展示一致）。
    pub fn config_display(&self) -> String {
        self.config_file.to_string_lossy().to_string()
    }

    pub fn data_display(&self) -> String {
        self.data_dir.to_string_lossy().to_string()
    }
}

/// 清洗用作文件名片段的标识符，防止路径穿越。
///
/// 规则：只保留 `[A-Za-z0-9_-]`，其它字符（含 `.` `/` `\` `:` 与 Unicode 分隔符）
/// 一律替换为 `_`；最多 64 个字符；清洗后若无有效字符则回落到 `unknown`。
///
/// 这一点是"白名单"而非"黑名单"：不需要枚举各种穿越写法，
/// 只要结果里不可能出现分隔符，拼接出的路径就必然停在目标目录内。
pub fn sanitize_id(raw: &str) -> String {
    const MAX_LEN: usize = 64;
    let mut out = String::with_capacity(raw.len().min(MAX_LEN));
    for ch in raw.chars().take(MAX_LEN) {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.chars().all(|ch| ch == '_') {
        return "unknown".to_string();
    }
    out
}

/// 敏感文件的统一安全写（唯一入口）。
///
/// `settings.json`、`subscriptions.json`、`profiles/*.yaml` 以及生成的
/// `config.yaml` 里含 controller secret、订阅 token、节点密码 / UUID 等凭据，
/// **必须**走这个函数，不要直接 `fs::write`。四道保证：
///
/// 1. **唯一临时名**：`.<文件名>.<pid>.<纳秒>.tmp`。配置重载与设置保存可能并发，
///    共用固定 `xxx.tmp` 会互相覆盖，最终落盘内容交错。
/// 2. **原子替换 + fsync**：先写临时文件并 `sync_all()` 落盘，再 rename。
///    少了 fsync 时，"rename 成功但数据仍在页缓存"的崩溃场景会留下 0 字节文件。
/// 3. **备份**：替换前把旧文件复制为 `<文件名>.bak`（best-effort，失败不阻塞写入），
///    极端情况下可人工回滚。
/// 4. **权限收敛**：Unix 下临时文件与目标文件都设为 0600（仅属主可读写）。
///    Windows 不做额外处理：配置目录位于用户私有 AppData，权限由父目录 ACL 继承；
///    若要更严格（如禁止同用户下其他低权限进程读取），应在安装/首次启动时
///    用 `icacls` 收紧目录 —— 那是安装器职责，放在每次写文件的热路径里会反复拉起进程。
pub fn write_sensitive(path: &Path, content: &str) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // 临时文件名带进程号与纳秒时间戳
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "tmp".to_string());
    let tmp = path.with_file_name(format!(".{file_name}.{}.{stamp}.tmp", std::process::id()));

    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    }
    secure_permissions(&tmp)?;

    if path.exists() {
        let backup = path.with_file_name(format!("{file_name}.bak"));
        if fs::copy(path, &backup).is_ok() {
            let _ = secure_permissions(&backup);
        }
    }

    if path.exists() {
        // Windows 上 rename 无法覆盖已存在文件，必须先移除，这会留下一个
        // 极短的"文件不存在"窗口。读取侧已有兜底（load_settings /
        // load_subscriptions 解析失败即回落默认值），故接受该窗口。
        let _ = fs::remove_file(path);
    }
    if let Err(error) = fs::rename(&tmp, path) {
        // rename 失败必须清掉临时文件，否则会在配置目录里堆积残留。
        let _ = fs::remove_file(&tmp);
        return Err(error.into());
    }
    secure_permissions(path)?;
    Ok(())
}

/// 兼容别名：历史调用点都写的是 `write_atomic`，内部统一委托给
/// [`write_sensitive`]，避免"两套写文件实现"再次分叉。
pub fn write_atomic(path: &Path, content: &str) -> AppResult<()> {
    write_sensitive(path, content)
}

/// 把可能含凭据的文件权限收敛到 0600（仅 Unix 生效）。
fn secure_permissions(path: &Path) -> AppResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

pub fn read_to_string(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

/* ------------------------------- 设置读写 ------------------------------- */

pub fn load_settings(paths: &AppPaths) -> AppSettings {
    match fs::read_to_string(&paths.settings_file) {
        Ok(content) => serde_json::from_str::<AppSettings>(&content).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

pub fn save_settings(paths: &AppPaths, settings: &AppSettings) -> AppResult<()> {
    let content = serde_json::to_string_pretty(settings)?;
    write_atomic(&paths.settings_file, &content)
}

/* ------------------------------- 订阅读写 ------------------------------- */

pub fn load_subscriptions(paths: &AppPaths) -> Vec<Subscription> {
    match fs::read_to_string(&paths.subscriptions_file) {
        Ok(content) => serde_json::from_str::<Vec<Subscription>>(&content).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub fn save_subscriptions(paths: &AppPaths, subscriptions: &[Subscription]) -> AppResult<()> {
    let content = serde_json::to_string_pretty(subscriptions)?;
    write_atomic(&paths.subscriptions_file, &content)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 在系统临时目录下开一个唯一子目录，测试结束由调用方清理。
    fn scratch_dir(tag: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "clash-gui-test-{tag}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("应能创建测试目录");
        dir
    }

    fn paths_under(root: &Path) -> AppPaths {
        AppPaths {
            config_dir: root.to_path_buf(),
            data_dir: root.join("data"),
            profiles_dir: root.join("profiles"),
            config_file: root.join("config.yaml"),
            settings_file: root.join("settings.json"),
            subscriptions_file: root.join("subscriptions.json"),
        }
    }

    #[test]
    fn sanitize_keeps_safe_identifiers() {
        assert_eq!(sanitize_id("3f9a1c2e-4b5d"), "3f9a1c2e-4b5d");
        assert_eq!(sanitize_id("default_01"), "default_01");
    }

    #[test]
    fn sanitize_blocks_path_traversal() {
        for raw in ["../settings", "..\\..\\evil", "C:\\Windows\\x", "a/b", "a\\b"] {
            let safe = sanitize_id(raw);
            assert!(!safe.contains('/'), "仍含 /: {safe}");
            assert!(!safe.contains('\\'), "仍含 \\: {safe}");
            assert!(!safe.contains(':'), "仍含 : {safe}");
            assert!(!safe.contains(".."), "仍含 ..: {safe}");
        }
        assert_eq!(sanitize_id(".."), "unknown");
        assert_eq!(sanitize_id(""), "unknown");
        assert_eq!(sanitize_id("..."), "unknown");
    }

    #[test]
    fn sanitize_truncates_long_input() {
        let safe = sanitize_id(&"a".repeat(200));
        assert_eq!(safe.chars().count(), 64);
    }

    #[test]
    fn profile_file_stays_inside_profiles_dir() {
        let paths = paths_under(Path::new("C:\\cfg"));
        let file = paths.profile_file("../../../settings.json");
        assert_eq!(file.parent().unwrap(), paths.profiles_dir.as_path());
        assert!(file.file_name().unwrap().to_string_lossy().ends_with(".yaml"));
    }

    #[test]
    fn write_sensitive_creates_file_and_parents() {
        let dir = scratch_dir("write-new");
        let file = dir.join("nested").join("settings.json");
        write_sensitive(&file, "{\"secret\":\"abc\"}").expect("写入应成功");
        assert_eq!(
            fs::read_to_string(&file).expect("应能读回"),
            "{\"secret\":\"abc\"}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_sensitive_overwrites_and_keeps_backup() {
        let dir = scratch_dir("write-over");
        let file = dir.join("settings.json");
        write_sensitive(&file, "v1").expect("首次写入应成功");
        write_sensitive(&file, "v2").expect("覆盖写入应成功");
        assert_eq!(fs::read_to_string(&file).unwrap(), "v2");
        let backup = dir.join("settings.json.bak");
        assert_eq!(fs::read_to_string(&backup).unwrap(), "v1", "应保留上一版备份");

        // 不应残留任何临时文件
        let leftovers: Vec<String> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "存在残留临时文件: {leftovers:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn write_sensitive_restricts_permissions_on_unix() {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch_dir("write-perm");
        let file = dir.join("config.yaml");
        write_sensitive(&file, "secret: 1").expect("写入应成功");
        let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "敏感文件权限应为 0600，实际 {mode:o}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_atomic_alias_matches_write_sensitive() {
        let dir = scratch_dir("alias");
        let file = dir.join("a.txt");
        write_atomic(&file, "x").expect("别名写入应成功");
        assert_eq!(fs::read_to_string(&file).unwrap(), "x");
        let _ = fs::remove_dir_all(&dir);
    }
}
