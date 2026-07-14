use async_trait::async_trait;

use crate::models::{Candle, Quote, ResolvedSymbol};

mod tencent;
mod yahoo;

use tencent::TencentProvider;
use yahoo::YahooProvider;

#[async_trait]
pub trait MarketDataProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn fetch_kline(
        &self,
        resolved: &ResolvedSymbol,
        interval: &str,
    ) -> Result<Vec<Candle>, String>;
    async fn fetch_quote(&self, resolved: &ResolvedSymbol) -> Result<Quote, String>;
}

pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 StockWidget/0.1")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())
}

/// K-line fallback order: Yahoo → Tencent (matches previous behavior).
fn kline_providers() -> Vec<&'static dyn MarketDataProvider> {
    static YAHOO: YahooProvider = YahooProvider;
    static TENCENT: TencentProvider = TencentProvider;
    vec![&YAHOO, &TENCENT]
}

/// Quote fallback order: Tencent → Yahoo (matches previous behavior).
fn quote_providers() -> Vec<&'static dyn MarketDataProvider> {
    static YAHOO: YahooProvider = YahooProvider;
    static TENCENT: TencentProvider = TencentProvider;
    vec![&TENCENT, &YAHOO]
}

pub async fn fetch_kline_with_fallback(
    resolved: &ResolvedSymbol,
    interval: &str,
) -> Result<Vec<Candle>, String> {
    let mut errors: Vec<String> = Vec::new();
    for provider in kline_providers() {
        match provider.fetch_kline(resolved, interval).await {
            Ok(candles) => return Ok(candles),
            Err(e) => errors.push(format!("{}: {e}", provider.name())),
        }
    }
    if errors.is_empty() {
        Err("No kline source available".into())
    } else {
        Err(errors.join("; "))
    }
}

pub async fn fetch_quote_with_fallback(resolved: &ResolvedSymbol) -> Result<Quote, String> {
    let mut errors: Vec<String> = Vec::new();
    for provider in quote_providers() {
        match provider.fetch_quote(resolved).await {
            Ok(quote) => return Ok(quote),
            Err(e) => errors.push(format!("{}: {e}", provider.name())),
        }
    }
    if errors.is_empty() {
        Err("No quote source available".into())
    } else {
        Err(errors.join("; "))
    }
}
