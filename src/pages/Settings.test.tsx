// Settings 页 TUN 区块测试：验证渲染分支、开关交互与错误降级路径。
// 依赖全部用 vi.mock 打桩，不启动 Tauri、不访问网络。
import { describe, expect, it, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { TunState } from "@/lib/types";

const mocks = vi.hoisted(() => {
  const settings = {
    mixedPort: 7897,
    controllerPort: 9097,
    controllerSecret: "",
    mode: "rule",
    logLevel: "info",
    allowLan: false,
    systemProxy: false,
    autoStart: false,
    startMinimized: false,
    subUpdateIntervalMinutes: 0,
    latencyTestUrl: "https://www.gstatic.com/generate_204",
    currentSubscriptionId: null,
    kernelPath: null,
    closeConnectionsOnSwitch: true,
    tunEnabled: false,
    tunStack: "gvisor",
    tunDeviceName: null,
    tunAutoRoute: true,
    tunAutoDetectInterface: true,
    tunStrictRoute: false,
    tunDnsHijack: ["any:53"],
  };
  const kernel = {
    kind: "stopped",
    pid: null,
    message: null,
    restartCount: 0,
    startedAt: null,
    version: null,
  };
  const snapshot = {
    kernel,
    settings,
    systemProxy: { enabled: false, backend: "unsupported", message: null },
    subscriptions: [],
    appVersion: "0.1.0",
    configPath: "C:/Users/demo/.config/clash-gui",
    dataDir: "C:/Users/demo/.local/share/clash-gui",
    platform: "windows",
  };
  return {
    snapshot,
    state: {
      settings,
      systemProxy: null,
      snapshot,
      kernel,
      traffic: { up: 0, down: 0, upTotal: 0, downTotal: 0 },
      refreshSnapshot: vi.fn(async () => undefined),
    },
    tunStatus: vi.fn(),
    tunToggle: vi.fn(),
    reloadConfig: vi.fn(async () => undefined),
    setSystemProxy: vi.fn(),
    setAutostart: vi.fn(),
    openConfigDir: vi.fn(async () => undefined),
    updateSettings: vi.fn(async (value: unknown) => value),
    toast: { success: vi.fn(), error: vi.fn(), warning: vi.fn() },
  };
});

vi.mock("@/store/appStore", () => ({
  useAppStore: (selector: (state: unknown) => unknown) => selector(mocks.state),
}));

vi.mock("@/hooks/useClashData", () => ({
  useUpdateSettings: () => ({ mutateAsync: mocks.updateSettings, isPending: false }),
}));

vi.mock("@/lib/api", () => ({
  isTauriRuntime: () => true,
  toErrorMessage: (error: unknown) =>
    typeof error === "string" ? error : String((error as Error)?.message ?? error),
  api: {
    getAppState: vi.fn(),
    updateSettings: mocks.updateSettings,
    kernelStart: vi.fn(),
    kernelStop: vi.fn(),
    kernelRestart: vi.fn(),
    kernelStatus: vi.fn(),
    kernelLogs: vi.fn(),
    openConfigDir: mocks.openConfigDir,
    reloadConfig: mocks.reloadConfig,
    getSystemProxy: vi.fn(),
    setSystemProxy: mocks.setSystemProxy,
    getAutostart: vi.fn(),
    setAutostart: mocks.setAutostart,
    tunStatus: mocks.tunStatus,
    tunCheckPrivilege: vi.fn(),
    tunToggle: mocks.tunToggle,
  },
}));

vi.mock("@/components/ui/toast", () => ({
  toast: mocks.toast,
  ToastProvider: ({ children }: { children: unknown }) => children,
}));

// `vi.mock` 会被提升到 import 之前，因此这里可以安全地静态导入被测组件
import { SettingsPage } from "./Settings";

function makeTunState(overrides: Partial<TunState> = {}): TunState {
  return {
    supported: true,
    enabled: false,
    active: false,
    stack: "gvisor",
    deviceName: "Mihomo",
    privilege: {
      supported: true,
      platform: "windows",
      elevated: true,
      mechanism: "administrator",
      wintunPresent: true,
      detail: null,
      hint: null,
    },
    message: null,
    ...overrides,
  };
}

beforeEach(() => {
  mocks.tunStatus.mockReset();
  mocks.tunToggle.mockReset();
  mocks.toast.success.mockReset();
  mocks.toast.error.mockReset();
  mocks.toast.warning.mockReset();
  mocks.tunStatus.mockResolvedValue(makeTunState());
});

describe("Settings TUN 区块", () => {
  it("渲染 TUN 卡片并展示探测到的状态与权限", async () => {
    mocks.tunStatus.mockResolvedValue(makeTunState({ enabled: true, active: true }));

    render(<SettingsPage />);

    expect(screen.getByText("TUN 模式")).toBeInTheDocument();
    expect(screen.getByText("启用 TUN")).toBeInTheDocument();
    // 状态徽标：已生效
    expect(await screen.findByText("已生效")).toBeInTheDocument();
    expect(screen.getByText("已提权（administrator）")).toBeInTheDocument();
    expect(screen.getByText("Mihomo")).toBeInTheDocument();
    expect(screen.getByText("已找到")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "启用 TUN" })).toBeChecked();
  });

  it("探测返回未启用时徽标显示未启用且开关关闭", async () => {
    render(<SettingsPage />);
    expect(await screen.findByText("未启用")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "启用 TUN" })).not.toBeChecked();
  });

  it("点击开关调用 tunToggle 并弹出成功提示", async () => {
    mocks.tunToggle.mockResolvedValue(makeTunState({ enabled: true, active: true }));
    render(<SettingsPage />);
    await screen.findByText("未启用");

    fireEvent.click(screen.getByRole("switch", { name: "启用 TUN" }));

    await waitFor(() => expect(mocks.tunToggle).toHaveBeenCalledWith(true));
    expect(mocks.toast.success).toHaveBeenCalledWith("TUN 模式已开启");
  });

  it("存在降级说明时用 warning 提示而不是 success", async () => {
    mocks.tunToggle.mockResolvedValue(
      makeTunState({
        enabled: true,
        active: false,
        message: "wintun.dll 未找到，已回落到系统栈代理",
      }),
    );
    render(<SettingsPage />);
    await screen.findByText("未启用");

    fireEvent.click(screen.getByRole("switch", { name: "启用 TUN" }));

    await waitFor(() =>
      expect(mocks.toast.warning).toHaveBeenCalledWith(
        "TUN 已触发，但存在降级",
        "wintun.dll 未找到，已回落到系统栈代理",
      ),
    );
    expect(mocks.toast.success).not.toHaveBeenCalled();
  });

  it("tunToggle 失败时提示错误并重新探测一次状态", async () => {
    mocks.tunToggle.mockRejectedValue("权限不足：需要管理员权限");
    render(<SettingsPage />);
    await screen.findByText("未启用");
    expect(mocks.tunStatus).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("switch", { name: "启用 TUN" }));

    await waitFor(() =>
      expect(mocks.toast.error).toHaveBeenCalledWith("TUN 操作失败", "权限不足：需要管理员权限"),
    );
    // 失败后应重新拉取后端状态，避免 UI 与内核真实状态不一致
    await waitFor(() => expect(mocks.tunStatus).toHaveBeenCalledTimes(2));
  });

  it("平台不支持 TUN 时开关被禁用", async () => {
    mocks.tunStatus.mockResolvedValue(
      makeTunState({
        supported: false,
        privilege: {
          supported: false,
          platform: "macos",
          elevated: false,
          mechanism: "unknown",
          wintunPresent: null,
          detail: null,
          hint: "当前平台尚未实现 TUN 接管",
        },
      }),
    );
    render(<SettingsPage />);

    expect(await screen.findByText("当前平台尚未实现 TUN 接管")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "启用 TUN" })).toBeDisabled();
  });
});
