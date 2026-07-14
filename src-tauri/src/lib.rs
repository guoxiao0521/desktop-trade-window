use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, WindowEvent,
};

const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default = "default_market")]
    pub market: String,
    #[serde(default = "default_last_symbols")]
    pub last_symbols: HashMap<String, String>,
    #[serde(default = "default_period")]
    pub period: String,
    #[serde(default = "default_color_scheme")]
    pub color_scheme: String,
    #[serde(default)]
    pub always_on_top: bool,
    /// Outer window position in physical pixels (from last drag).
    #[serde(default)]
    pub window_x: Option<i32>,
    #[serde(default)]
    pub window_y: Option<i32>,
    /// Inner window size in physical pixels (from last resize).
    #[serde(default)]
    pub window_width: Option<u32>,
    #[serde(default)]
    pub window_height: Option<u32>,
}

fn default_market() -> String {
    "US".into()
}

fn default_period() -> String {
    "1m".into()
}

fn default_color_scheme() -> String {
    "green-up".into()
}

fn default_last_symbols() -> HashMap<String, String> {
    HashMap::from([
        ("US".into(), "AAPL".into()),
        ("HK".into(), "00700".into()),
        ("KR".into(), "005930".into()),
    ])
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            market: default_market(),
            last_symbols: default_last_symbols(),
            period: default_period(),
            color_scheme: default_color_scheme(),
            always_on_top: false,
            window_x: None,
            window_y: None,
            window_width: None,
            window_height: None,
        }
    }
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("app_config_dir: {e}"))?;
    Ok(dir.join(SETTINGS_FILE))
}

#[tauri::command]
fn load_settings(app: AppHandle) -> Result<Option<AppSettings>, String> {
    let path = settings_path(&app)?;
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|e| format!("read settings: {e}"))?;
    match serde_json::from_str::<AppSettings>(&text) {
        Ok(settings) => Ok(Some(settings)),
        Err(e) => {
            eprintln!("Failed to parse settings ({e}), using defaults");
            Ok(Some(AppSettings::default()))
        }
    }
}

