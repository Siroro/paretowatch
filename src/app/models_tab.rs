//! Models tab: every model in the price snapshot — not just the
//! benchmark-matched ones the Pareto chart can plot — with search, filters,
//! sortable price columns, and a workload cost calculator that bills the
//! Surplus cache-read rate when one is published.
//!
//! The tab clones the quote list once per frame so the whole render pass can
//! mutate UI state (selections, filters, pinned widgets) without holding the
//! snapshot borrow.

use eframe::egui;

use crate::bench::normalize;
use crate::format::{format_price_tick, format_usd};
use crate::theme::{PRICE_DOWN, creator_color, discount_color, free_offer_badge, group_label};
use crate::types::{
    AlertMode, CostBasis, ModalityFilter, PriceMetric, Quote, blended_price, token_workload_cost,
};

use super::{ParetoWatchApp, Tab};

/// Which feed the table rows come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum ModelsSource {
    #[default]
    All,
    LiveMarket,
    Catalog,
}

impl ModelsSource {
    fn label(self) -> &'static str {
        match self {
            Self::All => "All sources",
            Self::LiveMarket => "Live market",
            Self::Catalog => "Catalog / comparison",
        }
    }

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum ModelsCacheFilter {
    #[default]
    Any,
    Priced,
    NoCachePrice,
}

