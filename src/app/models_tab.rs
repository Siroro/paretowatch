//! Models tab: every model in the price snapshot — not just the
//! benchmark-matched ones the Pareto chart can plot — with search, filters,
//! sortable price and usage columns, and a workload cost calculator that bills
//! the Surplus cache-read rate when one is published.
//!
//! The tab clones the quote list once per frame so the whole render pass can
//! mutate UI state (selections, filters, pinned widgets) without holding the
//! snapshot borrow.

use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::bench::normalize;
use crate::fetch::{ON_DEMAND_REFRESH_SECS, performance_key};
use crate::format::{
    format_compact_number, format_compact_usd, format_latency_ms, format_price_tick,
    format_throughput, format_usd,
};
use crate::theme::{
    PRICE_DOWN, copyable_slug, creator_color, discount_color, free_offer_badge, group_label,
};
use crate::types::{
    AlertMode, CostBasis, ModalityFilter, PriceMetric, Quote, SpeedFilter, UncensoredFilter,
    assumed_cache_price, token_workload_cost,
};

use super::controls::{option_row, popover_button, radio_list};
use super::{ParetoWatchApp, Tab};

/// Which feed the table rows come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(super) enum ModelsSource {
    #[default]
    All,
    LiveMarket,
    Catalog,
}

impl ModelsSource {
    fn allows(self, quote: &Quote) -> bool {
        match self {
            Self::All => true,
            Self::LiveMarket => quote.live_market,
            Self::Catalog => !quote.live_market,
        }
    }
}

/// Cache-pricing availability filter; the rate it tests is the same
/// best-available cache rate the calculator bills.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(super) enum ModelsCacheFilter {
    #[default]
    Any,
    Priced,
    NoCachePrice,
}

impl ModelsCacheFilter {
    fn allows(self, quote: &Quote) -> bool {
        match self {
            Self::Any => true,
            Self::Priced => quote.best_available_cache_read().is_some(),
            Self::NoCachePrice => quote.best_available_cache_read().is_none(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(super) enum ModelsSort {
    #[default]
    Name,
    Input,
    CacheRead,
    Output,
    Blended,
    Discount,
    Requests24h,
    Volume24h,
    /// Market-wide median tokens/second (24h route telemetry).
    Throughput,
    /// Market-wide median time-to-first-token.
    Ttft,
}

/// Where the cache-read rate a calculation bills came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheRateSource {
    /// The priced provider's own cache ask.
    ProviderAsk,
    /// The headline provider publishes no cache ask, so the cheapest cache
    /// ask among the market's other providers is billed for that leg.
    MarketBest,
    /// No market provider publishes a cache ask; the model's catalogue
    /// list price is billed instead.
    CatalogList,
    /// Nothing published a cache rate; cached tokens are billed at the
    /// configured assumed share of the fresh-input rate.
    AssumedShare,
}

/// The per-1M token legs one calculator run bills for a quote.
#[derive(Debug, Clone, PartialEq)]
struct CalcLegs {
    provider: String,
    input: f64,
    output: f64,
    cache_read: Option<f64>,
    cache_from: CacheRateSource,
}

/// Resolve the calculator's price legs. `provider` names an entry of the
/// quote's market options (None = the headline cheapest ask). A stale or
/// unknown provider name falls back to the headline legs. The cache leg
/// prefers published numbers — the provider's own ask, else (headline only)
/// the market-wide best ask, else the model's catalogue list price — and
/// only falls to the assumed input share when nothing is published. An
/// explicit provider never borrows another provider's cache ask.
fn resolve_calc_legs(quote: &Quote, provider: Option<&str>) -> CalcLegs {
    let from_rate = |rate: Option<f64>| match rate {
        Some(_) => CacheRateSource::ProviderAsk,
        None => match quote.catalog_cache_read {
            Some(_) => CacheRateSource::CatalogList,
            None => CacheRateSource::AssumedShare,
        },
    };
    if let Some(option) = provider.and_then(|name| {
        quote
            .market_options
            .iter()
            .find(|option| option.provider == name)
    }) {
        let cache_read = option.cache_read.or(quote.catalog_cache_read);
        return CalcLegs {
            provider: option.provider.clone(),
            input: option.input,
            output: option.output,
            cache_read,
            cache_from: from_rate(option.cache_read),
        };
    }
    let market_best = quote
        .market_options
        .iter()
        .filter_map(|option| option.cache_read)
        .fold(f64::INFINITY, f64::min);
    let (cache_read, cache_from) = match quote.cache_read {
        Some(rate) => (Some(rate), CacheRateSource::ProviderAsk),
        None if market_best.is_finite() => (Some(market_best), CacheRateSource::MarketBest),
        None => (
            quote.catalog_cache_read,
            match quote.catalog_cache_read {
                Some(_) => CacheRateSource::CatalogList,
                None => CacheRateSource::AssumedShare,
            },
        ),
    };
    CalcLegs {
        provider: quote.provider.clone(),
        input: quote.input,
        output: quote.output,
        cache_read,
        cache_from,
    }
}

/// True when the search query matches the model id, display name, or creator.
/// Matching is case-insensitive and ignores punctuation and spaces, like the
/// Pareto chart search.
fn models_search_matches(query: &str, quote: &Quote) -> bool {
    let q = normalize(query).replace(' ', "");
    if q.is_empty() {
        return true;
    }
    [
        quote.model.as_str(),
        quote.display_name.as_str(),
        quote.creator.as_str(),
    ]
    .iter()
    .any(|field| normalize(field).replace(' ', "").contains(&q))
}

/// Token split for the calculator's "agentic" preset: a 10M-token session
/// portioned by the workload weights (fresh input / cache read / output).
pub(super) fn agentic_preset_m(
    input_weight: f64,
    cache_read_weight: f64,
    output_weight: f64,
) -> (f64, f64, f64) {
    const SESSION_TOKENS_M: f64 = 10.0;
    let input_weight = input_weight.max(0.0);
    let cache_read_weight = cache_read_weight.max(0.0);
    let output_weight = output_weight.max(0.0);
    let total = input_weight + cache_read_weight + output_weight;
    if total <= f64::EPSILON {
        return (
            SESSION_TOKENS_M / 3.0,
            SESSION_TOKENS_M / 3.0,
            SESSION_TOKENS_M / 3.0,
        );
    }
    (
        SESSION_TOKENS_M * input_weight / total,
        SESSION_TOKENS_M * cache_read_weight / total,
        SESSION_TOKENS_M * output_weight / total,
    )
}

/// One rendered table row: the quote plus the derived values the columns and
/// sort keys share, so each is computed once per frame.
struct ModelsRow<'a> {
    quote: &'a Quote,
    blended: f64,
    /// Best-available cache rate (`None` when nobody publishes one).
    cache_read: Option<f64>,
    /// AA standardized-speed family matched to the quote, when one exists —
    /// the primary speed figures (true medians vs the market feed's coarse
    /// buckets).
    aa_speed: Option<crate::types::AaSpeedEntry>,
}

impl ModelsRow<'_> {
    /// The throughput a speed filter or sort should test: AA's standardized
    /// median where the family matched, else the live-market bucket.
    fn resolved_throughput(&self) -> Option<f64> {
        self.aa_speed
            .as_ref()
            .and_then(|entry| entry.median_tps)
            .or(self.quote.throughput_p50)
    }

    /// The TTFT (in ms) a filter or sort should test, same resolution order.
    fn resolved_ttft_ms(&self) -> Option<f64> {
        self.aa_speed
            .as_ref()
            .and_then(|entry| entry.median_ttft_s)
            .map(|s| s * 1000.0)
            .or(self.quote.ttft_p50_ms)
    }
}

impl ParetoWatchApp {
    pub(super) fn models_tab(&mut self, ui: &mut egui::Ui) {
        let quotes: Vec<Quote> = self
            .price_snapshot
            .as_ref()
            .map(|s| s.quotes.clone())
            .unwrap_or_default();

        egui::Panel::right("models_calculator")
            .resizable(true)
            .default_size(370.0)
            .min_size(300.0)
            .frame(
                egui::Frame::central_panel(ui.style()).inner_margin(egui::Margin::symmetric(8, 6)),
            )
            .show(ui, |ui| self.models_calculator(ui, &quotes));

        if quotes.is_empty() {
            ui.spinner();
            ui.label("Waiting for pricing data…");
            return;
        }

        self.models_filter_bar(ui, &quotes);

        let query = self.models_search.clone();
        let creator = self.models_creator.clone();
        let source = self.models_source;
        let modality = self.models_modality;
        let safety = self.models_uncensored;
        let cache_filter = self.models_cache;
        let speed_filter = self.models_speed;
        let aa_speed = self.aa_speed_lookup();
        let weights = (
            self.settings.input_weight,
            self.settings.cache_read_weight,
            self.settings.output_weight,
            self.settings.cache_read_fallback_pct,
        );
        let mut rows: Vec<ModelsRow<'_>> = quotes
            .iter()
            .filter(|quote| models_search_matches(&query, quote))
            .filter(|quote| source.allows(quote))
            .filter(|quote| modality.allows(quote.vision))
            .filter(|quote| safety.allows(quote.uncensored))
            .filter(|quote| cache_filter.allows(quote))
            .filter(|quote| {
                creator
                    .as_deref()
                    .is_none_or(|c| group_label(&quote.creator) == c)
            })
            .map(|quote| ModelsRow {
                aa_speed: aa_speed
                    .as_ref()
                    .and_then(|join| join.get(&quote.model))
                    .cloned(),
                quote,
                blended: quote.price(
                    PriceMetric::Blended,
                    weights.0,
                    weights.1,
                    weights.2,
                    weights.3,
                ),
                cache_read: quote.best_available_cache_read(),
            })
            // Speed filters test the resolved figure: AA's standardized
            // median where the family matched, else the market bucket.
            .filter(|row| speed_filter.allows(row.resolved_throughput()))
            .collect();
        sort_models_rows(&mut rows, self.models_sort, self.models_sort_desc);

        ui.horizontal(|ui| {
            ui.strong(format!("{} of {} models", rows.len(), quotes.len()));
            if rows.len() != quotes.len() && ui.small_button("✗ Clear filters").clicked() {
                self.models_search.clear();
                self.models_creator = None;
                self.models_source = ModelsSource::All;
                self.models_modality = ModalityFilter::All;
                self.models_uncensored = UncensoredFilter::All;
                self.models_cache = ModelsCacheFilter::Any;
                self.models_speed = SpeedFilter::Any;
            }
        });
        ui.small(
            "Click a model to load it into the calculator · ✚ adds it to the pinned price window",
        );
        ui.add_space(4.0);

        if rows.is_empty() {
            ui.label("No models match the current filters.");
            return;
        }

        egui::ScrollArea::both()
            .id_salt("models_table_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.models_table(ui, &rows, weights);
            });
    }

