//! 凭据脱敏。
//!
//! 背景：本程序会把订阅地址、订阅里的 token、内核 API 的 `secret`
//! 等信息写进日志面板、错误提示与诊断文本。用户复制日志贴到工单、
//! 截图发群时，等于把机场账号和本地控制端口一起公开。
//!
//! 设计约束：
//! * 纯 std、零第三方依赖：脱敏是安全底线，不能因为某个 regex crate
//!   版本问题在 CI 或个别平台上失效；
//! * 唯一入口 [`redact_text`]，由 `state::push_log` 在日志落地前统一调用，
//!   这样"任何来源"（内核 stderr、clash_api、订阅服务、系统代理）
//!   的日志都被覆盖，不需要在每个调用点逐个打补丁 —— 逐个打补丁的做法
//!   一旦漏掉一处就等于没做；
//! * 只替换"值"，保留键名和整体结构：排障时仍能看出"是哪个参数被脱敏"，
//!   例如 `https://api.example.com/sub?token=***&flag=clash`。
//!
//! 已知边界（如实说明，不假装完备）：
//! * 不解析 JSON/YAML 结构，只做词法级替换；若某个上游把凭据放在
//!   无键名的裸串里（例如把 token 当作路径段 `https://host/abc123/sub`），
//!   本模块无法识别 —— 这类情况需要调用方把该字段标注为敏感后单独脱敏；
//! * 指纹/哈希类（如订阅返回的 `profile-web-page-url` 携带的 sid）
//!   只按常见键名匹配，未知键名不覆盖。

/// 脱敏占位符。
pub const MASK: &str = "***";

/// 敏感键名（小写后"包含"即判定为敏感，覆盖 token / access_token / sub_token 等变体）。
const SENSITIVE_KEY_PARTS: [&str; 12] = [
    "token", "secret", "password", "passwd", "pwd", "apikey", "api_key", "auth", "sign", "uuid",
    "sid", "sub_url",
];

/// 对一段自由文本做脱敏：先处理其中的 URL（含 userinfo / query），
/// 再处理形如 `key=value` 的键值对，最后处理 `Bearer xxx` 形式的凭据。
pub fn redact_text(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let with_urls = redact_urls(text);
    let with_pairs = redact_key_values(&with_urls);
    redact_bearer(&with_pairs)
}

/// 仅对单个 URL 脱敏（`user:pass@host` 的 userinfo 与查询串中的敏感参数）。
pub fn redact_url(url: &str) -> String {
    let mut out = String::with_capacity(url.len());

    // 1) scheme://
    let (scheme, rest) = match url.find("://") {
        Some(index) => (&url[..index + 3], &url[index + 3..]),
        None => ("", url),
    };
    out.push_str(scheme);

    // 2) authority（到第一个 '/' '?' '#' 为止）
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(authority_end);

    match authority.rsplit_once('@') {
        // 保留用户名便于排障（部分机场用用户名区分套餐），只抹掉口令。
        Some((userinfo, host)) => {
            let user = userinfo.split(':').next().unwrap_or("");
            if user.is_empty() {
                out.push_str(MASK);
            } else {
                out.push_str(user);
                out.push(':');
                out.push_str(MASK);
            }
            out.push('@');
            out.push_str(host);
        },
        None => out.push_str(authority),
    }

    // 3) path / query / fragment：只处理 query 中的敏感键
    match tail.find('?') {
        Some(index) => {
            out.push_str(&tail[..index]);
            out.push('?');
            let after_query = &tail[index + 1..];
            let (query, fragment) = match after_query.find('#') {
                Some(hash) => (&after_query[..hash], Some(&after_query[hash..])),
                None => (after_query, None),
            };
            out.push_str(&redact_query(query));
            if let Some(fragment) = fragment {
                out.push_str(fragment);
            }
        },
        None => out.push_str(tail),
    }

    out
}

/// 判断键名是否敏感。
fn is_sensitive_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    SENSITIVE_KEY_PARTS
        .iter()
        .any(|part| lowered.contains(part))
}

/// 处理 `a=1&token=xxx&b=2` 形式的查询串。
fn redact_query(query: &str) -> String {
    let mut out = String::with_capacity(query.len());
    for (index, pair) in query.split('&').enumerate() {
        if index > 0 {
            out.push('&');
        }
        match pair.split_once('=') {
            Some((key, value)) if !value.is_empty() && is_sensitive_key(key) => {
                out.push_str(key);
                out.push('=');
                out.push_str(MASK);
            },
            _ => out.push_str(pair),
        }
    }
    out
}

/// 把文本中的 URL 片段替换为脱敏版本，其余文本原样保留。
fn redact_urls(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;

    while cursor < text.len() {
        match find_url_start(&text[cursor..]) {
            Some(start) => {
                let absolute_start = cursor + start;
                out.push_str(&text[cursor..absolute_start]);
                let end = url_end(text, absolute_start);
                out.push_str(&redact_url(&text[absolute_start..end]));
                cursor = end;
            },
            None => {
                out.push_str(&text[cursor..]);
                break;
            },
        }
    }

    out
}

/// 返回 `http://` / `https://` 在切片中的最早起始位置。
fn find_url_start(text: &str) -> Option<usize> {
    let https = text.find("https://");
    let http = text.find("http://");
    match (https, http) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    }
}

