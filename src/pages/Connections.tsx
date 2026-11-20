import { useMemo, useState } from "react";
import { Ban, RefreshCw, Search, XCircle } from "lucide-react";
import { toErrorMessage } from "@/lib/api";
import { formatBytes, formatRfc3339 } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/store/appStore";
import { useCloseAllConnections, useCloseConnection, useConnections } from "@/hooks/useClashData";
import type { ConnectionItem } from "@/lib/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { toast } from "@/components/ui/toast";
import { EmptyState, PageHeader } from "@/components/common/primitives";

function networkLabel(item: ConnectionItem): string {
  const { network, type } = item.metadata;
  if (type === "Dns") return "DNS";
  return (network || type || "-").toUpperCase();
}

export function ConnectionsPage() {
  const kernel = useAppStore((state) => state.kernel);
  const storeConnections = useAppStore((state) => state.connections);
  const running = kernel.kind === "running";
  const { data: polled, isFetching, refetch, error } = useConnections();
  const closeOne = useCloseConnection();
  const closeAll = useCloseAllConnections();
  const [keyword, setKeyword] = useState("");
  const [confirmOpen, setConfirmOpen] = useState(false);

  const payload = storeConnections ?? polled;
  // 用 useMemo 固定兜底空数组的引用：否则每次渲染都产生新数组，
  // 会让下方 filtered 的 useMemo 依赖恒变、记忆化完全失效（react-hooks/exhaustive-deps）。
  const list = useMemo(() => payload?.connections ?? [], [payload]);

  const filtered = useMemo(() => {
    const term = keyword.trim().toLowerCase();
    if (!term) return list;
    return list.filter((item) => {
      const host = item.metadata.host || item.metadata.destinationIP;
      return (
        host.toLowerCase().includes(term) ||
        item.metadata.process?.toLowerCase().includes(term) ||
        item.rule.toLowerCase().includes(term) ||
        item.chains.join(" ").toLowerCase().includes(term)
      );
    });
  }, [list, keyword]);

  const handleClose = async (item: ConnectionItem) => {
    try {
      await closeOne.mutateAsync(item.id);
      toast.success("已关闭连接", item.metadata.host || item.metadata.destinationIP);
    } catch (err) {
      toast.error("关闭连接失败", toErrorMessage(err));
    }
  };

  const handleCloseAll = async () => {
    try {
      await closeAll.mutateAsync();
      setConfirmOpen(false);
      toast.success("已关闭全部连接");
    } catch (err) {
      toast.error("关闭连接失败", toErrorMessage(err));
    }
  };

  return (
    <div className="space-y-4">
      <PageHeader
        title="连接"
        description="实时连接明细，来自 mihomo /connections 接口"
        actions={
          <>
            <div className="relative">
              <Search className="pointer-events-none absolute left-2.5 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={keyword}
                onChange={(event) => setKeyword(event.target.value)}
                placeholder="搜索主机 / 进程 / 规则"
                className="h-8 w-60 pl-8 text-xs"
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
            <Button
              variant="destructive"
              size="sm"
              onClick={() => setConfirmOpen(true)}
              disabled={list.length === 0}
            >
              <Ban className="h-3.5 w-3.5" />
              全部关闭
            </Button>
          </>
        }
      />

      <div className="grid gap-3 sm:grid-cols-3">
        <Card>
          <CardContent className="p-4">
            <p className="text-[11px] uppercase text-muted-foreground">连接数</p>
            <p className="stat-value">{list.length}</p>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4">
            <p className="text-[11px] uppercase text-muted-foreground">会话下载</p>
            <p className="stat-value text-info">{formatBytes(payload?.downloadTotal ?? 0)}</p>
          </CardContent>
        </Card>
        <Card>
          <CardContent className="p-4">
            <p className="text-[11px] uppercase text-muted-foreground">会话上传</p>
            <p className="stat-value text-success">{formatBytes(payload?.uploadTotal ?? 0)}</p>
          </CardContent>
        </Card>
      </div>

      {error && !payload ? (
        <p className="rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
          {toErrorMessage(error)}
        </p>
      ) : null}

      {!running ? (
        <EmptyState title="内核未运行" description="启动内核后可查看实时连接。" />
      ) : filtered.length === 0 ? (
        <EmptyState
          icon={<XCircle className="h-6 w-6" />}
          title={keyword ? "没有匹配的连接" : "当前没有活动连接"}
          description={keyword ? "尝试其他关键词。" : "有流量经过内核时，连接会实时出现在这里。"}
        />
      ) : (
        <Card>
          <CardContent className="p-0">
            <div className="max-h-[calc(100vh-320px)] overflow-auto">
              <table className="w-full text-left text-xs">
                <thead className="sticky top-0 z-10 bg-card/95 backdrop-blur">
                  <tr className="border-b border-border/70 text-[10px] uppercase tracking-wide text-muted-foreground">
                    <th className="px-3 py-2 font-medium">主机</th>
                    <th className="px-3 py-2 font-medium">网络</th>
                    <th className="px-3 py-2 font-medium">链路</th>
                    <th className="px-3 py-2 font-medium">规则</th>
                    <th className="px-3 py-2 text-right font-medium">上行</th>
                    <th className="px-3 py-2 text-right font-medium">下行</th>
                    <th className="px-3 py-2 font-medium">开始</th>
                    <th className="px-3 py-2 text-right font-medium">操作</th>
                  </tr>
                </thead>
                <tbody>
                  {filtered.map((item) => (
                    <tr
                      key={item.id}
                      className="border-b border-border/40 transition-colors last:border-0 hover:bg-white/[0.02]"
                    >
                      <td className="max-w-[280px] px-3 py-2">
                        <p className="truncate font-medium">
                          {item.metadata.host || item.metadata.destinationIP || "-"}
                        </p>
                        <p className="truncate text-[10px] text-muted-foreground">
                          {item.metadata.destinationIP}
                          {item.metadata.destinationPort ? `:${item.metadata.destinationPort}` : ""}
                          {item.metadata.process ? ` · ${item.metadata.process}` : ""}
                        </p>
                      </td>
                      <td className="px-3 py-2">
                        <Badge variant="outline">{networkLabel(item)}</Badge>
                      </td>
                      <td className="max-w-[220px] px-3 py-2">
                        <p className="truncate text-[11px] text-muted-foreground">
                          {item.chains.length > 0 ? item.chains.join(" → ") : "DIRECT"}
                        </p>
                      </td>
                      <td className="max-w-[180px] px-3 py-2">
                        <p className="truncate text-[11px] text-muted-foreground">
                          {item.rule}
                          {item.rulePayload ? `(${item.rulePayload})` : ""}
                        </p>
                      </td>
                      <td className="px-3 py-2 text-right tabular-nums text-success">
                        {formatBytes(item.upload)}
                      </td>
                      <td className="px-3 py-2 text-right tabular-nums text-info">
                        {formatBytes(item.download)}
                      </td>
                      <td className="px-3 py-2 tabular-nums text-muted-foreground">
                        {formatRfc3339(item.start)}
                      </td>
                      <td className="px-3 py-2 text-right">
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          aria-label="关闭连接"
                          onClick={() => void handleClose(item)}
                        >
                          <XCircle className="h-3.5 w-3.5 text-muted-foreground hover:text-destructive" />
                        </Button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </CardContent>
        </Card>
      )}

      <Dialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title="关闭全部连接"
        description={`将断开当前 ${list.length} 条活动连接，已建立的 TCP 会话会被立即中断。`}
        footer={
          <>
            <Button variant="ghost" size="sm" onClick={() => setConfirmOpen(false)}>
              取消
            </Button>
            <Button
              variant="destructive"
              size="sm"
              onClick={() => void handleCloseAll()}
              disabled={closeAll.isPending}
            >
              确认关闭
            </Button>
          </>
        }
      />
    </div>
  );
}
