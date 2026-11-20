//! 订阅解析与 mihomo 配置生成。
//!
//! 设计原则：
//! * 只解析 Clash/mihomo 标准 YAML 订阅（`proxies` / `proxy-groups` / `rules`）；
//!   若订阅内容是 base64 包裹的 YAML（部分机场的行为），会自动解码后再解析。
//! * 代理入口固定保留 mixed-port；TUN 段仅在用户于设置页开启 TUN 时按需生成，
//!   关闭时不出现任何 `tun` 字段，保持与首版一致的"纯系统代理"行为。
//! * 无可用节点时生成最小可用配置，保证内核仍然能够启动，便于用户先进入界面。

use base64::Engine;

use serde_yaml::{Mapping, Value};

use crate::error::{AppError, AppResult};
use crate::models::AppSettings;

/// 解析后的订阅内容（保持 YAML 原始结构，避免丢失厂商自定义字段）。
#[derive(Debug, Clone, Default)]
pub struct ProfileContent {
    pub proxies: Vec<Value>,
    pub proxy_groups: Vec<Value>,
    pub rules: Vec<Value>,
    pub dns: Option<Value>,
    /// 订阅服务端返回的流量信息头
    pub user_info: Option<String>,
}

impl ProfileContent {
    pub fn proxy_names(&self) -> Vec<String> {
        self.proxies
            .iter()
            .filter_map(|proxy| proxy.get("name").and_then(|name| name.as_str()))
            .map(|name| name.to_string())
            .collect()
    }

    pub fn node_count(&self) -> u32 {
        self.proxies.len() as u32
    }
}

fn key(name: &str) -> Value {
    Value::String(name.to_string())
}

fn as_sequence(value: Option<&Value>) -> Vec<Value> {
    match value {
        Some(Value::Sequence(items)) => items.clone(),
        _ => Vec::new(),
    }
}

/// 解析订阅文本。
pub fn parse_profile(content: &str, user_info: Option<String>) -> AppResult<ProfileContent> {
    match parse_profile_raw(content, user_info.clone()) {
        Ok(profile) => Ok(profile),
        Err(first_error) => {
            // 兜底：部分订阅会把 YAML 再套一层 base64。
            let trimmed: String = content.split_whitespace().collect();
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(trimmed.as_bytes())
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok());
            match decoded {
                Some(text) => parse_profile_raw(&text, user_info),
                None => Err(first_error),
            }
        }
    }
}

fn parse_profile_raw(content: &str, user_info: Option<String>) -> AppResult<ProfileContent> {
    let value: Value = serde_yaml::from_str(content)
        .map_err(|error| AppError::Subscription(format!("订阅内容不是合法的 Clash YAML：{error}")))?;

    let mapping = value
        .as_mapping()
        .ok_or_else(|| AppError::Subscription("订阅内容不是 YAML 映射结构".to_string()))?;

    let proxies: Vec<Value> = as_sequence(mapping.get(&key("proxies")))
        .into_iter()
        .filter(|proxy| {
            proxy
                .get("name")
                .and_then(|name| name.as_str())
                .map(|name| !name.trim().is_empty())
                .unwrap_or(false)
        })
        .collect();

    if proxies.is_empty() {
        return Err(AppError::Subscription(
            "订阅中没有解析到任何节点（proxies 为空）".to_string(),
        ));
    }

    let proxy_groups = as_sequence(mapping.get(&key("proxy-groups")));
    let rules = as_sequence(mapping.get(&key("rules")));
    let dns = mapping.get(&key("dns")).cloned();

    Ok(ProfileContent {
        proxies,
        proxy_groups,
        rules,
        dns,
        user_info,
    })
}

/// 仅统计节点数量，失败时返回 0。
pub fn count_nodes(content: &str) -> u32 {
    parse_profile(content, None)
        .map(|profile| profile.node_count())
        .unwrap_or(0)
}

/* ------------------------------ 配置生成 ------------------------------ */