/// URL 结束位置：遇到空白或常见分隔符即视为 URL 结束。
fn url_end(text: &str, start: usize) -> usize {
    const STOPPERS: [char; 10] = [' ', '\t', '\n', '\r', '"', '\'', ')', ']', '}', '<'];
    let slice = &text[start..];
    match slice.find(STOPPERS) {
        Some(index) => start + index,
        None => text.len(),
    }
}

/// 处理自由文本里的 `key=value`（键名敏感时把值替换为占位符）。
///
/// 只认"键名紧邻 `=`、且值非空"的形式，避免把 `a == b` 之类的代码片段误伤。
fn redact_key_values(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut index = 0usize;
    let mut last_copied = 0usize;

    while index < bytes.len() {
        if bytes[index] != b'=' {
            index += 1;
            continue;
        }

        // 键名：向左回溯 ASCII 标识符字符
        let mut key_start = index;
        while key_start > 0 && is_key_byte(bytes[key_start - 1]) {
            key_start -= 1;
        }
        // 键名前一个字符必须是分隔符，避免把 `foo.bar=baz` 里的长串当成键名之外的误判；
        // 这里允许 '.' '-' '_' 出现在键名内部，故仅在 key_start == index 时跳过。
        if key_start == index {
            index += 1;
            continue;
        }

        let key = &text[key_start..index];
        // 值：向右到分隔符为止。这里按字符（而非按字节）推进：
        // `password=密码` 这类值仍需脱敏，但 `token=ab，请检查` 中的中文标点
        // 必须终止值边界，否则后面的正常文本会被整段吞进占位符。
        let mut value_end = index + 1;
        while value_end < bytes.len() && !is_value_stop_byte(bytes[value_end]) {
            match text[value_end..].chars().next() {
                Some(ch) if !ch.is_ascii() && !ch.is_alphanumeric() => break,
                Some(ch) => value_end += ch.len_utf8(),
                None => break,
            }
        }

        if value_end > index + 1 && is_sensitive_key(key) {
            out.push_str(&text[last_copied..index + 1]);
            out.push_str(MASK);
            last_copied = value_end;
        }

        index = value_end.max(index + 1);
    }

    out.push_str(&text[last_copied..]);
    out
}

/// 把 `Bearer <token>` 形式里的 token 抹掉。
fn redact_bearer(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(index) = find_bearer(rest) {
        let (before, after) = rest.split_at(index);
        out.push_str(before);
        out.push_str("Bearer ");
        out.push_str(MASK);

        let token_start = "Bearer ".len();
        let mut end = token_start;
        let bytes = after.as_bytes();
        while end < bytes.len() && !is_value_stop_byte(bytes[end]) {
            end += 1;
        }
        rest = &after[end..];
    }

    out.push_str(rest);
    out
}

fn find_bearer(text: &str) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    lower.find("bearer ")
}

fn is_key_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')
}

fn is_value_stop_byte(byte: u8) -> bool {
    matches!(
        byte,
        b' ' | b'\t'
            | b'\n'
            | b'\r'
            | b'&'
            | b','
            | b';'
            | b'"'
            | b'\''
            | b')'
            | b']'
            | b'}'
            | b'<'
            | b'?'
            | b'#'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_subscription_query_token() {
        let raw =
            "拉取订阅：https://sub.example.com/api/v1/client/subscribe?token=abcd1234&flag=clash";
        let masked = redact_text(raw);
        assert!(!masked.contains("abcd1234"), "token 未被脱敏：{masked}");
        assert!(masked.contains("token=***"), "键名应保留：{masked}");
        assert!(masked.contains("flag=clash"), "非敏感参数应保留：{masked}");
        assert!(masked.contains("拉取订阅"), "普通文本应保留：{masked}");
    }

    #[test]
    fn redacts_url_userinfo_password_only() {
        let masked = redact_url("https://alice:s3cr3t@proxy.example.com:8443/path?secret=x");
        assert!(
            masked.starts_with("https://alice:***@proxy.example.com:8443/path"),
            "{masked}"
        );
        assert!(!masked.contains("s3cr3t"), "{masked}");
        assert!(masked.contains("secret=***"), "{masked}");
    }

    #[test]
    fn redacts_bare_key_value_without_url() {
        let masked = redact_text("controller secret=my-secret-value port=9090");
        assert!(!masked.contains("my-secret-value"), "{masked}");
        assert!(masked.contains("secret=***"), "{masked}");
        assert!(masked.contains("port=9090"), "{masked}");
    }

    #[test]
    fn redacts_bearer_token() {
        let masked = redact_text("Authorization: Bearer abc.def.ghi 请求失败");
        assert!(!masked.contains("abc.def.ghi"), "{masked}");
        assert!(masked.contains("Bearer ***"), "{masked}");
    }

    #[test]
    fn keeps_plain_text_intact() {
        let text = "内核已启动，PID=1234，端口 7890";
        assert_eq!(redact_text(text), text);
    }

    #[test]
    fn handles_multiple_urls_and_empty_input() {
        let masked = redact_text("a=https://x/sub?token=1 b=https://y/sub?token=2 结束");
        assert_eq!(masked.matches("token=***").count(), 2, "{masked}");
        assert!(!masked.contains("token=1"), "{masked}");
        assert!(!masked.contains("token=2"), "{masked}");
        assert_eq!(redact_text(""), "");
    }

    #[test]
    fn does_not_touch_multibyte_text() {
        // 中文紧邻敏感键名时不能切坏 UTF-8 边界
        let masked = redact_text("订阅失败：token=泄漏值，请检查");
        assert!(masked.contains("token=***"), "{masked}");
        assert!(masked.contains("请检查"), "{masked}");
    }
}
