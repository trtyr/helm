import { useEffect, useRef } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";

export interface SeriesData {
  label: string;
  points: { ts: number; value: number }[];
  color: string; // CSS color（--ds-* 解析后的值）
  fillArea?: boolean;
}

/**
 * uPlot 时序图封装（决策 006：uPlot 主图，折线 + 8% 面积；无 ECharts）。
 * 百分比图传 threshold 画 90% 红虚线。
 */
export default function TimeSeriesChart({
  series,
  unit = "%",
  threshold,
  height = 160,
  yMax,
  valueFormat,
}: {
  series: SeriesData[];
  unit?: string;
  threshold?: number;
  height?: number;
  yMax?: number;
  valueFormat?: (v: number) => string;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const fmtRef = useRef<(v: number) => string>((v) => `${v.toFixed(1)}%`);
  const fmtEffect = valueFormat ?? ((v: number) => `${v.toFixed(1)}${unit}`);
  useEffect(() => {
    fmtRef.current = fmtEffect;
  });

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const fmt = fmtRef.current;

    // x 轴 = 所有系列的并集时间戳（秒）；系列数据转 Float64Array（uPlot 要求 TypedArray，
    // 缺口用 NaN 表示，spanGaps 跨越）
    const tsSet = new Set<number>();
    for (const s of series) for (const p of s.points) tsSet.add(Math.floor(p.ts / 1000));
    const ts = [...tsSet].sort((a, b) => a - b);
    const xArr = Float64Array.from(ts);

    const data: (Float64Array | number[])[] = [xArr];
    for (const s of series) {
      const map = new Map(s.points.map((p) => [Math.floor(p.ts / 1000), p.value]));
      data.push(Float64Array.from(ts, (t) => map.get(t) ?? Number.NaN));
    }

    const hasData = ts.length > 0;
    const yRange: [number, number] | undefined = hasData
      ? yMax != null
        ? [0, yMax]
        : undefined
      : [0, 100];

    const opts: uPlot.Options = {
      width: el.clientWidth,
      height,
      padding: [8, 8, 0, 8],
      cursor: { drag: { x: false, y: false } },
      legend: { show: false },
      scales: {
        x: { time: true },
        y: yRange ? { range: () => yRange } : {},
      },
      axes: [
        {
          stroke: "#666",
          grid: { show: false },
          ticks: { show: false },
          values: (_u, vals) =>
            vals.map((v) => new Date(v * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })),
          font: '11px "Geist Mono", monospace',
          size: 16,
        },
        {
          stroke: "#666",
          grid: { stroke: "#242424", width: 1 },
          size: 42,
          font: '11px "Geist Mono", monospace',
          values: (_u, vals) => vals.map((v) => fmt(v)),
        },
      ],
      series: [
        {},
        ...series.map((s) => ({
          label: s.label,
          stroke: s.color,
          width: 1.5,
          fill: s.fillArea ? `${s.color}14` : undefined, // 8% 面积（hex alpha）
          points: { show: false },
          spanGaps: true,
        })),
      ],
      hooks: {},
    };

    // 阈值线（90% 红虚线，40% 透明）：附加 series 绘制
    if (threshold != null && hasData) {
      data.push(Float64Array.from(ts, () => threshold));
      opts.series!.push({
        label: "阈值",
        stroke: "#ee000066",
        width: 1,
        dash: [4, 4],
        points: { show: false },
      });
    }

    const plot = new uPlot(opts, hasData ? (data as uPlot.AlignedData) : [new Float64Array(0)], el);

    const ro = new ResizeObserver(() => {
      plot.setSize({ width: el.clientWidth, height });
    });
    ro.observe(el);

    return () => {
      ro.disconnect();
      plot.destroy();
    };
    // fmt 经 ref 读取（unit/valueFormat 变化吞入 fmtEffect 闭包），无需列入依赖
  }, [series, threshold, height, yMax]);

  return <div ref={containerRef} className="w-full" style={{ height }} />;
}
