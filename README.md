# ParetoWatch 0.4

A lightweight native Rust tray utility for watching AI model prices and plotting price/quality Pareto frontiers.

No Python. No Electron. No browser server. No benchmark API keys.

## Pricing — Surplus Intelligence

ParetoWatch uses Surplus in three layers:

1. `GET /v1/prices` for the provider comparison matrix.
2. `GET /v1/models` as a public catalog/pricing fallback.
3. `GET /api/markets` as the live marketplace overlay.

Current `/api/markets` token fields such as `best_input_per_1m` and `best_output_per_1m` are integer micro-dollars per 1M tokens. ParetoWatch converts them to USD per 1M before plotting and alerting. Media-only markets are excluded. The market endpoint currently advertises a 30-second cache, so the minimum poll interval is 30 seconds.

## Benchmark sources

Use the **Benchmark** dropdown on the Pareto tab to swap the Y-axis without changing pricing.

### Composite agentic — default

A multi-source agentic-coding view. It combines:

- **40% Terminal-Bench 3.0**
- **40% DeepSWE v1.1**
- **10% SWE-bench Verified**
- **10% LiveBench Agentic Coding**

Raw benchmark scales are not directly comparable, so each source is converted to a within-source percentile rank first. A model must have at least two contributing sources to appear in the composite. If a model is missing one or more sources, the available weights are renormalized rather than treating missing results as zero.

This is deliberately a consensus view, not a claim that one scalar benchmark is ground truth.

### LiveBench

Public files from the official `LiveBench/new-livebench` repository. ParetoWatch follows the newest published release and keeps the existing **Overall**, **Coding**, and **Agentic coding** selectors.

### Terminal-Bench 3.0

Uses the **live public Harbor Hub leaderboard-read endpoint** that powers the current FrontierBench Terminal-Bench 3.0 table (`terminal-bench/terminal-bench`, board `3-0-0`). No Harbor login or API key is required. If that live read is temporarily unavailable or changes shape, ParetoWatch falls back to the maintainers' point-in-time announcement snapshot instead of dropping the source entirely.

ParetoWatch keeps the strongest displayed row per underlying model and plots `metrics.accuracy` as a percentage. Terminal-Bench is harness-sensitive, so the selected model detail preserves the upstream agent/harness and reasoning-effort label where available.

### DeepSWE v1.1

Uses the official public machine-readable artifact:

`https://deepswe.datacurve.ai/artifacts/v1.1/leaderboard-live.json`

ParetoWatch restricts rows to the benchmark's common `mini-swe-agent` harness and prefers the strongest published reasoning-effort tier (`max`, then `xhigh`, `high`, etc.) for each model. The Y-axis is pass@1 × 100.

### SWE-bench Verified

Uses the official SWE-bench website's `data/leaderboards.json` and reads the Verified leaderboard. SWE-bench results include the agent/harness as part of the published submission, so this source should be interpreted as model+agent evidence rather than a pure model-isolation score.

All four benchmark feeds are public/no-key and refresh independently every six hours. A failure in one source does not remove the others; see **Settings → Benchmarks** for per-source status/errors.

## Run on Windows

```powershell
cargo test
cargo run --release
```

The app starts hidden in the tray. Opening it creates a normal Windows taskbar and Alt-Tab entry; hiding it removes the window/taskbar entry while the tray process keeps polling and evaluating alerts.

## Pareto chart controls

- **Mouse wheel / trackpad:** zoom around the pointer.
- **Middle-button drag:** pan.
- **Right-button drag:** box zoom.
- **Double-click** or **Reset zoom:** fit all points.
- **Log price axis:** enabled by default so expensive outliers do not crush the low-price region.
- **Hover an orb:** model name, price and selected benchmark score.
- **Click an orb:** pin the model label and show details.
- Pareto-frontier points are labelled directly; the frontier line does not steal hover events.

## Alerts

ParetoWatch supports two alert types:

- **Threshold:** input, output, or blended price crosses above/below a target. These are edge-triggered and re-arm after the price crosses back.
- **Any price change:** notify on every observed price move for a model. The notification shows the direction plus old → new **blended, input, and output** prices. Source transitions between the live market and a fallback feed are ignored so an upstream outage/recovery does not look like a price move.

The Alerts tab also keeps an in-memory recent-moves feed. Falling blended cost is shown with a green down arrow; rising cost is shown with a red up arrow.

## Agentic-coding blended price

The old v0.3 blend used an unnormalized `1 input : 3 output` formula. That was both output-heavy for agentic coding and misleadingly labelled as `$ / 1M` even though the weights were being summed as a workload bill.

v0.4 uses a **normalized weighted average**, so blended price remains a true `$ / 1M-token` comparison metric. It also accounts for Surplus prompt-cache-read pricing where available.

Default **Agentic coding** mix:

- 15% fresh input
- 80% cache-read input
- 5% output

This is a practical input-heavy preset, not a universal constant. You can change the three weights or use the included `95/0/5` no-cache and `50/0/50` balanced presets. If a provider does not publish cache-read pricing, ParetoWatch conservatively uses its normal input price for that portion of the blend.

When `/api/markets` exposes provider-level token legs, ParetoWatch chooses a *single provider* using this workload mix instead of combining an input minimum from one provider with an output minimum from another.

## Diagnostics

If a Surplus source changes shape, ParetoWatch shows the endpoint/parser error instead of silently returning zero models. If `/api/markets` fails, comparison/catalog prices continue to drive the dashboard.

Benchmark fetch problems are isolated by source. For example, a temporary SWE-bench GitHub download failure does not stop LiveBench, DeepSWE or Terminal-Bench from rendering.

## Build stack

- Rust 2024
- `eframe` / `egui` 0.36
- `egui_plot`
- `tray-icon`
- `notify-rust`
- blocking `reqwest` on a background worker thread

No Python runtime is used by the application.
