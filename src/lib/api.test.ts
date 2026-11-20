// api.ts 单元测试：只验证「前端 → 后端」的命令名与参数形状，不触达真实 Tauri 运行时。
// 命令名写错是这里最容易漏、也最容易在真机上报 "command not found" 的缺陷，故逐条断言。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { api, isTauriRuntime, onEvent, toErrorMessage } from "./api";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

const invokeMock = vi.mocked(invoke);
const listenMock = vi.mocked(listen);

function enterTauriRuntime() {
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    value: { invoke: () => undefined },
    configurable: true,
    writable: true,
  });
}

beforeEach(() => {
  invokeMock.mockResolvedValue(undefined as never);
});

afterEach(() => {
  invokeMock.mockReset();
  listenMock.mockReset();
});

describe("isTauriRuntime", () => {
  it("浏览器环境下返回 false", () => {
    expect(isTauriRuntime()).toBe(false);
  });

  it("注入 __TAURI_INTERNALS__ 后返回 true", () => {
    enterTauriRuntime();
    expect(isTauriRuntime()).toBe(true);
  });
});

describe("api 命令名与参数", () => {
  it("非 Tauri 环境直接抛出可读错误，且不调用 invoke", async () => {
    await expect(api.getAppState()).rejects.toThrow(/不在 Tauri 运行时/);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("无参命令只传命令名", async () => {
    enterTauriRuntime();
    await api.getAppState();
    expect(invokeMock).toHaveBeenCalledWith("get_app_state", undefined);

    await api.kernelStop();
    expect(invokeMock).toHaveBeenCalledWith("kernel_stop", undefined);
  });

  it("updateSettings 使用 settings 键名（Rust 侧参数名一致）", async () => {
    enterTauriRuntime();
    const settings = { mixedPort: 7897 } as never;
    await api.updateSettings(settings);
    expect(invokeMock).toHaveBeenCalledWith("update_settings", { settings });
  });

  it("可选参数被规范化为 null，避免 Rust 侧 Option 反序列化失败", async () => {
    enterTauriRuntime();
    await api.kernelStart();
    expect(invokeMock).toHaveBeenCalledWith("kernel_start", { kernelPath: null });

    await api.kernelStart("D:/app/mihomo.exe");
    expect(invokeMock).toHaveBeenLastCalledWith("kernel_start", {
      kernelPath: "D:/app/mihomo.exe",
    });

    await api.proxyDelay("节点A");
    expect(invokeMock).toHaveBeenLastCalledWith("proxy_delay", {
      name: "节点A",
      url: null,
      timeout: null,
    });
  });

  it("TUN 相关命令与参数名保持不变", async () => {
    enterTauriRuntime();
    await api.tunStatus();
    expect(invokeMock).toHaveBeenCalledWith("tun_status", undefined);

    await api.tunCheckPrivilege();
    expect(invokeMock).toHaveBeenLastCalledWith("tun_check_privilege", undefined);

    await api.tunToggle(true);
    expect(invokeMock).toHaveBeenLastCalledWith("tun_toggle", { enabled: true });
  });

  it("后端错误原样上抛，供 UI 展示", async () => {
    enterTauriRuntime();
    invokeMock.mockRejectedValueOnce("内核未运行");
    await expect(api.kernelRestart()).rejects.toBe("内核未运行");
  });
});

describe("onEvent", () => {
  it("解包 event payload 后交给业务回调", async () => {
    const unlisten = vi.fn();
    let captured: ((event: { payload: unknown }) => void) | undefined;
    listenMock.mockImplementation(async (_name, handler) => {
      captured = handler as unknown as (event: { payload: unknown }) => void;
      return unlisten;
    });

    const handler = vi.fn();
    const result = await onEvent("clash://kernel-status", handler);
    expect(listenMock).toHaveBeenCalledWith("clash://kernel-status", expect.any(Function));

    captured?.({ payload: { kind: "running" } });
    expect(handler).toHaveBeenCalledWith({ kind: "running" });
    expect(result).toBe(unlisten);
  });
});

describe("toErrorMessage", () => {
  it("覆盖字符串、Error、类错误对象与兜底值", () => {
    expect(toErrorMessage("后端直接返回的字符串")).toBe("后端直接返回的字符串");
    expect(toErrorMessage(new Error("boom"))).toBe("boom");
    expect(toErrorMessage({ message: "结构体错误" })).toBe("结构体错误");
    expect(toErrorMessage(500)).toBe("500");
    expect(toErrorMessage(null)).toBe("null");
    expect(toErrorMessage(undefined)).toBe("undefined");
  });
});
