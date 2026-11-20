//! 全局共享状态。
//!
//! 使用单把 `Mutex` 保护所有可变数据，读写都在极短时间内完成（不做 IO），
//! 因此不会阻塞 async 运行时。

use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard};

use crate::models::{
    AppSettings, KernelStatus, LogEntry, Subscription, SystemProxyState, TrafficSample,
};
use crate::storage::AppPaths;

/// 内存中保留的日志条数上限。
pub const MAX_LOGS: usize = 500;

pub struct AppState {
    pub paths: AppPaths,
    inner: Mutex<Inner>,
}

struct Inner {
    settings: AppSettings,
    subscriptions: Vec<Subscription>,
    kernel: KernelStatus,
    system_proxy: SystemProxyState,
    traffic: TrafficSample,
    logs: VecDeque<LogEntry>,
}

impl AppState {
    pub fn new(paths: AppPaths, settings: AppSettings, subscriptions: Vec<Subscription>) -> Self {
        Self {
            paths,
            inner: Mutex::new(Inner {
                settings,
                subscriptions,
                kernel: KernelStatus::default(),
                system_proxy: SystemProxyState::default(),
                traffic: TrafficSample::default(),
                logs: VecDeque::new(),
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        // 单条数据读写不会 panic 到污染锁，一旦发生也继续使用内部数据。
        self.inner.lock().unwrap_or_else(|error| error.into_inner())
    }

    /* ------------------------------ 设置 ------------------------------ */

    pub fn settings_snapshot(&self) -> AppSettings {
        self.lock().settings.clone()
    }

    pub fn update_settings(&self, settings: AppSettings) {
        self.lock().settings = settings;
    }

    /* ------------------------------ 订阅 ------------------------------ */

    pub fn subscriptions(&self) -> Vec<Subscription> {
        self.lock().subscriptions.clone()
    }

    pub fn set_subscriptions(&self, subscriptions: Vec<Subscription>) {
        self.lock().subscriptions = subscriptions;
    }

    pub fn current_subscription(&self) -> Option<Subscription> {
        let inner = self.lock();
        let current_id = inner.settings.current_subscription_id.clone()?;
        inner
            .subscriptions
            .iter()
            .find(|item| item.id == current_id)
            .cloned()
    }

    /* ------------------------------ 内核 ------------------------------ */

    pub fn kernel_status(&self) -> KernelStatus {
        self.lock().kernel.clone()
    }

    /// 写入内核状态；新状态未带重启计数时继承旧值，避免状态回退丢失统计。
    pub fn set_kernel_status(&self, mut status: KernelStatus) {
        let mut inner = self.lock();
        if status.restart_count == 0 {
            status.restart_count = inner.kernel.restart_count;
        }
        if status.version.is_none() {
            status.version = inner.kernel.version.clone();
        }
        inner.kernel = status;
    }

    pub fn bump_restart_count(&self) -> u32 {
        let mut inner = self.lock();
        inner.kernel.restart_count = inner.kernel.restart_count.saturating_add(1);
        inner.kernel.restart_count
    }

    /// 内核健康启动成功后清零自动重启计数。
    ///
    /// `set_kernel_status` 会继承旧的 restart_count，若不显式清零，
    /// 计数只增不减：长时间使用中偶发退出几次就会撞上上限，
    /// 之后内核彻底失去自动重启能力。
    pub fn reset_restart_count(&self) {
        let mut inner = self.lock();
        inner.kernel.restart_count = 0;
    }

    pub fn set_kernel_version(&self, version: String) {
        let mut inner = self.lock();
        inner.kernel.version = Some(version);
    }

    /* ---------------------------- 系统代理 ---------------------------- */

    pub fn system_proxy(&self) -> SystemProxyState {
        self.lock().system_proxy.clone()
    }

    pub fn set_system_proxy(&self, state: SystemProxyState) {
        self.lock().system_proxy = state;
    }

    /* ------------------------------ 流量 ------------------------------ */

    /// 累加流量并返回最新采样（含累计值）。
    pub fn accumulate_traffic(&self, up: u64, down: u64) -> TrafficSample {
        let mut inner = self.lock();
        inner.traffic.up = up;
        inner.traffic.down = down;
        inner.traffic.up_total = inner.traffic.up_total.saturating_add(up);
        inner.traffic.down_total = inner.traffic.down_total.saturating_add(down);
        inner.traffic
    }

    pub fn traffic(&self) -> TrafficSample {
        self.lock().traffic
    }

    pub fn reset_traffic(&self) -> TrafficSample {
        let mut inner = self.lock();
        inner.traffic = TrafficSample::default();
        inner.traffic
    }

    /* ------------------------------ 日志 ------------------------------ */

    /// 追加一条日志。
    ///
    /// **脱敏在唯一写入口统一做**：订阅地址通常带 `?token=`、内核
    /// external-controller 带 `secret`、请求头带 `Authorization`，
    /// 这些东西一旦进日志，就会同时出现在界面"日志"页和用户反馈截图里。
    /// 把脱敏收敛到 `push_log`，比要求几十个调用点各自记得处理可靠得多
    /// （新增日志位置不可能漏掉）。具体规则见 `crate::redact`。
    pub fn push_log(&self, mut entry: LogEntry) {
        entry.message = crate::redact::redact_text(&entry.message);
        let mut inner = self.lock();
        if inner.logs.len() >= MAX_LOGS {
            inner.logs.pop_front();
        }
        inner.logs.push_back(entry);
    }

    pub fn recent_logs(&self, limit: usize) -> Vec<LogEntry> {
        let inner = self.lock();
        let skip = inner.logs.len().saturating_sub(limit);
        inner.logs.iter().skip(skip).cloned().collect()
    }
}
