import { useEffect, useRef, useState } from "react";
import { api, onEvent, toErrorMessage } from "@/lib/api";
import type { LogEntry } from "@/lib/types";
import { cn } from "@/lib/utils";
import { X } from "lucide-react";
import { Button } from "@/components/ui/button";

const MAX_LINES = 500;

const LEVEL_CLASS: Record<string, string> = {
  error: "text-destructive",
  warning: "text-warning",
  info: "text-info",
  debug: "text-muted-foreground",
};

function formatTime(ts: number): string {
  const date = new Date(ts);
  const pad = (value: number) => value.toString().padStart(2, "0");
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

export function LogPanel({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    let disposed = false;

    void api
      .kernelLogs(MAX_LINES)
      .then((items) => {
        if (!disposed) setLogs(items.slice(-MAX_LINES));
      })
      .catch((err) => {
        if (!disposed) setError(toErrorMessage(err));
      });

    const unlistenPromise = onEvent("clash://log", (entry) => {
      setLogs((prev) => {
        const next = [...prev, entry];
        return next.length > MAX_LINES ? next.slice(next.length - MAX_LINES) : next;
      });
    });

    return () => {
      disposed = true;
      void unlistenPromise.then((unlisten) => unlisten()).catch(() => undefined);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const node = scrollRef.current;
    if (node) node.scrollTop = node.scrollHeight;
  }, [logs, open]);

  if (!open) return null;

  return (
    <div className="flex h-56 shrink-0 flex-col border-t border-border/70 bg-black/40">
      <div className="flex items-center justify-between border-b border-border/60 px-3 py-1.5">
        <p className="text-[11px] font-medium text-muted-foreground">内核日志</p>
        <div className="flex items-center gap-1">
          <Button variant="ghost" size="sm" onClick={() => setLogs([])}>
            清空
          </Button>
          <Button variant="ghost" size="icon-sm" aria-label="关闭日志" onClick={onClose}>
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>
      <div
        ref={scrollRef}
        className="min-h-0 flex-1 overflow-y-auto px-3 py-2 font-mono text-[11px]"
      >
        {error ? <p className="text-destructive">{error}</p> : null}
        {logs.length === 0 && !error ? (
          <p className="text-muted-foreground">暂无日志输出，启动内核后会实时显示。</p>
        ) : null}
        {logs.map((entry, index) => (
          <div key={`${entry.time}-${index}`} className="flex gap-2 leading-5">
            <span className="shrink-0 text-muted-foreground/70">{formatTime(entry.time)}</span>
            <span
              className={cn(
                "w-14 shrink-0 uppercase",
                LEVEL_CLASS[entry.level] ?? "text-muted-foreground",
              )}
            >
              {entry.level}
            </span>
            <span className="shrink-0 text-muted-foreground/60">
              {entry.source === "kernel" ? "mihomo" : "app"}
            </span>
            <span className="min-w-0 break-all">{entry.message}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
