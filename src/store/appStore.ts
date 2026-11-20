import { create } from "zustand";
import { api, onEvent, toErrorMessage } from "@/lib/api";
import type {
  AppSettings,
  AppStateSnapshot,
  ConnectionsResponse,
  KernelStatus,
  SystemProxyState,
  TrafficSample,
} from "@/lib/types";

export const MAX_TRAFFIC_POINTS = 120;

export interface TrafficPoint {
  t: number;
  up: number;
  down: number;
}

export type PageKey =
  "overview" | "proxies" | "connections" | "rules" | "subscriptions" | "settings";

interface AppStore {
  page: PageKey;
  snapshot: AppStateSnapshot | null;
  kernel: KernelStatus;
  settings: AppSettings | null;
  systemProxy: SystemProxyState | null;
  subscriptions: AppStateSnapshot["subscriptions"];
  traffic: TrafficSample;
  trafficHistory: TrafficPoint[];
  connections: ConnectionsResponse | null;
  booting: boolean;
  offline: boolean;
  error: string | null;

  setPage: (page: PageKey) => void;
  bootstrap: () => Promise<void>;
  refreshSnapshot: () => Promise<void>;
  setKernel: (kernel: KernelStatus) => void;
  setSystemProxy: (state: SystemProxyState) => void;
  setSettings: (settings: AppSettings) => void;
  pushTraffic: (sample: TrafficSample) => void;
  setConnections: (payload: ConnectionsResponse) => void;
  clearHistory: () => void;
}

const OFFLINE_KERNEL: KernelStatus = {
  kind: "stopped",
  pid: null,
  message: null,
  restartCount: 0,
  startedAt: null,
  version: null,
};

const EMPTY_TRAFFIC: TrafficSample = { up: 0, down: 0, upTotal: 0, downTotal: 0 };

export const useAppStore = create<AppStore>((set, get) => ({
  page: "overview",
  snapshot: null,
  kernel: OFFLINE_KERNEL,
  settings: null,
  systemProxy: null,
  subscriptions: [],
  traffic: EMPTY_TRAFFIC,
  trafficHistory: [],
  connections: null,
  booting: true,
  offline: false,
  error: null,

  setPage: (page) => set({ page }),

  bootstrap: async () => {
    try {
      const snapshot = await api.getAppState();
      set({
        snapshot,
        kernel: snapshot.kernel,
        settings: snapshot.settings,
        systemProxy: snapshot.systemProxy,
        subscriptions: snapshot.subscriptions,
        booting: false,
        offline: false,
        error: null,
      });
      if (snapshot.kernel.kind === "running") {
        try {
          const connections = await api.getConnections();
          set({ connections });
        } catch {
          // 内核刚启动时接口可能尚未就绪，忽略
        }
      }
    } catch (error) {
      set({
        booting: false,
        offline: true,
        error: toErrorMessage(error),
      });
    }
  },

  refreshSnapshot: async () => {
    try {
      const snapshot = await api.getAppState();
      set({
        snapshot,
        kernel: snapshot.kernel,
        settings: snapshot.settings,
        systemProxy: snapshot.systemProxy,
        subscriptions: snapshot.subscriptions,
        offline: false,
        error: null,
      });
    } catch (error) {
      set({ error: toErrorMessage(error) });
    }
  },

  setKernel: (kernel) => set({ kernel }),

  setSystemProxy: (systemProxy) => set({ systemProxy }),

  setSettings: (settings) =>
    set((state) => ({
      settings,
      snapshot: state.snapshot ? { ...state.snapshot, settings } : state.snapshot,
    })),

  pushTraffic: (sample) => {
    const point: TrafficPoint = { t: Date.now(), up: sample.up, down: sample.down };
    set((state) => {
      const history = [...state.trafficHistory, point];
      if (history.length > MAX_TRAFFIC_POINTS) {
        history.splice(0, history.length - MAX_TRAFFIC_POINTS);
      }
      return { traffic: sample, trafficHistory: history };
    });
  },

  setConnections: (connections) => set({ connections }),

  clearHistory: () => set({ trafficHistory: [], traffic: { ...get().traffic, up: 0, down: 0 } }),
}));

/** 注册后端事件订阅，返回取消订阅函数集合 */
export async function registerBackendEvents(): Promise<() => void> {
  const unlisteners: Array<() => void> = [];
  try {
    unlisteners.push(
      await onEvent("clash://kernel-status", (payload) => {
        useAppStore.getState().setKernel(payload);
      }),
    );
    unlisteners.push(
      await onEvent("clash://traffic", (payload) => {
        useAppStore.getState().pushTraffic(payload);
      }),
    );
    unlisteners.push(
      await onEvent("clash://connections", (payload) => {
        useAppStore.getState().setConnections(payload);
      }),
    );
  } catch {
    // 非 Tauri 环境（纯浏览器预览）下忽略事件订阅
  }
  return () => {
    for (const off of unlisteners) off();
  };
}
