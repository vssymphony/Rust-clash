import { useMemo, useState } from "react";
import { Loader2, RefreshCw, Search, Zap } from "lucide-react";
import { toErrorMessage } from "@/lib/api";
import { delayTone, formatDelay } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/store/appStore";
import { useGroupDelayTest, useProxies, useSelectProxy } from "@/hooks/useClashData";
import type { ProxyNode } from "@/lib/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/switch";
import { toast } from "@/components/ui/toast";
import { EmptyState, PageHeader } from "@/components/common/primitives";

const GROUP_TYPES = new Set(["Selector", "URLTest", "Fallback", "LoadBalance", "Relay", "Smart"]);

const TONE_CLASS: Record<ReturnType<typeof delayTone>, string> = {
  none: "text-muted-foreground/60",
  good: "text-success",
  fair: "text-warning",
  poor: "text-orange-400",
  bad: "text-destructive",
};

function resolveDelay(node?: ProxyNode): number | null {
  if (!node) return null;
  if (typeof node.delay === "number" && node.delay > 0) return node.delay;
  const history = node.history;
  if (history && history.length > 0) {
    const last = history[history.length - 1];
    if (last && last.delay > 0) return last.delay;
  }
  return null;
}

function isGroup(node: ProxyNode): boolean {
  return GROUP_TYPES.has(node.type) || (Array.isArray(node.all) && node.all.length > 0);
}

export function ProxiesPage() {
  const kernel = useAppStore((state) => state.kernel);
  const settings = useAppStore((state) => state.settings);
  const running = kernel.kind === "running";
  const { data, isLoading, isFetching, refetch, error } = useProxies(running);
  const selectProxy = useSelectProxy();
  const groupDelay = useGroupDelayTest();
  const [keyword, setKeyword] = useState("");
  const [testingGroup, setTestingGroup] = useState<string | null>(null);

  const groups = useMemo(() => {
    const proxies = data?.proxies ?? {};
    return Object.values(proxies)
      .filter((node) => isGroup(node) && node.all && node.all.length > 0 && !node.hidden)
      .sort((a, b) => (a.name > b.name ? 1 : -1));
  }, [data]);

  const filteredGroups = useMemo(() => {
    const term = keyword.trim().toLowerCase();
    if (!term) return groups;
    return groups
      .map((group) => ({
        ...group,
        all: (group.all ?? []).filter((name) => name.toLowerCase().includes(term)),
      }))
      .filter((group) => group.all.length > 0 || group.name.toLowerCase().includes(term));
  }, [groups, keyword]);

  const handleSelect = async (group: string, name: string) => {
    try {
      await selectProxy.mutateAsync({ group, name });
      toast.success("已切换节点", `${group} → ${name}`);
    } catch (err) {
      toast.error("切换节点失败", toErrorMessage(err));
    }
  };

  const handleTestGroup = async (group: string) => {
    setTestingGroup(group);
    try {
      const results = await groupDelay.mutateAsync({
        group,
        url: settings?.latencyTestUrl,
      });
      const ok = results.filter((item) => !item.error && item.delay > 0).length;
      toast.success("测速完成", `${group}：${ok}/${results.length} 个节点可用`);
    } catch (err) {
      toast.error("测速失败", toErrorMessage(err));
    } finally {
      setTestingGroup(null);
    }
  };

  if (!running) {
    return (
      <div className="space-y-4">
        <PageHeader title="代理" description="策略组与节点选择" />
        <EmptyState
          title="内核未运行"
          description="请先在左侧启动内核，代理组与节点信息将在启动后自动加载。"
        />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <PageHeader
        title="代理"
        description={`共 ${groups.length} 个策略组`}
        actions={
          <>
            <div className="relative">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={keyword}
                onChange={(event) => setKeyword(event.target.value)}
                placeholder="搜索节点"
                className="h-8 w-52 pl-8 text-xs"
              />
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void refetch()}
              disabled={isFetching}
            >
              <RefreshCw className={cn("h-3.5 w-3.5", isFetching && "animate-spin")} />
              刷新
            </Button>
          </>
        }
      />

      {error ? (
        <p className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {toErrorMessage(error)}
        </p>
      ) : null}

      {isLoading ? (
        <div className="space-y-3">
          {[0, 1, 2].map((index) => (
            <Skeleton key={index} className="h-32 w-full" />
          ))}
        </div>
      ) : filteredGroups.length === 0 ? (
        <EmptyState
          title="暂无策略组"
          description="当前配置中未找到策略组，请检查订阅配置是否包含 proxy-groups 字段。"
        />
      ) : (
        <div className="grid gap-3 2xl:grid-cols-2">
          {filteredGroups.map((group) => {
            const nodes = group.all ?? [];
            const current = group.now ?? "";
            const testing = testingGroup === group.name;
            return (
              <Card key={group.name}>
                <CardHeader className="flex-row items-center justify-between gap-2">
                  <div className="min-w-0 space-y-1">
                    <p className="truncate text-sm font-semibold">{group.name}</p>
                    <div className="flex items-center gap-2 text-[10px] text-muted-foreground">
                      <Badge variant="outline">{group.type}</Badge>
                      <span className="truncate">当前：{current || "-"}</span>
                    </div>
                  </div>
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={() => void handleTestGroup(group.name)}
                    disabled={testing || groupDelay.isPending}
                  >
                    {testing ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <Zap className="h-3.5 w-3.5" />
                    )}
                    测速
                  </Button>
                </CardHeader>
                <CardContent>
                  <div className="flex max-h-64 flex-wrap gap-1.5 overflow-y-auto pr-1">
                    {nodes.map((name) => {
                      const node = data?.proxies[name];
                      const delay = resolveDelay(node);
                      const active = name === current;
                      const nested = node ? isGroup(node) : false;
                      return (
                        <button
                          key={name}
                          type="button"
                          onClick={() => void handleSelect(group.name, name)}
                          disabled={selectProxy.isPending}
                          className={cn(
                            "flex max-w-full items-center gap-1.5 rounded-md border px-2 py-1 text-[11px] transition-colors",
                            active
                              ? "border-primary/50 bg-primary/12 text-primary"
                              : "border-border/70 bg-white/[0.02] text-muted-foreground hover:border-border hover:text-foreground",
                          )}
                          title={`${name}${nested ? "（策略组）" : ""}`}
                        >
                          {!nested ? (
                            <span
                              className={cn(
                                "h-1.5 w-1.5 shrink-0 rounded-full bg-current",
                                TONE_CLASS[delayTone(delay)],
                              )}
                            />
                          ) : null}
                          <span className="max-w-[180px] truncate">{name}</span>
                          {!nested ? (
                            <span
                              className={cn("shrink-0 tabular-nums", TONE_CLASS[delayTone(delay)])}
                            >
                              {formatDelay(delay)}
                            </span>
                          ) : null}
                        </button>
                      );
                    })}
                  </div>
                </CardContent>
              </Card>
            );
          })}
        </div>
      )}
    </div>
  );
}
