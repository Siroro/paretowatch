//! Surplus price feeds: the comparison-matrix price page, the `/v1/models`
//! catalog (modalities, creators), and the live market overlay.

use std::collections::HashMap;

use anyhow::{Result, anyhow};
use chrono::Utc;
use reqwest::blocking::Client;
use serde_json::Value;

use super::fetch_json;
use super::{first_number_path, first_string_path, json_shape, string_at};
use crate::bench::matching::normalize;
use crate::theme::infer_creator;
use crate::types::*;

pub(crate) const SURPLUS_MARKETS_URL: &str = "https://api.surplusintelligence.ai/api/markets";
pub(crate) const SURPLUS_PRICES_URL: &str = "https://api.surplusintelligence.ai/v1/prices";
pub(crate) const SURPLUS_MODELS_URL: &str = "https://api.surplusintelligence.ai/v1/models";

pub(crate) fn fetch_prices(client: &Client, settings: &Settings) -> Result<PriceSnapshot> {
    let mut base_errors = Vec::new();
    let mut comparison_updated_at = None;
    let mut base_source = String::new();

    // `/v1/models` is the authority for modality. We fetch it even when the
    // comparison matrix succeeds so non-text products can never leak in from
    // `/v1/prices` or `/api/markets` just because they also expose numeric prices.
    let catalog_quotes = match fetch_json(client, SURPLUS_MODELS_URL, "Surplus /v1/models") {
        Ok(value) => match parse_model_catalog(&value) {
            Ok(parsed) if !parsed.is_empty() => parsed,
            Ok(_) => {
                base_errors.push(format!(
                    "/v1/models returned no text-output model prices; JSON shape: {}",
                    json_shape(&value)
                ));
                Vec::new()
            }
            Err(err) => {
                base_errors.push(format!("/v1/models parse: {err:#}"));
                Vec::new()
            }
        },
        Err(err) => {
            base_errors.push(format!("/v1/models: {err:#}"));
            Vec::new()
        }
    };

    let mut quotes = Vec::new();
    match fetch_json(client, SURPLUS_PRICES_URL, "Surplus /v1/prices") {
        Ok(value) => {
            comparison_updated_at = value
                .get("updated_at")
                .and_then(Value::as_str)
                .map(str::to_owned);
            match parse_price_matrix(&value, settings) {
                Ok(parsed) if !parsed.is_empty() => {
                    quotes = parsed;
                    base_source = "/v1/prices + text catalog".into();
                }
                Ok(_) => base_errors.push(format!(
                    "/v1/prices returned no usable model prices; JSON shape: {}",
                    json_shape(&value)
                )),
                Err(err) => base_errors.push(format!("/v1/prices parse: {err:#}")),
            }
        }
        Err(err) => base_errors.push(format!("/v1/prices: {err:#}")),
    }

    if !catalog_quotes.is_empty() {
        let catalog_by_key = catalog_quotes
            .iter()
            .cloned()
            .map(|q| (normalize(&q.model), q))
            .collect::<HashMap<_, _>>();

        // Drop anything the text-output catalog does not recognize, and use its
        // canonical display/provider metadata for more reliable benchmark joins.
        quotes.retain(|q| catalog_by_key.contains_key(&normalize(&q.model)));
        for quote in &mut quotes {
            if let Some(catalog) = catalog_by_key.get(&normalize(&quote.model)) {
                quote.model = catalog.model.clone();
                quote.display_name = catalog.display_name.clone();
                quote.creator = catalog.creator.clone();
                quote.vision = catalog.vision;
            }
        }

        // The comparison matrix may lag the catalog. Keep missing text models
        // available using their catalog price until the matrix catches up.
        for catalog in catalog_quotes.iter().cloned() {
            let key = normalize(&catalog.model);
            if !quotes.iter().any(|q| normalize(&q.model) == key) {
                quotes.push(catalog);
            }
        }
        if base_source.is_empty() {
            base_source = "/v1/models text fallback".into();
        }
    } else {
        // If modality metadata is temporarily unavailable, fail closed on obvious
        // non-LLM products instead of filling the tray with image/video/audio rows.
        quotes.retain(|q| is_probably_text_model_label(&q.model, &q.display_name));
    }

    if quotes.is_empty() {
        return Err(anyhow!(
            "No Surplus text-model pricing source produced models. {}",
            base_errors.join(" | ")
        ));
    }

    let mut market_error = None;
    let mut overlay_count = 0usize;
    match fetch_json(client, SURPLUS_MARKETS_URL, "Surplus /api/markets") {
        Ok(value) => {
            let markets = parse_market_overlay(&value, settings);
            if markets.is_empty() {
                market_error = Some(format!(
                    "/api/markets returned no usable text-token quotes; JSON shape: {}",
                    json_shape(&value)
                ));
            } else {
                for market in markets {
                    let target = normalize(&market.model);
                    // Only overlay models already admitted by the text-output
                    // catalog. Do not append unmatched market products: the market
                    // feed also contains image, video, music and other media SKUs.
                    if let Some(q) = quotes.iter_mut().find(|q| normalize(&q.model) == target) {
                        q.input = market.input;
                        q.output = market.output;
                        q.cache_read = market.cache_read;
                        if let Some(provider) = market.provider {
                            q.provider = provider;
                        }
                        q.seller_count = market.seller_count;
                        q.healthy_seller_count = market.healthy_seller_count;
                        q.provider_trusted = market.provider_trusted;
                        q.requests_24h = market.requests_24h;
                        q.volume_24h = market.volume_24h;
                        q.discount_pct = market.discount_pct;
                        q.discount_direction = market.discount_direction.clone();
                        q.free_offer_listed = market.free_offer_listed;
                        q.market_options = market.provider_options.clone();
                        q.live_market = true;
                        overlay_count += 1;
                    }
                }
            }
        }
        Err(err) => market_error = Some(format!("{err:#}")),
    }

    quotes.sort_by(|a, b| a.display_name.cmp(&b.display_name));
    Ok(PriceSnapshot {
        quotes,
        fetched_at: Utc::now(),
        comparison_updated_at,
        market_overlay_count: overlay_count,
        market_error,
        base_source,
    })
}

