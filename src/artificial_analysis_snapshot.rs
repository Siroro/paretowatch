//! Bundled Artificial Analysis Intelligence Index snapshot.
//!
//! This data lives apart from `main.rs` so the table can be bumped without
//! touching any fetch/parse code. It is never presented as live data: the UI
//! labels it as a static snapshot and the composite treats it as a broad
//! capability prior that works without an API key.
//!
//! # Keeping the snapshot up to date
//!
//! 1. Open the Artificial Analysis Intelligence Index for [`ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION`]
//!    and read each relevant model page.
//! 2. Store ONE representative score per model family:
//!    - Prefer the best reasoning/thinking variant that Surplus actually lists;
//!      fall back to instruct/non-reasoning numbers only when no reasoning run
//!      exists.
//!    - Deployment wrappers (E2EE/web/private/fast/highspeed) intentionally
//!      inherit the base score because benchmark matching strips those tokens,
//!      so never add separate rows for them.
//!    - When AA only rates a sibling variant, proxy it and mark the row with
//!      `~proxy`; pure AA estimates get `~est`.
//!    - SKUs that are execution modes of an already-rated model (same weights,
//!      for example `reasoning.mode: "pro"`) get one row per purchasable SKU
//!      with the base score, and MUST be listed in
//!      [`INHERITED_EXECUTION_MODE_NOTES`] so the UI discloses the inheritance.
//! 3. Bump [`ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION`] and
//!    [`ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE`] together with the values.
//! 4. Run `cargo test`: the tests below reject duplicate normalized keys,
//!    out-of-range scores, and accidental row loss during bumps.
//!
//! The 2026-09-19 rebaseline moved the snapshot from index v4.1.2 to v4.3.
//! v4.2 (2026-09-04) made tasks harder and more realistic with more private
//! test sets, and v4.3 (2026-09-07) upgraded Terminal-Bench to 4.0 and replaced
//! 𝜏³-Banking with AutomationBench-AA, so the whole scale compressed: the
//! leader went from AA 66 to AA 53. Every value below was re-read from the
//! v4.3 model pages; the composite's `AA_CALIBRATION_POINTS` was recalibrated
//! in the same change.
//!
//! The 2026-09-22 bump moved the snapshot to v4.3.2, a patch revision on the
//! same scale as v4.3: the v4.3-read rows round to their live v4.3.2 displays
//! (Fable 5.1 53.4 → 53, Opus 5 50.7 → 51, GPT-5.6 Sol 47.1 → 47), so only the
//! new-model rows were read fresh rather than the whole table. AA now shows
//! integer scores on the model pages, so the new rows are integers. Claude
//! Opus 5.5 entered as the new AA leader at 58, and the composite's top
//! `AA_CALIBRATION_POINTS` anchor moved with it so the leader tier keeps
//! spreading below the 99th percentile instead of piling onto the ceiling.
//!
//! The 2026-09-29 bump stayed on v4.3.2: the leader (Claude Opus 5.5, 58) and
//! the scale are unchanged and the older rows still round to their live
//! displays (Fable 5.1 53.4 → 53, GPT-6 Astra 52.8 → 53), so only the week's
//! new-model rows were read fresh. Claude Sonnet 5.5 — Anthropic's new
//! Sonnet-class model, released 2026-09-28 — entered at 56 (#3 of 216 on its
//! page, adaptive reasoning at max effort with default fallback, read like
//! Opus 5.5), slotting between the leader and the Fable 5.1/Astra tier.
//! Cohere's Command A+ is the snapshot's first Cohere row (AA 13). AA
//! publishes no Qwen3.8 Omni page (its sitemap now lists only
//! `qwen3-8-flash-next`, a distinct Aug-26 preview), so the Surplus Omni Flash
//! SKU ~proxies the text sibling's existing row. GLM 5.3 Prime is a
//! same-checkpoint high-speed serving variant of GLM-5.3 (per the Surplus
//! listing, "inheriting its full capabilities"), so it has no row of its own:
//! benchmark matching strips the new `prime` deployment token (alongside
//! fast/highspeed/ultraspeed) and the SKU joins the GLM-5.3 row exactly, the
//! same inheritance MiMo-V2.6-Pro-UltraSpeed uses.
//!
//! The 2026-09-30 bump stayed on v4.3.2 again: the leader (Claude Opus 5.5,
//! 58) and the scale are unchanged, so only the two days' new-model rows were
//! read fresh. GPT-6.1 Sol — OpenAI's Sol-tier upgrade over GPT-6 Sol, still
//! positioned below the flagship GPT-6 Astra — entered at 52 (#10 of 222, read
//! at the max effort the Surplus SKU exposes via `reasoning_effort`), landing
//! between the Fable 5.1/Astra tier and GPT-6 Sol's 48. GPT-6.1 Sol Pro is
//! the same weights served with `reasoning.mode: "pro"`, so it inherits the
//! row through [`INHERITED_EXECUTION_MODES`] like the GPT-6 pro SKUs.
//! Inception's Mercury 2.5 (12) is the diffusion-LLM refresh of Mercury 2; its
//! row carries the `Inception: ` prefix so it exact-joins the Surplus display
//! name — `inception`, like `cohere`, is not a creator prefix that benchmark
//! matching strips. ByteDance's Seed-2.0-Code has no AA page (404, and the
//! sitemap lists no Seed URL) and joins the unrated list below.
//!
//! A second 2026-09-30 bump the same day added Gemini 4 Argon, Google
//! DeepMind's first proprietary model above the Flash class in over seven
//! months (the last was Gemini 3.1 Pro Preview). Read at high reasoning — the
//! highest effort Argon exposes — it entered at 53, tying GPT-6 Astra's live
//! display (the stored Astra row keeps its 52.8 v4.3 decimal read), one point
//! over GPT-6.1 Sol, 12 over Gemini 3.8 Flash and 23 over Gemini 3.1 Pro
//! Preview, which puts Google back among the top three labs by intelligence.
//! The leader (Claude Opus 5.5, 58) and the scale are unchanged, so the index
//! stays v4.3.2 and the calibration keeps its anchors. Argon is rolling out
//! to selected users only, behind a 50% launch pricing discount on $4/$20
//! pricing, so the row lands ahead of a general Surplus listing; its agentic
//! results (AutomationBench-AA 77.5, Terminal-Bench 4 at 57, AA-Briefcase
//! 1494 Elo) live on boards the app fetches live, not in this snapshot.
//!
//! Deliberately unrated (no AA presence as of 2026-09-30): Aion 2.0/3.0/3.0
//! Mini, Hermes 3 405B, Palmyra Vision 7B, Qwen 2.5 7B, GPT-OSS Safeguard
//! 20B/120B, Grok 4.20 Multi-Agent Beta, Venice rebrands, uncensored finetunes
//! such as GLM 4.7 Flash Heretic, Seed 2.1 Turbo and Seed-2.0-Code, the
//! 2026-09-11 Sakana
//! listings (Fugu Max, Fugu Ultra v2), MiMo-V2.6-Flash (the Pro sibling is
//! rated; AA has no Flash page yet), TypeSafe's JEV 1.13 decisions model, and
//! Fireworks Ember-1 (a Kimi-K3-derived reasoning model; AA has no page, and
//! it deliberately does NOT proxy Kimi K3 — it is a derivative build, not the
//! same checkpoint).
//! MiMo-V2.6-Pro-UltraSpeed is a same-checkpoint speed wrapper, not an
//! unrated model: it inherits the MiMo-V2.6-Pro row because benchmark
//! matching strips the `ultraspeed` deployment token. AA had additionally
//! REMOVED the GLM-4.5 and Llama 3.2 3B pages between v4.1.2 and v4.3, so
//! those families lost their rows then rather than keeping stale v4.1-scale
//! numbers. Unrated models fall back to the composite's small neutral prior
//! for the missing evidence instead of renormalizing it away, so absent rows
//! cost coverage confidence but never invent capability.

