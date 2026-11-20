import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { CheckCircle2, Copy, Download, FileUp, Plus, RefreshCw, Trash2 } from "lucide-react";
import { toErrorMessage } from "@/lib/api";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/store/appStore";
import {
  useActivateSubscription,
  useAddSubscription,
  useRemoveSubscription,
  useSubscriptions,
  useUpdateAllSubscriptions,
  useUpdateSubscription,
} from "@/hooks/useClashData";
import type { Subscription } from "@/lib/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Dialog } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { toast } from "@/components/ui/toast";
import { EmptyState, PageHeader } from "@/components/common/primitives";

function formatTime(ts: number): string {
  if (!ts) return "-";
  return new Date(ts).toLocaleString("zh-CN", { hour12: false });
}

export function SubscriptionsPage() {
  const settings = useAppStore((state) => state.settings);
  const { data = [], isLoading, isFetching, refetch, error } = useSubscriptions();
  const addSub = useAddSubscription();
  const removeSub = useRemoveSubscription();
  const updateSub = useUpdateSubscription();
  const updateAll = useUpdateAllSubscriptions();
  const activate = useActivateSubscription();

  const [addOpen, setAddOpen] = useState(false);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [pendingDelete, setPendingDelete] = useState<Subscription | null>(null);

  const resetForm = () => {
    setName("");
    setUrl("");
  };

  const submitAdd = async () => {
    if (!name.trim() || !url.trim()) {
      toast.error("请填写订阅名称与链接");
      return;
    }
    try {
      await addSub.mutateAsync({ name: name.trim(), url: url.trim() });
      toast.success("订阅已添加", name.trim());
      setAddOpen(false);
      resetForm();
    } catch (err) {
      toast.error("添加订阅失败", toErrorMessage(err));
    }
  };

  const importFile = async () => {
    try {
      const selected = await openDialog({
        multiple: false,
        filters: [{ name: "配置文件", extensions: ["yaml", "yml", "txt"] }],
      });
      if (!selected || Array.isArray(selected)) return;
      const fileName = selected.split(/[\\/]/).pop() ?? "本地订阅";
      await addSub.mutateAsync({
        name: fileName.replace(/\.(ya?ml|txt)$/i, ""),
        filePath: selected,
      });
      toast.success("本地配置已导入", fileName);
    } catch (err) {
      toast.error("导入失败", toErrorMessage(err));
    }
  };

  const handleUpdate = async (sub: Subscription) => {
    try {
      await updateSub.mutateAsync(sub.id);
      toast.success("订阅已更新", sub.name);
    } catch (err) {
      toast.error("更新失败", toErrorMessage(err));
    }
  };

  const handleActivate = async (sub: Subscription) => {
    try {
      await activate.mutateAsync(sub.id);
      toast.success("已切换订阅", sub.name);
    } catch (err) {
      toast.error("切换失败", toErrorMessage(err));
    }
  };

  const handleDelete = async () => {
    if (!pendingDelete) return;
    try {
      await removeSub.mutateAsync(pendingDelete.id);
      toast.success("订阅已删除", pendingDelete.name);
      setPendingDelete(null);
    } catch (err) {
      toast.error("删除失败", toErrorMessage(err));
    }
  };

  const handleUpdateAll = async () => {
    try {
      const result = await updateAll.mutateAsync();
      toast.success("全部订阅已更新", `共 ${result.length} 个`);
    } catch (err) {
      toast.error("批量更新失败", toErrorMessage(err));
    }
  };

  const copyUrl = async (sub: Subscription) => {
    if (!sub.url) return;
    try {
      await navigator.clipboard.writeText(sub.url);
      toast.success("订阅链接已复制");
    } catch {
      toast.error("复制失败", "当前环境不支持剪贴板");
    }
  };

  return (
    <div className="space-y-4">
      <PageHeader
        title="订阅"
        description={`共 ${data.length} 个订阅，当前生效：${
          data.find((item) => item.id === settings?.currentSubscriptionId)?.name ?? "无"
        }`}
        actions={
          <>
            <Button variant="outline" size="sm" onClick={() => void importFile()}>
              <FileUp className="h-3.5 w-3.5" />
              导入本地
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void handleUpdateAll()}
              disabled={updateAll.isPending || data.length === 0}
            >
              <Download className={cn("h-3.5 w-3.5", updateAll.isPending && "animate-pulse")} />
              全部更新
            </Button>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void refetch()}
              disabled={isFetching}
            >
              <RefreshCw className={cn("h-3.5 w-3.5", isFetching && "animate-spin")} />
              刷新
            </Button>
            <Button size="sm" onClick={() => setAddOpen(true)}>
              <Plus className="h-3.5 w-3.5" />
              添加订阅
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
        <p className="text-xs text-muted-foreground">加载中…</p>
      ) : data.length === 0 ? (
        <EmptyState
          title="还没有订阅"
          description="添加远程订阅链接，或导入本地的 Clash / mihomo YAML 配置文件。"
          action={
            <Button size="sm" onClick={() => setAddOpen(true)}>
              <Plus className="h-3.5 w-3.5" />
              添加订阅
            </Button>
          }
        />
      ) : (
        <div className="grid gap-3 xl:grid-cols-2">
          {data.map((sub) => {
            const active = sub.id === settings?.currentSubscriptionId;
            return (
              <Card
                key={sub.id}
                className={cn(
                  active && "border-primary/45 shadow-[0_0_0_1px_hsl(var(--primary)/0.25)]",
                )}
              >
                <CardHeader className="flex-row items-start justify-between gap-2">
                  <div className="min-w-0 space-y-1">
                    <div className="flex items-center gap-2">
                      <CardTitle className="truncate text-sm">{sub.name}</CardTitle>
                      {active ? (
                        <Badge variant="success">
                          <CheckCircle2 className="mr-1 h-3 w-3" />
                          生效中
                        </Badge>
                      ) : null}
                    </div>
                    <p className="truncate text-[10px] text-muted-foreground">
                      {sub.url ?? (sub.filePath ? `本地文件：${sub.filePath}` : "本地配置")}
                    </p>
                  </div>
                  <Badge variant="secondary">{sub.nodeCount} 节点</Badge>
                </CardHeader>
                <CardContent className="space-y-3">
                  <div className="flex items-center justify-between text-[10px] text-muted-foreground">
                    <span>更新于 {formatTime(sub.updatedAt)}</span>
                    {sub.userInfo ? (
                      <span className="max-w-[50%] truncate" title={sub.userInfo}>
                        {sub.userInfo}
                      </span>
                    ) : null}
                  </div>
                  <div className="flex flex-wrap gap-1.5">
                    <Button
                      size="sm"
                      variant={active ? "secondary" : "default"}
                      onClick={() => void handleActivate(sub)}
                      disabled={active || activate.isPending}
                    >
                      使用
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => void handleUpdate(sub)}
                      disabled={updateSub.isPending}
                    >
                      <Download className="h-3.5 w-3.5" />
                      更新
                    </Button>
                    {sub.url ? (
                      <Button size="sm" variant="ghost" onClick={() => void copyUrl(sub)}>
                        <Copy className="h-3.5 w-3.5" />
                        复制链接
                      </Button>
                    ) : null}
                    <Button
                      size="sm"
                      variant="ghost"
                      className="text-destructive hover:text-destructive"
                      onClick={() => setPendingDelete(sub)}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                      删除
                    </Button>
                  </div>
                </CardContent>
              </Card>
            );
          })}
        </div>
      )}

      <Dialog
        open={addOpen}
        onOpenChange={(next) => {
          setAddOpen(next);
          if (!next) resetForm();
        }}
        title="添加订阅"
        description="填写订阅名称与远程链接，保存后会自动下载并解析节点。"
        footer={
          <>
            <Button variant="ghost" size="sm" onClick={() => setAddOpen(false)}>
              取消
            </Button>
            <Button size="sm" onClick={() => void submitAdd()} disabled={addSub.isPending}>
              保存并下载
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <div className="space-y-1.5">
            <label className="text-[11px] text-muted-foreground">订阅名称</label>
            <Input
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="例如：机场 A"
            />
          </div>
          <div className="space-y-1.5">
            <label className="text-[11px] text-muted-foreground">订阅链接</label>
            <Input
              value={url}
              onChange={(event) => setUrl(event.target.value)}
              placeholder="https://example.com/api/v1/client/subscribe?token=..."
            />
          </div>
        </div>
      </Dialog>

      <Dialog
        open={pendingDelete !== null}
        onOpenChange={(next) => {
          if (!next) setPendingDelete(null);
        }}
        title="删除订阅"
        description={`将删除订阅「${pendingDelete?.name ?? ""}」及其本地配置缓存，该操作不可撤销。`}
        footer={
          <>
            <Button variant="ghost" size="sm" onClick={() => setPendingDelete(null)}>
              取消
            </Button>
            <Button
              variant="destructive"
              size="sm"
              onClick={() => void handleDelete()}
              disabled={removeSub.isPending}
            >
              确认删除
            </Button>
          </>
        }
      />
    </div>
  );
}
