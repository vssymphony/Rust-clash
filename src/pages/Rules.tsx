import { useMemo, useState } from "react";
import { ListTree, RefreshCw, Search } from "lucide-react";
import { toErrorMessage } from "@/lib/api";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/store/appStore";
import { useRules } from "@/hooks/useClashData";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/input";
import { Skeleton } from "@/components/ui/switch";
import { EmptyState, PageHeader } from "@/components/common/primitives";

export function RulesPage() {
  const kernel = useAppStore((state) => state.kernel);
  const running = kernel.kind === "running";
  const { data, isLoading, isFetching, refetch, error } = useRules(running);
  const [keyword, setKeyword] = useState("");
  const [typeFilter, setTypeFilter] = useState("all");

  // 同 Connections：固定兜底空数组引用，避免 types / filtered 的 useMemo 每渲染重建
  const rules = useMemo(() => data?.rules ?? [], [data]);

  const types = useMemo(() => {
    const set = new Set<string>();
    for (const rule of rules) if (rule.type) set.add(rule.type);
    return Array.from(set).sort();
  }, [rules]);

  const filtered = useMemo(() => {
    const term = keyword.trim().toLowerCase();
    return rules.filter((rule) => {
      const matchType = typeFilter === "all" || rule.type === typeFilter;
      if (!matchType) return false;
      if (!term) return true;
      return (
        rule.payload.toLowerCase().includes(term) ||
        rule.proxy.toLowerCase().includes(term) ||
        rule.type.toLowerCase().includes(term)
      );
    });
  }, [rules, keyword, typeFilter]);

  return (
    <div className="space-y-4">
      <PageHeader
        title="规则"
        description={`共 ${rules.length} 条规则，当前显示 ${filtered.length} 条`}
        actions={
          <>
            <div className="relative">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={keyword}
                onChange={(event) => setKeyword(event.target.value)}
                placeholder="搜索域名 / IP / 策略"
                className="h-8 w-56 pl-8 text-xs"
              />
            </div>
            <Select
              value={typeFilter}
              onChange={(event) => setTypeFilter(event.target.value)}
              className="h-8 w-32 text-xs"
            >
              <option value="all">全部类型</option>
              {types.map((type) => (
                <option key={type} value={type}>
                  {type}
                </option>
              ))}
            </Select>
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

      {!running ? (
        <EmptyState title="内核未运行" description="启动内核后可查看当前生效的规则列表。" />
      ) : isLoading ? (
        <div className="space-y-2">
          {[0, 1, 2, 3].map((index) => (
            <Skeleton key={index} className="h-10 w-full" />
          ))}
        </div>
      ) : filtered.length === 0 ? (
        <EmptyState
          icon={<ListTree className="h-6 w-6" />}
          title="没有匹配的规则"
          description="调整关键词或类型筛选后重试。"
        />
      ) : (
        <Card>
          <CardContent className="p-0">
            <div className="max-h-[calc(100vh-260px)] overflow-auto">
              <table className="w-full text-left text-xs">
                <thead className="sticky top-0 z-10 bg-card/95 backdrop-blur">
                  <tr className="border-b border-border/70 text-[10px] uppercase tracking-wide text-muted-foreground">
                    <th className="w-12 px-3 py-2 font-medium">#</th>
                    <th className="w-32 px-3 py-2 font-medium">类型</th>
                    <th className="px-3 py-2 font-medium">匹配内容</th>
                    <th className="w-48 px-3 py-2 font-medium">策略</th>
                  </tr>
                </thead>
                <tbody>
                  {filtered.map((rule, index) => (
                    <tr
                      key={`${rule.type}-${rule.payload}-${index}`}
                      className="border-b border-border/40 transition-colors last:border-0 hover:bg-white/[0.02]"
                    >
                      <td className="px-3 py-2 tabular-nums text-muted-foreground">{index + 1}</td>
                      <td className="px-3 py-2">
                        <Badge variant="outline">{rule.type}</Badge>
                      </td>
                      <td className="max-w-[520px] truncate px-3 py-2 font-mono text-[11px]">
                        {rule.payload || "-"}
                      </td>
                      <td className="max-w-[220px] truncate px-3 py-2 text-[11px] text-primary/90">
                        {rule.proxy}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  );
}
