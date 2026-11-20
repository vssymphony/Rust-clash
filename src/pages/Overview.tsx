import { useState } from "react";
import type { ReactNode } from "react";
import {
  ArrowDownToLine,
  ArrowUpFromLine,
  Cable,
  Cpu,
  FolderOpen,
  RefreshCw,
  RotateCcw,
} from "lucide-react";
import { api, isTauriRuntime, toErrorMessage } from "@/lib/api";
import { formatBytes, formatDuration, formatSpeed } from "@/lib/format";
import { useAppStore } from "@/store/appStore";
import type { ProxyMode } from "@/lib/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { toast } from "@/components/ui/toast";
import { PageHeader, StatusDot } from "@/components/common/primitives";
import { ChartLegend, TrafficChart } from "@/components/charts/TrafficChart";

const MODES: Array<{ key: ProxyMode; label: string }> = [
  { key: "rule", label: "规则" },
  { key: "global", label: "全局" },
  { key: "direct", label: "直连" },
];

function StatCard({
  icon,
  label,
  value,
  hint,
  tone,
}: {
  icon: ReactNode;
  label: string;
  value: string;
  hint?: string;
  tone?: string;
}) {
  return (
    <Card>
      <CardContent className="flex items-start justify-between gap-3 p-4">
        <div className="space-y-1">
          <p className="text-[11px] uppercase tracking-wide text-muted-foreground">{label}</p>
          <p className="stat-value">{value}</p>
          {hint ? <p className="text-[10px] text-muted-foreground">{hint}</p> : null}
        </div>
        <span className={tone ?? "text-muted-foreground"}>{icon}</span>
      </CardContent>
    </Card>
  );
}

