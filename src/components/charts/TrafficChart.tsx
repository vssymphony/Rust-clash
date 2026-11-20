import { useId, useMemo } from "react";
import type { TrafficPoint } from "@/store/appStore";
import { MAX_TRAFFIC_POINTS } from "@/store/appStore";
import { formatSpeed } from "@/lib/format";

interface TrafficChartProps {
  data: TrafficPoint[];
  height?: number;
}

const VIEW_W = 1000;
const VIEW_H = 300;

function buildPath(values: number[], max: number): { line: string; area: string } {
  if (values.length === 0) {
    return { line: "", area: "" };
  }
  const step = values.length > 1 ? VIEW_W / (values.length - 1) : VIEW_W;
  const safeMax = max <= 0 ? 1 : max;
  const points = values.map((value, index) => {
    const x = index * step;
    const y = VIEW_H - (Math.max(value, 0) / safeMax) * (VIEW_H * 0.92) - 6;
    return { x, y };
  });
  const line = points
    .map((point, index) => `${index === 0 ? "M" : "L"}${point.x.toFixed(2)},${point.y.toFixed(2)}`)
    .join(" ");
  const area = `${line} L${VIEW_W},${VIEW_H} L0,${VIEW_H} Z`;
  return { line, area };
}

export function TrafficChart({ data, height = 180 }: TrafficChartProps) {
  const gradientId = useId().replace(/:/g, "");
  const upId = `${gradientId}-up`;
  const downId = `${gradientId}-down`;

  const { upPath, downPath, upArea, downArea, peak } = useMemo(() => {
    const ups = data.map((point) => point.up);
    const downs = data.map((point) => point.down);
    const max = Math.max(1, ...ups, ...downs);
    const up = buildPath(ups, max);
    const down = buildPath(downs, max);
    return {
      upPath: up.line,
      upArea: up.area,
      downPath: down.line,
      downArea: down.area,
      peak: max,
    };
  }, [data]);

  const padding = MAX_TRAFFIC_POINTS - data.length;
  const offsetX = padding > 0 ? (padding / (MAX_TRAFFIC_POINTS - 1)) * VIEW_W : 0;

  return (
    <div className="relative w-full" style={{ height }}>
      <div className="pointer-events-none absolute left-1 top-0 text-[10px] text-muted-foreground">
        峰值 {formatSpeed(peak)}
      </div>
      <svg
        viewBox={`0 0 ${VIEW_W} ${VIEW_H}`}
        preserveAspectRatio="none"
        className="h-full w-full overflow-visible"
        role="img"
        aria-label="实时流量曲线"
      >
        <defs>
          <linearGradient id={downId} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="hsl(var(--info))" stopOpacity="0.35" />
            <stop offset="100%" stopColor="hsl(var(--info))" stopOpacity="0" />
          </linearGradient>
          <linearGradient id={upId} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="hsl(var(--success))" stopOpacity="0.3" />
            <stop offset="100%" stopColor="hsl(var(--success))" stopOpacity="0" />
          </linearGradient>
        </defs>

        {[0.25, 0.5, 0.75].map((ratio) => (
          <line
            key={ratio}
            x1="0"
            x2={VIEW_W}
            y1={VIEW_H * ratio}
            y2={VIEW_H * ratio}
            stroke="currentColor"
            className="text-white/5"
            strokeWidth="1"
            vectorEffect="non-scaling-stroke"
          />
        ))}

        <g transform={`translate(${offsetX}, 0)`}>
          <path d={downArea} fill={`url(#${downId})`} />
          <path
            d={downPath}
            fill="none"
            stroke="hsl(var(--info))"
            strokeWidth="2"
            strokeLinejoin="round"
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
          />
          <path d={upArea} fill={`url(#${upId})`} />
          <path
            d={upPath}
            fill="none"
            stroke="hsl(var(--success))"
            strokeWidth="2"
            strokeLinejoin="round"
            strokeLinecap="round"
            vectorEffect="non-scaling-stroke"
          />
        </g>
      </svg>
    </div>
  );
}

export function ChartLegend() {
  return (
    <div className="flex items-center gap-3 text-[10px] text-muted-foreground">
      <span className="flex items-center gap-1">
        <span className="h-1.5 w-3 rounded-full bg-info" />
        下行
      </span>
      <span className="flex items-center gap-1">
        <span className="h-1.5 w-3 rounded-full bg-success" />
        上行
      </span>
    </div>
  );
}