pub(crate) fn parse_price_matrix(value: &Value, settings: &Settings) -> Result<Vec<Quote>> {
    let models = find_model_entries(value);
    if models.is_empty() {
        return Err(anyhow!(
            "no model array found; JSON shape: {}",
            json_shape(value)
        ));
    }
    let mut quotes = Vec::new();

    for model in models {
        let Some(model_id) = string_at(model, &["model", "id", "model_id", "modelId"]) else {
            continue;
        };
        let display_name = string_at(model, &["displayName", "display_name", "name"])
            .unwrap_or_else(|| model_id.clone());
        let creator = string_at(model, &["provider", "creator", "model_creator"])
            .unwrap_or_else(|| infer_creator(&model_id));
        let mut best: Option<(String, f64, f64, Option<f64>, f64)> = None;

        if let Some(providers) = model.get("providers").and_then(Value::as_object) {
            for (provider, price) in providers {
                let Some((input, output)) = price_pair_per_million(price) else {
                    continue;
                };
                let cache_read = cache_read_per_million(price);
                let weighted = blended_price(
                    input,
                    cache_read,
                    output,
                    settings.input_weight,
                    settings.cache_read_weight,
                    settings.output_weight,
                );
                if best
                    .as_ref()
                    .map(|(_, _, _, _, current)| weighted < *current)
                    .unwrap_or(true)
                {
                    best = Some((provider.clone(), input, output, cache_read, weighted));
                }
            }
        }
        if best.is_none() {
            if let Some(cheapest) = model.get("cheapest") {
                if let Some((input, output)) = price_pair_per_million(cheapest) {
                    let cache_read = cache_read_per_million(cheapest);
                    let provider = string_at(cheapest, &["provider", "name"])
                        .unwrap_or_else(|| "cheapest".into());
                    let weighted = blended_price(
                        input,
                        cache_read,
                        output,
                        settings.input_weight,
                        settings.cache_read_weight,
                        settings.output_weight,
                    );
                    best = Some((provider, input, output, cache_read, weighted));
                }
            }
        }
        if best.is_none() {
            if let Some((input, output)) = price_pair_per_million(model) {
                let cache_read = cache_read_per_million(model);
                let weighted = blended_price(
                    input,
                    cache_read,
                    output,
                    settings.input_weight,
                    settings.cache_read_weight,
                    settings.output_weight,
                );
                best = Some(("comparison".into(), input, output, cache_read, weighted));
            }
        }

        if let Some((provider, input, output, cache_read, _)) = best {
            quotes.push(Quote {
                model: model_id,
                display_name,
                creator,
                provider,
                input,
                output,
                cache_read,
                seller_count: None,
                healthy_seller_count: None,
                provider_trusted: None,
                requests_24h: None,
                volume_24h: None,
                discount_pct: None,
                discount_direction: None,
                free_offer_listed: false,
                market_options: vec![],
                live_market: false,
                vision: false,
            });
        }
    }
    Ok(quotes)
}