    fn models_filter_bar(&mut self, ui: &mut egui::Ui, quotes: &[Quote]) {
        // Creator options are derived from the live snapshot, biggest group
        // first. Computed over every quote so the list stays stable while a
        // creator filter is active.
        let mut creator_counts: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for quote in quotes {
            *creator_counts
                .entry(group_label(&quote.creator))
                .or_insert(0) += 1;
        }
        let mut creators: Vec<(&str, usize)> = creator_counts.into_iter().collect();
        creators.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));

        // Search stays on the bar; the five secondary selectors share one
        // Filters popover, mirroring the Pareto toolbar.
        ui.horizontal_wrapped(|ui| {
            ui.label("Search:");
            ui.add(
                egui::TextEdit::singleline(&mut self.models_search)
                    .hint_text("model, creator…")
                    .desired_width(200.0)
                    .clip_text(true),
            );
            if !self.models_search.trim().is_empty()
                && ui.small_button("✗").on_hover_text("Clear search").clicked()
            {
                self.models_search.clear();
            }
            ui.separator();

            let active_filters = usize::from(self.models_source != ModelsSource::All)
                + usize::from(self.models_modality != ModalityFilter::All)
                + usize::from(self.models_uncensored != UncensoredFilter::All)
                + usize::from(self.models_cache != ModelsCacheFilter::Any)
                + usize::from(self.models_speed != SpeedFilter::Any)
                + usize::from(self.models_creator.is_some());
            let filters_label = if active_filters > 0 {
                format!("Filters · {active_filters} ▾")
            } else {
                "Filters ▾".to_owned()
            };
            popover_button(
                ui,
                "models_filters",
                egui::RichText::new(filters_label),
                Some("Creator, source, modality, safety, and cache-pricing filters"),
                |ui| {
                    ui.set_min_width(360.0);
                    egui::Grid::new("models_filter_grid")
                        .num_columns(2)
                        .show(ui, |ui| {
                            ui.label("Source").on_hover_text(
                                "Whether the price comes from the live Surplus market or only \
                                 the comparison/catalog list.",
                            );
                            option_row(
                                ui,
                                &mut self.models_source,
                                &[
                                    (ModelsSource::All, "All"),
                                    (ModelsSource::LiveMarket, "Live market"),
                                    (ModelsSource::Catalog, "Catalog"),
                                ],
                            );
                            ui.end_row();
                            ui.label("Modality");
                            option_row(
                                ui,
                                &mut self.models_modality,
                                &[
                                    (ModalityFilter::All, "All"),
                                    (ModalityFilter::Vision, "Vision"),
                                    (ModalityFilter::TextOnly, "Text-only"),
                                ],
                            );
                            ui.end_row();
                            ui.label("Safety");
                            option_row(
                                ui,
                                &mut self.models_uncensored,
                                &[
                                    (UncensoredFilter::All, "All"),
                                    (UncensoredFilter::Uncensored, "Uncensored"),
                                    (UncensoredFilter::SafetyTuned, "Safety-tuned"),
                                ],
                            );
                            ui.end_row();
                            ui.label("Cache pricing").on_hover_text(
                                "Has any published cache-read rate (provider ask or catalogue \
                                 list price) vs none at all.",
                            );
                            option_row(
                                ui,
                                &mut self.models_cache,
                                &[
                                    (ModelsCacheFilter::Any, "Any"),
                                    (ModelsCacheFilter::Priced, "Priced"),
                                    (ModelsCacheFilter::NoCachePrice, "No cache price"),
                                ],
                            );
                            ui.end_row();
                            ui.label("Speed").on_hover_text(
                                "Tokens/second over AA's standardized benchmark where the model \
                                 matched (true medians under a uniform protocol), else the \
                                 live-market feed's coarse bucket. Measured keeps only models \
                                 with speed data; the thresholds cut the slow tail. Per-provider \
                                 numbers live on each model's detail.",
                            );
                            let speed_options =
                                SpeedFilter::ALL.map(|filter| (filter, filter.label()));
                            option_row(ui, &mut self.models_speed, &speed_options);
                            ui.end_row();
                        });
                    ui.label("Creator:");
                    let mut creator = self.models_creator.clone().unwrap_or_default();
                    let mut creator_rows = vec![(String::new(), "All creators".to_owned())];
                    creator_rows.extend(
                        creators
                            .iter()
                            .map(|(name, count)| ((*name).to_owned(), format!("{name} ({count})"))),
                    );
                    egui::ScrollArea::vertical()
                        .max_height(180.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if radio_list(ui, &mut creator, &creator_rows) {
                                self.models_creator = (!creator.is_empty()).then_some(creator);
                            }
                        });
                },
            );
        });
    }

    fn models_table(
        &mut self,
        ui: &mut egui::Ui,
        rows: &[ModelsRow<'_>],
        weights: (f64, f64, f64, f64),
    ) {
        let mut sort_clicked: Option<ModelsSort> = None;
        {
            let active_sort = self.models_sort;
            let sort_desc = self.models_sort_desc;
            let mut header = |ui: &mut egui::Ui, label: &str, sort: ModelsSort| {
                let active = active_sort == sort;
                let arrow = if active {
                    if sort_desc { " ▾" } else { " ▴" }
                } else {
                    ""
                };
                if ui
                    .selectable_label(
                        active,
                        egui::RichText::new(format!("{label}{arrow}")).strong(),
                    )
                    .clicked()
                {
                    sort_clicked = Some(sort);
                }
            };

            egui::Grid::new("models_table")
                .striped(true)
                .min_col_width(0.0)
                .show(ui, |ui| {
                    header(ui, "Model", ModelsSort::Name);
                    ui.strong("Creator");
                    header(ui, "In $/1M", ModelsSort::Input);
                    header(ui, "Cache $/1M", ModelsSort::CacheRead);
                    header(ui, "Out $/1M", ModelsSort::Output);
                    header(ui, "Blend $/1M", ModelsSort::Blended);
                    ui.strong("Provider");
                    header(ui, "Req /24h", ModelsSort::Requests24h);
                    header(ui, "Vol /24h", ModelsSort::Volume24h);
                    header(ui, "tok/s", ModelsSort::Throughput);
                    header(ui, "TTFT", ModelsSort::Ttft);
                    header(ui, "Disc", ModelsSort::Discount);
                    ui.strong("");
                    ui.end_row();

                    for row in rows {
                        self.models_table_row(ui, row, weights);
                    }
                });
        }
        if let Some(sort) = sort_clicked {
            if self.models_sort == sort {
                self.models_sort_desc = !self.models_sort_desc;
            } else {
                self.models_sort = sort;
                // Cheapest-first for price columns, A→Z for names, and
                // fastest-first for throughput (higher is better there).
                self.models_sort_desc = sort == ModelsSort::Throughput;
            }
        }
    }

    fn models_table_row(
        &mut self,
        ui: &mut egui::Ui,
        row: &ModelsRow<'_>,
        weights: (f64, f64, f64, f64),
    ) {
        let quote = row.quote;
        let is_selected = self.calc_model.as_deref() == Some(quote.model.as_str());
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("●")
                    .color(creator_color(&quote.creator))
                    .size(11.0),
            );
            let label = if quote.free_offer_listed {
                format!("{}*", quote.display_name)
            } else {
                quote.display_name.clone()
            };
            if ui.selectable_label(is_selected, label).clicked() {
                self.calc_model = Some(quote.model.clone());
                self.calc_provider = None;
            }
            if quote.free_offer_listed {
                free_offer_badge(ui);
            }
            if quote.vision {
                ui.label(
                    egui::RichText::new("👁")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(120, 200, 255)),
                )
                .on_hover_text("Accepts image input (vision LLM)");
            }
            if quote.uncensored {
                ui.label(
                    egui::RichText::new("🔓")
                        .size(11.0)
                        .color(egui::Color32::from_rgb(240, 170, 60)),
                )
                .on_hover_text(
                    "Uncensored — refusal behavior removed (abliterated/uncensored finetune)",
                );
            }
        });
        ui.small(group_label(&quote.creator));
        ui.label(format_price_tick(quote.input));
        match row.cache_read {
            Some(rate) => {
                // The headline provider's own ask, or — marked — the
                // market-wide best when it publishes none.
                if quote.cache_read.is_some() {
                    ui.label(format_price_tick(rate));
                } else {
                    ui.label(format!("{}†", format_price_tick(rate)))
                        .on_hover_text(
                            "Market-wide best cache ask; the cheapest provider of this model \
                         publishes no cache price",
                        );
                }
            }
            None => {
                ui.label(egui::RichText::new("—").weak())
                    .on_hover_text("No provider in this market publishes a cache-read price");
            }
        }
        ui.label(format_price_tick(quote.output));
        ui.label(format_price_tick(row.blended)).on_hover_text(format!(
            "Blended at your workload weights ({:.0}/{:.0}/{:.0} fresh/cache/output). Cache rate falls back provider ask \u{2192} market best \u{2192} catalogue list \u{2192} assumed {:.0}% of input when nothing is published.",
            weights.0, weights.1, weights.2, weights.3,
        ));
        let provider_label = ui.small(if quote.live_market {
            quote.provider.clone()
        } else {
            format!("{} (catalog)", quote.provider)
        });
        let mut market_info = Vec::new();
        if let Some(trusted) = quote.provider_trusted {
            market_info.push(if trusted {
                "trusted".to_owned()
            } else {
                "untrusted".to_owned()
            });
        }
        if let Some(healthy) = quote.healthy_seller_count {
            market_info.push(format!("{healthy} healthy sellers"));
        }
        if let Some(total) = quote.seller_count {
            market_info.push(format!("{total} total"));
        }
        // Per-provider speed from the route analytics join, when this model's
        // breakdown has been fetched.
        let market_perf = self.performance.get(&quote.model);
        if let Some(perf) = market_perf {
            for option in &quote.market_options {
                if let Some(route) = perf.provider(&option.provider) {
                    let speed = match (route.tps_p50, route.ttft_p50_ms) {
                        (Some(tps), Some(ttft)) => {
                            format!(
                                "{} · TTFT {}",
                                format_throughput(tps),
                                format_latency_ms(ttft)
                            )
                        }
                        (Some(tps), None) => format_throughput(tps),
                        (None, Some(ttft)) => format!("TTFT {}", format_latency_ms(ttft)),
                        (None, None) => continue,
                    };
                    market_info.push(format!("{}: {speed}", option.provider));
                }
            }
        }
        if !market_info.is_empty() {
            provider_label.on_hover_text(market_info.join("\n"));
        }
        match quote.requests_24h {
            Some(requests) => {
                ui.label(format_compact_number(requests as f64))
                    .on_hover_text("Requests routed through this market in the last 24h");
            }
            None => {
                ui.label(egui::RichText::new("—").weak())
                    .on_hover_text("Market telemetry only; catalog rows carry none");
            }
        }
        match quote.volume_24h {
            Some(volume) => {
                ui.label(format_compact_usd(volume))
                    .on_hover_text("Dollars traded through this market in the last 24h");
            }
            None => {
                ui.label(egui::RichText::new("—").weak())
                    .on_hover_text("Market telemetry only; catalog rows carry none");
            }
        }
        // Speed cells lead with AA's standardized median (true percentile,
        // uniform protocol); the live-market bucket is the fallback and is
        // marked † so the two sources are never confused for each other.
        match row
            .aa_speed
            .as_ref()
            .and_then(|entry| entry.median_tps)
            .or(quote.throughput_p50)
        {
            Some(tps) => {
                let standardized = row.aa_speed.as_ref().and_then(|e| e.median_tps) == Some(tps);
                let label = if standardized {
                    format_throughput(tps)
                } else {
                    format!("{}†", format_throughput(tps))
                };
                let hover = if let Some(entry) = row.aa_speed.as_ref().filter(|_| standardized) {
                    let band = match (entry.tps_p05, entry.tps_p95) {
                        (Some(lo), Some(hi)) => {
                            format!(
                                " · p05–p95 {}–{}",
                                format_throughput(lo),
                                format_throughput(hi)
                            )
                        }
                        _ => String::new(),
                    };
                    format!(
                        "AA standardized benchmark{}: median of their controlled generations \
                         under a uniform protocol.{}",
                        entry
                            .effort
                            .as_deref()
                            .map(|e| format!(" ({e} variant)"))
                            .unwrap_or_default(),
                        band,
                    )
                } else {
                    format!(
                        "Live-market bucket: the feed publishes only coarse histogram midpoints \
                         ({} streaming samples in 24h) pooled across every seller — treat as \
                         approximate. AA has no standardized read for this model.",
                        quote.tps_samples.unwrap_or(0),
                    )
                };
                ui.label(label).on_hover_text(hover);
            }
            None => {
                ui.label(egui::RichText::new("—").weak()).on_hover_text(
                    "No speed data: not in AA's standardized table and no live-market \
                     throughput telemetry in 24h",
                );
            }
        }
        match row
            .aa_speed
            .as_ref()
            .and_then(|entry| entry.median_ttft_s)
            .map(|s| s * 1000.0)
            .or(quote.ttft_p50_ms)
        {
            Some(ms) => {
                let standardized = row
                    .aa_speed
                    .as_ref()
                    .and_then(|e| e.median_ttft_s.map(|s| s * 1000.0))
                    == Some(ms);
                let label = if standardized {
                    format_latency_ms(ms)
                } else {
                    format!("{}†", format_latency_ms(ms))
                };
                ui.label(label).on_hover_text(if standardized {
                    "AA standardized benchmark: median time to first token under their uniform \
                     protocol"
                } else {
                    "Live-market bucket: coarse histogram midpoint pooled across every seller \
                     (24h) — treat as approximate"
                });
            }
            None => {
                ui.label(egui::RichText::new("—").weak())
                    .on_hover_text("No time-to-first-token data from either source");
            }
        }
        match quote.discount_pct {
            Some(pct) => {
                ui.colored_label(discount_color(pct), format!("{pct:.0}%"))
                    .on_hover_text("Live-market discount versus list prices");
            }
            None => {
                ui.label(egui::RichText::new("—").weak());
            }
        }
        let pin = ui
            .small_button("✚")
            .on_hover_text("Add to pinned price window");
        if pin.clicked() {
            self.widgets.spawn(
                ui.ctx(),
                self.price_snapshot.as_ref(),
                &self.settings,
                self.liquidity_filter,
                &quote.model,
                pin.rect.center(),
            );
        }
        ui.end_row();
    }

    fn models_calculator(&mut self, ui: &mut egui::Ui, quotes: &[Quote]) {
        ui.heading("Cost calculator");
        ui.add_space(2.0);

        // Default to the first listed model so the panel is never empty.
        if self.calc_model.is_none()
            && let Some(first) = quotes.first()
        {
            self.calc_model = Some(first.model.clone());
        }
        let selected = self
            .calc_model
            .as_deref()
            .and_then(|id| quotes.iter().find(|q| q.model == id));

        ui.horizontal(|ui| {
            ui.label("Model:");
            egui::ComboBox::from_id_salt("calc_model")
                .width(ui.available_width() - 12.0)
                .selected_text(selected.map_or_else(
                    || {
                        self.calc_model
                            .clone()
                            .unwrap_or_else(|| "Select a model…".into())
                    },
                    |quote| quote.display_name.clone(),
                ))
                .show_ui(ui, |ui| {
                    let mut picked = self.calc_model.clone().unwrap_or_default();
                    for quote in quotes {
                        if ui
                            .selectable_value(&mut picked, quote.model.clone(), &quote.display_name)
                            .changed()
                        {
                            self.calc_model = Some(quote.model.clone());
                            self.calc_provider = None;
                        }
                    }
                });
        });

        let Some(quote) = selected else {
            if self.calc_model.is_some() {
                ui.label(
                    egui::RichText::new("This model is no longer listed in the price feed.").weak(),
                );
                if ui.button("✗ Clear selection").clicked() {
                    self.calc_model = None;
                    self.calc_provider = None;
                }
            } else {
                ui.label("Pick a model to price a workload.");
            }
            return;
        };

        ui.horizontal(|ui| {
            ui.label("Slug:");
            copyable_slug(ui, &quote.model);
        });

        // Drop a provider selection that vanished from the market.
        if let Some(provider) = self.calc_provider.clone()
            && !quote
                .market_options
                .iter()
                .any(|option| option.provider == provider)
        {
            self.calc_provider = None;
        }
        if !quote.market_options.is_empty() {
            ui.horizontal(|ui| {
                ui.label("Provider:");
                egui::ComboBox::from_id_salt("calc_provider")
                    .width(ui.available_width() - 12.0)
                    .selected_text(
                        self.calc_provider
                            .clone()
                            .unwrap_or_else(|| "Cheapest ask (auto)".into()),
                    )
                    .show_ui(ui, |ui| {
                        let mut picked = self.calc_provider.clone().unwrap_or_default();
                        if ui
                            .selectable_value(&mut picked, String::new(), "Cheapest ask (auto)")
                            .changed()
                        {
                            self.calc_provider = None;
                        }
                        for option in &quote.market_options {
                            if ui
                                .selectable_value(
                                    &mut picked,
                                    option.provider.clone(),
                                    &option.provider,
                                )
                                .changed()
                            {
                                self.calc_provider = Some(option.provider.clone());
                            }
                        }
                    });
            });
        }

        let legs = resolve_calc_legs(quote, self.calc_provider.as_deref());
        ui.horizontal_wrapped(|ui| {
            ui.label(
                egui::RichText::new("●")
                    .color(creator_color(&quote.creator))
                    .size(13.0),
            );
            ui.label(format!(
                "In {} · Cache {} · Out {} per 1M",
                format_price_tick(legs.input),
                legs.cache_read.map_or_else(
                    || format!(
                        "{} (assumed {:.0}% of input)",
                        format_price_tick(assumed_cache_price(
                            legs.input,
                            self.settings.cache_read_fallback_pct
                        )),
                        self.settings.cache_read_fallback_pct
                    ),
                    format_price_tick,
                ),
                format_price_tick(legs.output),
            ));
        });
        match legs.cache_from {
            CacheRateSource::ProviderAsk => {}
            CacheRateSource::MarketBest => {
                ui.small("Cache rate is the market-wide best ask — the selected provider publishes none.");
            }
            CacheRateSource::CatalogList => {
                ui.small(
                    "No market cache ask — billing the model's published catalogue cache rate.",
                );
            }
            CacheRateSource::AssumedShare => {
                ui.small(format!(
                    "No cache price published anywhere — cached tokens billed at an assumed {:.0}% of the input rate (adjust in Settings).",
                    self.settings.cache_read_fallback_pct
                ));
            }
        }

        ui.add_space(4.0);
        egui::Grid::new("calc_tokens")
            .num_columns(2)
            .show(ui, |ui| {
                for (label, value) in [
                    ("Fresh input", &mut self.calc_input_m),
                    ("Cache read", &mut self.calc_cache_m),
                    ("Output", &mut self.calc_output_m),
                ] {
                    ui.label(label);
                    ui.add(
                        egui::DragValue::new(value)
                            .speed(0.1)
                            .range(0.0..=1_000_000.0)
                            .suffix("M"),
                    );
                    ui.end_row();
                }
            });
        ui.horizontal(|ui| {
            if ui
                .small_button("Agentic")
                .on_hover_text("10M tokens split by your workload weights (fresh/cache/output)")
                .clicked()
            {
                let (input, cache, output) = agentic_preset_m(
                    self.settings.input_weight,
                    self.settings.cache_read_weight,
                    self.settings.output_weight,
                );
                self.calc_input_m = input;
                self.calc_cache_m = cache;
                self.calc_output_m = output;
            }
            if ui
                .small_button("Chat")
                .on_hover_text("1M fresh input · no cached context · 0.25M output")
                .clicked()
            {
                self.calc_input_m = 1.0;
                self.calc_cache_m = 0.0;
                self.calc_output_m = 0.25;
            }
            if ui
                .small_button("Clear tokens")
                .on_hover_text("Zero every token field")
                .clicked()
            {
                self.calc_input_m = 0.0;
                self.calc_cache_m = 0.0;
                self.calc_output_m = 0.0;
            }
        });

        let cost = token_workload_cost(
            self.calc_input_m,
            self.calc_cache_m,
            self.calc_output_m,
            legs.input,
            legs.cache_read,
            legs.output,
            self.settings.cache_read_fallback_pct,
        );
        ui.add_space(4.0);
        egui::Grid::new("calc_cost").num_columns(2).show(ui, |ui| {
            ui.label("Fresh input");
            ui.label(format_usd(cost.input));
            ui.end_row();
            ui.label("Cache read");
            ui.label(format_usd(cost.cache_read));
            ui.end_row();
            ui.label("Output");
            ui.label(format_usd(cost.output));
            ui.end_row();
            ui.separator();
            ui.separator();
            ui.end_row();
            ui.strong("Total");
            ui.label(
                egui::RichText::new(format_usd(cost.total()))
                    .strong()
                    .size(16.0),
            );
            ui.end_row();
        });
        let savings = cost.cache_savings();
        if savings > 0.0 && self.calc_cache_m > 0.0 {
            let pct = if cost.cache_read_at_input_rate > f64::EPSILON {
                savings / cost.cache_read_at_input_rate * 100.0
            } else {
                0.0
            };
            ui.colored_label(
                PRICE_DOWN,
                format!(
                    "Prompt caching saves {} on this workload ({pct:.0}% off the fresh-input rate)",
                    format_usd(savings),
                ),
            );
        }

        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if quote.live_market {
                let trust = match quote.provider_trusted {
                    Some(true) => "trusted",
                    Some(false) => "untrusted",
                    None => "trust unknown",
                };
                ui.label(format!(
                    "live market · {trust} · {} healthy sellers",
                    quote.healthy_seller_count.unwrap_or(0),
                ));
            } else {
                ui.label("catalog / comparison price");
            }
            if let Some(pct) = quote.discount_pct {
                ui.separator();
                ui.colored_label(discount_color(pct), format!("{pct:.0}% off"));
            }
        });

        // AA standardized speed for the priced model: the true-median
        // reference beside whatever the marketplace telemetry says.
        if let Some(entry) = self
            .aa_speed_lookup()
            .and_then(|join| join.get(&quote.model).cloned())
        {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.strong("AA speed:");
                match (entry.median_tps, entry.median_ttft_s) {
                    (Some(tps), Some(ttft)) => ui.label(format!(
                        "{} \u{00b7} TTFT {}",
                        format_throughput(tps),
                        format_latency_ms(ttft * 1000.0)
                    )),
                    (Some(tps), None) => ui.label(format_throughput(tps)),
                    (None, Some(ttft)) => {
                        ui.label(format!("TTFT {}", format_latency_ms(ttft * 1000.0)))
                    }
                    (None, None) => ui.label("—"),
                };
                if let Some(e2e) = entry.median_e2e_s {
                    ui.separator();
                    ui.label(format!("end-to-end {}", format_latency_ms(e2e * 1000.0)));
                }
            })
            .response
            .on_hover_text({
                let band = match (entry.tps_p05, entry.tps_p95) {
                    (Some(lo), Some(hi)) => {
                        format!(
                            " · p05–p95 {}–{}",
                            format_throughput(lo),
                            format_throughput(hi)
                        )
                    }
                    _ => String::new(),
                };
                format!(
                    "Median of AA's controlled generations under a uniform protocol{} — the \
                     standardized reference, independent of any marketplace route.{}",
                    entry
                        .effort
                        .as_deref()
                        .map(|e| format!(" ({e} variant)"))
                        .unwrap_or_default(),
                    band,
                )
            });
        }

        // Per-provider speed: fetched on demand for the model the calculator
        // is pricing, then cached (see ensure_performance).
        self.ensure_performance(&quote.model, ON_DEMAND_REFRESH_SECS);
        let route_key = performance_key(&quote.model);
        if let Some(perf) = self.performance.get(&quote.model) {
            ui.add_space(4.0);
            egui::CollapsingHeader::new("Route performance · 24h")
                .default_open(true)
                .show(ui, |ui| {
                    egui::Grid::new("calc_route_perf")
                        .striped(true)
                        .show(ui, |ui| {
                            ui.strong("");
                            ui.strong("Provider").on_hover_text(
                                "Every provider route observed on this model's market, busiest \
                                 first. ● marks the provider whose prices the calculator bills.",
                            );
                            ui.strong("tok/s");
                            ui.strong("TTFT p50");
                            ui.strong("p95 total");
                            ui.strong("Uptime");
                            ui.strong("Req /24h");
                            ui.end_row();
                            for route in perf.providers_by_traffic() {
                                let billed = match self.calc_provider.as_deref() {
                                    Some(name) => name == route.provider,
                                    None => route.provider == quote.provider,
                                };
                                // The billed route's numbers stand out.
                                let cell = |text: String| {
                                    let rich = egui::RichText::new(text);
                                    if billed { rich.strong() } else { rich }
                                };
                                let dash = || egui::RichText::new("\u{2014}").weak();
                                ui.label(if billed { "\u{25cf}" } else { "" });
                                ui.label(&route.provider);
                                ui.label(match route.tps_p50 {
                                    Some(tps) => cell(format_throughput(tps)),
                                    None => dash(),
                                })
                                .on_hover_text(
                                    match (route.tps_p50, route.tps_samples) {
                                        (Some(_), Some(n)) => {
                                            format!("Median of {n} streaming completions")
                                        }
                                        _ => "No throughput samples on this route".to_owned(),
                                    },
                                );
                                ui.label(match route.ttft_p50_ms {
                                    Some(ms) => cell(format_latency_ms(ms)),
                                    None => dash(),
                                });
                                ui.label(match route.p95_latency_ms {
                                    Some(ms) => cell(format_latency_ms(ms)),
                                    None => dash(),
                                });
                                ui.label(match route.uptime_pct {
                                    Some(pct) => cell(format!("{pct:.1}%")),
                                    None => dash(),
                                });
                                ui.label(match route.requests {
                                    Some(n) => cell(format_compact_number(n as f64)),
                                    None => dash(),
                                });
                                ui.end_row();
                            }
                        });
                    ui.small(
                        "Median speed and latency of actual Surplus traffic per provider route, \
                         rolling 24h.",
                    );
                });
        } else if self.performance_pending.contains(&route_key) {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.spinner();
                ui.small("Fetching route analytics…");
            });
        } else if self.performance_backoff.contains_key(&route_key) {
            ui.add_space(4.0);
            ui.small("Route analytics unavailable for this model — retrying later.");
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .button("🔔 Alert…")
                    .on_hover_text("Create a price alert for this model")
                    .clicked()
                {
                    self.alert_model = quote.model.clone();
                    self.alert_mode = AlertMode::Threshold;
                    self.alert_metric = PriceMetric::Blended;
                    self.alert_cost_basis = CostBasis::PerMillion;
                    self.alert_threshold = quote.price(
                        PriceMetric::Blended,
                        self.settings.input_weight,
                        self.settings.cache_read_weight,
                        self.settings.output_weight,
                        self.settings.cache_read_fallback_pct,
                    );
                    self.tab = Tab::Alerts;
                }
                if ui
                    .button("✚ Price window")
                    .on_hover_text("Add to the pinned floating price window")
                    .clicked()
                {
                    let at = ui.cursor().center();
                    self.widgets.spawn(
                        ui.ctx(),
                        self.price_snapshot.as_ref(),
                        &self.settings,
                        self.liquidity_filter,
                        &quote.model,
                        at,
                    );
                }
                if ui
                    .button("→ Pareto")
                    .on_hover_text("Inspect this model on the Pareto chart")
                    .clicked()
                {
                    self.selected_pareto_model = Some(quote.model.clone());
                    self.tab = Tab::Pareto;
                }
            });
        });
    }
}

