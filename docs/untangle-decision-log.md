# Untangle decision log — 2026-11-07

Baseline commit `5a57588` (0.34.1). Goal: structural refactor only — intended behavior unchanged.
Scope additions chosen by user: **Comments** (remove comments that only restate the code) and
**Tests** (remove tests that wouldn't catch a real bug, fix ones that fail without one — done last).
Evidence: module dep graph (production vs test split), three Explore sweeps (fetch layer, app layer,
infra), git change-frequency over last 30 commits (hottest: pareto_tab, types, snapshot, scoring, app/mod).

## The problems

### Hidden links (must change together, code doesn't show it)

- H1. `src/fetch/aa_evals.rs:42` defines `AA_3D_URL`; `src/types.rs:16` defines it again.
  `live_validation.rs:544` reads the types copy while `fetch_aa_3d` fetches the aa_evals copy —
  change one and the validation validates the wrong page.
- H2. `NAMED_EFFORTS` defined 3×: aa_evals.rs:46 (shared), frontiercode.rs:26 and swebench_pro.rs:23
  (private duplicates). Its order is load-bearing (position-indexed by `variant_preference`
  aa_index.rs:198, inverted by `speed_variant_rank` aa_speed.rs:130).
- H3. `alias_display_name` shared in aa_evals.rs:51 AND partially re-implemented in
  swebench_pro.rs:27 (duplicated `Inkling` arm). Also consumed by artificial_analysis_snapshot.rs:1394,
  whose comments say snapshot names must track it.
- H4. `ATTRIBUTION_WINDOW = 2_500` duplicated aa_index.rs:39 / aa_speed.rs:30 ("mirrors …" comment).
- H5. Enum option lists enumerated per tab (2–3 lists each): ModalityFilter pareto_tab.rs:221-233 +
  models_tab.rs:412-420; UncensoredFilter pareto_tab.rs:237-249 + models_tab.rs:423-431;
  LiquidityFilter pareto_tab.rs:182-190 + alerts_tab.rs:255-266; ComparisonMode pareto_tab.rs:163-171 +
  alerts_tab.rs:225-235; BenchmarkSource display list pareto_tab.rs:73-78 + alerts_tab.rs:190-195;
  BenchmarkMetric trio pareto_tab.rs:102-106 + alerts_tab.rs:208-212; PriceMetric trio
  pareto_tab.rs:2019-2024 + alerts_tab.rs:107-111, 273-277.
- H6. Workload presets triplicated: settings_tab.rs:79-96, pareto_tab.rs:318-322, reset fallback
  app/mod.rs:1780-1782 (plus `agentic_preset_m` models_tab.rs:186-208); "Current mix" label duplicated
  settings_tab.rs:133-143 / pareto_tab.rs:331-341.
- H7. Benchmark metric forced to AgenticCoding when source ≠ LiveBench — implemented twice:
  app/mod.rs:1099-1103 (cache key) and 1400-1404 (alert eval).
- H8. ui_state persisted fields must be added in 6 places (AppState field, UiState struct, Default,
  snapshot_ui_state, apply_ui_state, constructor defaults) — `#[serde(default)]` masks a miss; no test
  round-trips the struct.
- H9. Alert form ↔ AlertRule field mapping kept aligned by hand across alerts_tab.rs:530-560 (build),
  585-608 (load), and ~17 `alert_*` AppState fields; three "create alert" menus each seed a different
  implicit subset (pareto_tab.rs:1144-1189, activity_tab.rs:730-753, models_tab.rs:1194-1211).
- H10. The four pricing weights are plumbed as loose f64s / `(f64,f64,f64,f64)` tuples through
  ~15 call sites (types.rs LiquidityFilter::apply, Quote::price/price_for; pareto.rs joined_points;
  alerts.rs; app/mod.rs floor alerts; history track/ui; activity/pareto tabs). Order errors compile fine.
- H11. Cache keys hand-rolled per memo (six constructions in app/mod.rs); `data_version` conflates
  "new data" with "hidden-frontier changed" (bumped at mod.rs:730/741/753 AND 1729/1739), so a legend
  toggle pointlessly invalidates scaffold/aa-speed joins.
- H12. `PriceChangeEvent` → `PriceMoveRecord` hand-copied field-by-field (10 copies, app/mod.rs:684-699).
- H13. models_tab.rs:119-165 `resolve_calc_legs` re-encodes the cache-rate fallback policy that
  `Quote::effective_cache_read` / `best_available_cache_read` own in types.rs.
- H14. "† = live-market bucket" tooltip prose and dagger convention duplicated ~7×
  (pareto_tab.rs:1346-1349, 1367-1370, 1480-1483; models_tab.rs:706-715, 717-723, 751-757, 1095-1104).
- H15. Consensus grid duplicated verbatim: pareto_tab.rs:1726-1743 vs activity_tab.rs:663-681.
- H16. `live_validation.rs:657-668` re-implements aa_speed's private `speed_variant_rank`
  ("aa_speed keeps it private" comment) — a documented copy that must track the original.
- H17. `BenchmarkSource::remote_sources()` (types.rs) must stay in lockstep with
  `fetch_benchmark_source` dispatch (fetch/mod.rs:338-359); the only guard is a #[cfg(test)] live test.
- H18. Widget spawn sites (5) each pass `self.liquidity_filter` into the pinned-price widgets —
  pinning while a chart filter is active silently changes the pinned row's basis forever.
- H19. Board URL constants: `AA_3D_URL` (H1); vals_sre fetches `types::VALS_SRE_BENCH_URL` while
  every sibling keeps its URL local; `types::BenchmarkSource::source_url` repeats board-specific
  display-URL facts (terminal-bench "4-0-0") that terminal_bench.rs:19 must match.
- H20. `performance_cache` LRU eviction depends on `fetched_at` doubling as last-use stamp, decided
  by the app-layer refresher — the cache's eviction order is governed by a caller convention.
- H21. AA speed / speed-suffix handling: `speed_variant_rank` private (H16); `variant_preference`
  (aa_index) and `speed_variant_rank` (aa_speed) encode opposite orderings of the same
  `NAMED_EFFORTS` list — documented only in comments.
- H22. live_validation `live_model_join_audit` re-implements matching internals (version_seq,
  surplus_tokens, speed_rank) to "independently" check joins — intentional for audit, but `speed_rank`
  is a plain copy, not an independent check.
- H23. Snapshot module name table ↔ `alias_display_name` ↔ `SAME_WEIGHTS_SKU_ALIASES` ↔ release_dates
  rows must stay name-aligned; enforced only by scattered tests referencing each other's tables
  (snapshot.rs:1394 imports fetch::aa_evals::alias_display_name).

### Needless links (tied together, change for different reasons)

- N1. `types ↔ bench::matching` production cycle. Root: types.rs:10 `use crate::bench::matching::normalize`
  (one call in `PerformanceSnapshot::get`), while matching needs types::{Benchmark, Quote}. The
  most-depended-on module (fan-in 37) sits above a mid-layer matcher.
- N2. `theme ↔ bench` and `fetch → theme`: theme.rs:6 `use crate::bench::normalize` for `infer_creator`;
  matching.rs:6 `use crate::theme::{canonical_group, infer_creator}`; six fetch boards import
  `theme::infer_creator`/`canonical_group` (prices, design_arena, frontiercode, livebench, aa_speed,
  fetch/mod). Creator *identification* (identity logic) lives in the UI-colors module.
- N3. `artificial_analysis_snapshot → fetch` (imports `fetch::score_benchmark` and
  `fetch::aa_evals::alias_display_name`) while `fetch::aa_index → artificial_analysis_snapshot` — a
  two-way coupling between a bundled-data module and the fetch layer. Root: `score_benchmark` (a
  Benchmark *constructor*) lives in fetch/mod.rs, and AA name conventions live in a board fetcher.
- N4. `bench/matching → artificial_analysis_snapshot` (DISTINCT_BUILD_NON_JOINS, matching.rs:5): a
  matching-policy table stored in the snapshot data module.
- N5. Glob re-exports hide name origins: `bench/mod.rs:7-8` (`pub use matching::*; pub use scoring::*;`)
  and `fetch/mod.rs:20-31` (11 globs). Callers write `crate::bench::normalize` /
  `crate::fetch::fetch_deepswe` and can't see which submodule owns the name.
- N6. `fetch/mod.rs` is a god-module: worker orchestration loop + alert-state persistence (201-252) +
  HTTP helpers + parse utils + benchmark dispatch + `score_benchmark`. Depends on eframe/egui only to
  call `ctx.request_repaint()` (130/161/185) — a data layer with a UI dependency.
- N7. `fetch/deepswe.rs:12` imports `format::format_compact_number` to build `token_profile` display
  text inside a data row (data layer → display formatter). Accepted (provenance string is the data);
  noted, not fixed.
- N8. 8 copies of the ProjectDirs triplet with per-module fallback filenames (settings_store.rs:16,
  benchmark_cache.rs:34, performance_cache.rs:39, aa_speed_cache.rs:28, notifications.rs:323,
  history/mod.rs:28, app/ui_state.rs:124, fetch/mod.rs:202).
- N9. 7 copies of tmp+rename atomic write with 4 divergent failure policies (settings_store remove-
  then-rename = riskiest; fetch alert-state same; notifications archive-aside; caches delete-tmp-give-up;
  history rewrite-over). One crash policy question, four answers.
- N10. Three disk caches are three copies of one pattern (CacheFile{version,saved_at,payload}, same
  load conditions, byte-identical save, same stamp format, same drift test) with per-type differences
  only (LRU cap in one).
- N11. `app/ui_state.rs:123` and `history/mod.rs:22` both define `ui_state_path` for two different
  files/formats/directories.
- N12. ON_DEMAND_REFRESH_SECS (fetch/performance.rs:33) is a fetch-layer constant consumed by three
  tabs to gate UI-triggered requests — refresh policy split between fetch constants and app sweep
  (app/mod.rs:614-634) with backoff constant living in fetch (performance.rs:38) but applied in app.
- N13. `dedupe key` convention `benchmark_model_key` spread by glob (see N5) — deepswe, terminal_bench,
  vals_sre each `use crate::bench::...` directly; fine once paths are explicit, no fix beyond N5.
- N14. `history::store::quantize_price` reaches into alerts.rs:388 `prices_differ` — the alert layer
  must agree with the storage format's write epsilon. Documented at alerts.rs:383-386; acceptable
  (kept; it is the point of the helper), noted.

### Surplus (no reason of their own to change)

- S1. `aa_v5_coverage_survey` (live_validation.rs:263-529): 270 lines of self-described "temporary
  tooling for the 2026-10-28 investigation" with two hardcoded tables — delete.
- S2. `NotificationKind::Test` never produced anywhere; exists only in ALL/label so the activity filter
  offers a choice that can never match — remove.
- S3. `history::track::carry_forward` pub(crate) but its only cross-module use is a test import —
  make private, move the test.
- S4. pub(crate) items used only in-file (fetch boards: `reasoning_effort_rank`, `DEEPSWE_LEADERBOARD_URL`,
  `strip_provider_prefix`, `TERMINAL_BENCH_*`, `SURPLUS_*_URL`, `find_*_entries`, `price_pair_per_million`,
  `cache_read_per_million`, `catalog_is_text_model`, `parse_*`, `split_scale_name`, `mean`, `is_release_label`,
  `quoted_string_array_items`, `livebench_bundle_path`, etc.) — tighten visibility.
- S5. `fetch/mod.rs:419-429` `first_number_path→value_as_f64` re-implements `number_value` (minor;
  folds into the parse-utils split).
- S6. `format_price_tick` vs `format_usd`: two parallel magnitude-tiered dollar formatters. Consolidate
  callers of `format_usd` onto one formatter only if behavior-identical; otherwise note and keep.
- S7. Regex::new per call throughout fetch (no LazyLock) — fold into shared helpers where the regex
  literals are duplicated; leave single-use cold-path ones alone.

### Special cases (each is a requirement the design never accounted for)

- SP1. settings_store.rs:99-104 remove-then-rename on rename failure — riskiest atomic-write policy
  (goes with N9's unified policy).
- SP2. app/mod.rs:314-323 collapse_consecutive_toggles — tray double-click quirk; KEEP (external
  platform behavior).
- SP3. app/mod.rs:328-337 delistings deduped by display_name because `ListingChange` carries no model
  id — give ListingChange the id, dedup by id.
- SP4. app/mod.rs:714-718 seeds `alert_model` from the first quote as a hidden side effect of
  receiving prices — move to the explicit place the UI first needs it (low churn acceptable; else keep).
- SP5. models_tab.rs:851-858 / pareto_tab.rs:498-548 state-repair in render paths (drop vanished
  providers/groups) — inherent to persisted UI state; KEEP.
- SP6. ui_state.rs:52-141 legacy `hide_catalog_prices` migration — KEEP (old-file migration).
- SP7. deepswe.rs:57 `mini swe agent` harness whitelist; terminal_bench.rs:119 demotes mini-swe-agent
  rows to Model kind — domain policy; KEEP, but both share the harness-string convention via
  matching::normalize (make the shared helper explicit).
- SP8. aa_evals.rs:162/260 Elo plausibility band 500..=3000 duplicated in live_validation — share
  via one const when live_validation is cleaned.
- SP9. aa_evals.rs:61-64 alias table hardcodes 5 model names (snapshot-coupled data in fetch code) —
  stays (it is the AA-board alias convention), but moves with the name-classification module so
  snapshot and fetchers read one table.
- SP10. pareto_tab.rs:602 user-facing board names hardcoded in a message ("choose DeepSWE,
  Terminal-Bench…") — use BenchmarkSource labels.
- SP11. livebench.rs:167-171 "Long Agentic" missing → silent overall-mean fallback — documented
  domain decision; KEEP (add nothing).
- SP12. history-ui selection write is the lone non-atomic persistence (history/ui.rs:713-720) —
  unify onto the shared atomic_write.
- SP13. Notification `.old-` / history `.bin.old-` / settings `.bad-` archive sidecars accumulate
  forever — accepted (bounded by crash frequency); no rotation machinery (speculative generality).

## The redesign (fixes above the problems they solve)

### F1. One identity module: `model_id`
Move normalization + creator identification into a new leaf module `src/model_id.rs`:
`normalize`, `model_core`, `benchmark_model_key`, `canonical_creator`, `creators_compatible`,
`token_jaccard`, `is_year_token`, `is_model_modifier`, `keys_differ_only_by_release_tag` (from
bench/matching.rs) and `canonical_group`, `infer_creator`, `group_label` + the creator tables (from
theme.rs). Deps: only std. bench/matching keeps the *join policy* (best_benchmark_match,
distinct-build guard) and imports model_id. theme keeps colors/badges only and imports model_id for
`creator_color`'s canonicalization needs (if any remain).
- N1 (types::PerformanceSnapshot::get → model_id::normalize; leaf dep, cycle gone), N2 (theme
  depends on model_id not bench; matching depends on model_id not theme; fetch imports model_id),
  part of N3 (infer_creator import path for score_benchmark's new home).

### F2. Constructors and tables live with their concept
- `score_benchmark` (Benchmark constructor) moves from fetch/mod.rs to types.rs (next to Benchmark);
  it calls model_id::infer_creator. Snapshot module imports types, not fetch.
- `DISTINCT_BUILD_NON_JOINS` moves from artificial_analysis_snapshot.rs to bench/matching.rs (the
  matcher owns its guard table); snapshot keeps the *data policy* tables (SNAPSHOT_ROWS,
  SAME_WEIGHTS_SKU_ALIASES, PRE_V5_READS).
- AA name-classification trio (`NAMED_EFFORTS`, `alias_display_name`, `split_aa_display_name`, the
  alias table, `classify_aa_name`, `variant_preference`) moves to a new `src/fetch/aa_names.rs`
  depending only on model_id + types; aa_evals/aa_index/aa_speed and the snapshot module all import
  it; swebench_pro's local copy is deleted in favor of the shared one.
- N3, N4, H2, H3, SP8, SP9.

### F3. Explicit import paths (kill glob re-exports)
Remove `pub(crate) use x::*;` from bench/mod.rs and fetch/mod.rs; fix all call sites to name the
owning module (mechanical, compiler-guided). N5, N13; makes every "who owns this name" question
greppable.

### F4. Split fetch/mod.rs
- `src/fetch/http.rs`: fetch_json/fetch_text/fetch_json_post + cap/read/decode (S5 folds in:
  value_as_f64 reuses number_value).
- `src/fetch/parse.rs`: strip_html_tags, collapse_whitespace, number_value, first_*_path,
  string_at, json_shape.
- `src/worker.rs` (already the enum home) gains the loop: start_worker moves there; alert-state
  persistence moves with it; repaint passed as `impl Fn() + Send + Clone` closure so worker.rs does
  not depend on egui. fetch/mod.rs becomes dispatch + re-export surface only. N6, part of N12.

### F5. Shared fetch-parse abstractions (single copies of the load-bearing duplicates)
- One RSC/shortName anchor-attribution helper (4 prod copies + 2 test copies) with the single
  ATTRIBUTION_WINDOW const (H4).
- One "keep best variant per family" fold parameterized by a rank fn (5 copies).
- One "rows else Err, sort desc" finisher used by all board parsers (9 + 7 copies) built on
  score_benchmark. H17 keeps its dispatch test; board parity preserved.
- terminal_bench switches to fetch_json_post. S1 survey deleted. speed_variant_rank shared with
  live_validation (H16). SP8 band shared. Regexes inside the new shared helpers become LazyLock (S7,
  only for shared ones).

### F6. Pricing weights become a type
`PricingWeights { input, cache_read, output, fallback_pct }` in types.rs with
`Settings::pricing_weights()`; `Quote::price/price_for/price_legs`, `LiquidityFilter::apply`,
`pareto::joined_points`, alert eval, floor-alert, history call sites take `&PricingWeights` or
`PricingWeights`. H10, H13 (models_tab resolve_calc_legs routes through Quote methods).

### F7. Enum option tables live with the enums
`pub(crate) const ALL: &[(Self, &str)]` (+ `label`) for ModalityFilter, UncensoredFilter,
LiquidityFilter, ComparisonMode, BenchmarkMetric, PriceMetric (where missing) and a
`BenchmarkSource::display_sources_with_labels()`; tabs render from these. H5, H6 (workload presets:
one `WORKLOAD_PRESETS` const + `current_mix` helper in types.rs), SP10 (labels from enum).

### F8. App-layer dedupe (single copies)
- `consensus_grid` widget (H15); `alert_seed_menu` on AppState (H9 — also makes field-seeding
  explicit in one place); `speed_note`/dagger tooltip formatter (H14); `price_move_arrow`,
  `search_box_with_clear`, weak-dash cell helpers; widgets.spawn through one AppState method with the
  liquidity coupling written down (H18); AgenticCoding-forcing helper (H7); PriceMoveRecord::from(
  &PriceChangeEvent) (H12); ListingChange gains model id, dedup by id (SP3).

### F9. Cache-key hygiene
Split `data_version` vs `frontier_version`: legend toggles bump only frontier_version, which only
the pareto and alert-status cache keys include. H11.

### F10. One persistence kit
`src/persist.rs`: `app_config_dir()/app_data_dir()/app_cache_dir()` + `atomic_write(path, bytes)`
(archive-aside policy, tmp cleaned) + `JsonDiskCache<T>` (version, saved_at stamp, load/save). The
three cache modules become thin users; settings/notifications/history/alert-state/ui_state share
atomic_write; history-ui selection write becomes atomic (SP12); `history::ui_state_path` renamed.
N8, N9, N10, N11, SP1, SP12.

### F11. Dead/visibility sweep
S1–S4, H20 note (leave eviction as-is but document the last-use convention on the cache), plus
pub(crate)→private tightening with grep verification.

### F12. ui_state round-trip test
save→load→compare-all-fields test so a field missed in snapshot/apply fails CI (H8).

## Rejected / considered
- Splitting types.rs into submodules: cohesion is real ("shared data model"), fan-in unchanged,
  churn high. Rejected.
- Renaming `fetch::ON_DEMAND_REFRESH_SECS` ownership (N12): policy genuinely spans worker + UI
  sweep; moving the constant next to the sweep would put a worker-used policy in app. Keep location,
  document. (Partially addressed by worker split clarity.)
- Sharing live_validation's version_seq/surplus_tokens with matching (H22): the audit's value is
  being an independent restatement; only the plain copies (speed_rank) are unified.
- Notification sidecar rotation (SP13): speculative generality; rejected.
- format_usd consolidation (S6): only if byte-identical output; otherwise keep both (defer).

## Verification plan
`cargo fmt`, `cargo check --locked`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --locked` (272 tests) must all pass at start and end; RELEASE_NOTES entry added;
behavior parity argued per fix (file formats byte-identical, policies identical, error strings kept).
