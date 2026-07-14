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

const UP = "#26a69a";
const DOWN = "#ef5350";

export class StockChart {
  private chart: IChartApi;
  private candleSeries: ISeriesApi<"Candlestick">;
  private volumeSeries: ISeriesApi<"Histogram">;
  private resizeObserver: ResizeObserver;

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
      upColor: UP,
      downColor: DOWN,
      borderVisible: false,
      wickUpColor: UP,
      wickDownColor: DOWN,
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

  setData(bars: CandleBar[]) {
    const candles = bars.map((b) => ({
      time: b.time as UTCTimestamp,
      open: b.open,
      high: b.high,
      low: b.low,
      close: b.close,
    }));
    const volumes = bars.map((b) => ({
      time: b.time as UTCTimestamp,
      value: b.volume,
      color: b.close >= b.open ? "rgba(38,166,154,0.45)" : "rgba(239,83,80,0.45)",
    }));
    this.candleSeries.setData(candles);
    this.volumeSeries.setData(volumes);
    this.chart.timeScale().fitContent();
  }

  destroy() {
    this.resizeObserver.disconnect();
    this.chart.remove();
  }
}
