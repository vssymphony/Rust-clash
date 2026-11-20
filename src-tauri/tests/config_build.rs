//! 配置拼装全链路集成测试。
//!
//! 这些用例只经过「纯逻辑」路径：`AppSettings` + 订阅文本 → `build_config` → YAML。
//! 不启动内核、不访问网络、不读写用户目录，因此在 CI 的任意 runner 上都能跑通。
//!
//! 断言方式统一为「解析回 YAML 再做结构断言」，避免直接比对字符串而受键序、
//! 引号风格、缩进变化影响。

use clash_gui_lib::models::{AppSettings, LogLevel, ProxyMode, TunStack};
use clash_gui_lib::subscription::{build_config, parse_profile};

const FULL_PROFILE: &str = r#"
port: 7890
proxies:
  - name: "香港 01"
    type: ss
    server: 198.51.100.10
    port: 8388
    cipher: aes-128-gcm
    password: pwd-a
  - name: "日本 01"
    type: vmess
    server: 198.51.100.11
    port: 443
    uuid: 11111111-2222-3333-4444-555555555555
    alterId: 0
    cipher: auto
proxy-groups:
  - name: "PROXY"
    type: select
    proxies:
      - "香港 01"
      - "日本 01"
rules:
  - DOMAIN-SUFFIX,example.com,PROXY
  - GEOIP,CN,DIRECT
  - MATCH,PROXY
dns:
  enable: true
  nameserver:
    - 223.5.5.5
    - 119.29.29.29
"#;

fn build(settings: &AppSettings, profile_yaml: Option<&str>) -> serde_yaml::Value {
    let profile = profile_yaml.map(|text| parse_profile(text, None).expect("订阅解析应成功"));
    let text = build_config(settings, profile.as_ref()).expect("配置生成应成功");
    serde_yaml::from_str::<serde_yaml::Value>(&text).expect("生成结果应为合法 YAML")
}

