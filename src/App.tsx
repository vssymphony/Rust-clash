import { useEffect, useState } from "react";
import { TerminalSquare } from "lucide-react";
import { useAppStore } from "@/store/appStore";
import { Sidebar } from "@/components/layout/Sidebar";
import { LogPanel } from "@/components/layout/LogPanel";
import { Toaster } from "@/components/ui/toast";
import { StatusDot } from "@/components/common/primitives";
import { Button } from "@/components/ui/button";
import { OverviewPage } from "@/pages/Overview";
import { ProxiesPage } from "@/pages/Proxies";
import { ConnectionsPage } from "@/pages/Connections";
import { RulesPage } from "@/pages/Rules";
import { SubscriptionsPage } from "@/pages/Subscriptions";
import { SettingsPage } from "@/pages/Settings";

type PageKey = "overview" | "proxies" | "connections" | "rules" | "subscriptions" | "settings";

function renderPage(page: PageKey) {
  switch (page) {
    case "proxies":
      return <ProxiesPage />;
    case "connections":
      return <ConnectionsPage />;
    case "rules":
      return <RulesPage />;
    case "subscriptions":
      return <SubscriptionsPage />;
    case "settings":
      return <SettingsPage />;
    case "overview":
    default:
      return <OverviewPage />;
  }
}

export default function App() {
  const page = useAppStore((state) => state.page) as PageKey;
  const booting = useAppStore((state) => state.booting);
  const kernel = useAppStore((state) => state.kernel);
  const bootstrap = useAppStore((state) => state.bootstrap);
  const [logsOpen, setLogsOpen] = useState(false);

  useEffect(() => {
    void bootstrap();
  }, [bootstrap]);

  return (
    <div className="flex h-screen w-screen flex-col overflow-hidden bg-background text-foreground">
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <main className="flex min-w-0 flex-1 flex-col">
          <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4 lg:px-6">
            {booting ? (
              <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
                正在初始化…
              </div>
            ) : (
              renderPage(page)
            )}
          </div>

          <footer className="flex h-8 shrink-0 items-center justify-between border-t border-border/60 px-3 text-[10px] text-muted-foreground">
            <div className="flex items-center gap-2">
              <StatusDot
                tone={kernel.kind === "running" ? "success" : "muted"}
                pulse={kernel.kind === "running"}
              />
              <span>内核：{kernel.kind}</span>
              {kernel.version ? (
                <span className="text-muted-foreground/70">{kernel.version}</span>
              ) : null}
            </div>
            <div className="flex items-center gap-1">
              <Button
                variant="ghost"
                size="sm"
                onClick={() => setLogsOpen((prev) => !prev)}
                className="h-6 px-2 text-[10px]"
              >
                <TerminalSquare className="h-3 w-3" />
                {logsOpen ? "收起日志" : "查看日志"}
              </Button>
            </div>
          </footer>
        </main>
      </div>

      <LogPanel open={logsOpen} onClose={() => setLogsOpen(false)} />
      <Toaster />
    </div>
  );
}
