use async_trait::async_trait;

use crate::models::{default_currency, Candle, Market, Quote, ResolvedSymbol, SymbolKind};
use crate::providers::{client, MarketDataProvider};

pub struct TencentProvider;

/// Map a vendor-agnostic symbol to Tencent qt.gtimg.cn codes.
/// Returns empty for symbols with no Tencent coverage (e.g. KOSPI / KOSDAQ).
fn tencent_codes(resolved: &ResolvedSymbol) -> Vec<String> {
    if resolved.kind == SymbolKind::Index {
        return match resolved.code.as_str() {
            "HSI" => vec!["hkHSI".into(), "s_hkHSI".into()],
            "DJI" => vec!["usDJI".into()],
            "IXIC" => vec!["usIXIC".into()],
            "SPX" => vec!["usSPX".into()],
            // KOSPI / KOSDAQ: no reliable Tencent codes
            _ => vec![],
        };
    }

    match resolved.market {
        Market::Us => vec![
            format!("us{}", resolved.code),
            format!("us{}.OQ", resolved.code),
            format!("us{}.N", resolved.code),
        ],
        Market::Hk => vec![format!("hk{}", resolved.code)],
        Market::Kr => vec![format!("kr{}", resolved.code)],
    }
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

async fn fetch_tencent_minute_as_candles(
    resolved: &ResolvedSymbol,
    interval: &str,
) -> Result<Vec<Candle>, String> {
    if resolved.kind == SymbolKind::Index
        && matches!(resolved.code.as_str(), "KOSPI" | "KOSDAQ")
    {
        return Err("Tencent: no minute feed for KOSPI/KOSDAQ".into());
    }

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

async fn fetch_tencent_quote(resolved: &ResolvedSymbol) -> Result<Quote, String> {
    let codes = tencent_codes(resolved);
    if codes.is_empty() {
        return Err(format!(
            "Tencent: no quote mapping for {}/{}",
            resolved.market.as_str(),
            resolved.code
        ));
    }

    let client = client()?;

    for code in &codes {
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

#[async_trait]
impl MarketDataProvider for TencentProvider {
    fn name(&self) -> &'static str {
        "tencent"
    }

    async fn fetch_kline(
        &self,
        resolved: &ResolvedSymbol,
        interval: &str,
    ) -> Result<Vec<Candle>, String> {
        fetch_tencent_minute_as_candles(resolved, interval).await
    }

    async fn fetch_quote(&self, resolved: &ResolvedSymbol) -> Result<Quote, String> {
        fetch_tencent_quote(resolved).await
    }
}
