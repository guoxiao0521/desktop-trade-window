use async_trait::async_trait;
use serde::Deserialize;

use crate::models::{default_currency, Candle, Market, Quote, ResolvedSymbol, SymbolKind};
use crate::providers::{client, MarketDataProvider};

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

pub struct YahooProvider;

/// Map a vendor-agnostic symbol to Yahoo Finance ticker candidates.
fn yahoo_symbols(resolved: &ResolvedSymbol) -> Vec<String> {
    if resolved.kind == SymbolKind::Index {
        return match resolved.code.as_str() {
            "KOSPI" => vec!["^KS11".into()],
            "KOSDAQ" => vec!["^KQ11".into()],
            "HSI" => vec!["^HSI".into()],
            "DJI" => vec!["^DJI".into()],
            "IXIC" => vec!["^IXIC".into()],
            "SPX" => vec!["^GSPC".into()],
            _ => vec![],
        };
    }

    match resolved.market {
        Market::Us => vec![resolved.code.clone()],
        Market::Hk => {
            let digits: String = resolved.code.chars().filter(|c| c.is_ascii_digit()).collect();
            let yahoo_num = digits.trim_start_matches('0');
            let yahoo_num = if yahoo_num.is_empty() { "0" } else { yahoo_num };
            let yahoo = format!("{yahoo_num:0>4}.HK");
            vec![yahoo, format!("{}.HK", resolved.code)]
        }
        Market::Kr => vec![
            format!("{}.KS", resolved.code),
            format!("{}.KQ", resolved.code),
        ],
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

#[async_trait]
impl MarketDataProvider for YahooProvider {
    fn name(&self) -> &'static str {
        "yahoo"
    }

    async fn fetch_kline(
        &self,
        resolved: &ResolvedSymbol,
        interval: &str,
    ) -> Result<Vec<Candle>, String> {
        let symbols = yahoo_symbols(resolved);
        if symbols.is_empty() {
            return Err(format!(
                "Yahoo: no symbol mapping for {}/{}",
                resolved.market.as_str(),
                resolved.code
            ));
        }

        let mut last_err = String::from("No Yahoo kline");
        for yahoo in &symbols {
            match fetch_yahoo_klines(yahoo, interval).await {
                Ok(candles) => return Ok(candles),
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }

    async fn fetch_quote(&self, resolved: &ResolvedSymbol) -> Result<Quote, String> {
        let symbols = yahoo_symbols(resolved);
        if symbols.is_empty() {
            return Err(format!(
                "Yahoo: no symbol mapping for {}/{}",
                resolved.market.as_str(),
                resolved.code
            ));
        }

        let client = client()?;
        let mut last_err = String::from("No Yahoo quote");

        for yahoo_symbol in &symbols {
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
}