impl ModelsCacheFilter {
    fn label(self) -> &'static str {
        match self {
            Self::Any => "Any cache pricing",
            Self::Priced => "Has cache price",
            Self::NoCachePrice => "No cache price",
        }
    }

    fn allows(self, quote: &Quote) -> bool {
        match self {
            Self::Any => true,
            Self::Priced => quote.best_available_cache_read().is_some(),
            Self::NoCachePrice => quote.best_available_cache_read().is_none(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum ModelsSort {
    #[default]
    Name,
    Input,
    CacheRead,
    Output,
    Blended,
    Discount,
}

/// Where the cache-read rate a calculation bills came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CacheRateSource {
    /// The priced provider's own cache ask.
    ProviderAsk,
    /// The headline provider publishes no cache ask, so the cheapest cache
    /// ask among the market's other providers is billed for that leg.
    MarketBest,
    /// No provider in this market publishes a cache price; cached tokens
    /// are billed at the fresh-input rate.
    InputRate,
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
/// unknown provider name falls back to the headline legs, and an explicit
/// provider without its own cache ask bills cached tokens at its input rate
/// instead of borrowing another provider's cache ask.
fn resolve_calc_legs(quote: &Quote, provider: Option<&str>) -> CalcLegs {
    if let Some(option) = provider.and_then(|name| {
        quote
            .market_options
            .iter()
            .find(|option| option.provider == name)
    }) {
        return CalcLegs {
            provider: option.provider.clone(),
            input: option.input,
            output: option.output,
            cache_read: option.cache_read,
            cache_from: if option.cache_read.is_some() {
                CacheRateSource::ProviderAsk
            } else {
                CacheRateSource::InputRate
            },
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
        None => (None, CacheRateSource::InputRate),
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
        let cache_filter = self.models_cache;
        let weights = (
            self.settings.input_weight,
            self.settings.cache_read_weight,
            self.settings.output_weight,
        );
        let mut rows: Vec<ModelsRow<'_>> = quotes
            .iter()
            .filter(|quote| models_search_matches(&query, quote))
            .filter(|quote| source.allows(quote))
            .filter(|quote| modality.allows(quote.vision))
            .filter(|quote| cache_filter.allows(quote))
            .filter(|quote| {
                creator
                    .as_deref()
                    .is_none_or(|c| group_label(&quote.creator) == c)
            })
            .map(|quote| ModelsRow {
                quote,
                blended: blended_price(
                    quote.input,
                    quote.cache_read,
                    quote.output,
                    weights.0,
                    weights.1,
                    weights.2,
                ),
                cache_read: quote.best_available_cache_read(),
            })
            .collect();
        sort_models_rows(&mut rows, self.models_sort, self.models_sort_desc);

        ui.horizontal(|ui| {
            ui.strong(format!("{} of {} models", rows.len(), quotes.len()));
            if rows.len() != quotes.len() && ui.small_button("✗ Clear filters").clicked() {
                self.models_search.clear();
                self.models_creator = None;
                self.models_source = ModelsSource::All;
                self.models_modality = ModalityFilter::All;
                self.models_cache = ModelsCacheFilter::Any;
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

        ui.horizontal_wrapped(|ui| {
            ui.label("Search:");
            ui.add(
                egui::TextEdit::singleline(&mut self.models_search)
                    .hint_text("model, creator…")
                    .desired_width(180.0)
                    .clip_text(true),
            );
            ui.separator();
            ui.label("Creator:");
            egui::ComboBox::from_id_salt("models_creator")
                .width(150.0)
                .selected_text(self.models_creator.as_deref().unwrap_or("All creators"))
                .show_ui(ui, |ui| {
                    let mut picked = self.models_creator.clone().unwrap_or_default();
                    if ui
                        .selectable_value(&mut picked, String::new(), "All creators")
                        .changed()
                    {
                        self.models_creator = None;
                    }
                    for (name, count) in &creators {
                        if ui
                            .selectable_value(
                                &mut picked,
                                (*name).to_owned(),
                                format!("{name} ({count})"),
                            )
                            .changed()
                        {
                            self.models_creator = Some((*name).to_owned());
                        }
                    }
                });
            ui.separator();
            ui.label("Source:");
            egui::ComboBox::from_id_salt("models_source")
                .selected_text(self.models_source.label())
                .show_ui(ui, |ui| {
                    for source in [
                        ModelsSource::All,
                        ModelsSource::LiveMarket,
                        ModelsSource::Catalog,
                    ] {
                        ui.selectable_value(&mut self.models_source, source, source.label());
                    }
                });
            ui.separator();
            ui.label("Modality:");
            egui::ComboBox::from_id_salt("models_modality")
                .selected_text(self.models_modality.label())
                .show_ui(ui, |ui| {
                    for filter in [
                        ModalityFilter::All,
                        ModalityFilter::Vision,
                        ModalityFilter::TextOnly,
                    ] {
                        ui.selectable_value(&mut self.models_modality, filter, filter.label());
                    }
                });
            ui.separator();
            ui.label("Cache:");
            egui::ComboBox::from_id_salt("models_cache")
                .selected_text(self.models_cache.label())
                .show_ui(ui, |ui| {
                    for filter in [
                        ModelsCacheFilter::Any,
                        ModelsCacheFilter::Priced,
                        ModelsCacheFilter::NoCachePrice,
                    ] {
                        ui.selectable_value(&mut self.models_cache, filter, filter.label());
                    }
                });
        });
    }

    fn models_table(
        &mut self,
        ui: &mut egui::Ui,
        rows: &[ModelsRow<'_>],
        weights: (f64, f64, f64),
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
                // Cheapest-first for price columns, A→Z for names.
                self.models_sort_desc = false;
            }
        }
    }

    fn models_table_row(
        &mut self,
        ui: &mut egui::Ui,
        row: &ModelsRow<'_>,
        weights: (f64, f64, f64),
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
        ui.label(format_price_tick(row.blended))
            .on_hover_text(format!(
                "Blended at your workload weights ({:.0}/{:.0}/{:.0} fresh/cache/output)",
                weights.0, weights.1, weights.2,
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
        if !market_info.is_empty() {
            provider_label.on_hover_text(market_info.join(" · "));
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
                    || format!("{} (at input rate)", format_price_tick(legs.input)),
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
            CacheRateSource::InputRate => {
                ui.small("No cache price published — cached tokens are billed at the input rate.");
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
                    self.alert_threshold = blended_price(
                        quote.input,
                        quote.cache_read,
                        quote.output,
                        self.settings.input_weight,
                        self.settings.cache_read_weight,
                        self.settings.output_weight,
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

/// Order the table rows by the active sort. Missing cache prices sort as the
/// most expensive cache rate so they sink in cheapest-first order; every key
/// tie-breaks on display name so the order is stable across polls.
fn sort_models_rows(rows: &mut [ModelsRow<'_>], sort: ModelsSort, desc: bool) {
    let by_name = |a: &ModelsRow<'_>, b: &ModelsRow<'_>| {
        a.quote
            .display_name
            .cmp(&b.quote.display_name)
            .then_with(|| a.quote.model.cmp(&b.quote.model))
    };
    let cache_key = |row: &ModelsRow<'_>| row.cache_read.unwrap_or(f64::INFINITY);
    let discount_key = |row: &ModelsRow<'_>| row.quote.discount_pct.unwrap_or(f64::NEG_INFINITY);
    rows.sort_by(|a, b| {
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

        // Nobody publishes one: cached tokens fall back to the input rate.
        q.market_options.clear();
        let legs = resolve_calc_legs(&q, None);
        assert_eq!(legs.cache_from, CacheRateSource::InputRate);
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
        // An explicit provider without its own cache ask is billed at its
        // input rate, never another provider's cache ask.
        assert_eq!(legs.cache_read, None);
        assert_eq!(legs.cache_from, CacheRateSource::InputRate);

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
}
