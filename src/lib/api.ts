import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  AppStateSnapshot,
  ClashConfigSnapshot,
  ConnectionsResponse,
  KernelStatus,
  LogEntry,
  ProxiesResponse,
  ProxyDelayResult,
  RulesResponse,
  Subscription,
  SubscriptionInput,
  SystemProxyState,
  TrafficSample,
  TunPrivilege,
  TunState,
} from "./types";

/** 是否运行在 Tauri 容器内（浏览器中直接 `vite dev` 时为 false） */
export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauriRuntime()) {
    throw new Error("当前不在 Tauri 运行时中，请使用 `npm run tauri:dev` 启动桌面应用");
  }
  return invoke<T>(command, args);
}

export const api = {
  /* ------------------------------- 应用状态 ------------------------------- */
  getAppState: () => call<AppStateSnapshot>("get_app_state"),
  updateSettings: (settings: AppSettings) => call<AppSettings>("update_settings", { settings }),

  /* -------------------------------- 内核控制 ------------------------------ */
  kernelStart: (kernelPath?: string | null) =>
    call<KernelStatus>("kernel_start", { kernelPath: kernelPath ?? null }),
  kernelStop: () => call<KernelStatus>("kernel_stop"),
  kernelRestart: () => call<KernelStatus>("kernel_restart"),
  kernelStatus: () => call<KernelStatus>("kernel_status"),
  kernelLogs: (limit = 300) => call<LogEntry[]>("kernel_logs", { limit }),
  openConfigDir: () => call<void>("open_config_dir"),
  reloadConfig: () => call<void>("reload_config"),

  /* ------------------------------ Clash REST ------------------------------ */
  getProxies: () => call<ProxiesResponse>("get_proxies"),
  selectProxy: (group: string, name: string) => call<void>("select_proxy", { group, name }),
  proxyDelay: (name: string, url?: string, timeout?: number) =>
    call<ProxyDelayResult>("proxy_delay", { name, url: url ?? null, timeout: timeout ?? null }),
  proxyGroupDelay: (group: string, url?: string, timeout?: number) =>
    call<ProxyDelayResult[]>("proxy_group_delay", {
      group,
      url: url ?? null,
      timeout: timeout ?? null,
    }),
  getConnections: () => call<ConnectionsResponse>("get_connections"),
  closeConnection: (id: string) => call<void>("close_connection", { id }),
  closeAllConnections: () => call<void>("close_all_connections"),
  getRules: () => call<RulesResponse>("get_rules"),
  getClashConfig: () => call<ClashConfigSnapshot>("get_clash_config"),
  patchClashConfig: (patch: Record<string, unknown>) => call<void>("patch_clash_config", { patch }),

  /* --------------------------------- 订阅 -------------------------------- */
  listSubscriptions: () => call<Subscription[]>("list_subscriptions"),
  addSubscription: (input: SubscriptionInput) => call<Subscription>("add_subscription", { input }),
  removeSubscription: (id: string) => call<void>("remove_subscription", { id }),
  updateSubscription: (id: string) => call<Subscription>("update_subscription", { id }),
  updateAllSubscriptions: () => call<Subscription[]>("update_all_subscriptions"),
  setActiveSubscription: (id: string) => call<Subscription>("set_active_subscription", { id }),

  /* -------------------------------- 系统代理 ------------------------------ */
  getSystemProxy: () => call<SystemProxyState>("get_system_proxy"),
  setSystemProxy: (enabled: boolean) => call<SystemProxyState>("set_system_proxy", { enabled }),

  /* -------------------------------- 开机自启 ------------------------------ */
  getAutostart: () => call<boolean>("get_autostart"),
  setAutostart: (enabled: boolean) => call<boolean>("set_autostart", { enabled }),

  /* --------------------------------- TUN 模式 ----------------------------- */
  tunStatus: () => call<TunState>("tun_status"),
  tunCheckPrivilege: () => call<TunPrivilege>("tun_check_privilege"),
  tunToggle: (enabled: boolean) => call<TunState>("tun_toggle", { enabled }),
};

/* --------------------------------- 事件订阅 -------------------------------- */

type EventMap = {
  "clash://kernel-status": KernelStatus;
  "clash://traffic": TrafficSample;
  "clash://connections": ConnectionsResponse;
  "clash://log": LogEntry;
};

export function onEvent<K extends keyof EventMap>(
  event: K,
  handler: (payload: EventMap[K]) => void,
): Promise<UnlistenFn> {
  return listen<EventMap[K]>(event, (e) => handler(e.payload));
}

/** 从后端抛出的错误统一取值 */
export function toErrorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  if (error && typeof error === "object" && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}