fn default_dns() -> Value {
    let mut dns = Mapping::new();
    dns.insert(key("enable"), Value::from(true));
    dns.insert(key("ipv6"), Value::from(false));
    dns.insert(key("enhanced-mode"), Value::from("fake-ip"));
    dns.insert(key("fake-ip-range"), Value::from("198.18.0.1/16"));
    dns.insert(
        key("nameserver"),
        Value::Sequence(vec![
            Value::from("223.5.5.5"),
            Value::from("119.29.29.29"),
            Value::from("8.8.8.8"),
        ]),
    );
    dns.insert(
        key("fallback"),
        Value::Sequence(vec![Value::from("https://1.1.1.1/dns-query")]),
    );
    Value::Mapping(dns)
}

/// 生成 mihomo 的 `tun` 配置段。
///
/// 字段对照（mihomo 文档）：
/// * `enable` TUN 总开关；
/// * `stack` 协议栈实现（gvisor / system / mixed）；
/// * `device` 虚拟网卡名（早期文档写作 `device-name`，mihomo 现用 `device`，留空回落平台默认值）；
/// * `auto-route` 自动接管系统路由表；
/// * `auto-detect-interface` 自动探测出口网卡；
/// * `strict-route` 严格路由，阻止流量绕开 TUN 泄漏；
/// * `dns-hijack` 交给内置 DNS 处理的劫持目标。
fn build_tun(settings: &AppSettings) -> Value {
    let mut tun = Mapping::new();
    tun.insert(key("enable"), Value::from(true));
    tun.insert(key("stack"), Value::from(settings.tun_stack.as_str()));
    tun.insert(
        key("device"),
        Value::from(crate::tun::effective_device(settings)),
    );
    tun.insert(key("auto-route"), Value::from(settings.tun_auto_route));
    tun.insert(
        key("auto-detect-interface"),
        Value::from(settings.tun_auto_detect_interface),
    );
    tun.insert(key("strict-route"), Value::from(settings.tun_strict_route));

    let hijack: Vec<Value> = settings
        .tun_dns_hijack
        .iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .map(Value::from)
        .collect();
    let hijack = if hijack.is_empty() {
        vec![Value::from("any:53")]
    } else {
        hijack
    };
    tun.insert(key("dns-hijack"), Value::Sequence(hijack));

    Value::Mapping(tun)
}

/// TUN 模式下补齐 DNS 必需项：`enable` 必须为 true，缺失 `enhanced-mode` 时补 fake-ip。
fn ensure_dns_for_tun(dns: &mut Value) {
    if let Value::Mapping(map) = dns {
        map.insert(key("enable"), Value::from(true));
        if !map.contains_key(&key("enhanced-mode")) {
            map.insert(key("enhanced-mode"), Value::from("fake-ip"));
        }
        if !map.contains_key(&key("fake-ip-range")) {
            map.insert(key("fake-ip-range"), Value::from("198.18.0.1/16"));
        }
    } else {
        // 订阅里 dns 是非法结构（非映射），直接换成内置默认配置
        *dns = default_dns();
    }
}

fn default_rules(has_proxies: bool) -> Value {
    let mut rules: Vec<Value> = Vec::new();
    let direct_rules = [
        "DOMAIN-SUFFIX,local,DIRECT",
        "IP-CIDR,127.0.0.0/8,DIRECT,no-resolve",
        "IP-CIDR,10.0.0.0/8,DIRECT,no-resolve",
        "IP-CIDR,172.16.0.0/12,DIRECT,no-resolve",
        "IP-CIDR,192.168.0.0/16,DIRECT,no-resolve",
        "IP-CIDR,100.64.0.0/10,DIRECT,no-resolve",
        "GEOIP,LAN,DIRECT,no-resolve",
        "GEOIP,CN,DIRECT",
    ];
    for rule in direct_rules {
        rules.push(Value::from(rule));
    }
    rules.push(Value::from(if has_proxies {
        "MATCH,PROXY"
    } else {
        "MATCH,DIRECT"
    }));
    Value::Sequence(rules)
}

