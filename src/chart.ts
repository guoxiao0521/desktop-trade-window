import {
  CandlestickSeries,
  ColorType,
  CrosshairMode,
  HistogramSeries,
  createChart,
  type IChartApi,
  type ISeriesApi,
  type UTCTimestamp,
} from "lightweight-charts";

export interface CandleBar {
  time: number;
  open: number;
  high: number;
  low: number;
  close: number;
  volume: number;
}

const DEFAULT_UP = "#26a69a";
const DEFAULT_DOWN = "#ef5350";

function withAlpha(hex: string, alpha: number): string {
  const r = parseInt(hex.slice(1, 3), 16);
  const g = parseInt(hex.slice(3, 5), 16);
  const b = parseInt(hex.slice(5, 7), 16);
  return `rgba(${r},${g},${b},${alpha})`;
}

export class StockChart {
  private chart: IChartApi;
  private candleSeries: ISeriesApi<"Candlestick">;
  private volumeSeries: ISeriesApi<"Histogram">;
  private resizeObserver: ResizeObserver;
  private upColor = DEFAULT_UP;
  private downColor = DEFAULT_DOWN;
  private lastBars: CandleBar[] = [];

  constructor(container: HTMLElement) {
    this.chart = createChart(container, {
      autoSize: true,
      layout: {
        background: { type: ColorType.Solid, color: "transparent" },
        textColor: "#8b95a8",
        fontSize: 10,
        attributionLogo: false,
      },
      grid: {
        vertLines: { color: "rgba(255,255,255,0.04)" },
        horzLines: { color: "rgba(255,255,255,0.04)" },
      },
      crosshair: {
        mode: CrosshairMode.Normal,
        vertLine: { color: "rgba(79,140,255,0.35)", labelBackgroundColor: "#2a3344" },
        horzLine: { color: "rgba(79,140,255,0.35)", labelBackgroundColor: "#2a3344" },
      },
      rightPriceScale: {
        borderVisible: false,
        scaleMargins: { top: 0.08, bottom: 0.22 },
      },
      timeScale: {
        borderVisible: false,
        timeVisible: true,
        secondsVisible: false,
        rightOffset: 2,
      },
      handleScroll: { mouseWheel: true, pressedMouseMove: true },
      handleScale: { axisPressedMouseMove: true, mouseWheel: true, pinch: true },
    });

    this.candleSeries = this.chart.addSeries(CandlestickSeries, {
      upColor: this.upColor,
      downColor: this.downColor,
      borderVisible: false,
      wickUpColor: this.upColor,
      wickDownColor: this.downColor,
    });

    this.volumeSeries = this.chart.addSeries(HistogramSeries, {
      priceFormat: { type: "volume" },
      priceScaleId: "volume",
    });
    this.volumeSeries.priceScale().applyOptions({
      scaleMargins: { top: 0.82, bottom: 0 },
    });

    this.resizeObserver = new ResizeObserver(() => {
      // autoSize handles most cases; force a layout pass after window chrome changes
      this.chart.timeScale().applyOptions({});
    });
    this.resizeObserver.observe(container);
  }

  setColors(up: string, down: string) {
    this.upColor = up;
    this.downColor = down;
    this.candleSeries.applyOptions({
      upColor: up,
      downColor: down,
      wickUpColor: up,
      wickDownColor: down,
    });
    if (this.lastBars.length > 0) {
      this.applyVolumeData(this.lastBars);
    }
  }

  setTheme(theme: "dark" | "light") {
    const light = theme === "light";
    this.chart.applyOptions({
      layout: {
        textColor: light ? "#64748b" : "#8b95a8",
      },
      grid: {
        vertLines: { color: light ? "rgba(0,0,0,0.06)" : "rgba(255,255,255,0.04)" },
        horzLines: { color: light ? "rgba(0,0,0,0.06)" : "rgba(255,255,255,0.04)" },
      },
      crosshair: {
        vertLine: {
          color: light ? "rgba(59,124,255,0.4)" : "rgba(79,140,255,0.35)",
          labelBackgroundColor: light ? "#e2e8f0" : "#2a3344",
        },
        horzLine: {
          color: light ? "rgba(59,124,255,0.4)" : "rgba(79,140,255,0.35)",
          labelBackgroundColor: light ? "#e2e8f0" : "#2a3344",
        },
      },
    });
  }

  setData(bars: CandleBar[]) {
    this.lastBars = bars;
    const candles = bars.map((b) => ({
      time: b.time as UTCTimestamp,
      open: b.open,
      high: b.high,
      low: b.low,
      close: b.close,
    }));
    this.candleSeries.setData(candles);
    this.applyVolumeData(bars);
    this.chart.timeScale().fitContent();
  }

  private applyVolumeData(bars: CandleBar[]) {
    const upVol = withAlpha(this.upColor, 0.45);
    const downVol = withAlpha(this.downColor, 0.45);
    const volumes = bars.map((b) => ({
      time: b.time as UTCTimestamp,
      value: b.volume,
      color: b.close >= b.open ? upVol : downVol,
    }));
    this.volumeSeries.setData(volumes);
  }

  destroy() {
    this.resizeObserver.disconnect();
    this.chart.remove();
  }
}
