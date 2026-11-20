import { useMemo, useState } from "react";
import type { ElementType } from "react";
import {
  Activity,
  Cable,
  LayoutDashboard,
  ListTree,
  Loader2,
  Power,
  Rss,
  Settings as SettingsIcon,
  ShieldCheck,
  Waypoints,
} from "lucide-react";
import { api, isTauriRuntime, toErrorMessage } from "@/lib/api";
import { formatSpeed } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/store/appStore";
import type { PageKey } from "@/store/appStore";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { toast } from "@/components/ui/toast";
import { StatusDot } from "@/components/common/primitives";

interface NavItem {
  key: PageKey;
  label: string;
  icon: ElementType;
}

const NAV_ITEMS: NavItem[] = [
  { key: "overview", label: "概览", icon: LayoutDashboard },
  { key: "proxies", label: "代理", icon: Waypoints },
  { key: "connections", label: "连接", icon: Cable },
  { key: "rules", label: "规则", icon: ListTree },
  { key: "subscriptions", label: "订阅", icon: Rss },
  { key: "settings", label: "设置", icon: SettingsIcon },
];

function kernelTone(kind: string): "success" | "warning" | "destructive" | "muted" {
  switch (kind) {
    case "running":
      return "success";
    case "starting":
    case "restarting":
      return "warning";
    case "failed":
      return "destructive";
    default:
      return "muted";
  }
}

function kernelLabel(kind: string): string {
  switch (kind) {
    case "running":
      return "内核运行中";
    case "starting":
      return "内核启动中";
    case "restarting":
      return "内核重启中";
    case "failed":
      return "内核异常";
    default:
      return "内核已停止";
  }
}

export function Sidebar() {
  const page = useAppStore((state) => state.page);
  const setPage = useAppStore((state) => state.setPage);
  const kernel = useAppStore((state) => state.kernel);
  const settings = useAppStore((state) => state.settings);
  const systemProxy = useAppStore((state) => state.systemProxy);
  const setSystemProxy = useAppStore((state) => state.setSystemProxy);
  const refreshSnapshot = useAppStore((state) => state.refreshSnapshot);
  const traffic = useAppStore((state) => state.traffic);
  const connections = useAppStore((state) => state.connections);
  const [busy, setBusy] = useState(false);
  const [proxyBusy, setProxyBusy] = useState(false);

  const connectionCount = connections?.connections.length ?? 0;

  const activeSubscription = useMemo(() => {
    const snapshot = useAppStore.getState().snapshot;
    if (!snapshot || !settings?.currentSubscriptionId) return null;
    return (
      snapshot.subscriptions.find((item) => item.id === settings.currentSubscriptionId) ?? null
    );
  }, [settings?.currentSubscriptionId]);

  const toggleKernel = async () => {
    if (!isTauriRuntime()) {
      toast.error("请通过 npm run tauri:dev 启动桌面应用");
      return;
    }
    setBusy(true);
    try {
      if (kernel.kind === "running" || kernel.kind === "starting" || kernel.kind === "restarting") {
        await api.kernelStop();
        toast.info("内核已停止");
      } else {
        await api.kernelStart(settings?.kernelPath ?? null);
        toast.success("内核已启动");
      }
      await refreshSnapshot();
    } catch (error) {
      toast.error("内核操作失败", toErrorMessage(error));
    } finally {
      setBusy(false);
    }
  };

  const toggleSystemProxy = async (next: boolean) => {
    setProxyBusy(true);
    try {
      const state = await api.setSystemProxy(next);
      setSystemProxy(state);
      if (next && !state.enabled) {
        toast.warning("系统代理未生效", state.message ?? "当前桌面环境不支持自动设置系统代理");
      } else {
        toast.success(next ? "系统代理已开启" : "系统代理已关闭", state.message ?? undefined);
      }
    } catch (error) {
      toast.error("系统代理切换失败", toErrorMessage(error));
    } finally {
      setProxyBusy(false);
    }
  };

  return (
    <aside className="flex h-full w-56 shrink-0 flex-col border-r border-border/60 bg-card/40">
      <div className="flex items-center gap-2.5 px-4 py-4">
        <div className="flex h-9 w-9 items-center justify-center rounded-lg bg-primary/15 text-primary">
          <ShieldCheck className="h-5 w-5" />
        </div>
        <div className="leading-tight">
          <p className="text-sm font-semibold tracking-tight">Clash GUI</p>
          <p className="text-[10px] text-muted-foreground">Tauri 2 · mihomo</p>
        </div>
      </div>

      <nav className="flex flex-col gap-0.5 px-2">
        {NAV_ITEMS.map((item) => {
          const Icon = item.icon;
          const active = page === item.key;
          return (
            <button
              key={item.key}
              type="button"
              onClick={() => setPage(item.key)}
              className={cn(
                "group flex items-center gap-2.5 rounded-md px-3 py-2 text-sm transition-colors",
                active
                  ? "bg-primary/12 text-primary"
                  : "text-muted-foreground hover:bg-white/5 hover:text-foreground",
              )}
            >
              <Icon className="h-4 w-4" />
              <span className="flex-1 text-left">{item.label}</span>
              {item.key === "connections" && connectionCount > 0 ? (
                <Badge variant="secondary" className="px-1.5 py-0 text-[10px]">
                  {connectionCount}
                </Badge>
              ) : null}
            </button>
          );
        })}
      </nav>

      <div className="mt-auto space-y-3 border-t border-border/60 p-3">
        <div className="rounded-lg bg-white/[0.03] p-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-1.5">
              <StatusDot tone={kernelTone(kernel.kind)} pulse={kernel.kind === "running"} />
              <span className="text-xs font-medium">{kernelLabel(kernel.kind)}</span>
            </div>
            <Button
              size="icon-sm"
              variant={kernel.kind === "running" ? "ghost" : "default"}
              onClick={toggleKernel}
              disabled={busy}
              aria-label={kernel.kind === "running" ? "停止内核" : "启动内核"}
            >
              {busy ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Power className="h-3.5 w-3.5" />
              )}
            </Button>
          </div>
          <p className="mt-1 truncate text-[10px] text-muted-foreground">
            {kernel.pid
              ? `PID ${kernel.pid}`
              : activeSubscription
                ? activeSubscription.name
                : "未加载配置"}
            {kernel.version ? ` · ${kernel.version}` : ""}
          </p>
        </div>

        <div className="flex items-center justify-between px-1">
          <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
            <Activity className="h-3.5 w-3.5" />
            系统代理
          </span>
          <Switch
            checked={Boolean(systemProxy?.enabled)}
            disabled={proxyBusy || !isTauriRuntime()}
            onCheckedChange={(value) => void toggleSystemProxy(value)}
            aria-label="系统代理开关"
          />
        </div>

        <div className="flex items-center justify-between px-1 text-[10px] tabular-nums text-muted-foreground">
          <span className="text-success">↑ {formatSpeed(traffic.up)}</span>
          <span className="text-info">↓ {formatSpeed(traffic.down)}</span>
        </div>
      </div>
    </aside>
  );
}