fn default_groups(names: &[String], test_url: &str) -> Value {
    let mut select_nodes: Vec<Value> = vec![Value::from("AUTO")];
    for name in names {
        select_nodes.push(Value::from(name.clone()));
    }

    let mut proxy_group = Mapping::new();
    proxy_group.insert(key("name"), Value::from("PROXY"));
    proxy_group.insert(key("type"), Value::from("select"));
    proxy_group.insert(key("proxies"), Value::Sequence(select_nodes));

    let auto_nodes: Vec<Value> = names.iter().map(|name| Value::from(name.clone())).collect();
    let mut auto_group = Mapping::new();
    auto_group.insert(key("name"), Value::from("AUTO"));
    auto_group.insert(key("type"), Value::from("url-test"));
    auto_group.insert(key("url"), Value::from(test_url));
    auto_group.insert(key("interval"), Value::from(300u64));
    auto_group.insert(key("tolerance"), Value::from(50u64));
    auto_group.insert(key("proxies"), Value::Sequence(auto_nodes));

    Value::Sequence(vec![
        Value::Mapping(proxy_group),
        Value::Mapping(auto_group),
    ])
}

/// 生成 mihomo 运行配置文本。
pub fn build_config(settings: &AppSettings, profile: Option<&ProfileContent>) -> AppResult<String> {
    let names = profile.map(|item| item.proxy_names()).unwrap_or_default();
    let has_proxies = !names.is_empty();

    let mut root = Mapping::new();

    // 基础运行参数
    root.insert(key("mixed-port"), Value::from(settings.mixed_port as u64));
    root.insert(key("allow-lan"), Value::from(settings.allow_lan));
    root.insert(key("bind-address"), Value::from("*"));
    root.insert(key("mode"), Value::from(settings.mode.as_str()));
    root.insert(key("log-level"), Value::from(settings.log_level.as_str()));
    root.insert(key("ipv6"), Value::from(false));
    root.insert(key("unified-delay"), Value::from(true));
    root.insert(key("tcp-concurrent"), Value::from(true));
    root.insert(key("find-process-mode"), Value::from("off"));
    root.insert(
        key("external-controller"),
        Value::from(format!("127.0.0.1:{}", settings.controller_port)),
    );
    root.insert(
        key("secret"),
        Value::from(settings.controller_secret.clone()),
    );
    root.insert(
        key("profile"),
        Value::Mapping({
            let mut profile_map = Mapping::new();
            profile_map.insert(key("store-selected"), Value::from(true));
            profile_map.insert(key("store-fake-ip"), Value::from(false));
            profile_map
        }),
    );

    // TUN：仅在启用时生成 tun 段，关闭时配置中完全不出现 tun 字段
    if settings.tun_enabled {
        root.insert(key("tun"), build_tun(settings));
    }

    // DNS：优先沿用订阅中的配置
    let mut dns = profile
        .and_then(|item| item.dns.clone())
        .unwrap_or_else(default_dns);
    if settings.tun_enabled {
        // TUN 会把 DNS 查询劫持到内核，DNS 未启用时会直接解析失败，这里强制补齐
        ensure_dns_for_tun(&mut dns);
    }
    root.insert(key("dns"), dns);

    // 节点与策略组
    let proxies = profile
        .map(|item| item.proxies.clone())
        .unwrap_or_default();
    root.insert(key("proxies"), Value::Sequence(proxies));

    let groups = match profile {
        Some(item) if !item.proxy_groups.is_empty() => Value::Sequence(item.proxy_groups.clone()),
        _ if has_proxies => default_groups(&names, &settings.latency_test_url),
        _ => Value::Sequence(Vec::new()),
    };
    root.insert(key("proxy-groups"), groups);

    let rules = match profile {
        Some(item) if !item.rules.is_empty() => Value::Sequence(item.rules.clone()),
        _ => default_rules(has_proxies),
    };
    root.insert(key("rules"), rules);

    let text = serde_yaml::to_string(&Value::Mapping(root))?;
    Ok(text)
}

/* ================================ 测试 ================================ */

/// `build_config` 的单元测试。
///
/// 全部基于纯函数 + 固定输入：不读写磁盘、不访问网络，可在任意平台与 CI 上稳定复现。
#[cfg(test)]
mod tests {
    use super::*;

    use crate::models::TunStack;

    const MINIMAL_PROFILE: &str = r#"
proxies:
  - name: "节点A"
    type: ss
    server: 192.0.2.10
    port: 8388
    cipher: aes-128-gcm
    password: secret
"#;

