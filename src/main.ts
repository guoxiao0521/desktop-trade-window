import { invoke } from "@tauri-apps/api/core";
import {
  PhysicalPosition,
  PhysicalSize,
  getCurrentWindow,
} from "@tauri-apps/api/window";
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

interface AppSettings {
  market: string;
  lastSymbols: Record<string, string>;
  period: string;
  colorScheme: string;
  alwaysOnTop: boolean;
  windowX?: number | null;
  windowY?: number | null;
  windowWidth?: number | null;
  windowHeight?: number | null;
}

const LEGACY_STORAGE_SYMBOL = "stock-widget:symbol";
const LEGACY_STORAGE_PERIOD = "stock-widget:period";
const LEGACY_STORAGE_MARKET = "stock-widget:market";
const LEGACY_STORAGE_COLOR_SCHEME = "stock-widget:colorScheme";
const LEGACY_STORAGE_ALWAYS_ON_TOP = "stock-widget:alwaysOnTop";

type ColorScheme = "green-up" | "red-up";

const SCHEME_COLORS: Record<ColorScheme, { up: string; down: string }> = {
  "green-up": { up: "#26a69a", down: "#ef5350" },
  "red-up": { up: "#ef5350", down: "#26a69a" },
};

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
const settingsBtn = document.getElementById("settings-btn") as HTMLButtonElement;
const settingsPanel = document.getElementById("settings-panel") as HTMLElement;
const alwaysOnTopInput = document.getElementById(
  "always-on-top",
) as HTMLInputElement;
const schemeBtns = Array.from(
  document.querySelectorAll<HTMLButtonElement>(".scheme-btn"),
);
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

function isColorScheme(v: string | null | undefined): v is ColorScheme {
  return v === "green-up" || v === "red-up";
}

function isPeriod(v: string | null | undefined): v is string {
  return v === "1m" || v === "5m";
}

function defaultLastSymbols(): Record<Market, string> {
  return { ...DEFAULTS };
}

function normalizeLastSymbols(
  raw: Record<string, string> | undefined,
): Record<Market, string> {
  const out = defaultLastSymbols();
  if (!raw) return out;
  for (const m of ["US", "HK", "KR"] as Market[]) {
    const v = raw[m]?.trim().toUpperCase();
    if (v) out[m] = v;
  }
  return out;
}

function readLegacyFromLocalStorage(): Partial<AppSettings> | null {
  const marketRaw = localStorage.getItem(LEGACY_STORAGE_MARKET);
  const symbolRaw = localStorage.getItem(LEGACY_STORAGE_SYMBOL);
  const periodRaw = localStorage.getItem(LEGACY_STORAGE_PERIOD);
  const schemeRaw = localStorage.getItem(LEGACY_STORAGE_COLOR_SCHEME);
  const alwaysRaw = localStorage.getItem(LEGACY_STORAGE_ALWAYS_ON_TOP);

  if (!marketRaw && !symbolRaw && !periodRaw && !schemeRaw && alwaysRaw === null) {
    return null;
  }

  const market = isMarket(marketRaw) ? marketRaw : "US";
  const lastSymbols = defaultLastSymbols();
  if (symbolRaw?.trim()) {
    lastSymbols[market] = symbolRaw.trim().toUpperCase();
  }

  return {
    market,
    lastSymbols,
    period: isPeriod(periodRaw) ? periodRaw : "1m",
    colorScheme: isColorScheme(schemeRaw) ? schemeRaw : "green-up",
    alwaysOnTop: alwaysRaw === "1",
  };
}

function clearLegacyLocalStorage() {
  localStorage.removeItem(LEGACY_STORAGE_SYMBOL);
  localStorage.removeItem(LEGACY_STORAGE_PERIOD);
  localStorage.removeItem(LEGACY_STORAGE_MARKET);
  localStorage.removeItem(LEGACY_STORAGE_COLOR_SCHEME);
  localStorage.removeItem(LEGACY_STORAGE_ALWAYS_ON_TOP);
}

