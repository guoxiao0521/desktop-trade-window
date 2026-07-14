import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { StockChart, type CandleBar } from "./chart";

type Market = "US" | "HK" | "KR";

interface Quote {
  symbol: string;
  name: string;
  price: number;
  prevClose: number;
  change: number;
  changePercent: number;
  currency: string;
  market: Market;
}

const STORAGE_SYMBOL = "stock-widget:symbol";
const STORAGE_PERIOD = "stock-widget:period";
const STORAGE_MARKET = "stock-widget:market";

const DEFAULTS: Record<Market, string> = {
  US: "AAPL",
  HK: "00700",
  KR: "005930",
};

const symbolInput = document.getElementById("symbol-input") as HTMLInputElement;
const symbolForm = document.getElementById("symbol-form") as HTMLFormElement;
const priceEl = document.getElementById("price") as HTMLElement;
const changeEl = document.getElementById("change") as HTMLElement;
const statusEl = document.getElementById("status") as HTMLElement;
const hideBtn = document.getElementById("hide-btn") as HTMLButtonElement;
const chartEl = document.getElementById("chart") as HTMLElement;
const periodBtns = Array.from(
  document.querySelectorAll<HTMLButtonElement>(".period-btn"),
);
const marketBtns = Array.from(
  document.querySelectorAll<HTMLButtonElement>(".market-btn"),
);

function isMarket(v: string | null | undefined): v is Market {
  return v === "US" || v === "HK" || v === "KR";
}

let market: Market = isMarket(localStorage.getItem(STORAGE_MARKET))
  ? (localStorage.getItem(STORAGE_MARKET) as Market)
  : "US";
let symbol = localStorage.getItem(STORAGE_SYMBOL) || DEFAULTS[market];
let period = localStorage.getItem(STORAGE_PERIOD) || "1m";
let timer: number | undefined;
let refreshVersion = 0;

symbolInput.value = symbol;
periodBtns.forEach((btn) => {
  btn.classList.toggle("active", btn.dataset.period === period);
});
marketBtns.forEach((btn) => {
  btn.classList.toggle("active", btn.dataset.market === market);
});
syncPlaceholder();

const chart = new StockChart(chartEl);
const appWindow = getCurrentWindow();
const toolbar = document.querySelector(".toolbar") as HTMLElement;

function isInteractiveTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(
    target.closest(
      "input, button, a, select, textarea, .period, .symbol-form, .market",
    ),
  );
}

toolbar.addEventListener("mousedown", (e) => {
  if (e.button !== 0 || isInteractiveTarget(e.target)) return;
  e.preventDefault();
  void appWindow.startDragging();
});

hideBtn.addEventListener("click", async () => {
  await appWindow.hide();
});

function syncPlaceholder() {
  const tips: Record<Market, string> = {
    US: "AAPL",
    HK: "00700",
    KR: "KOSPI",
  };
  symbolInput.placeholder = tips[market];
  symbolInput.title =
    `代码示例：美股 AAPL·DJI / 港股 00700·HSI / 韩股 005930·KOSPI（当前 ${market}）`;
}

symbolForm.addEventListener("submit", (e) => {
  e.preventDefault();
  const next = symbolInput.value.trim().toUpperCase();
  if (!next || next === symbol) return;
  symbol = next;
  localStorage.setItem(STORAGE_SYMBOL, symbol);
  void refresh(true);
});

marketBtns.forEach((btn) => {
  btn.addEventListener("click", () => {
    const next = btn.dataset.market;
    if (!isMarket(next) || next === market) return;
    market = next;
    localStorage.setItem(STORAGE_MARKET, market);
    marketBtns.forEach((b) =>
      b.classList.toggle("active", b.dataset.market === market),
    );
    // Switch to a sensible default ticker for the new market.
    symbol = DEFAULTS[market];
    symbolInput.value = symbol;
    localStorage.setItem(STORAGE_SYMBOL, symbol);
    syncPlaceholder();
    void refresh(true);
  });
});