pub(crate) fn parse_model_catalog(value: &Value) -> Result<Vec<Quote>> {
    let models = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("no data array found; JSON shape: {}", json_shape(value)))?;
    let mut out = Vec::new();
    for model in models {
        let Some(id) = string_at(model, &["id", "model", "model_id"]) else {
            continue;
        };
        let name = string_at(model, &["name", "displayName", "display_name"])
            .unwrap_or_else(|| id.clone());
        if !catalog_is_text_model(model, &id, &name) {
            continue;
        }
        let Some(pricing) = model.get("pricing") else {
            continue;
        };
        let Some(prompt) = first_number_path(pricing, &[&["prompt"], &["input"]]) else {
            continue;
        };
        let Some(completion) = first_number_path(pricing, &[&["completion"], &["output"]]) else {
            continue;
        };
        if !prompt.is_finite() || !completion.is_finite() || prompt < 0.0 || completion < 0.0 {
            continue;
        }
        let cache_read = first_number_path(
            pricing,
            &[&["input_cache_read"], &["cache_read"], &["cacheRead"]],
        )
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| v * 1_000_000.0);
        let creator = string_at(model, &["provider", "creator", "organization"])
            .unwrap_or_else(|| infer_creator(&id));
        out.push(Quote {
            model: id.clone(),
            display_name: name,
            creator,
            provider: "Surplus catalog".into(),
            input: prompt * 1_000_000.0,
            output: completion * 1_000_000.0,
            cache_read,
            seller_count: None,
            healthy_seller_count: None,
            provider_trusted: None,
            requests_24h: None,
            volume_24h: None,
            discount_pct: None,
            discount_direction: None,
            free_offer_listed: false,
            market_options: vec![],
            live_market: false,
            vision: catalog_supports_vision(model),
        });
    }
    Ok(out)
}

/// Vision = accepts image *input* while still emitting text output. Derived
/// from the same `/v1/models` architecture metadata that gates text models.
pub(crate) fn catalog_supports_vision(model: &Value) -> bool {
    let Some(architecture) = model.get("architecture") else {
        return false;
    };
    if let Some(inputs) = architecture
        .get("input_modalities")
        .and_then(Value::as_array)
    {
        return inputs
            .iter()
            .filter_map(Value::as_str)
            .any(|m| normalize(m) == "image");
    }
    architecture
        .get("modality")
        .and_then(Value::as_str)
        .and_then(|m| m.split_once("->"))
        .map(|(input, _)| normalize(input).contains("image"))
        .unwrap_or(false)
}