use crate::fetch::score_benchmark;
use crate::types::{Benchmark, BenchmarkKind};

pub(crate) const ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE: &str = "2026-09-30";
pub(crate) const ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION: &str = "v4.3.2";

/// `(model name, AA Intelligence score)` pairs, one representative row per
/// family. Names should mirror Surplus display names so exact-key joins hit
/// before fuzzy matching has to.
pub(crate) const SNAPSHOT_ROWS: &[(&str, f64)] = &[
    // Added 2026-09-30 (second same-day bump), read from the still-v4.3.2
    // index: Gemini 4 Argon, Google DeepMind's first proprietary model above
    // the Flash class in over seven months. Read at high reasoning — the
    // highest effort Argon exposes — it entered at 53, tying GPT-6 Astra's
    // live display (the stored Astra row keeps its 52.8 v4.3 read), one point
    // over GPT-6.1 Sol and 12 over Gemini 3.8 Flash, putting Google back
    // among the top three labs. Argon is rolling out to selected users only
    // (50% launch discount on $4/$20 pricing), so the row lands ahead of the
    // general Surplus listing; `argon` is a codename, not a stripped
    // deployment token, so the key joins nothing else.
    ("Gemini 4 Argon", 53.0),
    // Added 2026-09-30 for the Surplus listings of 2026-09-29..09-30, read
    // from the still-v4.3.2 index (leader and scale unchanged). GPT-6.1 Sol is
    // OpenAI's Sol-tier upgrade over GPT-6 Sol, still below flagship GPT-6
    // Astra; read at max effort — the effort the Surplus SKU exposes via
    // `reasoning_effort` — it entered at #10 of 222, between the Fable 5.1/
    // Astra tier and GPT-6 Sol's 48. Its Pro SKU is a `reasoning.mode: "pro"`
    // execution mode of the same weights (see INHERITED_EXECUTION_MODES).
    // Mercury 2.5 is Inception's diffusion-LLM refresh of Mercury 2; the name
    // carries the `Inception: ` prefix so it exact-joins the Surplus display
    // name (`inception` is not a stripped creator prefix, same as Cohere).
    // Seed-2.0-Code has no AA page and is unrated — see the header notes.
    ("GPT-6.1 Sol", 52.0),
    ("GPT-6.1 Sol Pro", 52.0), // same weights as GPT-6.1 Sol; see INHERITED_EXECUTION_MODES
    ("Inception: Mercury 2.5", 12.0),
    // Added 2026-09-29 for the Surplus listings of 2026-09-23..09-29, read
    // from the still-v4.3.2 index (leader and scale unchanged). Claude Sonnet
    // 5.5 (released 2026-09-28) entered at #3 of 216, adaptive reasoning at
    // max effort with default fallback — the same page configuration Opus 5.5
    // was read at. Command A+ is the first Cohere row; the name carries the
    // `Cohere: ` prefix so it exact-joins the Surplus display name (the id
    // `command-a-plus` normalizes differently: `+` collapses away). The
    // Qwen3.8 Omni Flash SKU has no AA page and ~proxies its text sibling.
    // GLM 5.3 Prime (same-checkpoint speed variant) and Fireworks Ember-1
    // (Kimi-K3 derivative, unrated) need no rows — see the header notes.
    ("Claude Sonnet 5.5", 56.0),
    ("Qwen3.8 Omni Flash", 39.9), // ~proxy: text sibling Qwen3.8 Flash; AA has no omni page
    ("Cohere: Command A+", 13.0),
    // Added 2026-09-22 for the Surplus listings of 2026-09-20..09-22, read
    // from the v4.3.2 index (same scale as v4.3, integer displays). Claude
    // Opus 5.5 is Anthropic's new flagship and entered as AA's new leader
    // (#1 of 212 on its page, adaptive reasoning at max effort with default
    // fallback). The GPT-6 series fills out below Astra: Sol is the
    // cost-efficient high-end tier and Luna the fast tier, both read at max
    // effort, the effort the Surplus SKUs expose via `reasoning_effort`;
    // their Pro SKUs are `reasoning.mode: "pro"` execution modes of the same
    // weights (see INHERITED_EXECUTION_MODES). Grok 4.7 has no max-effort
    // measurement, so its row is the xhigh run AA labels on the model page.
    // MiMo-V2.6-Pro is Xiaomi's new 1T flagship at AA 46; the UltraSpeed SKU
    // and the unrated V2.6-Flash are covered by the header notes.
    ("Claude Opus 5.5", 58.0),
    ("GPT-6 Sol", 48.0),
    ("GPT-6 Sol Pro", 48.0), // same weights as GPT-6 Sol; see INHERITED_EXECUTION_MODES
    ("Grok 4.7", 46.0),
    ("MiMo-V2.6-Pro", 46.0),
    ("GPT-6 Luna", 37.0),
    ("GPT-6 Luna Pro", 37.0), // same weights as GPT-6 Luna; see INHERITED_EXECUTION_MODES
    // Added 2026-09-19 for the Surplus listings of 2026-09-03..09-11, read
    // from the v4.3 index. GPT-6 Astra entered at AA's top alongside Fable 5.1
    // (AA's own writeup calls it a tie at lower cost); DeepSeek V4.1 Flash is
    // the 09-10 refresh; Muse Spark 1.3 Contributor is Meta's contributor-
    // licensed SKU of the 1.3 weights, which AA does not rate separately, so
    // it ~proxies the purchasable xhigh variant like its base row.
    ("Claude Fable 5.1", 53.4),
    ("GPT-6 Astra", 52.8),
    ("Claude Opus 5", 50.7),
    ("Claude Fable 5", 49.7),
    ("GPT-5.6 Sol", 47.1),
    ("Grok 4.6", 44.4),
    ("Kimi K3", 43.8),
    ("GLM-5.3", 44.9),
    ("GLM-5.3-Flash", 41.9),
    ("Qwen3.8 Max", 45.4),
    ("Qwen3.8 2.4T A95B", 40.0),
    ("GPT-5.6 Terra", 42.3),
    ("Muse Spark 1.3", 45.2),
    ("Meta: Muse Spark 1.3 Contributor", 45.2), // ~proxy: xhigh sibling, AA rates no Contributor page
    ("Muse Spark 1.2", 39.8),
    ("DeepSeek V4.1 Flash", 39.5),
    ("Gemini 3.8 Flash", 41.2),
    ("Gemini 3.7 Flash", 39.4),
    ("Gemini 3.6 Flash", 34.3),
    ("Grok 4.5", 39.1),
    ("Claude Sonnet 5", 38.4),
    ("GPT-5.5", 38.6),
    ("Claude Opus 4.8", 42.0),
    ("Claude Opus 4.7", 40.7),
    ("Muse Spark 1.1", 34.3),
    ("Muse Glimmer 30B", 18.1),
    ("DeepSeek V4 Pro 0813", 36.3),
    ("GLM-5.2", 34.0),
    ("GPT-5.4", 39.0),
    ("GPT-5.4 Mini", 24.6),
    ("GPT-5.4 Nano", 21.2),
    ("GPT-5.2", 30.4),
    ("GPT-5 Mini", 17.4),
    ("DeepSeek V4 Flash 0731", 34.5),
    ("GPT-5.6 Luna", 37.5),
    ("Gemini 3.5 Flash", 33.0),
    ("Gemini 3.5 Flash Lite", 22.7),
    ("Gemini 3.1 Pro Preview", 30.4),
    ("Gemini 3.1 Flash-Lite", 16.0),
    ("Gemini 3 Flash Preview", 26.3),
    ("Claude Sonnet 4.6", 24.7),
    ("MiniMax-M3", 29.6),
    ("MiniMax-M2.7", 23.2),
    ("MiniMax-M2.5", 22.8),
    ("MiniMax-M2.1", 20.9),
    ("MiniMax-M2", 18.6),
    ("DeepSeek V4 Pro", 30.9),
    ("Kimi K2.6", 27.5),
    ("Claude Opus 4.6", 26.4),
    ("Qwen3.8 27B", 33.9),
    ("Qwen3.8 Flash", 39.9),
    ("Kimi K2.7 Code", 26.3),
    ("Kimi K2 Thinking", 22.0),
    ("Kimi K2", 12.7),
    ("MiMo-V2.5-Pro", 26.4),
    ("MiMo-V2.5", 22.3),
    ("Hy3", 25.8),
    ("DeepSeek V4 Flash", 24.6),
    ("GLM-5.1", 26.4),
    ("GLM-5", 27.9),
    ("GLM-4.7", 22.2),
    ("GLM-4.7-Flash", 14.9),
    ("GLM-4.6", 14.9),
    // GLM-4.5 had a v4.1.1-era row (20.0); AA removed its model page before
    // v4.3, so the family is unrated now and falls back to the neutral prior.
    ("GLM-4.5-Air", 11.1),
    ("Grok 4.3", 25.4),
    ("Grok 4.1 Fast", 17.9),
    ("Grok 4", 22.5),
    ("NVIDIA Nemotron 3 Ultra 550B A55B", 23.4),
    ("NVIDIA Nemotron 3.5 Lightning 30B", 13.6),
    ("Claude 4.5 Sonnet", 19.3),
    ("Claude Opus 4.5", 23.7),
    ("Kimi K2.5", 23.5),
    ("Qwen3.5 397B A17B", 19.1),
    ("Qwen3.5 122B A10B", 16.2),
    ("DeepSeek V3.2", 16.0),
    ("DeepSeek V3.1", 13.7),
    ("DeepSeek V3.1 Terminus", 13.9),
    // AA folded the R1 0528 refresh into the base DeepSeek R1 page, so both
    // Surplus SKUs read the same v4.3 measurement.
    ("DeepSeek R1 0528", 13.1),
    ("DeepSeek R1", 13.1),
    ("Qwen3.6 27B", 21.9),
    ("Qwen3.6 35B A3B", 18.8),
    ("Qwen3.5 35B A3B", 19.3),
    ("Qwen3 Next 80B A3B Instruct", 9.6),
    ("Qwen3 Coder Next", 10.1),
    ("Qwen3 Coder 480B", 11.9),
    ("Qwen3 Coder 480B Turbo", 11.9), // ~proxy: no separate AA page for the turbo SKU
    ("Gemma 4 31B", 15.4),
    ("Gemma 4 26B A4B", 16.7),
    ("Claude 4.5 Haiku", 15.4),
    ("GPT-5.5 Instant", 26.8),
    ("gpt-oss-120b", 12.3),
    ("gpt-oss-20b", 9.0),
    ("Qwen3 235B A22B 2507", 12.0),
    ("Mistral Small 3.2", 7.0),
    ("Mistral Small 3.2 24B Instruct", 7.0),
    ("Mistral Small 4", 11.5),
    ("Mistral Large 3", 9.7),
    ("Llama 4 Scout", 6.5),
    ("Llama 3.3 70B", 7.7),
    //
    // Backfill (re-read from the v4.3 model pages on 2026-09-19): remaining
    // Surplus catalog text models. `~proxy` rows reuse the closest variant AA
    // does rate.
    //
    // OpenAI
    ("GPT-4o", 8.4),
    ("GPT-4o Mini", 6.7),
    ("GPT-5 Nano", 13.0),
    ("GPT-5.2 Codex", 28.5),
    ("GPT-5.3 Codex", 32.5),
    ("GPT-5.6 Sol Pro", 47.1), // same weights as GPT-5.6 Sol; see INHERITED_EXECUTION_MODE_NOTES
    ("GPT-5.6 Terra Pro", 42.3), // same weights as GPT-5.6 Terra; see INHERITED_EXECUTION_MODE_NOTES
    ("GPT-5.6 Luna Pro", 37.5),  // same weights as GPT-5.6 Luna; see INHERITED_EXECUTION_MODE_NOTES
    // Google
    ("Gemini 2.5 Pro", 16.7),
    ("Gemini 2.5 Flash", 9.9),
    // xAI
    ("Grok 4.20 Beta", 25.7),
    ("Grok Build 0.1", 27.2),
    ("Grok Code Fast 1", 14.1),
    // Z AI
    ("GLM-4.7-Thinking", 22.2),
    ("GLM-5 Turbo", 26.6),
    ("GLM 5V Turbo", 23.5),
    // Meta — Llama 3.2 3B lost its AA page along with GLM-4.5; it was an
    // ~est row already and stays unrated under v4.3.
    // Gemma open models
    ("Gemma 3 27B", 4.9),
    ("Gemma 3 12B", 3.8),
    ("Gemma 3 4B", 4.8),
    ("Gemma 4 E2B", 7.8),
    // Mistral family
    ("Magistral Small 2509", 8.6),
    ("Ministral 14B 3.0", 6.0),
    ("Ministral 3 8B", 5.5),
    ("Ministral 3B", 4.8),
    ("Devstral 2 123B", 9.4),
    // NVIDIA
    ("NVIDIA Nemotron 3 Super 120B", 13.6),
    ("NVIDIA Nemotron Cascade 2 30B", 11.7),
    ("NVIDIA Nemotron 3 Nano 30B A3B", 8.9),
    ("NVIDIA Nemotron Nano 12B v2", 7.5), // ~est: VL Reasoning page (non-reasoning unrated)
    ("NVIDIA Nemotron Nano 9B v2", 6.8),
    // Qwen
    ("Qwen 3.7 Max", 29.9),
    ("Qwen 3.7 Plus", 25.8),
    ("Qwen3.5 Plus", 20.4),  // ~proxy: Qwen3.5 Omni Plus
    ("Qwen3.5 Flash", 12.5), // ~proxy: Qwen3.5 Omni Flash
    ("Qwen 3.5 9B", 13.7),
    ("Qwen3 VL 235B A22B", 13.4),
    ("Qwen3 235B A22B Thinking 2507", 12.7),
    ("Qwen3 32B", 7.2),
    ("Qwen3 30B A3B", 7.6), // reasoning variant (instruct is lower)
    ("Qwen3 Coder 30B A3B Instruct", 9.6),
    // Others
    ("Mercury 2", 11.5),
    ("Trinity Large Thinking", 10.9),
];

