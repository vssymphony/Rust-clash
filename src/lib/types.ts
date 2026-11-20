/**
 * 与 Rust 后端共享的数据结构定义。
 * 所有结构体在 Rust 侧均使用 `#[serde(rename_all = "camelCase")]`，
 * 因此这里的字段名与后端保持驼峰一致。
 */

export type KernelStatusKind = "stopped" | "starting" | "running" | "restarting" | "failed";

export interface KernelStatus {
  kind: KernelStatusKind;
  pid: number | null;
  message: string | null;
  restartCount: number;
  /** 启动时间（Unix 毫秒），未运行时为 null */
  startedAt: number | null;
  /** mihomo 上报的版本号，例如 v1.18.10 */
  version: string | null;
}

export type ProxyMode = "rule" | "global" | "direct";
export type LogLevel = "silent" | "error" | "warning" | "info" | "debug";

export interface AppSettings {
  /** 混合代理端口（HTTP + SOCKS） */
  mixedPort: number;
  /** Clash 外部控制器端口 */
  controllerPort: number;
  controllerSecret: string;
  mode: ProxyMode;
  logLevel: LogLevel;
  allowLan: boolean;
  /** 期望的系统代理状态（用于崩溃后恢复） */
  systemProxy: boolean;
  autoStart: boolean;
  startMinimized: boolean;
  subUpdateIntervalMinutes: number;
  latencyTestUrl: string;
  currentSubscriptionId: string | null;
  /** 自定义内核二进制路径，为空则使用随包 sidecar */
  kernelPath: string | null;
  closeConnectionsOnSwitch: boolean;

  /* ------------------------------- TUN 模式 ------------------------------ */
  /** TUN 开关（需管理员 / root 或 CAP_NET_ADMIN，Windows 还需 wintun.dll） */
  tunEnabled: boolean;
  /** TUN 协议栈实现 */
  tunStack: TunStack;
  /** 虚拟网卡名，null / 空串时使用平台默认值 */
  tunDeviceName: string | null;
  /** 自动接管系统路由表 */
  tunAutoRoute: boolean;
  /** 自动探测出口网卡 */
  tunAutoDetectInterface: boolean;
  /** 严格路由，阻止流量绕过 TUN 泄漏 */
  tunStrictRoute: boolean;
  /** DNS 劫持目标，例如 ["any:53"] */
  tunDnsHijack: string[];
}

/** TUN 协议栈实现，取值与 mihomo 的 `tun.stack` 一致 */
export type TunStack = "gvisor" | "system" | "mixed";

export interface TunPrivilege {
  /** 当前平台是否实现了 TUN 支持（Windows / Linux 为 true） */
  supported: boolean;
  /** 平台标识：windows / linux / 其他 */
  platform: string;
  /** 是否具备创建虚拟网卡所需权限 */
  elevated: boolean;
  /** 权限来源：administrator / root / cap_net_admin / unknown */
  mechanism: string;
  /** Windows 专用：是否找到 wintun.dll */
  wintunPresent: boolean | null;
  /** 探测到的具体信息（euid、CapEff、执行命令等），便于排障 */
  detail: string | null;
  /** 权限不足时的手动处理指引 */
  hint: string | null;
}

export interface TunState {
  /** 平台是否支持 TUN */
  supported: boolean;
  /** 配置中是否启用 TUN（用户意图） */
  enabled: boolean;
  /** 内核当前是否真的跑在 TUN 模式 */
  active: boolean;
  stack: TunStack;
  /** 实际生效的设备名 */
  deviceName: string;
  privilege: TunPrivilege;
  /** 面向用户的综合说明（正常 / 降级 / 失败原因） */
  message: string | null;
}

export interface Subscription {
  id: string;
  name: string;
  /** 远程订阅地址，本地文件订阅时为 null */
  url: string | null;
  /** 本地导入时的源文件路径 */
  filePath: string | null;
  nodeCount: number;
  /** Unix 毫秒 */
  createdAt: number;
  updatedAt: number;
  /** 订阅服务端返回的流量信息（原样保存） */
  userInfo: string | null;
}

export type SystemProxyBackend = "windows" | "gnome" | "kde" | "unsupported";

export interface SystemProxyState {
  enabled: boolean;
  backend: SystemProxyBackend;
  /** 降级或失败时的说明文案 */
  message: string | null;
}

export interface AppStateSnapshot {
  kernel: KernelStatus;
  settings: AppSettings;
  systemProxy: SystemProxyState;
  subscriptions: Subscription[];
  appVersion: string;
  configPath: string;
  dataDir: string;
  platform: string;
}

/* ----------------------------- Clash REST API ---------------------------- */

export interface TrafficSample {
  /** 上行字节/秒 */
  up: number;
  /** 下行字节/秒 */
  down: number;
  upTotal: number;
  downTotal: number;
}

export interface ProxyHistoryItem {
  time: string;
  delay: number;
}

export interface ProxyNode {
  name: string;
  type: string;
  /** 策略组当前选中项（仅 Selector/URLTest 等组类型存在） */
  now?: string;
  all?: string[];
  delay?: number;
  alive?: boolean;
  udp?: boolean;
  history?: ProxyHistoryItem[];
  hidden?: boolean;
}

export interface ProxiesResponse {
  proxies: Record<string, ProxyNode>;
}

export interface ConnectionMeta {
  network: string;
  type: string;
  sourceIP: string;
  destinationIP: string;
  sourcePort: string;
  destinationPort: string;
  host: string;
  dnsMode?: string;
  process?: string;
  processPath?: string;
}

export interface ConnectionItem {
  id: string;
  metadata: ConnectionMeta;
  upload: number;
  download: number;
  /** RFC3339 时间戳 */
  start: string;
  chains: string[];
  rule: string;
  rulePayload: string;
}

export interface ConnectionsResponse {
  downloadTotal: number;
  uploadTotal: number;
  connections: ConnectionItem[];
  memory?: number;
}

export interface RuleItem {
  type: string;
  payload: string;
  proxy: string;
  size?: number;
}

export interface RulesResponse {
  rules: RuleItem[];
}

export interface ClashConfigSnapshot {
  port?: number;
  "socks-port"?: number;
  "mixed-port"?: number;
  mode?: string;
  "log-level"?: string;
  "allow-lan"?: boolean;
  "external-controller"?: string;
  secret?: string;
}

export interface ProxyDelayResult {
  name: string;
  delay: number;
  error: string | null;
}

export interface LogEntry {
  /** Unix 毫秒 */
  time: number;
  level: string;
  message: string;
  source: "kernel" | "app";
}

export interface SubscriptionInput {
  name: string;
  url?: string | null;
  filePath?: string | null;
}
