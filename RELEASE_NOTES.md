# ParetoWatch v0.29.0

## Features

- **A "Catalog list" pricing mode joins the chart — every model billed at its published list rates.** The Filters popover's pricing row is now a three-way choice, with the old "Hide catalog-only prices" checkbox folded in as its "Live market" option:
  - **All sources** (default) — unchanged: live-market asks, with catalogue list prices filling in for models that have no live ask.
  - **Live market** — the old toggle: models without a live-market ask leave the chart so list-price orbs cannot hold a frontier spot against live asks.
  - **Catalog list** (new) — every model on the chart is priced at its published list rates, the official prices a provider or the catalogue publishes, ignoring marketplace discounts entirely. The list legs are preserved through the live-market overlay (the feed's own direct list prices when it carries them — fresher than the catalogue — else the catalogue's rates), and the cache leg prefers the catalogue's list cache price. The result is a stable apples-to-apples frontier: models whose market asks fluctuate seller-to-seller are compared at the prices they officially cost, and catalog-only models compete on equal footing with live ones.
  - Every view discloses the mode: the status line gains a "catalog list pricing" filter entry, the detail card and frontier table switch their source label to "catalog list price"/"Catalog list", chart tooltips badge the basis, and the displayed per-leg prices are always exactly what the plotted cost bills. The historical "lowest price" line is suppressed in this mode — the history log records observed market prices, which would read as inflated "% above low" against a list price.
- **The choice persists.** `ui_state.json` carries the pricing source across restarts; files written by older versions migrate their hide toggle into the equivalent "Live market" setting.

## Notes

- Alert evaluation is untouched: rules keep billing live asks with the published fallbacks regardless of the chart's pricing source, so switching the chart's view can never re-arm or re-float an alert.
- The workload-mix cache-read fallback (10% of the input price by default) also applies in catalog mode when neither the catalogue nor the market publishes a cache rate — the same published-first rule as everywhere else.

## Validation

Formatting, locked check, locked Clippy with warnings denied (`--all-targets`), and locked test (193 tests) all pass.

# ParetoWatch v0.28.0

## Features

- **Speed numbers you can trust: AA's standardized benchmarks are now the primary speed figures.** The Surplus market feed's tok/s and TTFT turned out to be coarse histogram-bucket midpoints — all 136 measured markets share just 21 distinct throughput values (19 different models "measure" exactly 73.3 tok/s), and 178 markets collapse into 15 distinct TTFT values — so small differences between models are literally invisible in that data. Artificial Analysis measures the same constructs properly, and their leaderboard payload — the exact page the app already downloads every six hours for the Intelligence Index — carries per-model medians with full precision: median tokens/second with p05/p25/p75/p95, median time-to-first-token with percentiles, and end-to-end response time, all under their uniform controlled protocol (161 records folding to 99 model families on the current board, every value distinct). Zero new HTTP requests:
  - **Models tab**: the tok/s and TTFT columns now show AA's standardized medians wherever the model matches, with the distribution (p05–p95) and the effort variant in the hover. Models AA doesn't track fall back to the live-market bucket, marked with † and a hover saying plainly that it's a coarse pooled midpoint. Sorting and the Speed filter test the resolved figure — AA first, market bucket second — so filtering at ≥100 tok/s now means "AA measured ≥100".
  - **Pareto detail card**: the speed line shows both sources — AA's standardized read first, the market buckets after it under a † marker ("market buckets · 24h"), so the standardized and marketplace-reality numbers sit side by side instead of being confused for each other.
  - **Cost calculator**: an "AA speed" line (median tok/s · TTFT · end-to-end, p05–p95 band in the hover) above the per-provider route grid, so the standardized reference and the Surplus-route reality are one glance apart.
  - The table rides the benchmark cadence (one leaderboard download yields both the Intelligence Index and the speed reads), persists in its own disk cache (`aa-speed.json`) so restarts start warm, and the footer discloses coverage ("AA speed benchmarks: N models").
- **Variant folding for speed deliberately differs from the Intelligence Index.** AA lists many models per effort variant, and throughput barely moves between variants — but time-to-first-token at max effort includes queue/fallback time that scales with effort (Claude Opus 5.5 measures 9 s TTFT at low effort and 706 s at max on the same board). Speed therefore folds to the calmest variant — bare entry first, then low→max efforts — while intelligence keeps preferring max effort; the chosen variant is disclosed in every hover.

## Notes

- The quote→family join reuses the benchmark matcher (exact canonical key, then the 0.80 fuzzy bar with creator compatibility and the release-tag fallback), so a model matches the same AA family its benchmark scores do.
- AA's speed is measured under their harness (standardized prompts, controlled generations), not on Surplus routes — it answers "how fast is this model", while the market buckets and the per-provider route grid answer "what does traffic on this marketplace actually see". The three are now visually distinguished everywhere they appear.

## Validation

Formatting, locked check, locked Clippy with warnings denied (`--all-targets`), and locked test (190 tests) all pass. The parser was additionally smoke-tested against the live leaderboard payload (99 families, all values distinct, effort folding verified).

# ParetoWatch v0.27.0

## Features

- **Speed analytics are in.** The market feed turns out to carry route telemetry for every live market (market-wide median tokens/second, median time-to-first-token, uptime, and the streaming sample counts behind them, all over a rolling 24h), so the existing 30-second poll now picks them up for free — no extra requests:
  - **Models tab**: two new sortable columns — **tok/s** and **TTFT** — beside the request/volume telemetry, with hovers disclosing the sample counts and uptime. tok/s sorts fastest-first by default (higher is better there) and unmeasured rows sink in *both* directions; TTFT sorts snappiest-first like the price columns.
  - **Filter by speed**: a Speed row joins the Filters popover — Any / Measured / 25+ / 50+ / 100+ / 150+ tok/s, cutting the slow tail or finding exactly the models Surplus has speed data for (~136 of 458 markets currently measure throughput). Persisted with the rest of the session view.
- **Per-provider breakdowns, fetched on demand.** Surplus's Provider Performance API (`/api/analytics/performance/{model_id}`, no bulk endpoint, no auth) splits that market-wide median by provider route: tok/s, TTFT p50, p95 total latency, uptime, requests, per provider. Rather than sweeping hundreds of per-model URLs, ParetoWatch fetches a model's breakdown when you actually look at it — selecting it on the Pareto chart, loading it in the cost calculator, or expanding its New-listings card — and shows a spinner while the pull lands. Rows carry their own fetch time:
  - A cached pull older than 15 minutes refetches on selection; everything cached is re-pulled by a background sweep every 6 hours (the benchmark cadence); a failed pull backs off 10 minutes instead of hammering a dead endpoint.
  - The rows persist in a new disk cache (`performance.json` beside the benchmark cache), so a restart starts from last session's analytics — the startup status line says so alongside the benchmark-cache note, and the footer carries a running "Route analytics: N models · fetched on demand · newest X ago".
- **Where the per-provider numbers show up:**
  - **Cost calculator**: a "Route performance · 24h" grid under the provider picker — every observed route, busiest first, with the billed provider's row bolded and dotted, so "cheapest ask" and "fastest route" can be weighed in one place.
  - **Pareto detail card** (and the shared Market-quality grid, so the New-listings cards get it too): a market-wide "Speed: N tok/s · TTFT Xs · N% uptime" summary line, plus tok/s · TTFT p50 · uptime columns appended to the per-provider ask grid whenever the model's breakdown is cached.
  - **Models tab provider hover**: each provider in the hover now carries its route's tok/s and TTFT when the breakdown is cached.

## Notes