/// Rows sold by Surplus as separate SKUs that are actually execution modes of
/// the same weights (for example OpenAI serves `gpt-5.6-sol-pro` as the Sol
/// model with `reasoning.mode: "pro"`). Each needs its own snapshot row so
/// Surplus prices join, but no leaderboard has measured the mode itself, so the
/// score is inherited from the base variant. `(execution-mode SKU, base SKU)`
/// pairs; `main.rs` uses this table to resolve the mode's canonical model key
/// to the base variant when joining live leaderboard rows, and the builder
/// appends an inheritance note to the rendered row name.
pub(crate) const INHERITED_EXECUTION_MODES: &[(&str, &str)] = &[
    ("GPT-6.1 Sol Pro", "GPT-6.1 Sol"),
    ("GPT-6 Sol Pro", "GPT-6 Sol"),
    ("GPT-6 Luna Pro", "GPT-6 Luna"),
    ("GPT-5.6 Sol Pro", "GPT-5.6 Sol"),
    ("GPT-5.6 Terra Pro", "GPT-5.6 Terra"),
    ("GPT-5.6 Luna Pro", "GPT-5.6 Luna"),
];

pub(crate) fn artificial_analysis_snapshot() -> Vec<Benchmark> {
    SNAPSHOT_ROWS.iter()
        .map(|(model, score)| {
            let mut row = score_benchmark(
                *model,
                *score,
                None,
                None,
                BenchmarkKind::Model,
            );
            let inherited = INHERITED_EXECUTION_MODES.iter()
                .find(|(mode, _)| *mode == *model)
                .map(|(_, base)| *base);
            row.name = match inherited {
                Some(base) => format!(
                    "{} [AA Intelligence {} · snapshot {} · inherited from {} (max effort); pro mode not separately benchmarked]",
                    model,
                    ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION,
                    ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE,
                    base,
                ),
                None => format!(
                    "{} [AA Intelligence {} · snapshot {}]",
                    model,
                    ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION,
                    ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE,
                ),
            };
            row
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bench::matching::benchmark_model_key;

    #[test]
    fn rows_are_well_formed() {
        for (model, score) in SNAPSHOT_ROWS {
            assert!(!model.trim().is_empty(), "blank row name");
            assert!(
                score.is_finite() && *score >= 0.0 && *score <= 100.0,
                "{model}: implausible score {score}",
            );
        }
    }

    #[test]
    fn rows_have_unique_normalized_keys() {
        let mut seen = std::collections::HashSet::new();
        for (model, _) in SNAPSHOT_ROWS {
            let key = benchmark_model_key(model);
            assert!(!key.is_empty(), "{model}: normalizes to an empty key");
            assert!(
                seen.insert(key.clone()),
                "duplicate normalized key `{key}` ({model}); collapse_benchmark_rows would silently drop one",
            );
        }
    }

    #[test]
    fn snapshot_keeps_growing_with_each_bump() {
        assert!(
            SNAPSHOT_ROWS.len() >= 120,
            "row count dropped to {}; a bump must never remove existing families",
            SNAPSHOT_ROWS.len(),
        );
    }

    #[test]
    fn snapshot_metadata_is_plausible() {
        assert_eq!(ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE.len(), 10);
        assert!(ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE.starts_with("20"));
        assert!(ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION.starts_with('v'));
    }

    #[test]
    fn builder_tags_every_row_with_snapshot_provenance() {
        let rows = artificial_analysis_snapshot();
        assert_eq!(rows.len(), SNAPSHOT_ROWS.len());
        assert!(rows.iter().all(|row| {
            row.name.contains("AA Intelligence")
                && row.name.contains(ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION)
                && row.name.contains(ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE)
        }));
    }

    #[test]
    fn inherited_mode_pairs_reference_real_rows_and_disclose_in_name() {
        let names: std::collections::HashSet<_> =
            SNAPSHOT_ROWS.iter().map(|(model, _)| *model).collect();
        assert!(!INHERITED_EXECUTION_MODES.is_empty());
        for (mode, base) in INHERITED_EXECUTION_MODES {
            assert!(
                names.contains(mode),
                "pair references unknown mode SKU `{mode}`"
            );
            assert!(
                names.contains(base),
                "pair references unknown base variant `{base}`"
            );
            assert_ne!(
                benchmark_model_key(mode),
                benchmark_model_key(base),
                "{mode} must normalize differently from {base}"
            );
        }
        let rows = artificial_analysis_snapshot();
        for (mode, base) in INHERITED_EXECUTION_MODES {
            let row = rows
                .iter()
                .find(|row| benchmark_model_key(&row.slug) == benchmark_model_key(mode))
                .unwrap();
            assert!(
                row.name.contains(base) && row.name.contains("not separately benchmarked"),
                "`{}` name does not disclose its inheritance from {base}",
                row.slug
            );
        }
    }

    #[test]
    fn aa_snapshot_covers_current_deepseek_and_glm_models() {
        let rows = artificial_analysis_snapshot();
        let deepseek_release = rows
            .iter()
            .find(|b| benchmark_model_key(&b.slug) == "deepseek v4 flash 0731")
            .unwrap();
        let deepseek_base = rows
            .iter()
            .find(|b| benchmark_model_key(&b.slug) == "deepseek v4 flash")
            .unwrap();
        let glm = rows
            .iter()
            .find(|b| benchmark_model_key(&b.slug) == "glm 5 3")
            .unwrap();
        assert_eq!(deepseek_release.agentic_coding, Some(34.5));
        assert_eq!(deepseek_base.agentic_coding, Some(24.6));
        assert_eq!(glm.agentic_coding, Some(44.9));
    }

    #[test]
    fn aa_snapshot_covers_september_2026_surplus_listings() {
        // Rows added 2026-09-02 (re-read 2026-09-19 under index v4.3) and
        // 2026-09-19 (GPT-6 Astra, DeepSeek V4.1 Flash, Muse Spark 1.3
        // Contributor). The Lite/Flash and V2.5/V2.5-Pro pairs must stay
        // distinct keys so the cheaper variants never inherit the flagship
        // score (the unique-key test enforces this globally).
        let rows = artificial_analysis_snapshot();
        let score_of = |name: &str| {
            rows.iter()
                .find(|b| benchmark_model_key(&b.slug) == benchmark_model_key(name))
                .and_then(|b| b.agentic_coding)
        };
        for (name, expected) in [
            ("Claude Fable 5.1", 53.4),
            ("GPT-6 Astra", 52.8),
            ("Gemini 3.6 Flash", 34.3),
            ("Gemini 3.5 Flash Lite", 22.7),
            ("Qwen3.8 Flash", 39.9),
            ("MiMo-V2.5", 22.3),
            ("Muse Glimmer 30B", 18.1),
            ("NVIDIA Nemotron 3.5 Lightning 30B", 13.6),
            ("Muse Spark 1.3", 45.2),
            ("Gemini 3.8 Flash", 41.2),
            ("DeepSeek V4.1 Flash", 39.5),
            ("Meta: Muse Spark 1.3 Contributor", 45.2),
        ] {
            assert_eq!(
                score_of(name),
                Some(expected),
                "{name} missing or wrong in snapshot"
            );
        }
    }

    #[test]
    fn aa_snapshot_covers_the_2026_09_22_flagship_refresh() {
        // Rows added 2026-09-22 under index v4.3.2: Claude Opus 5.5 (the new
        // AA leader), the GPT-6 Sol/Luna tiers plus their pro execution-mode
        // SKUs, Grok 4.7 (xhigh; AA measured no max run), and Xiaomi's
        // MiMo-V2.6-Pro. The UltraSpeed SKU must collapse onto the Pro key so
        // the same-checkpoint wrapper inherits the row, and the pro modes must
        // stay distinct keys resolved through INHERITED_EXECUTION_MODES.
        let rows = artificial_analysis_snapshot();
        let score_of = |name: &str| {
            rows.iter()
                .find(|b| benchmark_model_key(&b.slug) == benchmark_model_key(name))
                .and_then(|b| b.agentic_coding)
        };
        for (name, expected) in [
            ("Claude Opus 5.5", 58.0),
            ("GPT-6 Sol", 48.0),
            ("GPT-6 Sol Pro", 48.0),
            ("GPT-6 Luna", 37.0),
            ("GPT-6 Luna Pro", 37.0),
            ("Grok 4.7", 46.0),
            ("MiMo-V2.6-Pro", 46.0),
            ("MiMo-V2.6-Pro-UltraSpeed", 46.0),
        ] {
            assert_eq!(
                score_of(name),
                Some(expected),
                "{name} missing or wrong in snapshot"
            );
        }
    }

    #[test]
    fn aa_snapshot_covers_the_2026_09_29_sonnet_refresh() {
        // Rows added 2026-09-29 under the still-v4.3.2 index: Claude Sonnet
        // 5.5 (the new #3, below Opus 5.5's 58 and above the Fable 5.1/Astra
        // tier), the first Cohere row, and the ~proxied Qwen3.8 Omni Flash.
        // GLM 5.3 Prime inherits the GLM-5.3 row through key equality (the
        // `prime` deployment token is stripped), NOT through a row of its own,
        // and the Sonnet family must keep 5 and 5.5 as distinct keys.
        let rows = artificial_analysis_snapshot();
        let score_of = |name: &str| {
            rows.iter()
                .find(|b| benchmark_model_key(&b.slug) == benchmark_model_key(name))
                .and_then(|b| b.agentic_coding)
        };
        for (name, expected) in [
            ("Claude Sonnet 5.5", 56.0),
            ("Qwen3.8 Omni Flash", 39.9),
            ("Cohere: Command A+", 13.0),
            ("Claude Sonnet 5", 38.4),
        ] {
            assert_eq!(
                score_of(name),
                Some(expected),
                "{name} missing or wrong in snapshot"
            );
        }
        assert_eq!(
            score_of("GLM 5.3 Prime"),
            Some(44.9),
            "GLM 5.3 Prime must inherit the GLM-5.3 row via key equality"
        );
        assert_ne!(
            benchmark_model_key("Claude Sonnet 5.5"),
            benchmark_model_key("Claude Sonnet 5"),
            "Sonnet 5.5 must not collapse onto Sonnet 5"
        );
        assert!(
            score_of("Ember-1").is_none(),
            "Fireworks Ember-1 is unrated and must not have a row"
        );
    }

    #[test]
    fn aa_snapshot_covers_the_2026_09_30_gpt_6_1_refresh() {
        // Rows added 2026-09-30 under the still-v4.3.2 index: GPT-6.1 Sol
        // (OpenAI's Sol-tier upgrade, read at max effort, #10 of 222) plus its
        // pro execution-mode SKU, and Inception's Mercury 2.5 refresh. The 6.1
        // keys must stay distinct from the GPT-6 family (version digits are
        // never stripped), Mercury 2.5 must not collapse onto Mercury 2, and
        // Seed-2.0-Code is unrated with no row of its own.
        let rows = artificial_analysis_snapshot();
        let score_of = |name: &str| {
            rows.iter()
                .find(|b| benchmark_model_key(&b.slug) == benchmark_model_key(name))
                .and_then(|b| b.agentic_coding)
        };
        for (name, expected) in [
            ("GPT-6.1 Sol", 52.0),
            ("GPT-6.1 Sol Pro", 52.0),
            ("Inception: Mercury 2.5", 12.0),
        ] {
            assert_eq!(
                score_of(name),
                Some(expected),
                "{name} missing or wrong in snapshot"
            );
        }
        assert_ne!(
            benchmark_model_key("GPT-6.1 Sol"),
            benchmark_model_key("GPT-6 Sol"),
            "GPT-6.1 must not collapse onto GPT-6"
        );
        assert_ne!(
            benchmark_model_key("Inception: Mercury 2.5"),
            benchmark_model_key("Mercury 2"),
            "Mercury 2.5 must not collapse onto Mercury 2"
        );
        assert!(
            score_of("Seed-2.0-Code").is_none(),
            "Seed-2.0-Code is unrated and must not have a row"
        );
    }

    #[test]
    fn aa_snapshot_covers_the_2026_09_30_gemini_4_argon_launch() {
        // Row added 2026-09-30 (second same-day bump) under the still-v4.3.2
        // index: Gemini 4 Argon at high reasoning — the highest effort it
        // exposes — ties GPT-6 Astra's live integer display while the stored
        // Astra row keeps its 52.8 v4.3 decimal read, and stands one point
        // over GPT-6.1 Sol, putting Google back among the top three labs with
        // the leader (Opus 5.5, 58) and the calibration anchors unchanged.
        // The codename must keep the row distinct from both the Flash family
        // and the last pre-Flash flagship.
        let rows = artificial_analysis_snapshot();
        let score_of = |name: &str| {
            rows.iter()
                .find(|b| benchmark_model_key(&b.slug) == benchmark_model_key(name))
                .and_then(|b| b.agentic_coding)
        };
        assert_eq!(
            score_of("Gemini 4 Argon"),
            Some(53.0),
            "Gemini 4 Argon missing or wrong in snapshot"
        );
        assert_eq!(
            score_of("GPT-6 Astra"),
            Some(52.8),
            "Astra keeps its v4.3 decimal read; Argon ties its live display"
        );
        assert_ne!(
            benchmark_model_key("Gemini 4 Argon"),
            benchmark_model_key("Gemini 3.8 Flash"),
            "Argon must not collapse onto the Flash family"
        );
        assert_ne!(
            benchmark_model_key("Gemini 4 Argon"),
            benchmark_model_key("Gemini 3.1 Pro Preview"),
            "Argon must not collapse onto the last pre-Flash flagship"
        );
    }
}
