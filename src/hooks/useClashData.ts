import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/lib/api";
import type { AppSettings, ProxyDelayResult, SubscriptionInput } from "@/lib/types";
import { useAppStore } from "@/store/appStore";

const PROXIES_KEY = ["clash", "proxies"] as const;
const RULES_KEY = ["clash", "rules"] as const;
const CONNECTIONS_KEY = ["clash", "connections"] as const;
const SUBS_KEY = ["app", "subscriptions"] as const;

export function useProxies(enabled: boolean) {
  return useQuery({
    queryKey: PROXIES_KEY,
    queryFn: () => api.getProxies(),
    enabled,
    refetchInterval: 4000,
    refetchOnWindowFocus: false,
  });
}

export function useRules(enabled: boolean) {
  return useQuery({
    queryKey: RULES_KEY,
    queryFn: () => api.getRules(),
    enabled,
    staleTime: 30_000,
    refetchOnWindowFocus: false,
  });
}

export function useConnections() {
  return useQuery({
    queryKey: CONNECTIONS_KEY,
    queryFn: () => api.getConnections(),
    refetchInterval: 5000,
    refetchOnWindowFocus: false,
  });
}

export function useSubscriptions() {
  return useQuery({
    queryKey: SUBS_KEY,
    queryFn: () => api.listSubscriptions(),
    refetchOnWindowFocus: false,
  });
}

export function useSelectProxy() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ group, name }: { group: string; name: string }) => api.selectProxy(group, name),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: PROXIES_KEY });
    },
  });
}

export function useDelayTest() {
  const queryClient = useQueryClient();
  return useMutation<ProxyDelayResult, unknown, { name: string; url?: string }>({
    mutationFn: ({ name, url }) => api.proxyDelay(name, url),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: PROXIES_KEY });
    },
  });
}

export function useGroupDelayTest() {
  const queryClient = useQueryClient();
  return useMutation<ProxyDelayResult[], unknown, { group: string; url?: string }>({
    mutationFn: ({ group, url }) => api.proxyGroupDelay(group, url),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: PROXIES_KEY });
    },
  });
}

export function useCloseConnection() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.closeConnection(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: CONNECTIONS_KEY });
    },
  });
}

export function useCloseAllConnections() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => api.closeAllConnections(),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: CONNECTIONS_KEY });
    },
  });
}

export function useAddSubscription() {
  const queryClient = useQueryClient();
  const refreshSnapshot = useAppStore((state) => state.refreshSnapshot);
  return useMutation({
    mutationFn: (input: SubscriptionInput) => api.addSubscription(input),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: SUBS_KEY });
      void refreshSnapshot();
    },
  });
}

export function useRemoveSubscription() {
  const queryClient = useQueryClient();
  const refreshSnapshot = useAppStore((state) => state.refreshSnapshot);
  return useMutation({
    mutationFn: (id: string) => api.removeSubscription(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: SUBS_KEY });
      void refreshSnapshot();
    },
  });
}

export function useUpdateSubscription() {
  const queryClient = useQueryClient();
  const refreshSnapshot = useAppStore((state) => state.refreshSnapshot);
  return useMutation({
    mutationFn: (id: string) => api.updateSubscription(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: SUBS_KEY });
      void refreshSnapshot();
    },
  });
}

export function useUpdateAllSubscriptions() {
  const queryClient = useQueryClient();
  const refreshSnapshot = useAppStore((state) => state.refreshSnapshot);
  return useMutation({
    mutationFn: () => api.updateAllSubscriptions(),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: SUBS_KEY });
      void refreshSnapshot();
    },
  });
}

export function useActivateSubscription() {
  const queryClient = useQueryClient();
  const refreshSnapshot = useAppStore((state) => state.refreshSnapshot);
  return useMutation({
    mutationFn: (id: string) => api.setActiveSubscription(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: SUBS_KEY });
      void queryClient.invalidateQueries({ queryKey: PROXIES_KEY });
      void refreshSnapshot();
    },
  });
}

export function useUpdateSettings() {
  const setSettings = useAppStore((state) => state.setSettings);
  return useMutation<AppSettings, unknown, AppSettings>({
    mutationFn: (settings: AppSettings) => api.updateSettings(settings),
    onSuccess: (settings) => setSettings(settings),
  });
}