    fn config_of(settings: &AppSettings, profile: Option<&ProfileContent>) -> serde_yaml::Value {
        let text = build_config(settings, profile).expect("配置生成应成功");
        serde_yaml::from_str::<serde_yaml::Value>(&text).expect("生成的配置应为合法 YAML")
    }

    fn tun_of(config: &serde_yaml::Value) -> Option<&serde_yaml::Value> {
        config.get("tun")
    }

    fn hijack_list(tun: &serde_yaml::Value) -> Vec<String> {
        tun.get("dns-hijack")
            .and_then(|value| value.as_sequence())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(|text| text.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn tun_disabled_omits_tun_section() {
        let config = config_of(&AppSettings::default(), None);
        assert!(tun_of(&config).is_none(), "TUN 关闭时不应输出 tun 段");
        assert!(config.get("dns").is_some(), "dns 段应始终存在");
    }

    #[test]
    fn tun_enabled_emits_enable_true_and_default_stack() {
        let settings = AppSettings {
            tun_enabled: true,
            ..AppSettings::default()
        };
        let config = config_of(&settings, None);
        let tun = tun_of(&config).expect("TUN 开启时应输出 tun 段");

        assert_eq!(tun.get("enable").and_then(|value| value.as_bool()), Some(true));
        assert_eq!(
            tun.get("stack").and_then(|value| value.as_str()),
            Some("gvisor"),
            "默认协议栈应为 gvisor"
        );
        assert_eq!(
            tun.get("device").and_then(|value| value.as_str()),
            Some(crate::tun::effective_device(&settings).as_str()),
            "设备名应与 tun::effective_device 一致"
        );
        assert_eq!(
            tun.get("auto-route").and_then(|value| value.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn tun_stack_serializes_each_variant() {
        for (stack, expected) in [
            (TunStack::Gvisor, "gvisor"),
            (TunStack::System, "system"),
            (TunStack::Mixed, "mixed"),
        ] {
            let settings = AppSettings {
                tun_enabled: true,
                tun_stack: stack,
                ..AppSettings::default()
            };
            let config = config_of(&settings, None);
            let tun = tun_of(&config).expect("应输出 tun 段");
            assert_eq!(
                tun.get("stack").and_then(|value| value.as_str()),
                Some(expected),
                "枚举 {stack:?} 应渲染为 {expected}"
            );
        }
    }

    #[test]
    fn tun_dns_hijack_keeps_configured_values() {
        let settings = AppSettings {
            tun_enabled: true,
            tun_dns_hijack: vec!["any:53".to_string(), "10.0.0.1:53".to_string()],
            ..AppSettings::default()
        };
        let config = config_of(&settings, None);
        let hijack = hijack_list(tun_of(&config).expect("应输出 tun 段"));
        assert_eq!(
            hijack,
            vec!["any:53".to_string(), "10.0.0.1:53".to_string()]
        );
    }

    #[test]
    fn tun_dns_hijack_drops_blank_entries_and_falls_back() {
        let only_blank = AppSettings {
            tun_enabled: true,
            tun_dns_hijack: vec!["   ".to_string(), String::new()],
            ..AppSettings::default()
        };
        let config = config_of(&only_blank, None);
        let hijack = hijack_list(tun_of(&config).expect("应输出 tun 段"));
        assert_eq!(
            hijack,
            vec!["any:53".to_string()],
            "空白配置应回落到 any:53"
        );

        let padded = AppSettings {
            tun_enabled: true,
            tun_dns_hijack: vec!["  any:53  ".to_string(), " ".to_string()],
            ..AppSettings::default()
        };
        let config = config_of(&padded, None);
        let hijack = hijack_list(tun_of(&config).expect("应输出 tun 段"));
        assert_eq!(hijack, vec!["any:53".to_string()], "两端空白应被裁掉");
    }

    #[test]
    fn tun_enabled_fills_dns_defaults() {
        let settings = AppSettings {
            tun_enabled: true,
            ..AppSettings::default()
        };
        let config = config_of(&settings, None);
        let dns = config.get("dns").expect("dns 段应存在");
        assert_eq!(
            dns.get("enable").and_then(|value| value.as_bool()),
            Some(true)
        );
        assert_eq!(
            dns.get("enhanced-mode").and_then(|value| value.as_str()),
            Some("fake-ip"),
            "TUN 需要 fake-ip 才能劫持 DNS"
        );
        assert!(
            dns.get("fake-ip-range").is_some(),
            "应补齐 fake-ip-range"
        );
    }

    #[test]
    fn profile_dns_is_preserved_and_completed() {
        let profile_yaml = r#"
proxies:
  - name: "节点A"
    type: ss
    server: 192.0.2.10
    port: 8388
    cipher: aes-128-gcm
    password: secret
dns:
  nameserver:
    - 223.5.5.5
"#;
        let profile = parse_profile(profile_yaml, None).expect("订阅解析应成功");
        let settings = AppSettings {
            tun_enabled: true,
            ..AppSettings::default()
        };
        let config = config_of(&settings, Some(&profile));
        let dns = config.get("dns").expect("dns 段应存在");
        assert_eq!(
            dns.get("enhanced-mode").and_then(|value| value.as_str()),
            Some("fake-ip"),
            "订阅自带 dns 时也应补齐 enhanced-mode"
        );
        let nameservers = dns
            .get("nameserver")
            .and_then(|value| value.as_sequence())
            .expect("应保留订阅的 nameserver");
        assert_eq!(
            nameservers.first().and_then(|value| value.as_str()),
            Some("223.5.5.5")
        );
    }

    #[test]
    fn controller_ports_and_secret_are_rendered() {
        let settings = AppSettings {
            mixed_port: 7897,
            controller_port: 9097,
            controller_secret: "s3cr3t-value".to_string(),
            ..AppSettings::default()
        };
        let config = config_of(&settings, None);
        assert_eq!(
            config.get("mixed-port").and_then(|value| value.as_u64()),
            Some(7897)
        );
        assert_eq!(
            config
                .get("external-controller")
                .and_then(|value| value.as_str()),
            Some("127.0.0.1:9097")
        );
        assert_eq!(
            config.get("secret").and_then(|value| value.as_str()),
            Some("s3cr3t-value")
        );
    }

    #[test]
    fn profile_rules_and_proxies_are_kept() {
        let profile_yaml = r#"
proxies:
  - name: "节点A"
    type: ss
    server: 192.0.2.10
    port: 8388
    cipher: aes-128-gcm
    password: secret
rules:
  - DOMAIN-SUFFIX,example.com,PROXY
  - MATCH,DIRECT
"#;
        let profile = parse_profile(profile_yaml, None).expect("订阅解析应成功");
        let config = config_of(&AppSettings::default(), Some(&profile));

        let rules = config
            .get("rules")
            .and_then(|value| value.as_sequence())
            .expect("应输出 rules");
        let texts: Vec<&str> = rules.iter().filter_map(|value| value.as_str()).collect();
        assert!(texts.contains(&"DOMAIN-SUFFIX,example.com,PROXY"));
        assert!(texts.contains(&"MATCH,DIRECT"));

        let proxies = config
            .get("proxies")
            .and_then(|value| value.as_sequence())
            .expect("应输出 proxies");
        assert_eq!(proxies.len(), 1);
        assert_eq!(
            proxies[0].get("name").and_then(|value| value.as_str()),
            Some("节点A")
        );
    }

    #[test]
    fn empty_profile_gets_default_rules_with_direct_fallback() {
        let config = config_of(&AppSettings::default(), None);
        let rules = config
            .get("rules")
            .and_then(|value| value.as_sequence())
            .expect("应输出默认 rules");
        let texts: Vec<&str> = rules.iter().filter_map(|value| value.as_str()).collect();
        assert!(
            texts.iter().any(|rule| *rule == "MATCH,DIRECT"),
            "无订阅时应直连兜底: {texts:?}"
        );
    }

    #[test]
    fn minimal_profile_parses_without_panicking() {
        let profile = parse_profile(MINIMAL_PROFILE, None).expect("订阅解析应成功");
        assert_eq!(profile.proxies.len(), 1);
        assert!(profile.rules.is_empty(), "订阅未提供 rules 时应为空");
        assert!(profile.proxy_groups.is_empty(), "订阅未提供 proxy-groups 时应为空");
    }
}



