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