fn rules_of(config: &serde_yaml::Value) -> Vec<String> {
    config
        .get("rules")
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
fn full_profile_with_tun_produces_expected_structure() {
    let settings = AppSettings {
        mixed_port: 7897,
        controller_port: 9097,
        controller_secret: "ctl-secret".to_string(),
        mode: ProxyMode::Global,
        tun_enabled: true,
        tun_stack: TunStack::System,
        tun_device_name: Some("CustomTun".to_string()),
        tun_dns_hijack: vec!["any:53".to_string(), "10.0.0.1:53".to_string()],
        ..AppSettings::default()
    };

    let config = build(&settings, Some(FULL_PROFILE));

    // 入口与控制器
    assert_eq!(
        config.get("mixed-port").and_then(|value| value.as_u64()),
        Some(7897)
    );
    assert_eq!(
        config
            .get("external-controller")
            .and_then(|value| value.as_str()),
        Some("127.0.0.1:9097"),
    );
    assert_eq!(
        config.get("secret").and_then(|value| value.as_str()),
        Some("ctl-secret")
    );
    assert_eq!(
        config.get("mode").and_then(|value| value.as_str()),
        Some("global")
    );

    // 订阅内容原样保留
    let proxies = config
        .get("proxies")
        .and_then(|value| value.as_sequence())
        .expect("应输出 proxies");
    assert_eq!(proxies.len(), 2);
    let groups = config
        .get("proxy-groups")
        .and_then(|value| value.as_sequence())
        .expect("应保留订阅 proxy-groups");
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].get("name").and_then(|value| value.as_str()),
        Some("PROXY")
    );

    // TUN 段
    let tun = config.get("tun").expect("TUN 开启时应输出 tun 段");
    assert_eq!(
        tun.get("enable").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert_eq!(
        tun.get("stack").and_then(|value| value.as_str()),
        Some("system")
    );
    assert_eq!(
        tun.get("device").and_then(|value| value.as_str()),
        Some("CustomTun"),
        "自定义设备名应被采用",
    );
    let hijack: Vec<&str> = tun
        .get("dns-hijack")
        .and_then(|value| value.as_sequence())
        .map(|items| items.iter().filter_map(|item| item.as_str()).collect())
        .unwrap_or_default();
    assert_eq!(hijack, vec!["any:53", "10.0.0.1:53"]);

    // DNS 在 TUN 下必须补成 fake-ip，否则流量无法劫持
    let dns = config.get("dns").expect("dns 段应存在");
    assert_eq!(
        dns.get("enhanced-mode").and_then(|value| value.as_str()),
        Some("fake-ip")
    );
    assert_eq!(
        dns.get("enable").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert!(dns.get("fake-ip-range").is_some());
}

#[test]
fn tun_disabled_keeps_system_proxy_only_shape() {
    let settings = AppSettings {
        tun_enabled: false,
        ..AppSettings::default()
    };
    let config = build(&settings, Some(FULL_PROFILE));

    assert!(config.get("tun").is_none(), "TUN 关闭时不得出现 tun 段");
    let dns = config.get("dns").expect("dns 段应存在");
    assert_eq!(
        dns.get("nameserver")
            .and_then(|value| value.as_sequence())
            .map(|items| items.len()),
        Some(2),
        "订阅的 dns.nameserver 应原样保留",
    );
}

#[test]
fn subscription_without_dns_gets_default_dns() {
    let settings = AppSettings {
        tun_enabled: true,
        ..AppSettings::default()
    };
    let profile_without_dns = r#"
proxies:
  - name: "节点A"
    type: ss
    server: 198.51.100.20
    port: 8388
    cipher: aes-128-gcm
    password: pwd
"#;
    let config = build(&settings, Some(profile_without_dns));

    let dns = config.get("dns").expect("应生成默认 dns 段");
    assert!(dns.get("nameserver").is_some(), "TUN 模式必须给出可用 DNS");
    assert_eq!(
        dns.get("enhanced-mode").and_then(|value| value.as_str()),
        Some("fake-ip")
    );
}

#[test]
fn without_subscription_falls_back_to_direct_rules() {
    let config = build(&AppSettings::default(), None);
    let rules = rules_of(&config);

    assert!(
        rules.iter().any(|rule| rule == "MATCH,DIRECT"),
        "无订阅时应直连兜底: {rules:?}"
    );
    assert!(
        rules.iter().any(|rule| rule.starts_with("GEOIP,LAN")),
        "本地流量应直连: {rules:?}"
    );
    assert!(
        config
            .get("proxies")
            .and_then(|value| value.as_sequence())
            .map(|items| items.is_empty())
            .unwrap_or(true),
        "无订阅时不应伪造节点"
    );
}

#[test]
fn generated_yaml_is_reparsable_and_stable() {
    let settings = AppSettings {
        tun_enabled: true,
        tun_stack: TunStack::Mixed,
        ..AppSettings::default()
    };
    let profile = parse_profile(FULL_PROFILE, None).expect("订阅解析应成功");
    let first = build_config(&settings, Some(&profile)).expect("首次生成应成功");
    let second = build_config(&settings, Some(&profile)).expect("二次生成应成功");
    assert_eq!(first, second, "同一输入应生成完全一致的配置");

    let value: serde_yaml::Value = serde_yaml::from_str(&first).expect("应为合法 YAML");
    assert_eq!(
        value
            .get("tun")
            .and_then(|tun| tun.get("stack"))
            .and_then(|stack| stack.as_str()),
        Some("mixed"),
    );
    assert!(
        !first.contains("!!"),
        "不应出现 YAML 标签，mihomo 解析会失败"
    );
}

/* -------------------- 追加：端口/日志/栈枚举/兜底组/DNS 合并 -------------------- */

/// 基础运行参数（含日志级别）必须逐项跟随设置，不能被硬编码覆盖。
#[test]
fn baseline_ports_mode_and_log_level_follow_settings() {
    let settings = AppSettings {
        mixed_port: 7899,
        controller_port: 9099,
        controller_secret: String::new(),
        allow_lan: true,
        mode: ProxyMode::Direct,
        log_level: LogLevel::Debug,
        ..AppSettings::default()
    };
    let config = build(&settings, None);

    assert_eq!(
        config.get("mixed-port").and_then(|value| value.as_u64()),
        Some(7899)
    );
    assert_eq!(
        config
            .get("external-controller")
            .and_then(|value| value.as_str()),
        Some("127.0.0.1:9099"),
    );
    assert_eq!(
        config.get("mode").and_then(|value| value.as_str()),
        Some("direct")
    );
    assert_eq!(
        config.get("log-level").and_then(|value| value.as_str()),
        Some("debug"),
        "日志级别应来自 AppSettings，且与 LogLevel::as_str 一致",
    );
    assert_eq!(
        config.get("allow-lan").and_then(|value| value.as_bool()),
        Some(true)
    );
    assert!(config.get("tun").is_none(), "未开启 TUN 时不得输出 tun 段");
}

/// 三种协议栈都要能正确落盘（Gvisor / System / Mixed 全覆盖）。
#[test]
fn every_tun_stack_variant_serializes_to_its_own_name() {
    let cases = [
        (TunStack::Gvisor, "gvisor"),
        (TunStack::System, "system"),
        (TunStack::Mixed, "mixed"),
    ];

    for (stack, expected) in cases {
        let settings = AppSettings {
            tun_enabled: true,
            tun_stack: stack,
            ..AppSettings::default()
        };
        let config = build(&settings, None);
        assert_eq!(
            config
                .get("tun")
                .and_then(|tun| tun.get("stack"))
                .and_then(|value| value.as_str()),
            Some(expected),
            "协议栈 {expected} 应原样写入配置",
        );
    }
}

/// dns-hijack 的自定义值保持顺序，空白项被剔除；全空白时回退到 any:53。
#[test]
fn custom_dns_hijack_drops_blank_entries_and_falls_back() {
    let settings = AppSettings {
        tun_enabled: true,
        tun_dns_hijack: vec![
            "any:53".to_string(),
            "   ".to_string(),
            "tcp://any:53".to_string(),
        ],
        ..AppSettings::default()
    };
    let tun = build(&settings, None);
    let hijack: Vec<&str> = tun
        .get("tun")
        .and_then(|value| value.get("dns-hijack"))
        .and_then(|value| value.as_sequence())
        .map(|items| items.iter().filter_map(|item| item.as_str()).collect())
        .unwrap_or_default();
    assert_eq!(hijack, vec!["any:53", "tcp://any:53"]);

    let blank = AppSettings {
        tun_enabled: true,
        tun_dns_hijack: vec!["   ".to_string(), String::new()],
        ..AppSettings::default()
    };
    let config = build(&blank, None);
    let hijack: Vec<&str> = config
        .get("tun")
        .and_then(|value| value.get("dns-hijack"))
        .and_then(|value| value.as_sequence())
        .map(|items| items.iter().filter_map(|item| item.as_str()).collect())
        .unwrap_or_default();
    assert_eq!(hijack, vec!["any:53"], "空白项应被剔除并回退默认值");
}

/// 订阅只有节点、没有 proxy-groups / rules 时，生成默认组，且测速地址来自设置。
#[test]
fn nodes_without_groups_get_default_groups_using_configured_test_url() {
    let settings = AppSettings {
        latency_test_url: "http://127.0.0.1:1234/ping".to_string(),
        ..AppSettings::default()
    };
    let nodes_only = r#"
proxies:
  - name: "节点C"
    type: ss
    server: 203.0.113.9
    port: 8388
    cipher: aes-128-gcm
    password: pwd-c
"#;
    let config = build(&settings, Some(nodes_only));

    let groups = config
        .get("proxy-groups")
        .and_then(|value| value.as_sequence())
        .expect("应生成默认策略组");
    assert_eq!(groups.len(), 2, "默认应生成 PROXY + AUTO 两组");
    assert_eq!(
        groups[0].get("name").and_then(|value| value.as_str()),
        Some("PROXY")
    );
    assert_eq!(
        groups[1].get("name").and_then(|value| value.as_str()),
        Some("AUTO")
    );
    assert_eq!(
        groups[1].get("type").and_then(|value| value.as_str()),
        Some("url-test")
    );
    assert_eq!(
        groups[1].get("url").and_then(|value| value.as_str()),
        Some("http://127.0.0.1:1234/ping"),
        "测速地址应来自 latency_test_url"
    );
    let auto_nodes: Vec<&str> = groups[1]
        .get("proxies")
        .and_then(|value| value.as_sequence())
        .map(|items| items.iter().filter_map(|item| item.as_str()).collect())
        .unwrap_or_default();
    assert_eq!(auto_nodes, vec!["节点C"]);

    // 有节点时兜底规则走 PROXY
    let rules = rules_of(&config);
    assert_eq!(rules.last().map(String::as_str), Some("MATCH,PROXY"));
}

/// 订阅里 dns.enable=false：不开 TUN 时保留，开 TUN 时强制为 true 且 nameserver 不丢。
#[test]
fn subscription_dns_is_preserved_but_forced_on_for_tun() {
    let profile = r#"
dns:
  enable: false
  nameserver:
    - 192.0.2.53
proxies:
  - name: "节点D"
    type: ss
    server: 203.0.113.10
    port: 8388
    cipher: aes-128-gcm
    password: pwd-d
"#;

    let off = build(&AppSettings::default(), Some(profile));
    assert_eq!(
        off.get("dns")
            .and_then(|dns| dns.get("enable"))
            .and_then(|value| value.as_bool()),
        Some(false),
    );

    let on = AppSettings {
        tun_enabled: true,
        ..AppSettings::default()
    };
    let config = build(&on, Some(profile));
    let dns = config.get("dns").expect("dns 段应存在");
    assert_eq!(
        dns.get("enable").and_then(|value| value.as_bool()),
        Some(true),
        "TUN 下 DNS 必须启用",
    );
    assert_eq!(
        dns.get("nameserver")
            .and_then(|value| value.as_sequence())
            .map(|items| items.len()),
        Some(1),
        "订阅原有 nameserver 不应被覆盖",
    );
}