export function OverviewPage() {
  const kernel = useAppStore((state) => state.kernel);
  const settings = useAppStore((state) => state.settings);
  const snapshot = useAppStore((state) => state.snapshot);
  const traffic = useAppStore((state) => state.traffic);
  const history = useAppStore((state) => state.trafficHistory);
  const connections = useAppStore((state) => state.connections);
  const refreshSnapshot = useAppStore((state) => state.refreshSnapshot);
  const setSettings = useAppStore((state) => state.setSettings);
  const [restarting, setRestarting] = useState(false);

  const running = kernel.kind === "running";
  const currentSub = snapshot?.subscriptions.find(
    (item) => item.id === settings?.currentSubscriptionId,
  );
  const uptime = kernel.startedAt ? formatDuration(Date.now() - kernel.startedAt) : "-";
  const memory = connections?.memory ?? 0;

  const switchMode = async (mode: ProxyMode) => {
    if (!settings) return;
    const next = { ...settings, mode };
    setSettings(next);
    try {
      await api.patchClashConfig({ mode });
      await api.updateSettings(next);
      toast.success(`已切换到${MODES.find((item) => item.key === mode)?.label}模式`);
    } catch (error) {
      toast.error("切换模式失败", toErrorMessage(error));
      await refreshSnapshot();
    }
  };

  const restartKernel = async () => {
    setRestarting(true);
    try {
      await api.kernelRestart();
      toast.success("内核已重启");
      await refreshSnapshot();
    } catch (error) {
      toast.error("内核重启失败", toErrorMessage(error));
    } finally {
      setRestarting(false);
    }
  };

  const openDir = async () => {
    try {
      await api.openConfigDir();
    } catch (error) {
      toast.error("打开目录失败", toErrorMessage(error));
    }
  };

  return (
    <div className="space-y-4">
      <PageHeader
        title="概览"
        description="内核状态、实时流量与常用操作"
        actions={
          <>
            <Button variant="outline" size="sm" onClick={() => void openDir()}>
              <FolderOpen className="h-3.5 w-3.5" />
              配置目录
            </Button>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => void restartKernel()}
              disabled={restarting || !isTauriRuntime()}
            >
              <RotateCcw className={restarting ? "h-3.5 w-3.5 animate-spin" : "h-3.5 w-3.5"} />
              重启内核
            </Button>
          </>
        }
      />

      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        <StatCard
          icon={<ArrowDownToLine className="h-4 w-4 text-info" />}
          label="下行速率"
          value={formatSpeed(traffic.down)}
          hint={`累计 ${formatBytes(traffic.downTotal)}`}
        />
        <StatCard
          icon={<ArrowUpFromLine className="h-4 w-4 text-success" />}
          label="上行速率"
          value={formatSpeed(traffic.up)}
          hint={`累计 ${formatBytes(traffic.upTotal)}`}
        />
        <StatCard
          icon={<Cable className="h-4 w-4 text-primary" />}
          label="活动连接"
          value={String(connections?.connections.length ?? 0)}
          hint={memory > 0 ? `内存 ${formatBytes(memory)}` : "实时连接数"}
        />
        <StatCard
          icon={<Cpu className="h-4 w-4 text-warning" />}
          label="运行时长"
          value={uptime}
          hint={kernel.version ? `内核 ${kernel.version}` : "等待内核就绪"}
        />
      </div>

      <div className="grid gap-3 xl:grid-cols-[1fr_320px]">
        <Card>
          <CardHeader className="flex-row items-center justify-between">
            <div className="space-y-1">
              <CardTitle>实时流量</CardTitle>
              <p className="text-[10px] text-muted-foreground">
                最近 {history.length} 个采样点（约 {Math.round(history.length)} 秒）
              </p>
            </div>
            <div className="flex items-center gap-3">
              <ChartLegend />
              <Button
                variant="ghost"
                size="icon-sm"
                aria-label="刷新状态"
                onClick={() => void refreshSnapshot()}
              >
                <RefreshCw className="h-3.5 w-3.5" />
              </Button>
            </div>
          </CardHeader>
          <CardContent>
            <TrafficChart data={history} height={200} />
            <div className="mt-3 flex flex-wrap items-center gap-4 text-[11px] text-muted-foreground">
              <span>↓ {formatSpeed(traffic.down)}</span>
              <span className="text-success">↑ {formatSpeed(traffic.up)}</span>
              <span>总下载 {formatBytes(traffic.downTotal)}</span>
              <span>总上传 {formatBytes(traffic.upTotal)}</span>
            </div>
          </CardContent>
        </Card>

        <div className="space-y-3">
          <Card>
            <CardHeader>
              <CardTitle>代理模式</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              <div className="grid grid-cols-3 gap-1 rounded-lg bg-muted/60 p-1">
                {MODES.map((mode) => {
                  const active = settings?.mode === mode.key;
                  return (
                    <button
                      key={mode.key}
                      type="button"
                      disabled={!running}
                      onClick={() => void switchMode(mode.key)}
                      className={
                        active
                          ? "rounded-md bg-background px-2 py-1.5 text-xs font-medium text-foreground shadow-sm"
                          : "rounded-md px-2 py-1.5 text-xs text-muted-foreground transition-colors hover:text-foreground disabled:opacity-50"
                      }
                    >
                      {mode.label}
                    </button>
                  );
                })}
              </div>
              <p className="text-[10px] text-muted-foreground">
                当前：{MODES.find((item) => item.key === settings?.mode)?.label ?? "-"}
                {running ? "" : "（内核未运行，切换不可用）"}
              </p>
            </CardContent>
          </Card>

          <Card>
            <CardHeader>
              <CardTitle>运行信息</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2.5 text-xs">
              <div className="flex items-center justify-between">
                <span className="text-muted-foreground">内核状态</span>
                <span className="flex items-center gap-1.5">
                  <StatusDot tone={running ? "success" : "muted"} pulse={running} />
                  {running ? "运行中" : "已停止"}
                </span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-muted-foreground">PID</span>
                <span className="tabular-nums">{kernel.pid ?? "-"}</span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-muted-foreground">重启次数</span>
                <span className="tabular-nums">{kernel.restartCount}</span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-muted-foreground">混合端口</span>
                <span className="tabular-nums">{settings?.mixedPort ?? "-"}</span>
              </div>
              <div className="flex items-center justify-between gap-2">
                <span className="text-muted-foreground">当前订阅</span>
                <span className="max-w-[160px] truncate">{currentSub?.name ?? "未选择"}</span>
              </div>
              {currentSub ? (
                <div className="flex items-center justify-between">
                  <span className="text-muted-foreground">节点数量</span>
                  <Badge variant="secondary">{currentSub.nodeCount}</Badge>
                </div>
              ) : null}
              {kernel.message ? (
                <p className="rounded-md bg-destructive/10 px-2 py-1.5 text-[10px] text-destructive">
                  {kernel.message}
                </p>
              ) : null}
            </CardContent>
          </Card>
        </div>
      </div>
    </div>
  );
}
