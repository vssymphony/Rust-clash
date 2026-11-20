import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { FolderOpen, RefreshCw, RotateCcw, Save, ShieldAlert, Terminal } from "lucide-react";
import { api, isTauriRuntime, toErrorMessage } from "@/lib/api";
import { formatBytes } from "@/lib/format";
import { useAppStore } from "@/store/appStore";
import { useUpdateSettings } from "@/hooks/useClashData";
import type { AppSettings, LogLevel, ProxyMode, TunStack, TunState } from "@/lib/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input, Select } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { toast } from "@/components/ui/toast";
import { PageHeader } from "@/components/common/primitives";

const BACKEND_LABEL: Record<string, string> = {
  windows: "Windows 注册表 Internet Settings",
  gnome: "GNOME gsettings",
  kde: "KDE kioslaverc",
  unsupported: "当前桌面环境不支持自动设置",
};

const LOG_LEVELS: LogLevel[] = ["silent", "error", "warning", "info", "debug"];
const MODES: Array<{ value: ProxyMode; label: string }> = [
  { value: "rule", label: "规则模式" },
  { value: "global", label: "全局模式" },
  { value: "direct", label: "直连模式" },
];
const TUN_STACKS: Array<{ value: TunStack; label: string }> = [
  { value: "gvisor", label: "gvisor（用户态，兼容性最好）" },
  { value: "system", label: "system（系统栈，性能更高）" },
  { value: "mixed", label: "mixed（TCP 走 system，UDP 走 gvisor）" },
];

function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5 sm:flex-row sm:items-center sm:justify-between">
      <div className="space-y-0.5">
        <p className="text-xs font-medium">{label}</p>
        {hint ? <p className="text-[10px] text-muted-foreground">{hint}</p> : null}
      </div>
      <div className="sm:w-64">{children}</div>
    </div>
  );
}