/// Order the table rows by the active sort. Missing cache prices and usage
/// telemetry sort as the largest key so they sink in the default ascending
/// order; every key tie-breaks on display name so the order is stable across
/// polls.
fn sort_models_rows(rows: &mut [ModelsRow<'_>], sort: ModelsSort, desc: bool) {
    let by_name = |a: &ModelsRow<'_>, b: &ModelsRow<'_>| {
        a.quote
            .display_name
            .cmp(&b.quote.display_name)
            .then_with(|| a.quote.model.cmp(&b.quote.model))
    };
    let cache_key = |row: &ModelsRow<'_>| row.cache_read.unwrap_or(f64::INFINITY);
    let discount_key = |row: &ModelsRow<'_>| row.quote.discount_pct.unwrap_or(f64::NEG_INFINITY);
    let requests_key =
        |row: &ModelsRow<'_>| row.quote.requests_24h.map_or(f64::INFINITY, |r| r as f64);
    let volume_key = |row: &ModelsRow<'_>| row.quote.volume_24h.unwrap_or(f64::INFINITY);
    // Missing speed telemetry sinks in the ascending (fastest-TTFT-first and
    // price-style) order, matching the other optional columns. Both speed
    // keys test the resolved figure: AA's standardized median where the
    // family matched, else the live-market bucket.
    let ttft_key = |row: &ModelsRow<'_>| row.resolved_ttft_ms().unwrap_or(f64::INFINITY);
    rows.sort_by(|a, b| {
        // Throughput's default order is fastest-first (descending), where a
        // sink-last key would float the unmeasured rows to the top, so that
        // arm applies the direction itself and keeps unmeasured rows last in
        // both directions.
        if sort == ModelsSort::Throughput {
            return match (a.resolved_throughput(), b.resolved_throughput()) {
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(x), Some(y)) => {
                    let key = x.total_cmp(&y);
                    if desc { key.reverse() } else { key }.then_with(|| by_name(a, b))
                }
                (None, None) => by_name(a, b),
            };
        }
        let ordering = match sort {
            ModelsSort::Name => by_name(a, b),
            ModelsSort::Input => a
                .quote
                .input
                .total_cmp(&b.quote.input)
                .then_with(|| by_name(a, b)),
            ModelsSort::CacheRead => cache_key(a)
                .total_cmp(&cache_key(b))
                .then_with(|| by_name(a, b)),
            ModelsSort::Output => a
                .quote
                .output
                .total_cmp(&b.quote.output)
                .then_with(|| by_name(a, b)),
            ModelsSort::Blended => a.blended.total_cmp(&b.blended).then_with(|| by_name(a, b)),
            ModelsSort::Discount => discount_key(a)
                .total_cmp(&discount_key(b))
                .then_with(|| by_name(a, b)),
            ModelsSort::Requests24h => requests_key(a)
                .total_cmp(&requests_key(b))
                .then_with(|| by_name(a, b)),
            ModelsSort::Volume24h => volume_key(a)
                .total_cmp(&volume_key(b))
                .then_with(|| by_name(a, b)),
            ModelsSort::Ttft => ttft_key(a)
                .total_cmp(&ttft_key(b))
                .then_with(|| by_name(a, b)),
            ModelsSort::Throughput => unreachable!("handled above"),
        };
        if desc { ordering.reverse() } else { ordering }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testfix::test_quote;
    use crate::types::ProviderMarketQuote;

    #[test]
    fn search_matches_ids_names_and_creators_ignoring_punctuation() {
        let q = test_quote("anthropic/claude-opus-4.8", 1.0, true);
        assert!(models_search_matches("opus", &q));
        assert!(models_search_matches("Claude OPUS", &q));
        assert!(models_search_matches("anthropic", &q));
        assert!(!models_search_matches("gpt", &q));
        assert!(models_search_matches("  ", &q));
    }

    #[test]
    fn calc_legs_default_to_provider_ask_then_market_best() {
        let mut q = test_quote("glm-5.3", 1.0, true);
        q.provider = "headline".into();
        q.cache_read = Some(0.05);
        q.market_options = vec![
            ProviderMarketQuote {
                provider: "headline".into(),
                input: 1.0,
                output: 2.0,
                cache_read: Some(0.05),
                trusted: Some(true),
                healthy_seller_count: Some(5),
            },
            ProviderMarketQuote {
                provider: "other".into(),
                input: 2.0,
                output: 4.0,
                cache_read: Some(0.02),
                trusted: Some(true),
                healthy_seller_count: Some(5),
            },
        ];
        let legs = resolve_calc_legs(&q, None);
        assert_eq!(legs.provider, "headline");
        assert!((legs.cache_read.unwrap() - 0.05).abs() < 1e-12);
        assert_eq!(legs.cache_from, CacheRateSource::ProviderAsk);

        // Headline publishes no cache ask: borrow the market-wide best.
        q.cache_read = None;
        let legs = resolve_calc_legs(&q, None);
        assert_eq!(legs.cache_from, CacheRateSource::MarketBest);
        assert!((legs.cache_read.unwrap() - 0.02).abs() < 1e-12);

        // No market asks at all: the model's published catalogue rate.
        q.market_options.clear();
        let legs = resolve_calc_legs(&q, None);
        assert_eq!(legs.cache_from, CacheRateSource::CatalogList);
        assert!((legs.cache_read.unwrap() - 0.1).abs() < 1e-12);

        // Nothing published anywhere: the assumed input share (billed by
        // the settings share, the legs carry no rate).
        q.catalog_cache_read = None;
        let legs = resolve_calc_legs(&q, None);
        assert_eq!(legs.cache_from, CacheRateSource::AssumedShare);
        assert_eq!(legs.cache_read, None);
    }

    #[test]
    fn calc_legs_use_the_selected_provider_verbatim() {
        let mut q = test_quote("model-a", 1.0, true);
        q.provider = "cheap".into();
        q.cache_read = Some(0.1);
        q.market_options = vec![
            ProviderMarketQuote {
                provider: "cheap".into(),
                input: 1.0,
                output: 2.0,
                cache_read: Some(0.1),
                trusted: Some(true),
                healthy_seller_count: Some(5),
            },
            ProviderMarketQuote {
                provider: "dearer".into(),
                input: 3.0,
                output: 6.0,
                cache_read: None,
                trusted: Some(true),
                healthy_seller_count: Some(2),
            },
        ];
        let legs = resolve_calc_legs(&q, Some("dearer"));
        assert_eq!(legs.provider, "dearer");
        assert!((legs.input - 3.0).abs() < 1e-12);
        // An explicit provider without its own cache ask falls to the
        // model's published catalogue rate, never another provider's ask.
        assert!((legs.cache_read.unwrap() - 0.1).abs() < 1e-12);
        assert_eq!(legs.cache_from, CacheRateSource::CatalogList);

        // With nothing published at all, the legs carry no rate and the
        // cost math bills the assumed input share.
        q.catalog_cache_read = None;
        let legs = resolve_calc_legs(&q, Some("dearer"));
        assert_eq!(legs.cache_read, None);
        assert_eq!(legs.cache_from, CacheRateSource::AssumedShare);

        // Unknown provider names fall back to the headline legs.
        let legs = resolve_calc_legs(&q, Some("vanished"));
        assert_eq!(legs.provider, "cheap");
        assert!((legs.input - 1.0).abs() < 1e-12);
    }

    #[test]
    fn agentic_preset_splits_session_by_workload_weights() {
        let (input, cache, output) = agentic_preset_m(15.0, 80.0, 5.0);
        assert!((input - 1.5).abs() < 1e-12);
        assert!((cache - 8.0).abs() < 1e-12);
        assert!((output - 0.5).abs() < 1e-12);
        let (input, cache, output) = agentic_preset_m(0.0, 0.0, 0.0);
        assert!((input - 10.0 / 3.0).abs() < 1e-12);
        assert!((cache - 10.0 / 3.0).abs() < 1e-12);
        assert!((output - 10.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn source_and_cache_filters_partition_quotes() {
        let live = test_quote("live-model", 1.0, true);
        let catalog = test_quote("catalog-model", 1.0, false);
        assert!(ModelsSource::All.allows(&live) && ModelsSource::All.allows(&catalog));
        assert!(
            ModelsSource::LiveMarket.allows(&live) && !ModelsSource::LiveMarket.allows(&catalog)
        );
        assert!(!ModelsSource::Catalog.allows(&live) && ModelsSource::Catalog.allows(&catalog));

        assert!(ModelsCacheFilter::Priced.allows(&live));
        assert!(!ModelsCacheFilter::NoCachePrice.allows(&live));
        let mut no_cache = test_quote("no-cache-model", 1.0, true);
        no_cache.cache_read = None;
        no_cache.market_options.clear();
        assert!(!ModelsCacheFilter::Priced.allows(&no_cache));
        assert!(ModelsCacheFilter::NoCachePrice.allows(&no_cache));
    }

    #[test]
    fn speed_sorts_order_measured_rows_and_sink_unmeasured_in_both_directions() {
        let mut fast = test_quote("fast-model", 1.0, true);
        fast.throughput_p50 = Some(133.6);
        fast.ttft_p50_ms = Some(679.0);
        let mut slow = test_quote("slow-model", 1.0, true);
        slow.throughput_p50 = Some(18.0);
        slow.ttft_p50_ms = Some(5546.0);
        let unmeasured = test_quote("unmeasured-model", 1.0, true);
        let mut rows = vec![
            ModelsRow {
                quote: &unmeasured,
                blended: 0.0,
                cache_read: None,
                aa_speed: None,
            },
            ModelsRow {
                quote: &slow,
                blended: 0.0,
                cache_read: None,
                aa_speed: None,
            },
            ModelsRow {
                quote: &fast,
                blended: 0.0,
                cache_read: None,
                aa_speed: None,
            },
        ];
        fn names<'a>(rows: &[ModelsRow<'a>]) -> Vec<&'a str> {
            rows.iter().map(|row| row.quote.model.as_str()).collect()
        }

        // Throughput's default view: fastest first, unmeasured last.
        sort_models_rows(&mut rows, ModelsSort::Throughput, true);
        assert_eq!(
            names(&rows),
            ["fast-model", "slow-model", "unmeasured-model"]
        );
        // Flipped: slowest first, unmeasured still last.
        sort_models_rows(&mut rows, ModelsSort::Throughput, false);
        assert_eq!(
            names(&rows),
            ["slow-model", "fast-model", "unmeasured-model"]
        );
        // TTFT ascending (default): snappiest first, unmeasured last.
        sort_models_rows(&mut rows, ModelsSort::Ttft, false);
        assert_eq!(
            names(&rows),
            ["fast-model", "slow-model", "unmeasured-model"]
        );
    }

    #[test]
    fn aa_standardized_figures_override_market_buckets() {
        // A slow market bucket (73.3) under an AA read of 150 tok/s: the
        // resolved figure must be AA's, for sorting and filtering both.
        let mut quote = test_quote("aa-model", 1.0, true);
        quote.throughput_p50 = Some(73.3);
        quote.ttft_p50_ms = Some(5546.0);
        let entry = crate::types::AaSpeedEntry {
            key: "aa model".into(),
            name: "AA Model".into(),
            creator: "Test".into(),
            effort: None,
            median_tps: Some(150.0),
            tps_p05: Some(120.0),
            tps_p25: Some(135.0),
            tps_p75: Some(170.0),
            tps_p95: Some(200.0),
            median_ttft_s: Some(0.4),
            ttft_p95_s: Some(0.9),
            median_e2e_s: Some(3.0),
        };
        let row = ModelsRow {
            quote: &quote,
            blended: 0.0,
            cache_read: None,
            aa_speed: Some(entry),
        };
        assert!((row.resolved_throughput().unwrap() - 150.0).abs() < 1e-9);
        assert!((row.resolved_ttft_ms().unwrap() - 400.0).abs() < 1e-9);
        // The AA figure clears a bar the market bucket would fail; a
        // market-only model keeps its bucket.
        assert!(SpeedFilter::T100.allows(row.resolved_throughput()));
        let mut market_only = test_quote("market-only", 1.0, true);
        market_only.throughput_p50 = Some(73.3);
        let fallback = ModelsRow {
            quote: &market_only,
            blended: 0.0,
            cache_read: None,
            aa_speed: None,
        };
        assert!((fallback.resolved_throughput().unwrap() - 73.3).abs() < 1e-9);
        assert!(!SpeedFilter::T100.allows(fallback.resolved_throughput()));

        // Sorting interleaves the two sources on the resolved figure.
        let mut rows = vec![
            ModelsRow {
                blended: 0.0,
                cache_read: None,
                quote: &market_only,
                aa_speed: None,
            },
            ModelsRow {
                blended: 0.0,
                cache_read: None,
                quote: &quote,
                aa_speed: row.aa_speed.clone(),
            },
        ];
        sort_models_rows(&mut rows, ModelsSort::Throughput, true);
        assert_eq!(
            rows.iter()
                .map(|r| r.quote.model.as_str())
                .collect::<Vec<_>>(),
            ["aa-model", "market-only"]
        );
    }

    #[test]
    fn usage_sorts_order_reported_values_and_sink_missing_telemetry() {
        let mut busy = test_quote("busy-model", 1.0, true);
        busy.requests_24h = Some(14_000);
        busy.volume_24h = Some(6_200_000.0);
        let mut quiet = test_quote("quiet-model", 1.0, true);
        quiet.requests_24h = Some(200);
        quiet.volume_24h = Some(900.0);
        let mut unreported = test_quote("unreported-model", 1.0, true);
        unreported.requests_24h = None;
        unreported.volume_24h = None;
        let mut rows = vec![
            ModelsRow {
                quote: &unreported,
                blended: 0.0,
                cache_read: None,
                aa_speed: None,
            },
            ModelsRow {
                quote: &busy,
                blended: 0.0,
                cache_read: None,
                aa_speed: None,
            },
            ModelsRow {
                quote: &quiet,
                blended: 0.0,
                cache_read: None,
                aa_speed: None,
            },
        ];
        fn names<'a>(rows: &[ModelsRow<'a>]) -> Vec<&'a str> {
            rows.iter().map(|row| row.quote.model.as_str()).collect()
        }

        sort_models_rows(&mut rows, ModelsSort::Requests24h, false);
        assert_eq!(
            names(&rows),
            ["quiet-model", "busy-model", "unreported-model"]
        );
        sort_models_rows(&mut rows, ModelsSort::Volume24h, false);
        assert_eq!(
            names(&rows),
            ["quiet-model", "busy-model", "unreported-model"]
        );
        sort_models_rows(&mut rows, ModelsSort::Requests24h, true);
        assert_eq!(
            names(&rows),
            ["unreported-model", "busy-model", "quiet-model"]
        );
    }
}