periodBtns.forEach((btn) => {
  btn.addEventListener("click", () => {
    const next = btn.dataset.period || "1m";
    if (next === period) return;
    period = next;
    localStorage.setItem(STORAGE_PERIOD, period);
    periodBtns.forEach((b) =>
      b.classList.toggle("active", b.dataset.period === period),
    );
    void refresh(true);
  });
});

function formatPrice(n: number, currency: string): string {
  if (!Number.isFinite(n)) return "--";
  if (currency === "KRW") return Math.round(n).toLocaleString("en-US");
  if (n >= 1000) return n.toFixed(1);
  if (n >= 100) return n.toFixed(2);
  return n.toFixed(3);
}

function applyQuote(q: Quote) {
  const up = q.change >= 0;
  priceEl.textContent = formatPrice(q.price, q.currency);
  priceEl.classList.toggle("up", up);
  priceEl.classList.toggle("down", !up);
  const sign = up ? "+" : "";
  const ch = formatPrice(Math.abs(q.change), q.currency);
  changeEl.textContent = `${sign}${ch} (${sign}${q.changePercent.toFixed(2)}%)`;
  changeEl.classList.toggle("up", up);
  changeEl.classList.toggle("down", !up);
  symbolInput.title = q.name
    ? `${q.name} (${q.market}:${q.symbol})`
    : `${q.market}:${q.symbol}`;
}

/** Approximate session hours in local exchange time. */
function isMarketOpen(m: Market, now = new Date()): boolean {
  const offsets: Record<Market, number> = { US: -4, HK: 8, KR: 9 };
  const localMs = now.getTime() + offsets[m] * 60 * 60 * 1000;
  const local = new Date(localMs);
  const day = local.getUTCDay();
  if (day === 0 || day === 6) return false;
  const minutes = local.getUTCHours() * 60 + local.getUTCMinutes();

  if (m === "US") {
    return minutes >= 9 * 60 + 30 && minutes < 16 * 60;
  }
  if (m === "HK") {
    // 09:30–12:00, 13:00–16:00 HKT
    return (
      (minutes >= 9 * 60 + 30 && minutes < 12 * 60) ||
      (minutes >= 13 * 60 && minutes < 16 * 60)
    );
  }
  // KR: 09:00–15:30 KST
  return minutes >= 9 * 60 && minutes < 15 * 60 + 30;
}

function scheduleNext() {
  if (timer) window.clearTimeout(timer);
  const delay = isMarketOpen(market) ? 30_000 : 5 * 60_000;
  timer = window.setTimeout(() => void refresh(false), delay);
}

async function refresh(showLoading: boolean) {
  const version = ++refreshVersion;
  const requestedMarket = market;
  const requestedSymbol = symbol;
  const requestedPeriod = period;

  if (timer) {
    window.clearTimeout(timer);
    timer = undefined;
  }
  if (showLoading) {
    statusEl.textContent = `加载 ${requestedMarket}:${requestedSymbol} …`;
  }

  try {
    const [bars, quote] = await Promise.all([
      invoke<CandleBar[]>("fetch_kline", {
        symbol: requestedSymbol,
        period: requestedPeriod,
        market: requestedMarket,
      }),
      invoke<Quote>("fetch_quote", {
        symbol: requestedSymbol,
        market: requestedMarket,
      }),
    ]);

    if (version !== refreshVersion) return;

    chart.setData(bars);
    applyQuote(quote);
    if (isMarket(quote.market)) {
      market = quote.market;
      localStorage.setItem(STORAGE_MARKET, market);
      marketBtns.forEach((b) =>
        b.classList.toggle("active", b.dataset.market === market),
      );
    }

    const when = new Date().toLocaleTimeString("zh-CN", { hour12: false });
    const session = isMarketOpen(market) ? "盘中 30s 刷新" : "休市 5min 刷新";
    statusEl.textContent = `${quote.market}:${quote.symbol} · ${bars.length} 根 · ${when} · ${session}`;
  } catch (err) {
    if (version !== refreshVersion) return;
    const msg = err instanceof Error ? err.message : String(err);
    statusEl.textContent = `错误: ${msg}`;
    console.error(err);
  } finally {
    if (version === refreshVersion) scheduleNext();
  }
}

void refresh(true);