export function SettingsPage() {
  const settings = useAppStore((state) => state.settings);
  const systemProxySnapshot = useAppStore((state) => state.systemProxy);
  const systemProxy = systemProxySnapshot ?? {
    enabled: false,
    backend: "unsupported" as const,
    message: null,
  };
  const snapshot = useAppStore((state) => state.snapshot);
  const kernel = useAppStore((state) => state.kernel);
  const traffic = useAppStore((state) => state.traffic);
  const refreshSnapshot = useAppStore((state) => state.refreshSnapshot);
  const updateSettings = useUpdateSettings();

  const [form, setForm] = useState<AppSettings | null>(settings);
  const [proxySwitchPending, setProxySwitchPending] = useState(false);
  const [tunState, setTunState] = useState<TunState | null>(null);
  const [tunPending, setTunPending] = useState(false);

  useEffect(() => {
    if (settings) setForm(settings);
    // 设置对象变化（含首次加载）时重新探测 TUN 权限与运行状态
    void loadTunState();
  }, [settings]);

  const patch = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    setForm((prev) => (prev ? { ...prev, [key]: value } : prev));
  };

  const dirty = Boolean(
    form &&
    settings &&
    JSON.stringify({ ...form, systemProxy: false, autoStart: false }) !==
      JSON.stringify({ ...settings, systemProxy: false, autoStart: false }),
  );

  const save = async () => {
    if (!form) return;
    // TUN 段变更无法通过 patch 生效，保存后需要重新下发整份配置
    const tunDirty =
      settings != null &&
      (form.tunStack !== settings.tunStack ||
        form.tunDeviceName !== settings.tunDeviceName ||
        form.tunAutoRoute !== settings.tunAutoRoute ||
        form.tunAutoDetectInterface !== settings.tunAutoDetectInterface ||
        form.tunStrictRoute !== settings.tunStrictRoute ||
        form.tunDnsHijack.join(",") !== settings.tunDnsHijack.join(","));
    try {
      await updateSettings.mutateAsync(form);
      if (tunDirty) {
        await api.reloadConfig();
        await loadTunState();
        toast.success("设置已保存", "TUN 配置已重新下发到内核");
      } else {
        toast.success("设置已保存", "端口与模式变更会在内核重启后完全生效");
      }
      await refreshSnapshot();
    } catch (error) {
      toast.error("保存失败", toErrorMessage(error));
    }
  };

  const toggleSystemProxy = async (enabled: boolean) => {
    setProxySwitchPending(true);
    try {
      const state = await api.setSystemProxy(enabled);
      await refreshSnapshot();
      if (state.message) {
        toast.warning(enabled ? "系统代理已部分生效" : "系统代理未完全还原", state.message);
      } else {
        toast.success(enabled ? "系统代理已开启" : "系统代理已关闭");
      }
    } catch (error) {
      toast.error("系统代理操作失败", toErrorMessage(error));
    } finally {
      setProxySwitchPending(false);
    }
  };

  const toggleAutostart = async (enabled: boolean) => {
    try {
      const actual = await api.setAutostart(enabled);
      patch("autoStart", actual);
      await refreshSnapshot();
      toast.success(actual ? "已开启开机自启" : "已关闭开机自启");
    } catch (error) {
      toast.error("开机自启设置失败", toErrorMessage(error));
    }
  };

  /** 重新探测 TUN 权限与运行状态；浏览器预览环境下跳过 */
  async function loadTunState() {
    if (!isTauriRuntime()) return;
    try {
      setTunState(await api.tunStatus());
    } catch (error) {
      // 探测失败不阻塞设置页，保留上一次结果
      console.warn("TUN 状态探测失败：", toErrorMessage(error));
    }
  }

  const toggleTun = async (enabled: boolean) => {
    if (!form) return;
    setTunPending(true);
    try {
      const state = await api.tunToggle(enabled);
      setTunState(state);
      // 同步表单，避免随后「保存设置」把 TUN 开关覆盖回旧值
      patch("tunEnabled", enabled);
      if (state.message) {
        toast.warning(enabled ? "TUN 已触发，但存在降级" : "TUN 已关闭", state.message);
      } else {
        toast.success(enabled ? "TUN 模式已开启" : "TUN 模式已关闭");
      }
    } catch (error) {
      toast.error("TUN 操作失败", toErrorMessage(error));
      await loadTunState();
    } finally {
      setTunPending(false);
    }
  };

  const openDir = async (target: "config" | "data") => {
    try {
      if (target === "data" && snapshot?.dataDir) {
        await navigator.clipboard.writeText(snapshot.dataDir).catch(() => undefined);
      }
      await api.openConfigDir();
    } catch (error) {
      toast.error("打开目录失败", toErrorMessage(error));
    }
  };

  if (!form) {
    return <p className="text-xs text-muted-foreground">加载设置中…</p>;
  }

  return (
    <div className="space-y-4">
      <PageHeader
        title="设置"
        description="端口、内核、系统代理与订阅更新策略"
        actions={
          <>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setForm(settings)}
              disabled={!dirty || updateSettings.isPending}
            >
              <RotateCcw className="h-3.5 w-3.5" />
              还原
            </Button>
            <Button
              size="sm"
              onClick={() => void save()}
              disabled={!dirty || updateSettings.isPending}
            >
              <Save className="h-3.5 w-3.5" />
              保存设置
            </Button>
          </>
        }
      />

      <div className="grid gap-3 xl:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>代理端口</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <Field label="混合端口" hint="HTTP 与 SOCKS5 共用端口">
              <Input
                type="number"
                value={form.mixedPort}
                onChange={(event) => patch("mixedPort", Number(event.target.value) || 0)}
              />
            </Field>
            <Field label="控制器端口" hint="mihomo external-controller">
              <Input
                type="number"
                value={form.controllerPort}
                onChange={(event) => patch("controllerPort", Number(event.target.value) || 0)}
              />
            </Field>
            <Field label="控制器密钥" hint="留空表示不校验">
              <Input
                value={form.controllerSecret}
                onChange={(event) => patch("controllerSecret", event.target.value)}
                placeholder="可选"
              />
            </Field>
            <Field label="允许局域网连接" hint="其他设备可通过本机代理上网">
              <div className="flex justify-end">
                <Switch checked={form.allowLan} onCheckedChange={(v) => patch("allowLan", v)} />
              </div>
            </Field>
            <p className="rounded-md bg-muted/60 px-2.5 py-2 text-[10px] text-muted-foreground">
              修改端口或密钥后需要重启内核才会生效。
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>内核与行为</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <Field label="代理模式">
              <Select
                value={form.mode}
                onChange={(event) => patch("mode", event.target.value as ProxyMode)}
              >
                {MODES.map((mode) => (
                  <option key={mode.value} value={mode.value}>
                    {mode.label}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="日志级别">
              <Select
                value={form.logLevel}
                onChange={(event) => patch("logLevel", event.target.value as LogLevel)}
              >
                {LOG_LEVELS.map((level) => (
                  <option key={level} value={level}>
                    {level}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="开机自启" hint="登录系统后自动启动应用">
              <div className="flex justify-end">
                <Switch checked={form.autoStart} onCheckedChange={(v) => void toggleAutostart(v)} />
              </div>
            </Field>
            <Field label="启动后最小化到托盘">
              <div className="flex justify-end">
                <Switch
                  checked={form.startMinimized}
                  onCheckedChange={(v) => patch("startMinimized", v)}
                />
              </div>
            </Field>
            <Field label="切换订阅时清空连接" hint="避免旧连接沿用旧节点">
              <div className="flex justify-end">
                <Switch
                  checked={form.closeConnectionsOnSwitch}
                  onCheckedChange={(v) => patch("closeConnectionsOnSwitch", v)}
                />
              </div>
            </Field>
            <Field label="内核二进制路径" hint="留空使用随包 sidecar">
              <Input
                value={form.kernelPath ?? ""}
                onChange={(event) => patch("kernelPath", event.target.value || null)}
                placeholder="自动（resources/mihomo）"
              />
            </Field>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>系统代理</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <p className="text-xs font-medium">启用系统代理</p>
                <p className="text-[10px] text-muted-foreground">
                  网关 127.0.0.1，端口跟随混合端口
                </p>
              </div>
              <Switch
                checked={systemProxy.enabled}
                disabled={proxySwitchPending || !isTauriRuntime()}
                onCheckedChange={(v) => void toggleSystemProxy(v)}
              />
            </div>
            <div className="flex items-center justify-between text-[11px]">
              <span className="text-muted-foreground">当前实现</span>
              <Badge variant={systemProxy.backend === "unsupported" ? "warning" : "secondary"}>
                {BACKEND_LABEL[systemProxy.backend] ?? systemProxy.backend}
              </Badge>
            </div>
            {systemProxy.message ? (
              <p className="flex gap-1.5 rounded-md border border-warning/35 bg-warning/10 px-2.5 py-2 text-[10px] leading-relaxed text-warning">
                <ShieldAlert className="mt-0.5 h-3 w-3 shrink-0" />
                {systemProxy.message}
              </p>
            ) : null}
            <p className="text-[10px] leading-relaxed text-muted-foreground">
              应用退出或内核异常退出时会自动还原系统代理；若因强制结束进程导致残留，可重新打开
              应用后关闭系统代理开关完成清理。
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>TUN 模式</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="flex items-center justify-between">
              <div className="space-y-0.5">
                <p className="text-xs font-medium">启用 TUN</p>
                <p className="text-[10px] text-muted-foreground">
                  接管全部系统流量，应用无需单独设置代理
                </p>
              </div>
              <Switch
                aria-label="启用 TUN"
                checked={tunState?.enabled ?? form.tunEnabled}
                disabled={tunPending || !isTauriRuntime() || tunState?.supported === false}
                onCheckedChange={(v) => void toggleTun(v)}
              />
            </div>
            <Field label="协议栈" hint="gvisor 兼容性最好，system 性能更高">
              <Select
                value={form.tunStack}
                onChange={(event) => patch("tunStack", event.target.value as TunStack)}
              >
                {TUN_STACKS.map((item) => (
                  <option key={item.value} value={item.value}>
                    {item.label}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label="虚拟网卡名" hint="留空使用默认名（Windows: Mihomo / Linux: mihomo）">
              <Input
                value={form.tunDeviceName ?? ""}
                onChange={(event) => patch("tunDeviceName", event.target.value || null)}
                placeholder="自动"
              />
            </Field>
            <Field label="自动配置路由" hint="auto-route，接管系统路由表">
              <div className="flex justify-end">
                <Switch
                  checked={form.tunAutoRoute}
                  onCheckedChange={(v) => patch("tunAutoRoute", v)}
                />
              </div>
            </Field>
            <Field label="自动探测出口网卡" hint="auto-detect-interface，多网卡环境建议开启">
              <div className="flex justify-end">
                <Switch
                  checked={form.tunAutoDetectInterface}
                  onCheckedChange={(v) => patch("tunAutoDetectInterface", v)}
                />
              </div>
            </Field>
            <Field label="严格路由" hint="strict-route，阻止流量绕过 TUN 泄漏">
              <div className="flex justify-end">
                <Switch
                  checked={form.tunStrictRoute}
                  onCheckedChange={(v) => patch("tunStrictRoute", v)}
                />
              </div>
            </Field>

            <div className="flex items-center justify-between text-[11px]">
              <span className="text-muted-foreground">运行状态</span>
              <Badge variant={tunState?.active ? "secondary" : "outline"}>
                {tunState === null
                  ? "未探测"
                  : tunState.active
                    ? "已生效"
                    : tunState.enabled
                      ? "已启用未生效"
                      : "未启用"}
              </Badge>
            </div>
            {tunState ? (
              <>
                <div className="flex items-center justify-between text-[11px]">
                  <span className="text-muted-foreground">权限</span>
                  <Badge variant={tunState.privilege.elevated ? "secondary" : "warning"}>
                    {tunState.privilege.elevated
                      ? `已提权（${tunState.privilege.mechanism}）`
                      : `权限不足（${tunState.privilege.mechanism}）`}
                  </Badge>
                </div>
                <div className="flex items-center justify-between text-[11px]">
                  <span className="text-muted-foreground">虚拟网卡</span>
                  <span className="font-mono text-[10px]">{tunState.deviceName}</span>
                </div>
                {tunState.privilege.platform === "windows" ? (
                  <div className="flex items-center justify-between text-[11px]">
                    <span className="text-muted-foreground">wintun.dll</span>
                    <Badge
                      variant={tunState.privilege.wintunPresent === false ? "warning" : "outline"}
                    >
                      {tunState.privilege.wintunPresent === null
                        ? "未检测"
                        : tunState.privilege.wintunPresent
                          ? "已找到"
                          : "未找到"}
                    </Badge>
                  </div>
                ) : null}
                {tunState.message ? (
                  <p className="flex gap-1.5 rounded-md border border-warning/35 bg-warning/10 px-2.5 py-2 text-[10px] leading-relaxed text-warning">
                    <ShieldAlert className="mt-0.5 h-3 w-3 shrink-0" />
                    {tunState.message}
                  </p>
                ) : null}
                {tunState.privilege.hint ? (
                  <p className="whitespace-pre-line rounded-md bg-muted/60 px-2.5 py-2 text-[10px] leading-relaxed text-muted-foreground">
                    {tunState.privilege.hint}
                  </p>
                ) : null}
              </>
            ) : (
              <p className="rounded-md bg-muted/60 px-2.5 py-2 text-[10px] text-muted-foreground">
                正在探测 TUN 支持与权限…
              </p>
            )}

            <div className="flex flex-wrap gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => void loadTunState()}
                disabled={tunPending || !isTauriRuntime()}
              >
                <RefreshCw className="h-3.5 w-3.5" />
                重新检测权限
              </Button>
            </div>
            <p className="text-[10px] leading-relaxed text-muted-foreground">
              Windows 需以管理员身份运行并放置 wintun.dll；Linux 需 root 或授予 CAP_NET_ADMIN，详见
              README「TUN 模式」章节。
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>订阅与测速</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <Field label="订阅自动更新间隔" hint="单位：分钟，0 表示不自动更新">
              <Input
                type="number"
                value={form.subUpdateIntervalMinutes}
                onChange={(event) =>
                  patch("subUpdateIntervalMinutes", Number(event.target.value) || 0)
                }
              />
            </Field>
            <Field label="延迟测试地址" hint="默认使用 gstatic 生成 204">
              <Input
                value={form.latencyTestUrl}
                onChange={(event) => patch("latencyTestUrl", event.target.value)}
              />
            </Field>
            <div className="flex items-center justify-between text-[11px]">
              <span className="text-muted-foreground">内核版本</span>
              <span className="tabular-nums">{kernel.version ?? "未运行"}</span>
            </div>
            <div className="flex items-center justify-between text-[11px]">
              <span className="text-muted-foreground">当前订阅节点数</span>
              <span>
                {snapshot?.subscriptions.find((s) => s.id === form.currentSubscriptionId)
                  ?.nodeCount ?? 0}
              </span>
            </div>
          </CardContent>
        </Card>

        <Card className="xl:col-span-2">
          <CardHeader>
            <CardTitle>应用信息</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="grid gap-3 sm:grid-cols-2">
              <div className="space-y-1 rounded-md bg-muted/40 px-3 py-2">
                <p className="text-[10px] uppercase text-muted-foreground">配置目录</p>
                <p className="break-all font-mono text-[11px]">{snapshot?.configPath ?? "-"}</p>
              </div>
              <div className="space-y-1 rounded-md bg-muted/40 px-3 py-2">
                <p className="text-[10px] uppercase text-muted-foreground">数据目录</p>
                <p className="break-all font-mono text-[11px]">{snapshot?.dataDir ?? "-"}</p>
              </div>
            </div>
            <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
              <Badge variant="outline">v{snapshot?.appVersion ?? "0.1.0"}</Badge>
              <span>平台：{snapshot?.platform ?? "-"}</span>
              <span>内核状态：{kernel.kind}</span>
              <span>累计下行：{formatBytes(traffic.downTotal)}</span>
            </div>
            <div className="flex flex-wrap gap-2">
              <Button variant="outline" size="sm" onClick={() => void openDir("config")}>
                <FolderOpen className="h-3.5 w-3.5" />
                打开配置目录
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={() =>
                  void api
                    .reloadConfig()
                    .then(() => toast.success("已触发配置重载"))
                    .catch((error) => toast.error("重载失败", toErrorMessage(error)))
                }
              >
                <Terminal className="h-3.5 w-3.5" />
                重载配置
              </Button>
            </div>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