#[tauri::command]
fn save_settings(app: AppHandle, settings: AppSettings) -> Result<(), String> {
    let path = settings_path(&app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("create config dir: {e}"))?;
    }
    let text = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("serialize settings: {e}"))?;
    fs::write(&path, text).map_err(|e| format!("write settings: {e}"))?;
    Ok(())
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Candle {
    pub time: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    pub symbol: String,
    pub name: String,
    pub price: f64,
    pub prev_close: f64,
    pub change: f64,
    pub change_percent: f64,
    pub currency: String,
    pub market: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Market {
    Us,
    Hk,
    Kr,
}

impl Market {
    fn as_str(self) -> &'static str {
        match self {
            Market::Us => "US",
            Market::Hk => "HK",
            Market::Kr => "KR",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_uppercase().as_str() {
            "US" | "USA" | "NYSE" | "NASDAQ" => Some(Market::Us),
            "HK" | "HKG" | "HKEX" => Some(Market::Hk),
            "KR" | "KS" | "KQ" | "KRX" | "KOSPI" | "KOSDAQ" => Some(Market::Kr),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedSymbol {
    market: Market,
    /// UI / Tencent numeric or ticker body, e.g. AAPL / 00700 / 005930
    code: String,
    /// Yahoo Finance symbol candidates in priority order
    yahoo_symbols: Vec<String>,
    /// Tencent qt.gtimg.cn codes
    tencent_codes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct YahooChartResponse {
    chart: YahooChart,
}

#[derive(Debug, Deserialize)]
struct YahooChart {
    result: Option<Vec<YahooResult>>,
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct YahooResult {
    meta: YahooMeta,
    timestamp: Option<Vec<i64>>,
    indicators: YahooIndicators,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct YahooMeta {
    symbol: String,
    short_name: Option<String>,
    regular_market_price: Option<f64>,
    chart_previous_close: Option<f64>,
    previous_close: Option<f64>,
    currency: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YahooIndicators {
    quote: Vec<YahooQuoteBars>,
}

#[derive(Debug, Deserialize)]
struct YahooQuoteBars {
    open: Option<Vec<Option<f64>>>,
    high: Option<Vec<Option<f64>>>,
    low: Option<Vec<Option<f64>>>,
    close: Option<Vec<Option<f64>>>,
    volume: Option<Vec<Option<f64>>>,
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 StockWidget/0.1")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())
}

/// Common index aliases → Yahoo / Tencent codes.
/// Users can type names like KOSPI / HSI instead of exchange numeric tickers.
fn resolve_index_alias(raw: &str) -> Option<ResolvedSymbol> {
    let key: String = raw
        .trim()
        .trim_start_matches('^')
        .to_uppercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();

    match key.as_str() {
        // Korea
        "KOSPI" | "KS11" | "KOSPI200" => Some(ResolvedSymbol {
            market: Market::Kr,
            code: "KOSPI".into(),
            yahoo_symbols: vec!["^KS11".into()],
            tencent_codes: vec![],
        }),
        "KOSDAQ" | "KQ11" => Some(ResolvedSymbol {
            market: Market::Kr,
            code: "KOSDAQ".into(),
            yahoo_symbols: vec!["^KQ11".into()],
            tencent_codes: vec![],
        }),
        // Hong Kong
        "HSI" | "HANGSENG" | "HSINDEX" => Some(ResolvedSymbol {
            market: Market::Hk,
            code: "HSI".into(),
            yahoo_symbols: vec!["^HSI".into()],
            tencent_codes: vec!["hkHSI".into(), "s_hkHSI".into()],
        }),
        // US majors (bonus; useful when typing index names)
        "DJI" | "DJIA" | "DOW" => Some(ResolvedSymbol {
            market: Market::Us,
            code: "DJI".into(),
            yahoo_symbols: vec!["^DJI".into()],
            tencent_codes: vec!["usDJI".into()],
        }),
        "IXIC" | "NASDAQ" | "NDXCOMP" => Some(ResolvedSymbol {
            market: Market::Us,
            code: "IXIC".into(),
            yahoo_symbols: vec!["^IXIC".into()],
            tencent_codes: vec!["usIXIC".into()],
        }),
        "SPX" | "SP500" | "GSPC" => Some(ResolvedSymbol {
            market: Market::Us,
            code: "SPX".into(),
            yahoo_symbols: vec!["^GSPC".into()],
            tencent_codes: vec!["usSPX".into()],
        }),
        _ => None,
    }
}

fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn yahoo_chart_url(yahoo_symbol: &str, interval: &str, range: &str) -> String {
    format!(
        "https://query1.finance.yahoo.com/v8/finance/chart/{}?interval={}&range={}",
        encode_path_segment(yahoo_symbol),
        encode_path_segment(interval),
        encode_path_segment(range)
    )
}

fn resolve_symbol(raw: &str, market_hint: Option<&str>) -> Result<ResolvedSymbol, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Empty symbol".into());
    }

    // Index aliases (KOSPI / HSI / DJI …) take priority over market digit rules.
    if let Some(alias) = resolve_index_alias(trimmed) {
        return Ok(alias);
    }

    let upper = trimmed.to_uppercase();
    let hinted = market_hint.and_then(Market::parse);

    // Explicit suffixes / prefixes in the typed symbol win over the toolbar market.
    let (market, body) = if let Some(rest) = upper.strip_suffix(".HK") {
        (Market::Hk, rest.to_string())
    } else if let Some(rest) = upper.strip_suffix(".KS") {
        (Market::Kr, rest.to_string())
    } else if let Some(rest) = upper.strip_suffix(".KQ") {
        (Market::Kr, rest.to_string())
    } else if let Some(rest) = upper.strip_prefix("HK") {
        (Market::Hk, rest.to_string())
    } else if let Some(rest) = upper.strip_prefix("KR") {
        (Market::Kr, rest.to_string())
    } else if upper.starts_with("US")
        && upper.len() > 2
        && upper[2..].chars().all(|c| c.is_ascii_alphabetic())
    {
        (Market::Us, upper[2..].to_string())
    } else if let Some(m) = hinted {
        (m, upper.clone())
    } else if upper.chars().all(|c| c.is_ascii_digit()) {
        // Pure digits without market context — prefer HK (common 5-digit codes).
        (Market::Hk, upper)
    } else {
        (Market::Us, upper)
    };

    let body: String = body
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if body.is_empty() {
        return Err("Invalid symbol".into());
    }

    // Alias may also match after stripping market prefixes (e.g. KRKOSPI).
    if let Some(alias) = resolve_index_alias(&body) {
        return Ok(alias);
    }

    match market {
        Market::Us => {
            let code = body;
            Ok(ResolvedSymbol {
                market,
                yahoo_symbols: vec![code.clone()],
                tencent_codes: vec![
                    format!("us{code}"),
                    format!("us{code}.OQ"),
                    format!("us{code}.N"),
                ],
                code,
            })
        }
        Market::Hk => {
            let digits: String = body.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                return Err("HK: use stock code (00700) or index name (HSI)".into());
            }
            let code = format!("{digits:0>5}"); // Tencent: hk00700
            let yahoo_num = digits.trim_start_matches('0');
            let yahoo_num = if yahoo_num.is_empty() { "0" } else { yahoo_num };
            let yahoo = format!("{yahoo_num:0>4}.HK"); // Yahoo: 0700.HK
            Ok(ResolvedSymbol {
                market,
                yahoo_symbols: vec![yahoo, format!("{code}.HK")],
                tencent_codes: vec![format!("hk{code}")],
                code,
            })
        }
        Market::Kr => {
            let digits: String = body.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                return Err("KR: use stock code (005930) or index name (KOSPI / KOSDAQ)".into());
            }
            let code = format!("{digits:0>6}");
            Ok(ResolvedSymbol {
                market,
                yahoo_symbols: vec![format!("{code}.KS"), format!("{code}.KQ")],
                tencent_codes: vec![format!("kr{code}")],
                code,
            })
        }
    }
}

fn candles_from_yahoo_result(result: YahooResult) -> Vec<Candle> {
    let timestamps = result.timestamp.unwrap_or_default();
    let quote = match result.indicators.quote.into_iter().next() {
        Some(q) => q,
        None => return Vec::new(),
    };

    let opens = quote.open.unwrap_or_default();
    let highs = quote.high.unwrap_or_default();
    let lows = quote.low.unwrap_or_default();
    let closes = quote.close.unwrap_or_default();
    let volumes = quote.volume.unwrap_or_default();

    let mut candles = Vec::with_capacity(timestamps.len());
    for (i, &ts) in timestamps.iter().enumerate() {
        let open = opens.get(i).and_then(|v| *v);
        let high = highs.get(i).and_then(|v| *v);
        let low = lows.get(i).and_then(|v| *v);
        let close = closes.get(i).and_then(|v| *v);
        let volume = volumes.get(i).and_then(|v| *v).unwrap_or(0.0);

        if let (Some(o), Some(h), Some(l), Some(c)) = (open, high, low, close) {
            if o.is_finite() && h.is_finite() && l.is_finite() && c.is_finite() {
                candles.push(Candle {
                    time: ts,
                    open: o,
                    high: h,
                    low: l,
                    close: c,
                    volume,
                });
            }
        }
    }
    candles
}

async fn fetch_yahoo_klines(yahoo_symbol: &str, interval: &str) -> Result<Vec<Candle>, String> {
    let url = yahoo_chart_url(yahoo_symbol, interval, "1d");
    let client = client()?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Yahoo HTTP {}", resp.status()));
    }

    let body: YahooChartResponse = resp
        .json()
        .await
        .map_err(|e| format!("Parse failed: {e}"))?;

    if let Some(err) = body.chart.error {
        return Err(format!("Yahoo error: {err}"));
    }

    let result = body
        .chart
        .result
        .and_then(|mut v| v.pop())
        .ok_or_else(|| format!("No data for {yahoo_symbol}"))?;

    let candles = candles_from_yahoo_result(result);
    if candles.is_empty() {
        Err(format!("Empty candles for {yahoo_symbol}"))
    } else {
        Ok(candles)
    }
}

#[tauri::command]
async fn fetch_kline(
    symbol: String,
    period: String,
    market: Option<String>,
) -> Result<Vec<Candle>, String> {
    let resolved = resolve_symbol(&symbol, market.as_deref())?;
    let interval = match period.as_str() {
        "5m" | "m5" => "5m",
        _ => "1m",
    };

    let mut last_err = String::from("No kline source available");
    for yahoo in &resolved.yahoo_symbols {
        match fetch_yahoo_klines(yahoo, interval).await {
            Ok(candles) => return Ok(candles),
            Err(e) => last_err = e,
        }
    }

    match fetch_tencent_minute_as_candles(&resolved, interval).await {
        Ok(candles) => Ok(candles),
        Err(e) => Err(format!("{last_err}; tencent fallback: {e}")),
    }
}

async fn fetch_tencent_minute_as_candles(
    resolved: &ResolvedSymbol,
    interval: &str,
) -> Result<Vec<Candle>, String> {
    let (url, key, utc_offset_hours) = match resolved.market {
        Market::Us => (
            format!(
                "https://web.ifzq.gtimg.cn/appstock/app/UsMinute/query?code=us{}",
                resolved.code
            ),
            format!("us{}", resolved.code),
            -4_i64, // EDT approx
        ),
        Market::Hk => (
            format!(
                "https://web.ifzq.gtimg.cn/appstock/app/minute/query?code=hk{}",
                resolved.code
            ),
            format!("hk{}", resolved.code),
            8_i64, // HKT
        ),
        Market::Kr => {
            // Tencent KR minute feed is unreliable; keep a best-effort attempt.
            (
                format!(
                    "https://web.ifzq.gtimg.cn/appstock/app/minute/query?code=kr{}",
                    resolved.code
                ),
                format!("kr{}", resolved.code),
                9_i64, // KST
            )
        }
    };

    let client = client()?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Tencent minute request failed: {e}"))?;
    let value: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Tencent minute parse failed: {e}"))?;

    let rows = value
        .pointer(&format!("/data/{key}/data/data"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("No Tencent minute data for {key}"))?;

    let date_hint = value
        .pointer(&format!("/data/{key}/qt/{key}/29"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .replace('/', "-");

    let date_owned = if date_hint.len() >= 10 {
        date_hint[..10].to_string()
    } else {
        today_utc_date()
    };

    let mut points: Vec<(i64, f64, f64)> = Vec::new();
    let mut prev_vol = 0.0_f64;
    for row in rows {
        let s = match row.as_str() {
            Some(v) => v.trim(),
            None => continue,
        };
        let parts: Vec<&str> = s.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }
        let hhmm = parts[0];
        if hhmm.len() < 3 {
            continue;
        }
        let price: f64 = parts[1].parse().unwrap_or(f64::NAN);
        let cum_vol: f64 = parts[2].parse().unwrap_or(0.0);
        if !price.is_finite() {
            continue;
        }
        let hour: i64 = hhmm[..hhmm.len() - 2].parse().unwrap_or(0);
        let minute: i64 = hhmm[hhmm.len() - 2..].parse().unwrap_or(0);
        let ts = local_to_utc_timestamp(&date_owned, hour, minute, utc_offset_hours);
        let vol = (cum_vol - prev_vol).max(0.0);
        prev_vol = cum_vol;
        points.push((ts, price, vol));
    }

    if points.is_empty() {
        return Err(format!("Empty minute series for {key}"));
    }

    Ok(bucket_points_to_candles(&points, interval))
}

fn bucket_points_to_candles(points: &[(i64, f64, f64)], interval: &str) -> Vec<Candle> {
    let bucket = if interval == "5m" { 300 } else { 60 };
    let mut candles: Vec<Candle> = Vec::new();
    let mut bucket_start = points[0].0 - (points[0].0 % bucket);
    let mut o = points[0].1;
    let mut h = points[0].1;
    let mut l = points[0].1;
    let mut c = points[0].1;
    let mut v = points[0].2;

    for &(ts, price, vol) in points.iter().skip(1) {
        let start = ts - (ts % bucket);
        if start != bucket_start {
            candles.push(Candle {
                time: bucket_start,
                open: o,
                high: h,
                low: l,
                close: c,
                volume: v,
            });
            bucket_start = start;
            o = price;
            h = price;
            l = price;
            c = price;
            v = vol;
        } else {
            h = h.max(price);
            l = l.min(price);
            c = price;
            v += vol;
        }
    }
    candles.push(Candle {
        time: bucket_start,
        open: o,
        high: h,
        low: l,
        close: c,
        volume: v,
    });
    candles
}

fn today_utc_date() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86400;
    let (y, m, d) = civil_from_days(days as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    // Howard Hinnant algorithm
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

fn local_to_utc_timestamp(date: &str, hour: i64, minute: i64, utc_offset_hours: i64) -> i64 {
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 {
        return 0;
    }
    let y: i64 = parts[0].parse().unwrap_or(1970);
    let m: i64 = parts[1].parse().unwrap_or(1);
    let d: i64 = parts[2].parse().unwrap_or(1);
    let days = days_from_civil(y, m, d);
    days * 86400 + (hour - utc_offset_hours) * 3600 + minute * 60
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = y - if m <= 2 { 1 } else { 0 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

#[tauri::command]
async fn fetch_quote(symbol: String, market: Option<String>) -> Result<Quote, String> {
    let resolved = resolve_symbol(&symbol, market.as_deref())?;

    if !resolved.tencent_codes.is_empty() {
        if let Ok(q) = fetch_tencent_quote(&resolved).await {
            return Ok(q);
        }
    }
    fetch_yahoo_quote(&resolved).await
}

fn default_currency(market: Market) -> &'static str {
    match market {
        Market::Us => "USD",
        Market::Hk => "HKD",
        Market::Kr => "KRW",
    }
}

async fn fetch_tencent_quote(resolved: &ResolvedSymbol) -> Result<Quote, String> {
    let client = client()?;

    for code in &resolved.tencent_codes {
        let url = format!("https://qt.gtimg.cn/q={code}");
        let bytes = client
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .bytes()
            .await
            .map_err(|e| e.to_string())?;

        let (cow, _, _) = encoding_rs::GBK.decode(&bytes);
        let text = cow.into_owned();
        if text.contains("v_pv_none_match") || !text.contains('~') {
            continue;
        }

        let start = text.find('"').ok_or("bad quote format")?;
        let end = text.rfind('"').ok_or("bad quote format")?;
        if end <= start + 1 {
            continue;
        }
        let payload = &text[start + 1..end];
        let parts: Vec<&str> = payload.split('~').collect();
        if parts.len() < 6 {
            continue;
        }

        let price: f64 = parts[3].parse().unwrap_or(0.0);
        let prev: f64 = parts[4].parse().unwrap_or(0.0);
        if price <= 0.0 {
            continue;
        }
        let change = price - prev;
        let change_percent = if prev.abs() > f64::EPSILON {
            change / prev * 100.0
        } else {
            0.0
        };

        let currency = parts
            .get(35)
            .map(|s| s.trim())
            .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphabetic()))
            .unwrap_or_else(|| default_currency(resolved.market))
            .to_string();

        return Ok(Quote {
            symbol: resolved.code.clone(),
            name: parts[1].to_string(),
            price,
            prev_close: prev,
            change,
            change_percent,
            currency,
            market: resolved.market.as_str().to_string(),
        });
    }

    Err("Tencent quote not found".into())
}

async fn fetch_yahoo_quote(resolved: &ResolvedSymbol) -> Result<Quote, String> {
    let client = client()?;
    let mut last_err = String::from("No Yahoo quote");

    for yahoo_symbol in &resolved.yahoo_symbols {
        let url = yahoo_chart_url(yahoo_symbol, "1d", "1d");
        let body: YahooChartResponse = match client.get(&url).send().await {
            Ok(resp) => match resp.json().await {
                Ok(v) => v,
                Err(e) => {
                    last_err = e.to_string();
                    continue;
                }
            },
            Err(e) => {
                last_err = e.to_string();
                continue;
            }
        };

        let Some(result) = body.chart.result.and_then(|mut v| v.pop()) else {
            last_err = format!("No Yahoo quote for {yahoo_symbol}");
            continue;
        };

        let price = result.meta.regular_market_price.unwrap_or(0.0);
        if price <= 0.0 {
            last_err = format!("Invalid Yahoo price for {yahoo_symbol}");
            continue;
        }
        let prev = result
            .meta
            .chart_previous_close
            .or(result.meta.previous_close)
            .unwrap_or(price);
        let change = price - prev;
        let change_percent = if prev.abs() > f64::EPSILON {
            change / prev * 100.0
        } else {
            0.0
        };

        return Ok(Quote {
            symbol: resolved.code.clone(),
            name: result.meta.short_name.unwrap_or_default(),
            price,
            prev_close: prev,
            change,
            change_percent,
            currency: result
                .meta
                .currency
                .unwrap_or_else(|| default_currency(resolved.market).into()),
            market: resolved.market.as_str().to_string(),
        });
    }

    Err(last_err)
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let show_i = MenuItem::with_id(app, "show", "显示 / 隐藏", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Stock Widget")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => toggle_main_window(app),
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    let _tray = builder.build(app)?;
    Ok(())
}

fn toggle_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(true) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            fetch_kline,
            fetch_quote,
            load_settings,
            save_settings
        ])
        .setup(|app| {
            setup_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Close button / Alt+F4 → hide to tray instead of quitting
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