pub(crate) fn catalog_is_text_model(model: &Value, id: &str, name: &str) -> bool {
    if !is_probably_text_model_label(id, name) {
        return false;
    }
    let Some(architecture) = model.get("architecture") else {
        return true;
    };

    if let Some(outputs) = architecture
        .get("output_modalities")
        .and_then(Value::as_array)
    {
        let modalities = outputs
            .iter()
            .filter_map(Value::as_str)
            .map(normalize)
            .collect::<Vec<_>>();
        // We want language models. Multimodal *input* is fine (vision LLMs), but
        // the output must be text-only; image/audio/video generators are excluded.
        return !modalities.is_empty() && modalities.iter().all(|m| m == "text");
    }
    if let Some(modality) = architecture.get("modality").and_then(Value::as_str) {
        let n = normalize(modality);
        if let Some((_, output)) = modality.split_once("->") {
            return normalize(output) == "text";
        }
        if n.contains("image") || n.contains("video") || n.contains("audio") {
            return false;
        }
    }
    true
}

pub(crate) fn is_probably_text_model_label(id: &str, name: &str) -> bool {
    let n = format!("{} {}", normalize(id), normalize(name));
    // Product families that are not text-generating LLMs. Keep multimodal-input
    // LLMs unless the product itself is explicitly an image/video/audio generator.
    let blocked = [
        "image to video",
        "text to video",
        "reference to video",
        "video",
        "image generator",
        "image generation",
        "music",
        "tts",
        "text to speech",
        "speech to text",
        "audio",
        "embedding",
        "embeddings",
        "rerank",
        "reranker",
        "whisper",
        "seedream",
        "veo",
        "kling",
        "pixverse",
        "ltx",
        "wan 2",
    ];
    !blocked.iter().any(|term| n.contains(term)) && !normalize(id).contains("image")
}

pub(crate) fn find_model_entries(root: &Value) -> Vec<&Value> {
    if let Some(arr) = root.as_array() {
        return arr.iter().collect();
    }
    for key in ["models", "items"] {
        if let Some(arr) = root.get(key).and_then(Value::as_array) {
            return arr.iter().collect();
        }
    }
    if let Some(data) = root.get("data") {
        if let Some(arr) = data.as_array() {
            return arr.iter().collect();
        }
        if let Some(arr) = data.get("models").and_then(Value::as_array) {
            return arr.iter().collect();
        }
    }
    Vec::new()
}

pub(crate) fn price_pair_per_million(value: &Value) -> Option<(f64, f64)> {
    let input = first_number_path(
        value,
        &[
            &["input"],
            &["input_price"],
            &["inputPrice"],
            &["price_input_per_1m"],
            &["inputPricePerMillion"],
        ],
    )?;
    let output = first_number_path(
        value,
        &[
            &["output"],
            &["output_price"],
            &["outputPrice"],
            &["price_output_per_1m"],
            &["outputPricePerMillion"],
        ],
    )?;
    if input.is_finite() && output.is_finite() && input >= 0.0 && output >= 0.0 {
        Some((input, output))
    } else {
        None
    }
}

pub(crate) fn cache_read_per_million(value: &Value) -> Option<f64> {
    first_number_path(
        value,
        &[
            &["cache_read"],
            &["cacheRead"],
            &["input_cache_read"],
            &["inputCacheRead"],
            &["cache_read_per_1m"],
            &["cacheReadPer1m"],
            &["best_cache_read_per_1m"],
            &["bestCacheReadPer1m"],
        ],
    )
    .filter(|v| v.is_finite() && *v >= 0.0)
}

#[derive(Debug)]
pub(crate) struct MarketOverlay {
    pub(crate) model: String,
    pub(crate) input: f64,
    pub(crate) output: f64,
    pub(crate) cache_read: Option<f64>,
    pub(crate) provider: Option<String>,
    pub(crate) seller_count: Option<u64>,
    pub(crate) healthy_seller_count: Option<u64>,
    pub(crate) provider_trusted: Option<bool>,
    pub(crate) requests_24h: Option<u64>,
    pub(crate) volume_24h: Option<f64>,
    pub(crate) discount_pct: Option<f64>,
    pub(crate) discount_direction: Option<String>,
    pub(crate) free_offer_listed: bool,
    pub(crate) provider_options: Vec<ProviderMarketQuote>,
}

