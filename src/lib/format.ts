const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"];

/** 字节数格式化，例如 1536 -> "1.5 KB" */
export function formatBytes(bytes: number, digits = 1): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), UNITS.length - 1);
  const value = bytes / Math.pow(1024, exponent);
  const precision = exponent === 0 ? 0 : digits;
  return `${value.toFixed(precision)} ${UNITS[exponent]}`;
}

/** 速率格式化，例如 2048 -> "2.0 KB/s" */
export function formatSpeed(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`;
}

/** 毫秒时长格式化 */
export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "0 秒";
  const totalSeconds = Math.floor(ms / 1000);
  const days = Math.floor(totalSeconds / 86400);
  const hours = Math.floor((totalSeconds % 86400) / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (days > 0) return `${days} 天 ${hours} 小时`;
  if (hours > 0) return `${hours} 小时 ${minutes} 分`;
  if (minutes > 0) return `${minutes} 分 ${seconds} 秒`;
  return `${seconds} 秒`;
}

function pad(value: number): string {
  return value.toString().padStart(2, "0");
}

/** 时间戳（毫秒）-> "2026-11-17 09:30" */
export function formatDateTime(timestamp: number): string {
  const date = new Date(timestamp);
  if (Number.isNaN(date.getTime())) return "-";
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(
    date.getHours(),
  )}:${pad(date.getMinutes())}`;
}

/** 时间戳（毫秒）-> 相对描述 */
export function formatRelativeTime(timestamp: number): string {
  const diff = Date.now() - timestamp;
  if (diff < 0) return "刚刚";
  const minute = 60_000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (diff < minute) return "刚刚";
  if (diff < hour) return `${Math.floor(diff / minute)} 分钟前`;
  if (diff < day) return `${Math.floor(diff / hour)} 小时前`;
  if (diff < 30 * day) return `${Math.floor(diff / day)} 天前`;
  return formatDateTime(timestamp);
}

/** 延迟对应的语义色 */
export function delayTone(delay?: number | null): "none" | "good" | "fair" | "poor" | "bad" {
  if (delay === null || delay === undefined || delay <= 0) return "none";
  if (delay < 150) return "good";
  if (delay < 300) return "fair";
  if (delay < 600) return "poor";
  return "bad";
}

/** 延迟显示文本 */
export function formatDelay(delay?: number | null): string {
  if (delay === null || delay === undefined || delay <= 0) return "-";
  return `${Math.round(delay)} ms`;
}

/** RFC3339 -> 本地时间字符串 */
export function formatRfc3339(input: string): string {
  const date = new Date(input);
  if (Number.isNaN(date.getTime())) return input;
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}