- The market-wide speed numbers are medians of observed traffic, not advertised limits — a market with no streaming telemetry in 24h (or a catalog-only row) honestly shows "—". Surplus publishes no mean tok/s or mean TTFT; the medians and percentiles here are the same statistics their model pages and API expose.
- Provider names in the performance payload share the namespace with the market feed's provider rows, so the join is a direct name match; rows join under the same normalization rule as the market overlay.

## Validation

Formatting, locked check, locked Clippy with warnings denied (`--all-targets`), and locked test (185 tests) all pass.

# ParetoWatch v0.26.1

## Fixes

- Dropped the colour-dot preview that sat in front of the "Providers ▾" button. Unlabelled dots could not actually map a colour to a provider (the popover does that), so the strip is now just the button — quieter, nothing functional lost.

## Validation

Formatting, locked check, locked Clippy with warnings denied (`--all-targets`), and locked test (175 tests) all pass.

# ParetoWatch v0.26.0

## Features

- **The main screen got a full go-over: three dense control rows collapsed into one compact toolbar.** The Pareto tab's toolbar had accreted a row of axis selectors plus four inline workload drag fields, a second row of comparison/market-quality combos, and a third row of modality/safety/search/zoom controls — six-plus wrapped lines of chrome above the chart. It is now:
  - **One toolbar row** holding only the things that reshape the chart on every use: the Price axis selector, the Benchmark selector (LiveBench's Quality metric appears beside it when relevant), the search box with its inline clear button, two popover buttons, and Reset zoom.
  - **Price metric and cost basis merged into one selector** — "Blended · $/1M", "Blended · est. $/task", "Input · $/1M", "Output · $/1M" — since the per-task basis only exists for the blended metric, the old side-by-side combo pair mostly offered impossible combinations. Each raw board's fixed metric (Result@1, pass@1, Elo…) moved from an inline label to a hover on the Benchmark selector.
  - **A "Filters · N ▾" popover** (with a live count of active filters) carrying the comparison basis, market-quality bar, modality, safety, the common-scaffold picker, and the log-axis / hide-catalog-only toggles. Options render as segmented button rows rather than nested combo boxes, so picking several filters never dismisses the panel; it closes only on an outside click or Escape.
  - **A "Workload mix ▾" popover** (replacing the four inline weight fields) with the fresh-input / cache-read / output / assumed-cache-% drag values, the Settings tab's Agentic / No-cache / Balanced presets, a live "Current mix: N% / N% / N%" readout, and Apply (with the ✓ confirmation and an ● on the toolbar button while edits are unsaved).
  - **The status line under the chart now lists every non-default filter** ("Filters: Best available agent · trusted · catalog-only hidden"), so a thin chart always says why it is thin.
- **The creator legend shrank from up to three wrapped chip rows to one strip.** A colour-dot preview of the visible providers plus a "Providers ▾" button opens the popover with the full clickable chip grid (click to hide/show a provider's models) and new Show all / Hide all shortcuts — Hide all enabling focus-one-provider workflows. The strip button reads "Providers · 2/24 hidden ▾" while any are hidden. (The legend was previously labelled "Groups"; it is now "Providers" everywhere, including the hidden-count status line and the all-hidden empty-state message.)
- **Models tab matches**: the five secondary selectors (Creator, Source, Modality, Safety, Cache pricing) fold into the same "Filters · N ▾" popover beside the search box, with the creator picker as a scrollable single-choice list with per-provider counts.
- Smaller polish: the selected-model detail card gained a ✗ clear-selection button in its header; the chart-interaction hint line is now quiet small grey text instead of a full-size row; the top-bar status text no longer shouts at body size; and the plot claims a slightly larger share of the window now that the chrome above it is one row (0.55→0.58 of the viewport height).

## Notes

- The popover panels are a deliberate middle ground between menus and dialogs (egui `Popup` with close-on-outside-click): drag values, checkboxes, search fields, and chip grids stay interactive in place instead of dismissing on first click the way menu items do.
- Nothing about the persisted UI state changed: `hidden_groups` keeps its field name and on-disk key, so restored sessions (hidden providers, filters, selection) carry over untouched.

## Validation

Formatting, locked check, locked Clippy with warnings denied (`--all-targets`), and locked test (175 tests) all pass.

# ParetoWatch v0.25.0

## Features

- **New listings gets its own pane in Activity.** A third sub-tab — "New listings (N)" — collects every model the persistent history log first saw inside a selectable window (24h / 7 days / 30 days / all time, persisted with the rest of the session view). "New" survives restarts by construction: the same event log that feeds "new model listed" alerts records each model's first-ever sighting, so a relaunch never re-flags the catalogue and the button's count is the true number of first sightings in the window. When the history log itself is younger than the window, the pane says so ("History log begins … — models listed before that were never recorded as new") instead of implying the whole catalogue just appeared.
- Each listing is a card with the full first-impression dossier:
  - **Pricing as listed** — workload-blended $/1M beside the raw input / cache / output legs, live-market vs catalog, current discount, free-offer badge, 24h request and volume telemetry, vision/uncensored flags, and the first-seen timestamp with a live age band (NEW badge under 48h).
  - **Benchmark coverage** — "benchmarked on N of 13 boards" per card, counted with the exact same matching rules as the consensus grid so the badge and the grid can never disagree; unbenchmarked models say so explicitly instead of silently missing from the Pareto chart.
  - **Expandable detail ("▸ Benchmarks & history")** — the blended price step-chart since listing with the all-time low and the recorded price-change count; daily volume and requests-per-day mini charts once two telemetry days exist; the full source-by-source consensus grid (percentile, rank, score, matched row across all 13 leaderboards); and every provider ask in the model's market with workload pricing (the Pareto detail card's market-quality section, now shared).
  - **One-click cross-links per listing** — show it on the Pareto chart, open it in the History tab, pin a floating price widget, or arm a price-threshold / any-change / all-time-low alert straight from the card.
- Performance: listing rows rebuild only when data actually changes (price/board version or comparison controls); board-coverage counts are computed lazily for the rows on screen and memoized per data version; and the all-time view caps at the newest 300 cards (disclosed on screen when trimming).

## Fixes

- `cached_consensus` now takes the quote directly, so its single-slot cache is shared cleanly by the Pareto detail card and the listing cards; the market-quality provider grid was extracted from the Pareto detail card for reuse.

## Validation

Formatting, locked check, locked Clippy with warnings denied (`--all-targets`), and locked test (175 tests) all pass.

# ParetoWatch v0.24.0

## Features

- **New benchmark board: SRE Bench (Vals).** vals.ai's agentic software-reverse-engineering benchmark joins the consensus grid, the composite, the Pareto board list, and the alert source picker: models must figure out what a real-world, contamination-free binary does without its source code, so the board is a security-adjacent sibling of the agentic coding boards. Vals runs every model under its own standardized protocol (uniform harness, like SWE-rebench), so it is fully weighted in both composite flavors; its mid-tier 0.10 weight reflects that reverse engineering is adjacent to, not the same construct as, agentic coding. The fetcher reads Vals' hashed `benchmark_view_sre_bench.*.json` data file, re-extracting its URL from the page HTML on every fetch because the hash changes with each update; rows carry the reasoning effort (and agent harness, when published) per model.

## Validation

Formatting, locked check, locked test (171 tests + 1 live vals.ai fetch test), and Clippy with warnings denied all pass. The live fetch returns all 35 leaderboard rows.

# ParetoWatch v0.23.0

## Features

- **The whole working view now survives restarts.** A new `ui_state.json` (beside `config.json`) persists every durable UI preference and restores it on launch: the active tab and Activity pane, the Pareto chart's price metric, cost basis, benchmark source and metric, comparison mode and scaffold, liquidity/modality/uncensored filters, the hide-catalog-only toggle, log-price axis, legend chips toggled off, the selected model, the Models tab's creator/source/modality/uncensored/cache filters and sort column+direction, the cost calculator's model and provider — and **the pinned price widgets**, which re-spawn automatically once the first price snapshot arrives (models that left the catalogue are skipped silently). Writes happen only when something actually changed, so a force-killed session loses at most its final moments.
- Deliberately NOT persisted: search boxes, the half-composed alert form, and per-frame caches — a stale search filter would hide models on reopen with no visible reason. `config.json` keeps its role as the deliberate-settings file (poll interval, weights, alerts, quiet hours); `ui_state.json` is the session view. Missing, corrupt, or version-drifted state files fall back to defaults.

## Validation

Formatting, locked check, locked test (169 tests), and Clippy with warnings denied all pass.

# ParetoWatch v0.22.0

## Features

- **Benchmark boards now survive restarts.** Every fetched board is persisted to a disk cache (atomic temp-file write, `%LOCALAPPDATA%/ParetoWatch/cache/benchmarks.json` on Windows) each time a source updates, and startup seeds the previous session's boards on top of the bundled AA snapshot — a restart used to lose every live board until the next six-hour refresh, and now the chart, composites, and alerts start from real data immediately (status line discloses "Loaded cached benchmark boards (saved …) — refreshing…"). The cache is best-effort by design: corrupt, missing, or version-drifted files are ignored and fresh fetches take over; the AA source's own bundled snapshot remains the always-available floor.
- **Hide any model from the Pareto frontier.** Select a model and use the new "⊖ Hide from frontier" button in its detail card (next to the 🔔 alarm menu): the orb stays on the chart, faded, but the frontier line reroutes around it. Frontier entry/exit alerts ignore hidden models too — they cannot enter or leave a line they are not part of — while cheapest-above-score alerts deliberately keep considering them (price shopping is independent of the line). The toolbar shows "N models hidden from the frontier (faded)" with a "Show all on frontier" restore button, and per-model restore is the same detail-card button ("⊕ Show on frontier"). The hidden set persists in settings like every other preference, and toggling it rebuilds the chart, composite, and alert-status caches immediately.

## Fixes

- `semantic_alert_status` now takes the settings bundle instead of four loose weight arguments (arity lint).

## Validation

Formatting, locked check, locked test (168 tests), and Clippy with warnings denied all pass.

# ParetoWatch v0.21.0

## Features

- **The main AA Intelligence Index is now automated — no API key.** The leaderboard page (`artificialanalysis.ai/leaderboards/models`) embeds every CURRENT model with its `intelligenceIndex` (plus an `intelligenceIndexIsEstimated` flag) in its server-rendered RSC payload, so one plain HTML request refreshes the whole frontier every six hours. The bundled snapshot becomes the offline BASE of a hybrid:
  - live non-estimated values replace the bundled score for the family — exact canonical key first, then the 0.80 fuzzy bar (the leaderboard drops parameter suffixes AA's own snapshot carries, e.g. "Nemotron 3.5 Lightning" vs "... 30B") — and the row name re-tags from "snapshot 2026-10-23" to "live <today>";
  - variant policy mirrors the snapshot's: prefer the best reasoning effort the page lists (max > xhigh > high > medium > low), fall back to the bare entry, and only then to an explicit non-reasoning run;
  - same-weights SKU aliases (`GPT-6.1 Sol Pro`, the Contributor SKUs, ...) follow their base variant's live value, keeping their inheritance disclosure;
  - current models the payload rates that the snapshot never curated get fresh "live" rows — Muse Wonder Full (AA 55.8) enters the composite at #2 on real measurements, with no Surplus listing to plot until one exists;
  - everything the payload cannot see — legacy families (GPT-5.2, Opus 4.x, Grok 4.6, GLM-5.2 ...), ~proxy rows, AA "~est" values — keeps the bundled read, and a fetch failure degrades to exactly today's behavior (the plain bundled snapshot). This source can no longer fail.
- The source joined the periodic benchmark refresh (`remote_sources`), so the app's startup seed of bundled rows is replaced by overlaid rows within the first refresh cycle. Labels updated: "Artificial Analysis (live + snapshot)", metric hint "Intelligence Index (live + snapshot fallback)". Manual snapshot bumps remain for what automation cannot do: legacy-row re-reads, new-family curation (proxy/alias/unrated policy), and the calibration anchors — the snapshot header documents the new division of labor.

## Validation

Formatting, locked check, locked test (166 tests), Clippy with warnings denied, and the live-data smoke test all pass. The smoke run shows the overlay live: AA rows carry fresh decimal reads (Opus 5.5 57.62 vs the bundled 58 integer), Muse Wonder Full joined, and the composite leader (Claude Opus 5.5, 99.5) is unchanged.

# ParetoWatch v0.20.0

## Features

- **Three new live benchmark boards** join the composite, all public pages needing no API key (fetched on the same six-hour cycle as the rest, in parallel with the price loop). The composite now fuses eleven boards plus the AA prior.
- **AA-Briefcase** (`artificialanalysis.ai/evaluations/aa-briefcase`, weight 0.10) — Artificial Analysis's knowledge-work benchmark: Elo ratings from blind pairwise comparisons on long, multi-week professional tasks with a **private task set**, so the number is contamination-resistant by construction. The page renders a plain server-side table (currently ~114 models; Claude Sonnet 5.5 leads at Elo 1816), and effort variants of the same model collapse to the best one. This is one of the flagship components of the Intelligence Index the app already trusts as its bundled prior — and it makes the snapshot header's old claim that AA-Briefcase results "live on boards the app fetches live" finally true.
- **AutomationBench-AA** (`artificialanalysis.ai/evaluations/automationbench-aa`, weight 0.05) — AA's long-horizon office-automation board; the headline is the *partial score* (how much of each task's objective tree completed with no violations). The page embeds its data as escaped JSON in the RSC payload; scores are attributed to the nearest preceding model `shortName`, and the parse is anchored to reality: Gemini 4 Argon reads exactly the 77.5 AA's changelog headlines for it. Both AA boards run every model under AA's own standardized protocol, so they count as model-level (full capability weight) — but they share a publisher with the AA prior, hence the deliberately modest weights (0.10/0.05, LiveBench/Code-Index tier).
- **SWE-Bench Pro V2 HARD** (`labs.scale.com/leaderboard/swe_bench_pro_public_v2`, weight 0.15) — Scale's long-horizon SWE benchmark: 1,865 tasks across 41 professional repositories, third-party run under a locked protocol, copyleft-sourced public split (contamination-resistant by design), and unsaturated where SWE-bench Verified topped out (~97% and retired by community consensus). No hidden XHR needed: the leaderboard's default HARD tab is server-rendered with `data-model-name` markers, so the fetcher is a plain HTML read. Rows score model+harness pairings (`Opus 5 (Claude Code) xhigh` → 98.0, `Gemini 4 Argon (mini-swe-agent) high` → 92.2), so the board joins the harness-specific family like FrontierCode — full deployment weight, demoted for capability — with the harness carried in the row's `agent` field.
- Fetcher-local display aliases: the AA boards and Scale name two models differently from Surplus — `Inkling` → `Thinking Machines: Inkling` (the creator prefix is not one benchmark matching strips) and Scale's `Muse Wonder 1.0` → `Muse Wonder`. The worker's benchmark refresh loop, the Models/consensus grid, the Pareto source selector, alerts, and the settings source list all pick the new boards up automatically via `remote_sources`/`display_sources`.
- The live smoke test now fetches all eleven sources; the composite leader is unchanged (Claude Opus 5.5 at 99.4 — the Briefcase Elo leader, Sonnet 5.5, stays #2 on the broader consensus), while Muse Wonder climbs to #4 on strong new-board evidence.

## Validation

Formatting, locked check, locked test (162 tests), Clippy with warnings denied, and the live-data smoke test (AA snapshot 170, AA-Briefcase 114, AutomationBench-AA 30, SWE-Bench Pro 16, LiveBench 49, FrontierCode 46, Terminal-Bench 4 35 rows) all pass.

# ParetoWatch v0.19.2

## Updates

- **AA Intelligence Index snapshot fully re-read** from v4.3.2 (read 2026-10-23, second same-day bump) to v4.3.2: the first full-table re-read since the September v4.3 rebaseline, after Artificial Analysis shipped a leaderboard update. Every one of the snapshot's rows was fetched from its live AA model page and compared against the stored value. The index revision, scale, and leader are unchanged — Claude Opus 5.5 still leads at 58 — so `AA_CALIBRATION_POINTS` keeps its anchors. The leaderboard itself now defaults to a Status filter showing only Current models (133 ranked); older models are hidden from the table but their pages still live, and the sweep verified them against the pages directly — GPT-6 Sol re-read unchanged at 48 (AA marks it "no publicly available API"; its new unevaluated "Daybreak Blue" variant has no Surplus listing and gets no row).
- **Thirty-four families moved to their live displays** after drifting past rounding since their reads — mostly a single point: GPT-6 Luna 37 → 38 (its Pro SKU inherits), MiniMax-M3 29.6 → 29, GPT-5.5 Instant 26.8 → 26, Qwen 3.7 Plus and Hy3 25.8 → 25, GPT-5.4 Mini 24.6 → 24, Kimi K2.5 and GLM 5V Turbo 23.5 → 23, Gemini 3.5 Flash Lite 22.7 → 22, Grok 4 22.5 → 22, Qwen3.6 27B 21.9 → 21, Qwen3.5 397B A17B 19.1 → 18, Qwen3.6 35B A3B 18.8 → 18, Muse Glimmer 30B 18.1 → 17, Gemini 2.5 Pro 16.7 → 16, Nemotron 3.5 Lightning 30B and Nemotron 3 Super 120B 13.6 → 13, Qwen 3.7 Max 29.9 → 29, GPT-5.5 38.6 → 38, DeepSeek V4.1 Flash 39.5 → 39, Qwen3.5 Flash 12.5 → 12 (the Omni Flash proxy source), Qwen 3.5 9B 13.7 → 11, Qwen3 Coder Next 10.1 → 9, Mistral Small 4 and Mercury 2 11.5 → 11, Llama 4 Scout 6.5 → 6, and Ministral 3 8B 5.5 → 5.
- **Four families moved for structural reasons.** AA folded the DeepSeek V4 Flash and V4 Pro canonical pages onto their dated refresh builds (the same fold DeepSeek R1 0528 got): the undated rows now read the 0731/0813 measurements (V4 Flash 24.6 → 34, V4 Pro 30.9 → 36) and the dated rows re-read 34/36 on the same page. Grok 4.1 Fast's reasoning evaluation is gone from AA — only the non-reasoning run remains — so the row falls back to 11 per the snapshot's no-reasoning-run policy. Qwen3 VL 235B A22B and Nemotron Nano 12B v2 likewise lost their reasoning pages and read the instruct/non-reasoning runs (10 and 6).
- **New snapshot rows**: **DeepSeek V4.1 Flash Uncensored** (45, `~proxy`: dealignai's uncensored finetune of the 1017 build, listed 2026-10-21 — the listing names its base checkpoint, so it proxies the 1017 row exactly the way Abliterated Large V2 proxies GLM-5.3, and the app's uncensored flag marks it 🔓 independently of the proxy), **MiMo-V2.6-Flash** (38, eight under the Pro sibling — AA finally published the Flash page the unrated list had been waiting for), and **Thinking Machines: Inkling** (25 at xhigh, the only effort AA measured; the Surplus listing has been live since July as an auto-proposed SKU with real asks. Inkling Small — 26 — is a separate SKU Surplus does not list, so it gets no row). The other 2026-10-21/22 Surplus listings are image-generation models the intelligence index does not rate.
- **Rows removed**: AA deleted the undated `qwen3-8-flash` page (404; only the distinct Aug-26 `qwen3-8-flash-next` preview remains rated), so the **Qwen3.8 Flash** row and the **Qwen3.8 Omni Flash** row that proxied it lost their rows rather than keeping stale reads — the GLM-4.5 precedent — and both undated Surplus SKUs fall back to the composite's neutral prior.
- **GLM-4.7-Thinking** joined `SAME_WEIGHTS_SKU_ALIASES`: AA folded its page into the base GLM-4.7 page, so the SKU now inherits the base's board evidence exactly like DeepSeek R1 0528. Four other pages were retitled onto dated/renamed builds with unchanged values (verified, not moved): Gemini 3 Flash Preview → the GA reasoning page (26), MiMo-V2.5 → its dated 0424 page, Grok Build 0.1 → 0616, and Grok 4.20 Beta → the 0309 v2 build.

## Validation

Formatting, locked check, locked test, Clippy with warnings denied, and the live-data smoke test all pass.

# ParetoWatch v0.19.1

## Fixes

- **The same model could show two different Composite scores when Surplus sells its weights as two SKUs.** Reported 2026-10-23 with Muse Spark 1.3: "Meta: Muse Spark 1.3 Contributor" showed Composite 85.4 while "Meta: Muse Spark 1.3" — literally the same checkpoint — showed 70.9. The composite joins boards by exact canonical key, and no leaderboard ever lists the Contributor variant, so its key carried only the AA prior (insertion-ranked 48 into the elite cohort → the 85.4 the v0.18.0 smoke run already printed); when the v0.19.0 LiveBench 2.0 migration added a `Muse Spark 1.3` row, the base key alone absorbed that board evidence and pulled down to 70.9. The fuzzy quote join the consensus grid uses never had this problem — only the composite's keyed join did. The GPT `pro` execution modes already solved this exact shape through `INHERITED_EXECUTION_MODES`; that table is now **`SAME_WEIGHTS_SKU_ALIASES`** (SKU, base variant, what the SKU is) and the composite's per-source alias pass copies the base variant's rows onto the SKU key in every board, so same-weights SKUs rate identically while native SKU rows still win when a board does list one.
- Audited the whole snapshot for other same-weights pairs with the same latent split and registered four more: **GLM-5.3-FlashX** (GLM-5.3-Flash served at up to 200 tok/s — previously only `~proxied` for AA with no board inheritance, since stripping `flashx` would collapse onto GLM-5.3, the wrong base), **DeepSeek R1 0528** (the refresh AA folded into the base DeepSeek R1 page, so both Surplus SKUs serve the same weights), **Mistral Small 3.2 24B Instruct** (the open-weights listing name of Mistral Small 3.2), and **Qwen3 Coder 480B Turbo** (the 480B checkpoint served faster, the same relationship the prime/ultraspeed wrappers have). Same-weights pairs that differ by *build* are deliberately not aliased — Qwen3.8 Max (0902) vs the deprecated undated 0803 row, and the dated DeepSeek refreshes, keep separate measurements. No AA scores moved; the row-name disclosure now reads "inherited from <base> — <what the SKU is>; same weights, not separately benchmarked".
- New regression test drives every alias pair through the composite with base-only board evidence and asserts identical scores for both the composite rows and the joined quotes.

## Updates

- AA Intelligence Index snapshot bumped from v4.3.2 (read 2026-10-20) to v4.3.2 (read 2026-10-23). The index revision, scale, and leader are unchanged — Claude Opus 5.5 still leads at 58 — so `AA_CALIBRATION_POINTS` keeps its anchors and only the release's rows moved.
- New snapshot rows for AA's 2026-10-22 release: **inclusionAI: Ling 3.1 Flash 1019** (44 — the 1019 dated refresh of the Flash family, three over the undated build's 41, which re-reads unchanged. The release tag stays in the key so the undated row keeps its own measurement, and the row lands ahead of its Surplus listing — the catalog still carries only the undated `ling-3.1-flash` SKU — the same way Gemini 4 Argon and Mistral Large 4 did) and **Meta: Muse Spark 1.2 Contributor** (39.8, the 1.2 weights; Surplus lists `muse-spark-1.2-contributor`, and the SKU — the sibling of the 1.3 Contributor bug above, surfaced by the same audit's catalog sweep — now carries the 1.2 measurement and inherits board evidence through `SAME_WEIGHTS_SKU_ALIASES` instead of joining the 1.2 rows through the 0.80 fuzzy bar).
- **NVIDIA Nemotron 3.5 Super VL** re-read 25 → 26: the page firmed past rounding since the 2026-10-17 entry read, matching AA's 2026-10-15 changelog number (still past the Nemotron 3 Ultra 550B's 23.4). Spot-checks of the other recently moved families found no further drift (GPT-6 Sol 48, Muse Spark 1.3 48, undated Ling 3.1 Flash 41), and AA's 2026-10-13..15 evaluations of models with no Surplus listing (Muse Wonder Full, Solar Pro 4, Bolt 74B-A9B) need no rows.

## Validation

Formatting, locked check, locked test (152 tests), and Clippy with warnings denied all pass.

# ParetoWatch v0.19.0

## Fixes

- **LiveBench had been dead since the `LiveBench/new-livebench` GitHub repository was deleted** in the project's October 2026 consolidation — the fetcher 404'd on `constants.js`, so the board silently contributed nothing. LiveBench now serves everything from livebench.ai itself, and the fetcher was migrated to the **LiveBench 2.0 boards**: it reads the SPA homepage for its app-bundle path, scans the bundle for the newest `YYYYMMDD` release label (the same read-a-JS-file approach the old `constants.js` flow used), and pulls `data/<label>/table.csv` + `categories.json` from the site. The 2.0 category set replaces v1's "Agentic Coding" with **"Long Agentic"** (long-horizon agentic work), which now feeds the agentic metric with an overall-mean fallback so a release without that column still maps; the legacy v1 question-set boards are gone from the site's navigation and are no longer fetched. The restored board already rates Beam 1.0, MiniMax M3.1 Flash, Muse Spark Open, and DeepSeek V4.1 Flash 1017 (49 rows in release 20261014).
- Cache-read pricing follow-up to v0.18.2: the market feed's provider rows never publish a per-provider cache ask (verified 2026-10: 0 of ~1.3k rows), so the quote's cache leg now falls back to the **market-wide best cache ask** — the rate marketplace receipts actually bill — before the catalogue list price or the assumed share is ever consulted. The overlay logic was extracted to `apply_market_overlay` unchanged in behavior apart from that fallback.

## Updates

- AA Intelligence Index snapshot bumped from v4.3.2 (read 2026-10-17) to v4.3.2 (read 2026-10-20). The index revision, scale, and leader are unchanged — Claude Opus 5.5 still leads at 58 — so only the new-model rows were read fresh and `AA_CALIBRATION_POINTS` keeps its anchors.
- New snapshot rows for the Surplus listings of 2026-10-18..10-20: **MiniMax M3.1 Flash** (43, the single max-default measurement — thirteen over MiniMax-M3 and MiniMax's strongest row on the index), **Qwen3.8 Max (0902)** (45.4, reading the canonical page AA re-pointed at the 0902 snapshot; the old undated 0803 build is deprecated at 40.15 and Surplus delisted the undated SKU, but the undated row stays so undated leaderboard slugs keep joining), **inclusionAI: Ling 3.1 Flash** (41; `inclusionAI` is not a stripped creator prefix, so the row carries the display prefix to exact-join, same as Cohere/Inception), **Reflection: Beam 1.0 501B A23B** (30, the open-weights 501B A23B reasoning model), **Muse Spark Open** (39, read at xhigh — the only effort AA measured for Meta's open-weights coding SKU), and **GLM 5.3 FlashX** (41.9, `~proxy`: GLM-5.3-Flash served at up to 200 tok/s — the listing names the base checkpoint, but `flashx` cannot be a stripped deployment token because that would collapse the key onto GLM-5.3 (44.9), the wrong base).
- **Qwen3.8 Max Prime** (higher-throughput same-checkpoint SKU) inherits the base row through the stripped `prime` token exactly like GLM 5.3 Prime — no row of its own. **DeepSeek V4.1 Flash 1017** was re-read 40 → 45: AA firmed its max-effort measurement three days after entry, the same past-rounding drift Muse Spark 1.3 needed. The remaining 2026-10-18 listings are unrated and documented in the snapshot header: Qwen3.7 Flash (AA rates the 3.7 Plus/Max tiers but has no Flash page), LiquidAI's LFM2.5-24B-A2B (AA's LFM2.5 pages cover only the 1.2B–8B small models), Moondream 3 Preview / 3.1 (vision-language; no AA pages), Pareto 26.9, and Elorian's Glimpse.