pub(crate) const SURPLUS_MARKET_MICRO_USD_PER_USD: f64 = 1_000_000.0;

pub(crate) fn parse_market_overlay(root: &Value, settings: &Settings) -> Vec<MarketOverlay> {
    let entries = find_market_entries(root);
    let mut out = Vec::new();

    for entry in entries {
        let Some(model) = string_at(entry, &["model", "model_name", "modelName", "id", "name"])
        else {
            continue;
        };
        let input_micro = first_number_path(
            entry,
            &[
                &["best_input_per_1m"],
                &["bestInputPer1m"],
                &["best_input"],
                &["bestInput"],
            ],
        );
        let output_micro = first_number_path(
            entry,
            &[
                &["best_output_per_1m"],
                &["bestOutputPer1m"],
                &["best_output"],
                &["bestOutput"],
            ],
        );
        let (Some(input_micro), Some(output_micro)) = (input_micro, output_micro) else {
            continue;
        };
        if !input_micro.is_finite()
            || !output_micro.is_finite()
            || input_micro < 0.0
            || output_micro < 0.0
        {
            continue;
        }
        // Media SKUs price per generated unit, not per token: their token pair
        // is $0 because tokens are not the product. Token markets with $0 asks
        // are real (100%-off offers) and must survive; they are only kept out of
        // the headline price in favour of the cheapest priced ask below. Rows
        // with no sellers at all have nothing to overlay regardless.
        let media_unit_present = entry.get("media_unit").is_some_and(|v| !v.is_null());
        let seller_free = first_number_path(
            entry,
            &[
                &["seller_count"],
                &["num_sellers"],
                &["sellerCount"],
                &["numSellers"],
            ],
        )
        .map(|n| n <= 0.0)
        .unwrap_or(false);
        if is_free_pair(input_micro, output_micro) && (media_unit_present || seller_free) {
            continue;
        }

        let top_input = input_micro / SURPLUS_MARKET_MICRO_USD_PER_USD;
        let top_output = output_micro / SURPLUS_MARKET_MICRO_USD_PER_USD;
        let top_cache_read = first_number_path(
            entry,
            &[&["best_cache_read_per_1m"], &["bestCacheReadPer1m"]],
        )
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| v / SURPLUS_MARKET_MICRO_USD_PER_USD);

        // Retain every real provider quote, then pick the cheapest for the
        // configured workload as the default display price. Liquidity filters can
        // later re-select the cheapest trusted / sufficiently healthy provider.
        let provider_options = entry
            .get("providers")
            .and_then(Value::as_array)
            .map(|providers| {
                providers
                    .iter()
                    .filter_map(|p| {
                        let provider = string_at(p, &["provider", "name"])?;
                        let i_micro =
                            first_number_path(p, &[&["best_input_per_1m"], &["bestInputPer1m"]])?;
                        let o_micro =
                            first_number_path(p, &[&["best_output_per_1m"], &["bestOutputPer1m"]])?;
                        if !i_micro.is_finite()
                            || !o_micro.is_finite()
                            || i_micro < 0.0
                            || o_micro < 0.0
                        {
                            return None;
                        }
                        let input = i_micro / SURPLUS_MARKET_MICRO_USD_PER_USD;
                        let output = o_micro / SURPLUS_MARKET_MICRO_USD_PER_USD;
                        let cache_read = first_number_path(
                            p,
                            &[&["best_cache_read_per_1m"], &["bestCacheReadPer1m"]],
                        )
                        .filter(|v| v.is_finite() && *v >= 0.0)
                        .map(|v| v / SURPLUS_MARKET_MICRO_USD_PER_USD);
                        let trusted = p.get("trusted").and_then(Value::as_bool);
                        let healthy_seller_count = first_number_path(
                            p,
                            &[&["healthy_seller_count"], &["healthySellerCount"]],
                        )
                        .and_then(|n| (n >= 0.0).then_some(n as u64));
                        Some(ProviderMarketQuote {
                            provider,
                            input,
                            output,
                            cache_read,
                            trusted,
                            healthy_seller_count,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        // A zero-priced aggregate means that provider's cheapest ask is a
        // 100%-off offer. It stays visible in provider_options but must not own
        // the headline price while any priced provider exists; only an entirely
        // free market keeps it.
        let by_workload = |a: &&ProviderMarketQuote, b: &&ProviderMarketQuote| {
            a.workload_price(
                settings.input_weight,
                settings.cache_read_weight,
                settings.output_weight,
            )
            .total_cmp(&b.workload_price(
                settings.input_weight,
                settings.cache_read_weight,
                settings.output_weight,
            ))
        };
        let priced_options = provider_options
            .iter()
            .filter(|option| !is_free_pair(option.input, option.output))
            .collect::<Vec<_>>();
        let provider_quote = priced_options
            .into_iter()
            .min_by(|a, b| by_workload(a, b))
            .or_else(|| provider_options.iter().min_by(|a, b| by_workload(a, b)));
        let free_offer_listed = provider_options.iter().any(|option| is_free_pair(option.input, option.output))
            // Payloads without a provider array still say so via a $0 best pair.
            || (provider_options.is_empty() && is_free_pair(top_input, top_output));

        let fallback_provider = first_string_path(
            entry,
            &[
                &["provider"],
                &["best_ask", "provider"],
                &["bestAsk", "provider"],
            ],
        );
        let (provider, input, output, cache_read, provider_trusted, provider_healthy) =
            match provider_quote {
                Some(option) => (
                    Some(option.provider.clone()),
                    option.input,
                    option.output,
                    option.cache_read,
                    option.trusted,
                    option.healthy_seller_count,
                ),
                None => (
                    fallback_provider,
                    top_input,
                    top_output,
                    top_cache_read,
                    None,
                    None,
                ),
            };

        let seller_count = first_number_path(
            entry,
            &[
                &["seller_count"],
                &["num_sellers"],
                &["sellerCount"],
                &["numSellers"],
            ],
        )
        .and_then(|n| (n >= 0.0).then_some(n as u64));
        let healthy_seller_count = provider_healthy.or_else(|| {
            first_number_path(entry, &[&["healthy_seller_count"], &["healthySellerCount"]])
                .and_then(|n| (n >= 0.0).then_some(n as u64))
        });
        let requests_24h = first_number_path(entry, &[&["requests_24h"], &["requests24h"]])
            .and_then(|n| (n >= 0.0).then_some(n as u64));
        let volume_24h = first_number_path(entry, &[&["volume_24h"], &["volume24h"]])
            .filter(|n| n.is_finite() && *n >= 0.0);
        let discount_pct = first_number_path(
            entry,
            &[
                &["discount_trend", "current_discount_pct"],
                &["best_discount_pct"],
            ],
        )
        .filter(|n| n.is_finite());
        let discount_direction = first_string_path(entry, &[&["discount_trend", "direction"]]);

        out.push(MarketOverlay {
            model,
            input,
            output,
            cache_read,
            provider,
            seller_count,
            healthy_seller_count,
            provider_trusted,
            requests_24h,
            volume_24h,
            discount_pct,
            discount_direction,
            free_offer_listed,
            provider_options,
        });
    }
    out
}

pub(crate) fn find_market_entries(root: &Value) -> Vec<&Value> {
    if let Some(arr) = root.as_array() {
        return arr.iter().collect();
    }
    for key in ["models", "markets", "data", "items"] {
        if let Some(arr) = root.get(key).and_then(Value::as_array) {
            return arr.iter().collect();
        }
    }
    if let Some(data) = root.get("data") {
        for key in ["models", "markets", "items"] {
            if let Some(arr) = data.get(key).and_then(Value::as_array) {
                return arr.iter().collect();
            }
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testfix::test_quote;

    #[test]
    fn parses_current_surplus_live_market_shape_and_metadata() {
        let v = serde_json::json!({
            "markets": [{
                "model": "openai-gpt-oss-120b",
                "best_input_per_1m": 3493,
                "best_output_per_1m": 14969,
                "best_cache_read_per_1m": 524,
                "media_unit": null,
                "seller_count": 187,
                "healthy_seller_count": 59,
                "requests_24h": 641,
                "volume_24h": 203913,
                "discount_trend": {"current_discount_pct": 37.83, "direction": "tightening"},
                "providers": [
                    {"provider": "bedrock", "trusted": true, "healthy_seller_count": 5,
                     "best_input_per_1m": 3500, "best_output_per_1m": 15000, "best_cache_read_per_1m": 525},
                    {"provider": "unknown", "trusted": false, "healthy_seller_count": 5,
                     "best_input_per_1m": 3493, "best_output_per_1m": 14969, "best_cache_read_per_1m": 524}
                ]
            }]
        });
        let rows = parse_market_overlay(&v, &Settings::default());
        assert_eq!(rows.len(), 1);
        assert!((rows[0].input - 0.003493).abs() < 1e-12);
        assert_eq!(rows[0].provider.as_deref(), Some("unknown"));
        assert_eq!(rows[0].provider_trusted, Some(false));
        assert_eq!(rows[0].healthy_seller_count, Some(5));
        assert_eq!(rows[0].requests_24h, Some(641));
        assert_eq!(rows[0].discount_direction.as_deref(), Some("tightening"));
        assert!(!rows[0].free_offer_listed);
    }

    #[test]
    fn zero_priced_ask_defers_to_cheapest_priced_provider() {
        // Live gpt-5.6-sol shape (2026-08): one seller lists at cost multiplier
        // zero, so the aggregate best pair is $0 while every other ask costs.
        let v = serde_json::json!({
            "markets": [{
                "model": "gpt-5.6-sol",
                "best_input_per_1m": 0,
                "best_output_per_1m": 0,
                "best_cache_read_per_1m": null,
                "media_unit": null,
                "seller_count": 924,
                "healthy_seller_count": 788,
                "discount_trend": {"current_discount_pct": 95.55, "direction": "tightening"},
                "providers": [
                    {"provider": "openai", "trusted": true, "healthy_seller_count": 548,
                     "best_input_per_1m": 0, "best_output_per_1m": 0},
                    {"provider": "inferhub", "trusted": false, "healthy_seller_count": 13,
                     "best_input_per_1m": 50826, "best_output_per_1m": 254130}
                ]
            }]
        });
        let rows = parse_market_overlay(&v, &Settings::default());
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.provider.as_deref(), Some("inferhub"));
        assert!((row.input - 0.050826).abs() < 1e-9);
        assert!((row.output - 0.25413).abs() < 1e-9);
        assert!(row.free_offer_listed);
        assert_eq!(row.discount_pct, Some(95.55));
    }

    #[test]
    fn entirely_free_token_markets_are_kept_at_zero_prices() {
        let v = serde_json::json!({
            "markets": [{
                "model": "free-model",
                "best_input_per_1m": 0,
                "best_output_per_1m": 0,
                "media_unit": null,
                "seller_count": 40,
                "providers": [
                    {"provider": "free-a", "trusted": true, "healthy_seller_count": 3,
                     "best_input_per_1m": 0, "best_output_per_1m": 0}
                ]
            }]
        });
        let rows = parse_market_overlay(&v, &Settings::default());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].input, 0.0);
        assert_eq!(rows[0].output, 0.0);
        assert!(rows[0].free_offer_listed);
    }

    #[test]
    fn media_unit_rows_still_skip_zero_token_prices() {
        let v = serde_json::json!({
            "markets": [{
                "model": "video-gen",
                "best_input_per_1m": 0,
                "best_output_per_1m": 0,
                "best_media_unit_price": 56000,
                "media_unit": "job",
                "seller_count": 1
            }]
        });
        assert!(parse_market_overlay(&v, &Settings::default()).is_empty());
    }

    #[test]
    fn market_quality_filter_can_reselect_a_trusted_provider() {
        let mut q = test_quote("model-a", 1.0, true);
        q.provider = "cheap-untrusted".into();
        q.input = 0.5;
        q.output = 1.0;
        q.cache_read = Some(0.05);
        q.provider_trusted = Some(false);
        q.market_options = vec![
            ProviderMarketQuote {
                provider: "cheap-untrusted".into(),
                input: 0.5,
                output: 1.0,
                cache_read: Some(0.05),
                trusted: Some(false),
                healthy_seller_count: Some(20),
            },
            ProviderMarketQuote {
                provider: "trusted-provider".into(),
                input: 0.6,
                output: 1.1,
                cache_read: Some(0.06),
                trusted: Some(true),
                healthy_seller_count: Some(5),
            },
        ];
        let selected = LiquidityFilter::Trusted.apply(&q, 15.0, 80.0, 5.0).unwrap();
        assert_eq!(selected.provider, "trusted-provider");
        assert_eq!(selected.provider_trusted, Some(true));
        assert!((selected.input - 0.6).abs() < 1e-12);
    }

    #[test]
    fn model_catalog_uses_provider_as_creator() {
        let value = serde_json::json!({"data":[{
            "id":"nvidia-nemotron-3-ultra-550b-a55b",
            "name":"NVIDIA Nemotron 3 Ultra",
            "provider":"NVIDIA",
            "pricing":{"prompt":"0.000001", "completion":"0.000002"}
        }]});
        let rows = parse_model_catalog(&value).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].creator, "NVIDIA");
    }

    #[test]
    fn catalog_filters_non_text_products_but_keeps_vision_llms() {
        let v = serde_json::json!({"data": [
            {
                "id":"vision-llm", "name":"Vision LLM", "provider":"Example",
                "architecture":{"modality":"text+image->text","input_modalities":["text","image"],"output_modalities":["text"]},
                "pricing":{"prompt":"0.000001","completion":"0.000002"}
            },
            {
                "id":"image-gen", "name":"Image Generator", "provider":"Example",
                "architecture":{"modality":"text->image","input_modalities":["text"],"output_modalities":["image"]},
                "pricing":{"prompt":"0.000001","completion":"0.000002"}
            },
            {
                "id":"video-model", "name":"Video Model", "provider":"Example",
                "architecture":{"output_modalities":["video"]},
                "pricing":{"prompt":"0.000001","completion":"0.000002"}
            }
        ]});
        let rows = parse_model_catalog(&v).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].model, "vision-llm");
    }

    #[test]
    fn catalog_vision_flag_follows_input_modalities() {
        let v = serde_json::json!({"data": [
            {
                "id":"vision-llm", "name":"Vision LLM", "provider":"Example",
                "architecture":{"modality":"text+image->text","input_modalities":["text","image"],"output_modalities":["text"]},
                "pricing":{"prompt":"0.000001","completion":"0.000002"}
            },
            {
                "id":"text-llm", "name":"Text LLM", "provider":"Example",
                "architecture":{"modality":"text->text","input_modalities":["text"],"output_modalities":["text"]},
                "pricing":{"prompt":"0.000001","completion":"0.000002"}
            },
            {
                "id":"modality-string-only", "name":"Modality String Only", "provider":"Example",
                "architecture":{"modality":"text+image->text"},
                "pricing":{"prompt":"0.000001","completion":"0.000002"}
            }
        ]});
        let rows = parse_model_catalog(&v).unwrap();
        let by_id = |id: &str| rows.iter().find(|q| q.model == id).unwrap();
        assert!(by_id("vision-llm").vision);
        assert!(!by_id("text-llm").vision);
        assert!(by_id("modality-string-only").vision);
    }
}
