//! Shared fixtures for the crate's #[cfg(test)] modules.

use crate::infer_creator;
use crate::types::{ProviderMarketQuote, Quote};

pub(crate) fn test_quote(model: &str, input: f64, live_market: bool) -> Quote {
    Quote {
        model: model.into(),
        display_name: model.into(),
        creator: infer_creator(model),
        provider: "test-provider".into(),
        input,
        output: input * 2.0,
        cache_read: Some(input * 0.1),
        seller_count: Some(12),
        healthy_seller_count: Some(8),
        provider_trusted: Some(true),
        requests_24h: Some(100),
        volume_24h: Some(5000.0),
        discount_pct: Some(25.0),
        discount_direction: Some("stable".into()),
        market_options: vec![ProviderMarketQuote {
            provider: "test-provider".into(),
            input,
            output: input * 2.0,
            cache_read: Some(input * 0.1),
            trusted: Some(true),
            healthy_seller_count: Some(8),
        }],
        live_market,
        vision: false,
    }
}