## Validation

Formatting, locked check, locked test (150 tests), Clippy with warnings denied, and the live-data smoke test all pass. The smoke run pins the composite leader — Claude Opus 5.5 — with all eight sources fetched (AA snapshot 167 rows, LiveBench 49 via the new 2.0 endpoint, FrontierCode 46, Terminal-Bench 4 35) and the new rows joining the live boards.

# ParetoWatch v0.18.2

## Fixes

- **Cache reads were billed at the full input price whenever no cache ask was published.** Roughly half the catalogue publishes no cache rate (200 of 451 `/v1/models` rows; 199 of 413 market rows carry no cache ask from any provider), and for every one of those models the blended price — chart, Models tab, pinned window, history charts, workload discounts, and every alert evaluation — priced the cache leg at 100% of fresh input. Billing evidence says that is far off: inverting a month of AA usage-dashboard "direct cost" rows against the feed's list prices puts real cache reads at 5–27% of input (Grok 4.6 inverts to 24.8% against its published 25%; GPT-6.1 Sol to 4.5% against 5%; Abliterated Large V2 to $0.2977/M against its published $0.30 — exactly 10% of its $3 input). Cache-read pricing now resolves **published numbers first**: the selected provider's own ask, then the market-wide best cache ask, then the model's catalogue list price — `catalog_cache_read` is newly preserved through the market overlay instead of being overwritten by an empty provider ask — and only when nothing at all is published does it assume a share of the input price.
- New setting: **"Assumed cache-read price (unpublished)"** — the share of the fresh-input price billed for cache reads when a model publishes no cache rate anywhere. It sits in Settings beside the workload weights and inline in the Pareto chart's weights row, defaults to 10% (the classic Anthropic/DeepSeek/GLM convention and the center of the observed range), and accepts 0–100 — 100 reproduces the old bill-at-input behavior. Existing configs load with the 10% default. The Models-tab calculator now labels its cache rate by source — provider ask, market-wide best, published catalogue rate, or assumed share — and shows the assumed tick inline, and the blended column's tooltip documents the whole fallback chain.

