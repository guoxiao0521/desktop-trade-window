use serde::Serialize;

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
pub enum Market {
    Us,
    Hk,
    Kr,
}

impl Market {
    pub fn as_str(self) -> &'static str {
        match self {
            Market::Us => "US",
            Market::Hk => "HK",
            Market::Kr => "KR",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_uppercase().as_str() {
            "US" | "USA" | "NYSE" | "NASDAQ" => Some(Market::Us),
            "HK" | "HKG" | "HKEX" => Some(Market::Hk),
            "KR" | "KS" | "KQ" | "KRX" | "KOSPI" | "KOSDAQ" => Some(Market::Kr),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Stock,
    Index,
}

/// Vendor-agnostic resolved symbol: market + canonical code + kind.
/// Provider-specific tickers (Yahoo `^KS11`, Tencent `hkHSI`, etc.) live inside each provider.
#[derive(Debug, Clone)]
pub struct ResolvedSymbol {
    pub market: Market,
    /// Canonical code shown in UI, e.g. AAPL / 00700 / 005930 / KOSPI
    pub code: String,
    pub kind: SymbolKind,
}

pub fn default_currency(market: Market) -> &'static str {
    match market {
        Market::Us => "USD",
        Market::Hk => "HKD",
        Market::Kr => "KRW",
    }
}

/// Common index aliases → market + canonical code.
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
        "KOSPI" | "KS11" | "KOSPI200" => Some(ResolvedSymbol {
            market: Market::Kr,
            code: "KOSPI".into(),
            kind: SymbolKind::Index,
        }),
        "KOSDAQ" | "KQ11" => Some(ResolvedSymbol {
            market: Market::Kr,
            code: "KOSDAQ".into(),
            kind: SymbolKind::Index,
        }),
        "HSI" | "HANGSENG" | "HSINDEX" => Some(ResolvedSymbol {
            market: Market::Hk,
            code: "HSI".into(),
            kind: SymbolKind::Index,
        }),
        "DJI" | "DJIA" | "DOW" => Some(ResolvedSymbol {
            market: Market::Us,
            code: "DJI".into(),
            kind: SymbolKind::Index,
        }),
        "IXIC" | "NASDAQ" | "NDXCOMP" => Some(ResolvedSymbol {
            market: Market::Us,
            code: "IXIC".into(),
            kind: SymbolKind::Index,
        }),
        "SPX" | "SP500" | "GSPC" => Some(ResolvedSymbol {
            market: Market::Us,
            code: "SPX".into(),
            kind: SymbolKind::Index,
        }),
        _ => None,
    }
}

pub fn resolve_symbol(raw: &str, market_hint: Option<&str>) -> Result<ResolvedSymbol, String> {
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
        Market::Us => Ok(ResolvedSymbol {
            market,
            code: body,
            kind: SymbolKind::Stock,
        }),
        Market::Hk => {
            let digits: String = body.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                return Err("HK: use stock code (00700) or index name (HSI)".into());
            }
            Ok(ResolvedSymbol {
                market,
                code: format!("{digits:0>5}"),
                kind: SymbolKind::Stock,
            })
        }
        Market::Kr => {
            let digits: String = body.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                return Err("KR: use stock code (005930) or index name (KOSPI / KOSDAQ)".into());
            }
            Ok(ResolvedSymbol {
                market,
                code: format!("{digits:0>6}"),
                kind: SymbolKind::Stock,
            })
        }
    }
}