let market: Market = "US";
let lastSymbols: Record<Market, string> = defaultLastSymbols();
let symbol = DEFAULTS.US;
let period = "1m";
let colorScheme: ColorScheme = "green-up";
let alwaysOnTop = false;
let windowX: number | null = null;
let windowY: number | null = null;
let windowWidth: number | null = null;
let windowHeight: number | null = null;
let timer: number | undefined;
let refreshVersion = 0;
let persistTimer: number | undefined;
let settingsReady = false;
let suppressWindowPersist = false;

const chart = new StockChart(chartEl);
const appWindow = getCurrentWindow();
const toolbar = document.querySelector(".toolbar") as HTMLElement;

function currentSettings(): AppSettings {
  return {
    market,
    lastSymbols: { ...lastSymbols },
    period,
    colorScheme,
    alwaysOnTop,
    windowX,
    windowY,
    windowWidth,
    windowHeight,
  };
}

function persist() {
  if (!settingsReady) return;
  if (persistTimer) window.clearTimeout(persistTimer);
  persistTimer = window.setTimeout(() => {
    void invoke("save_settings", { settings: currentSettings() }).catch((err) => {
      console.error("save_settings failed", err);
    });
  }, 300);
}

function parseCoord(v: unknown): number | null {
  return typeof v === "number" && Number.isFinite(v) ? Math.round(v) : null;
}

async function restoreWindowGeometry() {
  const hasPos = windowX !== null && windowY !== null;
  const hasSize =
    windowWidth !== null &&
    windowHeight !== null &&
    windowWidth >= 320 &&
    windowHeight >= 220;
  if (!hasPos && !hasSize) return;

  suppressWindowPersist = true;
  try {
    if (hasSize) {
      await appWindow.setSize(new PhysicalSize(windowWidth!, windowHeight!));
    }
    if (hasPos) {
      await appWindow.setPosition(new PhysicalPosition(windowX!, windowY!));
    }
  } catch (err) {
    console.error("restore window geometry failed", err);
  } finally {
    // Allow move/resize events from setPosition/setSize to settle first.
    window.setTimeout(() => {
      suppressWindowPersist = false;
    }, 500);
  }
}

void appWindow.onMoved(({ payload }) => {
  if (suppressWindowPersist || !settingsReady) return;
  windowX = payload.x;
  windowY = payload.y;
  persist();
});

void appWindow.onResized(({ payload }) => {
  if (suppressWindowPersist || !settingsReady) return;
  windowWidth = payload.width;
  windowHeight = payload.height;
  persist();
});

function applyColorScheme(scheme: ColorScheme, persistChange = true) {
  colorScheme = scheme;
  document.body.classList.toggle("red-up", scheme === "red-up");
  const colors = SCHEME_COLORS[scheme];
  chart.setColors(colors.up, colors.down);
  schemeBtns.forEach((btn) =>
    btn.classList.toggle("active", btn.dataset.scheme === scheme),
  );
  if (persistChange) persist();
}

function setSettingsOpen(open: boolean) {
  settingsPanel.classList.toggle("hidden", !open);
}

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

function applyUiFromState() {
  symbol = lastSymbols[market] || DEFAULTS[market];
  symbolInput.value = symbol;
  periodBtns.forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.period === period);
  });
  marketBtns.forEach((btn) => {
    btn.classList.toggle("active", btn.dataset.market === market);
  });
  alwaysOnTopInput.checked = alwaysOnTop;
  applyColorScheme(colorScheme, false);
  syncPlaceholder();
}

function isInteractiveTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return Boolean(
    target.closest(
      "input, button, a, select, textarea, .period, .symbol-form, .market, .settings-panel",
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

settingsBtn.addEventListener("click", (e) => {
  e.stopPropagation();
  setSettingsOpen(settingsPanel.classList.contains("hidden"));
});

settingsPanel.addEventListener("click", (e) => {
  e.stopPropagation();
});

document.addEventListener("click", () => {
  setSettingsOpen(false);
});

schemeBtns.forEach((btn) => {
  btn.addEventListener("click", () => {
    const next = btn.dataset.scheme;
    if (!isColorScheme(next) || next === colorScheme) return;
    applyColorScheme(next);
  });
});

alwaysOnTopInput.addEventListener("change", () => {
  alwaysOnTop = alwaysOnTopInput.checked;
  void appWindow.setAlwaysOnTop(alwaysOnTop);
  persist();
});

symbolForm.addEventListener("submit", (e) => {
  e.preventDefault();
  const next = symbolInput.value.trim().toUpperCase();
  if (!next || next === symbol) return;
  symbol = next;
  lastSymbols[market] = symbol;
  symbolInput.value = symbol;
  persist();
  void refresh(true);
});

marketBtns.forEach((btn) => {
  btn.addEventListener("click", () => {
    const next = btn.dataset.market;
    if (!isMarket(next) || next === market) return;
    market = next;
    symbol = lastSymbols[market] || DEFAULTS[market];
    lastSymbols[market] = symbol;
    symbolInput.value = symbol;
    marketBtns.forEach((b) =>
      b.classList.toggle("active", b.dataset.market === market),
    );
    syncPlaceholder();
    persist();
    void refresh(true);
  });
});

periodBtns.forEach((btn) => {
  btn.addEventListener("click", () => {
    const next = btn.dataset.period || "1m";
    if (next === period) return;
    period = next;
    periodBtns.forEach((b) =>
      b.classList.toggle("active", b.dataset.period === period),
    );
    persist();
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

    let dirty = false;
    if (isMarket(quote.market) && quote.market !== market) {
      market = quote.market;
      marketBtns.forEach((b) =>
        b.classList.toggle("active", b.dataset.market === market),
      );
      dirty = true;
    }
    const resolved = quote.symbol.trim().toUpperCase();
    if (resolved && lastSymbols[market] !== resolved) {
      lastSymbols[market] = resolved;
      symbol = resolved;
      if (symbolInput.value.trim().toUpperCase() !== resolved) {
        symbolInput.value = resolved;
      }
      dirty = true;
    }
    if (dirty) persist();

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

function defaultSettings(): AppSettings {
  return {
    market: "US",
    lastSymbols: defaultLastSymbols(),
    period: "1m",
    colorScheme: "green-up",
    alwaysOnTop: false,
    windowX: null,
    windowY: null,
    windowWidth: null,
    windowHeight: null,
  };
}

async function bootstrap() {
  statusEl.textContent = "加载配置…";
  let settings = defaultSettings();
  let fileMissing = false;

  try {
    const loaded = await invoke<AppSettings | null>("load_settings");
    if (loaded) {
      settings = loaded;
    } else {
      fileMissing = true;
      const legacy = readLegacyFromLocalStorage();
      if (legacy) {
        settings = {
          market: legacy.market ?? settings.market,
          lastSymbols: {
            ...defaultLastSymbols(),
            ...(legacy.lastSymbols ?? {}),
          },
          period: legacy.period ?? settings.period,
          colorScheme: legacy.colorScheme ?? settings.colorScheme,
          alwaysOnTop: legacy.alwaysOnTop ?? settings.alwaysOnTop,
        };
      }
    }
  } catch (err) {
    console.error("load_settings failed", err);
  }

  market = isMarket(settings.market) ? settings.market : "US";
  lastSymbols = normalizeLastSymbols(settings.lastSymbols);
  period = isPeriod(settings.period) ? settings.period : "1m";
  colorScheme = isColorScheme(settings.colorScheme)
    ? settings.colorScheme
    : "green-up";
  alwaysOnTop = Boolean(settings.alwaysOnTop);
  windowX = parseCoord(settings.windowX);
  windowY = parseCoord(settings.windowY);
  windowWidth = parseCoord(settings.windowWidth);
  windowHeight = parseCoord(settings.windowHeight);
  symbol = lastSymbols[market] || DEFAULTS[market];

  applyUiFromState();
  if (alwaysOnTop) {
    void appWindow.setAlwaysOnTop(true);
  }
  await restoreWindowGeometry();

  settingsReady = true;
  if (fileMissing) {
    clearLegacyLocalStorage();
    persist();
  }

  void refresh(true);
}

void bootstrap();