## Validation

Formatting, locked check, locked test (143 tests), and Clippy with warnings denied all pass.

# ParetoWatch v0.18.1

## Updates

- AA Intelligence Index snapshot bumped from v4.3.2 (read 2026-10-15) to v4.3.2 (read 2026-10-17). The index revision, scale, and leader are unchanged — Claude Opus 5.5 still leads at 58 — so only the new-model rows were read fresh and `AA_CALIBRATION_POINTS` keeps its anchors.
- New snapshot rows for the Surplus listings of 2026-10-15..10-17: **DeepSeek V4.1 Flash 1017** (40 — the 1017 dated refresh of the V4.1 Flash family, five weeks after the 0910 build the undated row reads; a point over the family's 39.5 v4.3 read and just under Gemini 3.8 Flash, keeping the family the value pick at its $0.15/$0.60 launch pricing. Like the V4 Flash 0731 build, the release tag stays in the key, so the undated family row keeps its own measurement) and **NVIDIA Nemotron 3.5 Super VL** (25 — the 3.5 generation's Super tier with native vision input, listed the evening of 2026-10-15 hours after that day's read; past the Nemotron 3 Ultra 550B's 23.4 as NVIDIA's strongest row on the index. `vl` names the model rather than a deployment wrapper so it survives key normalization, and the `nvidia` creator prefix strips so the "NVIDIA: " Surplus display name joins exactly).
- **Mistral Large 4** priced its matrix at $0.0000 across the board — a 100%-off launch window for AA's "Le Chonk" — so the existing Preview-read row (38, added in v0.18.0 ahead of purchasable asks) finally carries live asks at literal zero. No code changed for it: the market parser already treats $0 token asks as real free offers (`free_offer_listed`) rather than dropping them, so the model simply anchors the cheap end of the frontier.

## Validation

Formatting, locked check, locked test (140 tests), Clippy with warnings denied, and the live-data smoke test all pass. The smoke run pins the composite leader — Claude Opus 5.5 at 99.3, unchanged — with all eight sources fetched (AA snapshot 161 rows, LiveBench 71, FrontierCode 46, Terminal-Bench 4 35) and both new rows joining below the printed top ten.

# ParetoWatch v0.18.0

## Updates

- AA Intelligence Index snapshot bumped from v4.3.2 (read 2026-09-30) to v4.3.2 (read 2026-10-15). The index revision, scale, and leader are unchanged — Claude Opus 5.5 still leads at 58 — so only the new-model rows were read fresh and `AA_CALIBRATION_POINTS` keeps its anchors.
- New snapshot rows for the Surplus listings of 2026-10-07..10-15: **Claude Haiku 5.5** (43, read at max effort — the same page configuration its Sonnet/Opus 5.5 siblings use; 26 points over the 4.5 sibling, between GPT-5.6 Sol and GPT-5.6 Terra, and above GPT-6 Luna), **Muse Wonder** (52 at max, #9 of 216 — Meta's API-available half of the Wonder launch, between Gemini 4 Argon and GPT-6.1 Sol; the stronger Muse Wonder Full, 56 tying Sonnet 5.5, has no public API and no Surplus listing, so it gets no row), **Mistral Large 4** (38, read off the Preview release dated 2026-10-06 — the only version AA rates — twenty-eight over Mistral Large 3 and the strongest non-US/China model on the index; the Surplus price matrix is still unpriced, so the row lands ahead of purchasable asks), and **Abliterated Large V2** (44.9, `~proxy`: Abliteration AI's GLM-5.3 with the refusal directions ablated out of the weights plus a post-ablation finetune — the listing names its base checkpoint, so unlike derivative builds such as Ember-1 it proxies the GLM-5.3 row instead of sitting unrated; AA rates no page).
- The half-price **Claude Haiku 5.5 (batch)** SKU inherits the base row exactly: benchmark matching now strips the `batch` deployment token alongside the speed/prime wrappers, because batch serves the same weights over the Batch API — an execution mode, not a distinct model. **Muse Spark 1.3** was re-measured at 48 at max for the Wonder launch (the stored 45.2 v4.3 read had drifted well past rounding), so its row and the Contributor `~proxy` moved with it. Gemini 4 Argon's general Surplus listing landed 2026-10-14 behind its existing row, and the new "Meta: Muse Spark 1.3" SKU joins the Spark row through the stripped `meta` creator prefix.
- New: **uncensored-model designation and filtering**. Surplus publishes no policy metadata, so listings are flagged when their id/name/creator text identifies an uncensored deployment — abliterated/abliteration (covers Abliterated Large V2 and its creator), "uncensored", Heretic-style finetunes, the Dolphin finetune family, and Venice AI's uncensored rebrands. The flag renders as an amber 🔓 badge in the Models table and an "uncensored" badge in the Pareto inspector and orb tooltips, and both views gain a **Safety** filter (All models / Uncensored / Safety-tuned): a combo beside Modality in the Models tab, and beside the Show filter on the Pareto chart where it resets zoom and drops the selection like the other chart filters. Abliterated Large V2 is the first flagged listing; its benchmark stays a disclosed `~proxy` of GLM-5.3, so the filter selects uncensored deployments without presenting that proxy as a measurement.

## Validation

Formatting, locked check, locked test (139 tests), Clippy with warnings denied, and the live-data smoke test all pass. The smoke run pins the composite leader — Claude Opus 5.5 at 99.3, unchanged — with all eight sources fetched (AA snapshot 159 rows, LiveBench 71, FrontierCode 46, Terminal-Bench 4 35) and the refreshed rows joining the live boards at the top of the chart: Gemini 4 Argon #3 (92.4), Muse Wonder #4 (92.3), and the re-read Muse Spark 1.3 tier at 85.9/85.4.

# ParetoWatch v0.17.10

## Fixes

- **24h market volume displayed 1,000,000× too large** everywhere it appeared. The Surplus feed reports every money amount in micro-USD — per-1M prices are divided by `SURPLUS_MARKET_MICRO_USD_PER_USD` at parse — but `volume_24h` had been read raw since market telemetry landed in v0.4.0, so Opus 5.5 showed "962M volume / 24h" when $962 was actually traded (20K requests ≈ $0.05 each, in line with an Opus-class model; DeepSeek V4.1 Flash's "205.2M" was $205 over 900K requests). Volume is now converted to dollars at the parse site, and both displays — the Models tab's Vol /24h column and the Pareto inspector — format it with a new compact-dollar helper that prefixes `$` and keeps two decimals on the sub-$10 tail ($962, $6.2M, $0.20), so volume can no longer read as a token count or a price.
- History recorded the misread too: once-per-day telemetry events stored `volume_cents` as micro-USD. The log format bumps to v2 (true cents); opening a v1 log rescales every telemetry event ÷1,000,000 and rewrites the file in place — encoded to a temp file and renamed over the original, so a crash mid-migration leaves the old log intact. If the rewrite cannot complete, the v1 file is archived as `.old` exactly like an unknown-format log, never mixed with new-scale appends.

## Validation

Formatting, locked check, locked test (135 tests), Clippy with warnings denied, and the live-data smoke test all pass. The smoke run pins the composite leader — Claude Opus 5.5 at 99.3, unchanged — with all eight sources fetched (AA snapshot 155 rows, LiveBench 66, FrontierCode 42, Terminal-Bench 4 27).

# ParetoWatch v0.17.9

## Updates

- New Models tab columns: **Req /24h** and **Vol /24h**. The live-market telemetry the Pareto chart's inspector already shows — requests routed through the market and dollars traded, both over the last 24h — is now a sortable column pair in the Models table (between Provider and Disc), so the table can rank by market activity rather than price alone. Values use the compact K/M formatting; catalog rows carry no market telemetry and show a dimmed dash with a hover saying why, and missing values sink below every reported one in the default ascending sort — the same convention the cache column documents — with every key still tie-breaking on display name so the order stays stable across polls.

## Validation

Formatting, locked check, locked test (133 tests), Clippy with warnings denied, and the live-data smoke test all pass. The smoke run pins the composite leader — Claude Opus 5.5 at 99.3, unchanged — with all eight sources fetched (AA snapshot 155 rows, LiveBench 66, FrontierCode 42, Terminal-Bench 4 27).

# ParetoWatch v0.17.8

## Updates

- AA Intelligence Index snapshot stays on v4.3.2 (read 2026-09-30, second same-day bump). The leader (Claude Opus 5.5, 58) and the scale are unchanged — Argon ties GPT-6 Astra's live display rather than challenging the leader — so only the new-model row was read fresh and `AA_CALIBRATION_POINTS` keeps its anchors.
- New snapshot row: **Gemini 4 Argon** (53 — Google DeepMind's first proprietary model above the Flash class in over seven months, read at high reasoning, the highest effort it exposes). It ties GPT-6 Astra's live integer display (the stored Astra row keeps its 52.8 v4.3 decimal read), sits one point over GPT-6.1 Sol (52), 12 over Gemini 3.8 Flash (41) and 23 over the last pre-Flash flagship Gemini 3.1 Pro Preview (30), putting Google back among the top three labs by intelligence. The model is rolling out to selected users only behind a 50% launch pricing discount ($2/$10 per 1M tokens; $1.99 per Intelligence Index task at the discount, rising to $3.98 after), so the row lands ahead of a general Surplus listing; `argon` is a codename rather than a stripped deployment token, so the key joins nothing else. Its agentic headline results — #1 on AutomationBench-AA at 77.5, Terminal-Bench 4 at 57, AA-Briefcase 1494 Elo, and the lowest AA-Omniscience hallucination rate (15%) of any model scoring 45+ — live on boards the app already fetches rather than in this snapshot.

## Validation

Formatting, locked check, locked test (132 tests), Clippy with warnings denied, and the live-data smoke test all pass. The smoke run pins the composite leader — Claude Opus 5.5 at 99.3, unchanged — with Gemini 4 Argon entering at #2 (96.8, between Opus 5.5 and Fable 5.1) off its fresh AA row plus the live rows boards already publish for it (Terminal-Bench 4 measures it at 57), and all eight sources fetched (42 live FrontierCode rows, 27 Terminal-Bench 4 rows).

# ParetoWatch v0.17.7

## Updates

- AA Intelligence Index snapshot bumped from v4.3.2 (read 2026-09-29) to v4.3.2 (read 2026-09-30). The index revision, scale, and leader are unchanged — Claude Opus 5.5 still leads at 58 — so only the two days' new-model rows were read fresh and `AA_CALIBRATION_POINTS` keeps its anchors.
- New snapshot rows for the Surplus listings of 2026-09-29..09-30: **GPT-6.1 Sol** (52 — OpenAI's Sol-tier upgrade over GPT-6 Sol, still positioned below the flagship GPT-6 Astra; read at the max effort the Surplus SKU exposes via `reasoning_effort`, it entered at #10 of 222, between the Fable 5.1/Astra tier and GPT-6 Sol's 48) and **GPT-6.1 Sol Pro** (52, inherited: the same weights served with `reasoning.mode: "pro"`, resolved through `INHERITED_EXECUTION_MODES` like the GPT-6 pro SKUs). **Inception: Mercury 2.5** (12) — the diffusion-LLM refresh of Mercury 2 — keeps the `Inception: ` prefix in its row name so it exact-joins the Surplus display name, the same treatment the Cohere row uses, because `inception` is not a creator prefix benchmark matching strips.
- **ByteDance Seed: Seed-2.0-Code** (listed 2026-09-29) has no AA presence — the model page 404s and the sitemap lists no Seed URL — so it falls back to the composite's neutral prior alongside Seed 2.1 Turbo instead of proxying an invented score.

## Validation

Formatting, locked check, locked test (131 tests), Clippy with warnings denied, and the live-data smoke test all pass. The smoke run pins the composite leader — Claude Opus 5.5 at 99.3 — with GPT-6.1 Sol entering the top 10 at 86.2 (between Sonnet 5.5 and MiMo V2.6 Pro) off its fresh AA row plus the xhigh rows the live boards already publish for it, and fetched 42 live FrontierCode rows. The base+Pro pair renders in the composite exactly like the established GPT-5.6 Sol pair (verified against the pre-change smoke run, which shows the same doubled display line for GPT-5.6 Sol).

# ParetoWatch v0.17.6

## Updates

- AA Intelligence Index snapshot bumped from v4.3.2 (read 2026-09-22) to v4.3.2 (read 2026-09-29). The index revision and scale are unchanged — Claude Opus 5.5 still leads at 58 and the existing rows still round to their live displays (Fable 5.1 53.4 → 53, GPT-6 Astra 52.8 → 53) — so only the week's new-model rows were read fresh and `AA_CALIBRATION_POINTS` keeps its anchors.
- New snapshot rows for the Surplus listings of 2026-09-23..09-29, headlined by **Claude Sonnet 5.5** (56, released 2026-09-28 — AA's new #3 of 216 at adaptive reasoning/max effort with default fallback, read at the same page configuration as Opus 5.5; it slots between the leader and the Fable 5.1/Astra tier and LiveBench and FrontierCode already carry live rows for it). **Cohere: Command A+** (13) is the snapshot's first Cohere row — the row name keeps the `Cohere: ` prefix so it exact-joins the Surplus display name (the id `command-a-plus` normalizes differently because `+` collapses away). **Qwen3.8 Omni Flash** ~proxies its text sibling (39.9): AA publishes no omni page, and its sitemap now lists only `qwen3-8-flash-next`, a distinct Aug-26 preview.
- **GLM 5.3 Prime** (listed 2026-09-24) is a same-checkpoint high-speed serving variant of GLM-5.3 — per the listing it inherits the full capabilities at 1.5–2× throughput — so it gets no row of its own: benchmark matching now strips the `prime` deployment token alongside fast/highspeed/ultraspeed and the SKU joins the GLM-5.3 row exactly, the same inheritance MiMo-V2.6-Pro-UltraSpeed uses (verified against every board and the whole Surplus catalogue: nothing else contains the token). **Fireworks Ember-1** has no AA presence, so it falls back to the composite's neutral prior; it deliberately does not proxy Kimi K3 despite being built on it — it is a derivative build, not the same checkpoint.
- New Pareto chart toggle: **Hide catalog-only prices**. A "Hide catalog-only prices" checkbox next to "Log price axis" removes every model whose price is only a Surplus catalog/comparison list price (no live-market ask), so list-price orbs can no longer plot against — or hold frontier spots against — live market asks while still using "Market quality: Any pricing". The Models tab's Source filter (All / Live market / Catalog) already covered its table; the chart had no equivalent under "Any pricing". Toggling resets zoom and drops the selection, like the other chart filters, and the empty state names the toggle when it is the reason nothing shows.

## Validation

Formatting, locked check, locked test (130 tests), Clippy with warnings denied, and the live-data smoke test all pass. The smoke run pins the composite leader — Claude Opus 5.5 at 99.3 — with Claude Sonnet 5.5 entering the top tier at 89.4 (#5, between Fable 5 and GPT-6 Astra's 89.3) off its fresh LiveBench and FrontierCode rows, and fetched 41 live FrontierCode rows (Sonnet 5.5 best 52.1 at xhigh on the main subset).

# ParetoWatch v0.17.5

## Updates

- AA Intelligence Index snapshot bumped from v4.3 (read 2026-09-19) to v4.3.2 (read 2026-09-22) for the week's flagship refresh. v4.3.2 is a patch revision on the same scale — the v4.3-read rows round to their live v4.3.2 displays (Fable 5.1 53.4 → 53, Opus 5 50.7 → 51, GPT-5.6 Sol 47.1 → 47) — so only the new-model rows were read fresh; AA's model pages now display integers.
- New snapshot rows for the Surplus listings of 2026-09-20..09-22: Claude Opus 5.5 (58 — Anthropic's new flagship, which entered as AA's new leader at #1 of 212, adaptive reasoning at max effort with default fallback), the GPT-6 tier fill-out below Astra — GPT-6 Sol (48, the cost-efficient high-end tier) and GPT-6 Luna (37, the fast tier), both read at the max effort the Surplus SKUs expose via `reasoning_effort`, plus their Pro SKUs as inherited execution modes (`reasoning.mode: "pro"`, same weights — GPT-6 Sol Pro 48, GPT-6 Luna Pro 37) — Grok 4.7 (46, xhigh: AA measured no max-effort run), and Xiaomi's 1T flagship MiMo-V2.6-Pro (46). MiMo-V2.6-Flash and TypeSafe's JEV 1.13 decisions model have no AA presence, so they fall back to the composite's neutral prior; MiMo-V2.6-Pro-UltraSpeed is a same-checkpoint speed wrapper and inherits the Pro row via benchmark matching, which now also strips the `ultraspeed` deployment token alongside fast/highspeed.
- The composite's `AA_CALIBRATION_POINTS` top anchor moved from 50 to 58 with the new AA leader. Under the old anchor the entire old leader tier (Fable 5.1 at 53.4, GPT-6 Astra at 52.8) extrapolated past the 100th-percentile ceiling and would have tied Opus 5.5 there in the pure-AA fallback — the same HashMap-order tie the v0.17.3 fix removed. With the anchor at 58, Opus 5.5 sits exactly at the 99th percentile while Fable 5.1 (96.8) and Astra spread strictly below it. TypeSafe also gets a creator colour so the JEV listing groups readably on the chart.

## Validation

Formatting, locked check, locked test (128 tests), Clippy with warnings denied, and the live-data smoke test all pass. The smoke run pins the new composite leader — Claude Opus 5.5 at 99.3 (already measured on LiveBench 71.7 and FrontierCode 54.6) ahead of Fable 5.1 at 96.8, with MiMo V2.6 Pro entering the top 10 off its live board rows — and fetched 40 live FrontierCode rows.

# ParetoWatch v0.17.4

## Updates

- AA Intelligence Index snapshot rebaselined from v4.1.2 (read 2026-08-26) to v4.3 (read 2026-09-19). AA shipped two index revisions in three days — v4.2 (Sept 4: more complex and realistic tasks, more private test sets) and v4.3 (Sept 7: Terminal-Bench moved to 4.0, 𝜏³-Banking replaced by AutomationBench-AA) — which compressed the whole scale (the leader tier moved from AA 66 to AA 53), so every one of the snapshot's rows was re-read from the current model pages rather than left mixing scales. Ranking order is broadly preserved: Claude Fable 5.1 still leads at 53.4 with GPT-6 Astra (see below) at 52.8 and Claude Opus 5 at 50.7.
- New snapshot rows for the Surplus listings of 2026-09-03..09-11: GPT-6 Astra (52.8 — OpenAI's new flagship, which AA's own writeup ties with Fable 5.1 at the top of the index at lower cost), DeepSeek V4.1 Flash (39.5, the Sept 10 refresh; AA's base DeepSeek V4 Flash page now tracks the 0731 refresh and the 0420 pages carry the older rows, matching how the snapshot already split them), and Meta: Muse Spark 1.3 Contributor (45.2, ~proxy of the purchasable xhigh variant AA rates, like its base row). The Sakana Fugu Max / Fugu Ultra v2 listings (Sept 11) have no AA presence yet, so they correctly fall back to the composite's neutral prior instead of getting invented scores.
- AA removed the GLM-4.5 and Llama 3.2 3B model pages between v4.1.2 and v4.3, so those families lost their snapshot rows rather than keeping stale v4.1-scale numbers; the DeepSeek R1 page was folded onto the 0528 refresh, so both Surplus R1 SKUs now read the same measurement. The composite's `AA_CALIBRATION_POINTS` were recalibrated in the same change (same reference models, new scale: top anchor 50 → 99th percentile, down to 2 → 3rd), keeping AA percentiles comparable with the other boards; all scoring fixtures that encoded the old scale were moved with it.

# ParetoWatch v0.17.3

## Additions

- FrontierCode (Cognition) joins the board lineup as the eighth remote source. Cognition publishes the leaderboard as a static JSON export (`/data/frontiercode-leaderboard/data.json`), so the fetcher reads that directly instead of scraping the page: for every model it takes the headline number the site shows — the best `new_score` across reasoning efforts on the 100-task `main` subset of FrontierCode 1.1, as a percentage — plus the vendor harness (claude-code, codex, chisel, grok-build, mini-swe-agent, cursor-cli), the winning effort when it is a named tier (low/medium/high/xhigh/max; `none` and raw parameters like `0.99` are run configs, not efforts), and the published tokens-per-task telemetry, which the cost calculator reprices with live Surplus quotes like every other board. Creators come from the export's own lab palette (so harness-native rows like SWE-1.7 → Cognition, Composer 2.5 → Cursor, Inkling → Thinking Machines are labelled correctly), with the name heuristic as fallback. The board is harness-specific for composite purposes — full 0.15 weight in the deployment flavor, demoted to a third in the capability flavor — and the live smoke test confirms the composite leader is unchanged with it in the mix.
- Copyable model slug in the Pareto detail card (under the model heading, next to the live-market badges) and in the Models tab's cost calculator (under the model selection). One click on the 📋 button copies the Surplus model id and flips to ✔ for two seconds. The clipboard/check glyphs are deliberately emoji code points: only the egui-bundled Noto Emoji / emoji-icon-font faces cover them, and plain symbols like U+29C9 render as tofu in every bundled font.

## Fixes

- The pure-AA fallback (boards not loaded yet: the first moments after launch, or every board fetch failing) could rank Opus 5 above Claude Fable 5.1. The calibration curve clamped every score at or above its top anchor (AA 63 → 99th percentile) to a flat 99.0, so Fable 5.1 (AA 66) tied Opus 5 there and the composite's HashMap iteration order decided the leader per launch. Above-anchor scores now keep climbing along the top segment's slope, clamped at the 100th percentile, so the tie is broken by the AA scale itself. With boards loaded, nothing changes: Fable 5.1 still tops the composite at 99.5 against Opus 5's 97.0 (re-verified live).
- `record_persists_and_reloads_identically` used raw `Utc::now()` for a +1h offset, so running the test suite in the hour before UTC midnight failed the once-per-day telemetry count. It now uses the existing noon-UTC `day_offset` anchor like its sibling tests.

## Validation

Formatting, locked check, locked test (127 tests), Clippy with warnings denied, and the live-data smoke test all pass; the smoke run fetched 34 live FrontierCode rows and the composite leader is unchanged.

# ParetoWatch v0.17.2

## Fixes

- Composite ranking corrected for the AA leader when it has thin board coverage. Claude Fable 5.1 (AA Intelligence 66) was showing 90.6 under Opus 5's 97.1 even after the v0.17.1 fix, because LiveBench had already published a Fable 5.1 row: a single thin board put it on the partial-coverage path, whose confidence ramp shrank the score toward population-neutral. Thin coverage now reverts toward the model's PRIOR standing — weak evidence moves a model partway from what the AA scale already established toward the measurement — plus a small ordinal uncertainty discount so near-tied models still order the broader-evidenced one first. Verified against the live leaderboards: Fable 5.1 tops the composite at 99.5, Opus 5 at 97.1.
- A new live-data smoke test (`cargo test -- --ignored`) runs the real board fetchers through the composite and pins the leader, so ranking rulings can be checked against production data before shipping.

## Validation

Formatting, locked check, locked test (120 tests), Clippy with warnings denied, and locked build all pass. All prior ranking regressions are preserved: the mid-AA newcomer enters mid-pack below measured leaders, the hot two-board model stays below broad top coverage, and the GLM-5.3/Sol thin-evidence ordering keeps its ordering via the uncertainty discount.
