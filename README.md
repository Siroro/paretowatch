# ParetoWatch

[![release](https://github.com/Siroro/paretowatch/actions/workflows/release.yml/badge.svg)](https://github.com/Siroro/paretowatch/actions/workflows/release.yml)
[![latest release](https://img.shields.io/github/v/release/Siroro/paretowatch)](https://github.com/Siroro/paretowatch/releases)

A lightweight native Rust tray dashboard for [Surplus Intelligence](https://surplusintelligence.ai) AI-model pricing and price/quality Pareto frontiers.

No Python. No Electron. No browser server. No benchmark API keys — every feed it consumes is public.

## What you get

- A tray app that polls Surplus market pricing every 30 seconds and re-fetches public benchmark boards every six hours, independently per source.
- A Pareto chart plotting **blended price** against your choice of benchmark score, with the efficient frontier labelled.
- Two locally-computed **composites** (capability and deployment) that fuse all boards into one robust score instead of trusting any single leaderboard.
- **Alerts**: desktop notifications on price thresholds, any price move, frontier entries/exits, and "cheapest above score" — plus a recent-moves feed.
- An **event-sourced history** log that records only actual changes (a poll that changes nothing costs zero bytes) and rebuilds per-model price/score series.
- Cross-source **model matching** so a Surplus quote joins the right benchmark rows even when names, spellings, or creators disagree between boards.

## Pricing — Surplus Intelligence

ParetoWatch uses Surplus in three layers:

1. `GET /v1/prices` — the provider comparison matrix.
2. `GET /v1/models` — public catalog fallback, including architecture metadata (vision/text modalities, provider-as-creator).
3. `GET /api/markets` — the live marketplace overlay.

Market token fields such as `best_input_per_1m` are integer micro-dollars per 1M tokens; they are converted to USD per 1M before plotting and alerting. Media-only markets are excluded. The market endpoint advertises a 30-second cache, so the minimum poll interval is 30 seconds.

When the market overlay exposes provider-level token legs, ParetoWatch selects a **single provider** by workload-blended price instead of mixing an input minimum from one provider with an output minimum from another. Liquidity filters (`Trusted`, `≥3 healthy sellers`, `≥10 healthy sellers`) reselect within the same market row, and a modality filter partitions vision-capable from text-only models.

## Blended price and cost basis

Blended price is a **normalized weighted average**, so it stays a true `$ / 1M-token` metric, and it accounts for prompt-cache-read pricing where a provider publishes it (falling back to that provider's input price when not). The default agentic-coding mix is 15 % fresh input / 80 % cache-read / 5 % output, with `95/0/5` no-cache and `50/0/50` balanced presets.

The cost-basis toggle switches the X-axis between $/1M and **estimated $ per benchmark task**, using token-per-task telemetry from boards that publish it — always repriced at *current* Surplus quotes, never at benchmark-time prices.

## Benchmark sources

Swap the Y-axis from the **Benchmark** dropdown without touching pricing.

| Source | Feed | Notes |
|---|---|---|
| Composite (capability) | computed locally | default; see below |
| Composite (deployment) | computed locally | same engine, deployment weighting |
| Artificial Analysis (snapshot) | bundled with the app | calibrated anchor scale, refreshed on app updates |
| SWE-rebench | public HTML leaderboard | uniform harness — full weight in both flavors |
| Terminal-Bench 3.0 | live Harbor leaderboard-read | falls back to a point-in-time snapshot |
| DeepSWE v1.1 | official `leaderboard-live.json` | restricted to `mini-swe-agent`, strongest effort tier |
| LiveBench | official repo, newest release | Overall / Coding / Agentic coding selectors |
| Revelo Code Index | public research page | scraped table |
| SWE-bench Live | official reports JSONL | latest rows aggregated across languages |
| SWE-bench Verified [legacy] | official `leaderboards.json` | model+agent evidence, kept for history |
| Frontend Design Elo | [Design Arena](https://designarena.ai/leaderboard) public API | crowdsourced blinded-vote Elo across design categories; model-level, not part of the agentic composite |

All feeds are public/no-key, refresh independently every six hours, and fail independently — one broken source shows its error in Settings without removing the others.

## How the composite is scored

The composite is a coverage-first, precision-weighted posterior — a model appears as soon as one credible board has evaluated it, and its score is:

```
score = (prior_w · prior_AA + Σ wᵢ · pctᵢ) / (prior_w + Σ wᵢ)
```

- Raw board scales are not comparable, so every source first becomes a **within-source percentile**.
- Each board's weight combines a hand-tuned base weight with a **population-precision factor** `n/(n+2)`: a #1-of-1 row on a tiny board carries almost no information and cannot outrank real measurements.
- Two symmetric **Huber passes** downweight sources sitting far from the consensus, replacing an older always-drop-the-lowest trim that biased well-covered models upward.
- **Missing-board skepticism**: a loaded board that skipped a model adds a partial pseudo-observation at her prior, so a hot model on two boards doesn't outrank broad, strong coverage.
- The **AA prior** is AA's percentile ranked within the union of models the agentic boards actually evaluate, with models AA hasn't measured falling back to a neutral prior; zero-evidence newcomers additionally take a confidence discount.
- **Capability vs deployment**: harness-specific boards (Terminal-Bench, DeepSWE, SWE-bench Live/Verified, Revelo) are demoted to one-third weight for capability questions; model-level boards (AA, LiveBench) take the demotion for deployment questions. SWE-rebench's uniform harness keeps full weight in both.
- The scoring method is **disclosed in each row's name** (measured / adjusted / prior / sparse evidence), and execution-mode SKUs (e.g. `-high`) inherit their base variant's rows.

Base weights: AA 0.25, SWE-rebench 0.20, Terminal-Bench 0.15, DeepSWE 0.15, LiveBench 0.10, SWE-bench Live 0.075, Revelo 0.05, SWE-bench Verified 0.025.

This is deliberately a consensus view, not a claim that one scalar is ground truth.

Selecting a model also shows a **consensus panel**: that model's rank, percentile, and score on every board it matched, side by side.

## Model matching

Surplus quotes and benchmark rows disagree on names (`Claude Opus 4.6` vs `Claude-4.6-Opus`), spellings (`Qwen3.5` vs `Qwen 3.5`), and creators. ParetoWatch canonicalizes both sides (lowercase token keys, creator normalization, release-tag and deployment-wrapper handling — `e2ee-`/`web` wrappers inherit the base model's benchmark) and joins on exact canonical key first, token-Jaccard fuzzy match second.

## Alerts

| Mode | Notifies when |
|---|---|
| Threshold | input/output/blended price crosses a target; edge-triggered, re-arms on crossing back |
| Any price change | every observed move, with old → new blended/input/output |
| Enters Pareto frontier | model joins the efficient frontier on the selected benchmark |
| Leaves Pareto frontier | model drops off the frontier |
| Cheapest above score | cheapest model above a score threshold changes |

Benchmark-dependent modes carry their own source/metric/comparison settings. Source transitions between the live market and a fallback feed are ignored, so an upstream outage doesn't fire a price alert. The Alerts tab keeps a recent-moves feed (green ↓ falling, red ↑ rising).

## History

Long-term history is an append-only event log (compact binary encoding) that records only actual changes, with per-model series rebuilt from it: blended/input/output prices and composite capability/deployment scores (recorded on engine version bumps or board data changes), plus a once-per-UTC-day market telemetry summary. The History tab plots and compares models over that log.

## The chart

- **Mouse wheel / trackpad:** zoom around the pointer.
- **Middle-button drag:** pan. **Right-button drag:** box zoom.
- **Double-click / Reset zoom:** fit all points.
- **Log price axis** by default so expensive outliers don't crush the low-price region.
- Hover an orb for name/price/score; click to pin and open details. Frontier points are labelled directly.

Comparison modes: **model capability** (model-only rows), **best available agent** (best row per model whatever the harness), and **same/common scaffold** (restrict to one harness, e.g. `mini-SWE-agent`).

## Running

```powershell
cargo test
cargo run --release
```

The app starts hidden in the tray; opening creates a normal taskbar/Alt-Tab entry, hiding removes it while polling and alerting continue. Runs on Windows and Linux.

Prebuilt binaries for tagged releases are on the [releases page](https://github.com/Siroro/paretowatch/releases) — built by CI from the tag, no local toolchain needed.

## Project layout

```
src/
  main.rs         app state, tabs (Pareto/History/Alerts/Settings), tray, alerts engine
  types.rs        shared data model: quotes, benchmarks, settings, filters
  bench/
    matching.rs   canonical model keys, creator canonicalization, fuzzy joins
    scoring.rs    composite posterior, AA calibration, consensus panel
    mod.rs        row filtering, scaffold collapse, per-source views
  fetch/
    mod.rs        background worker, HTTP helpers, source dispatch
    prices.rs     Surplus price matrix, model catalog, market overlay
    swe_rebench.rs  terminal_bench.rs  deepswe.rs
    livebench.rs    swebench.rs        revelo.rs
  history/        event-sourced history: store / track / ui
  artificial_analysis_snapshot.rs   bundled AA snapshot data
```

## Build stack

Rust 2024 · `eframe`/`egui` 0.36 · `egui_plot` · `tray-icon` · `notify-rust` · blocking `reqwest` (rustls) on a background worker thread. MIT licensed — see [LICENSE](LICENSE).
