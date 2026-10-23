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
//!    - SKUs that are the same weights as an already-rated model (execution
//!      modes such as `reasoning.mode: "pro"`, licensing tiers, speed
//!      variants the key normalizer cannot strip, duplicate listing names)
//!      get one row per purchasable SKU with the base score, and MUST be
//!      listed in [`SAME_WEIGHTS_SKU_ALIASES`] so the UI discloses the
//!      inheritance and the composite rates the SKU identically to its base
//!      variant.
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
//! The 2026-10-15 bump stayed on v4.3.2: the leader (Claude Opus 5.5, 58)
//! and the scale are unchanged, so only the new-model rows were read fresh —
//! plus Muse Spark 1.3, whose max-effort page the Wonder launch re-measured
//! at 48 (the stored 45.2 v4.3 read had drifted well past rounding), so both
//! of its rows moved with it. Claude Haiku 5.5 — listed 2026-10-07 with a
//! half-price batch SKU — entered at 43 read at max effort, the same page
//! configuration its Sonnet/Opus 5.5 siblings use: 26 points over the 4.5
//! sibling (15.4), between GPT-5.6 Sol (47.1) and GPT-5.6 Terra (42.3), and
//! above GPT-6 Luna (37). The batch SKU serves the same weights over the
//! Batch API, so benchmark matching now strips the `batch` deployment token
//! and the SKU joins the base row exactly, the same inheritance the speed
//! wrappers use. Abliteration AI's Abliterated Large V2 (listed 2026-10-13)
//! is GLM-5.3 with the refusal directions ablated out of the weights plus a
//! post-ablation finetune; AA rates no page, but unlike derivative builds
//! such as Ember-1 the base checkpoint is explicit, so the row ~proxies
//! GLM-5.3's 44.9 — the snapshot's first uncensored-model row, and the app
//! flags uncensored listings separately so they can be filtered without
//! trusting a proxy as a measurement. Meta's Muse Wonder (listed 2026-10-14)
//! is the API-available half of the Wonder launch and entered at 52 (#9 of
//! 216, max effort), between Gemini 4 Argon (53) and GPT-6.1 Sol (52); its
//! stronger Muse Wonder Full sibling (56, tying Sonnet 5.5) has no public
//! API and no Surplus listing, so it gets no row. Mistral Large 4 — the
//! trillion-parameter launch AA's tweet nicknamed "Le Chonk", listed
//! 2026-10-15 with the Surplus price matrix still unpriced — is read off its
//! Preview release (2026-10-06, the only version AA rates) at 38: twenty-eight
//! over Mistral Large 3, and the strongest model on the index from outside
//! the US and China. Gemini 4 Argon's general Surplus listing landed
//! 2026-10-14 behind its existing row, and the new "Meta: Muse Spark 1.3"
//! SKU joins the Spark row through the stripped `meta` creator prefix.
//!
//! The 2026-10-17 bump stayed on v4.3.2: the leader (Claude Opus 5.5, 58)
//! and the scale are unchanged, so only the new-model rows were read fresh.
//! DeepSeek V4.1 Flash 1017 — the 1017 dated refresh of the V4.1 Flash
//! family, five weeks after the 0910 build the undated row reads — entered
//! at 40, a point over the family's 39.5 v4.3 read and just under Gemini
//! 3.8 Flash (41.2), keeping the family the value pick at its $0.15/$0.60
//! launch pricing. NVIDIA's Nemotron 3.5 Super VL — the 3.5 generation's
//! Super tier with native vision input, listed the evening of 2026-10-15,
//! hours after that day's read — entered at 25, past the Nemotron 3 Ultra
//! 550B (23.4) as NVIDIA's strongest row on the index; `vl` names the model
//! rather than a deployment wrapper, so it stays in the key, and the
//! `nvidia` creator prefix strips so the "NVIDIA: " Surplus display name
//! joins the row exactly. Mistral Large 4's listing priced its matrix at
//! $0.0000 across the board — a 100%-off launch window for "Le Chonk" — so
//! the existing Preview-read row (38) finally carries purchasable asks at
//! literal zero; the market parser's free-offer handling already keeps $0
//! asks real, so no code changed for it.
//!
//! The 2026-10-20 bump stayed on v4.3.2: the leader (Claude Opus 5.5,
//! live 57.6 → integer display 58) and the scale are unchanged, so only the
//! new-model rows were read fresh — plus DeepSeek V4.1 Flash 1017, whose
//! max-effort page firmed up at 45 (the 40 read three days earlier had
//! drifted past rounding, the same re-read Muse Spark 1.3 needed), and the
//! Qwen3.8 Max family, which AA reorganized: the canonical page now measures
//! the 0902 snapshot (45.42, chart default) while the old undated 0803 build
//! is deprecated and retained only as a non-default 40.15 entry. Surplus
//! delisted the undated Max SKU the same week, so the dated listing gets a
//! row of its own reading the same 45.4 the undated row carries — the
//! undated row stays because undated leaderboard slugs (LiveBench 2.0 still
//! publishes `qwen3.8-max`) keep joining it. Qwen3.8 Max Prime is a
//! higher-throughput same-checkpoint SKU and inherits through the stripped
//! `prime` token exactly like GLM 5.3 Prime. MiniMax M3.1 Flash entered at
//! 43 (single max-default measurement): thirteen over MiniMax-M3's 29.6 and
//! MiniMax's strongest row on the index. inclusionAI's Ling 3.1 Flash
//! entered at 41; `inclusionAI` is not a stripped creator prefix (same as
//! Cohere/Inception), so the row carries the display prefix to exact-join.
//! Reflection's Beam 1.0 (501B A23B, Apache 2.0) entered at 30. Meta's Muse
//! Spark Open — the open-weights coding SKU — entered at 39, read at xhigh,
//! the only effort AA measured; nine under the purchasable Spark 1.3 (48).
//! Z.ai's GLM 5.3 FlashX is GLM-5.3-Flash served at up to 200 tok/s, but
//! unlike `prime` it cannot be a stripped deployment token: stripping
//! `flashx` would collapse the key onto GLM-5.3 (44.9), the wrong base, so
//! it gets a ~proxy row onto GLM-5.3-Flash instead.
//!
//! The 2026-10-23 fix (same day, no index bump — no scores moved) renamed
//! `INHERITED_EXECUTION_MODES` to [`SAME_WEIGHTS_SKU_ALIASES`] and widened it
//! past `pro` execution modes after a reported regression: "Meta: Muse Spark
//! 1.3 Contributor" and "Meta: Muse Spark 1.3" are the same weights sold as
//! two SKUs, but the composite keyed the Contributor SKU alone — the AA prior
//! was its only evidence (85.4, 0 sources), while the base key joined the
//! LiveBench 2.0 boards the v0.19.0 migration restored and landed at 70.9, so
//! the same checkpoint showed two different Composite scores. Every
//! same-weights pair whose canonical key differs from its base now lives in
//! the alias table so the composite's per-source alias pass copies the base
//! variant's rows onto the SKU key in every board: the Contributor SKU, plus
//! four more pairs audited the same day — GLM-5.3-FlashX (same checkpoint at
//! up to 200 tok/s; previously only ~proxied for AA with no board
//! inheritance), DeepSeek R1 0528 (the refresh AA folded into the base DeepSeek
//! R1 page, so both Surplus SKUs serve the same weights), Mistral Small 3.2
//! 24B Instruct (the open-weights listing name of Mistral Small 3.2), and
//! Qwen3 Coder 480B Turbo (the 480B checkpoint served faster, the same
//! relationship the prime/ultraspeed wrappers have). Same-weights pairs that
//! differ by BUILD are deliberately NOT aliased: Qwen3.8 Max (0902) vs the
//! undated 0803 row, and the dated DeepSeek refreshes, keep separate rows
//! because a different snapshot is different weights even when the AA score
//! happens to match.
//!
//! The 2026-10-23 bump stayed on v4.3.2: the leader (Claude Opus 5.5, 58) and
//! the scale are unchanged, so `AA_CALIBRATION_POINTS` keeps its anchors and
//! only the new release's rows moved. AA's 2026-10-22 release evaluated Ling
//! 3.1 Flash 1019 — the 1019 dated refresh of inclusionAI's Flash family — at
//! 44, three over the undated build's 41 (re-read unchanged); like the other
//! dated refreshes the tag stays in the key, and the row lands ahead of its
//! Surplus listing the way Gemini 4 Argon and Mistral Large 4 did. The same
//! bump's Surplus catalog sweep surfaced `muse-spark-1.2-contributor`, which
//! had no row and joined the Muse Spark 1.2 rows only through the 0.80 fuzzy
//! bar; it now carries the 1.2 measurement and a SAME_WEIGHTS_SKU_ALIASES
//! entry. Nemotron 3.5 Super VL was re-read 25 → 26 (past rounding, matching
//! AA's 2026-10-15 changelog number), and spot-checks of GPT-6 Sol (48), Muse
//! Spark 1.3 (48), and the undated Ling row found no further drift. AA's
//! 2026-10-13..15 evaluations of models with no Surplus listing (Muse Wonder
//! Full, Solar Pro 4, Bolt 74B-A9B) need no rows.
//!
//! Deliberately unrated (no AA presence as of 2026-10-20): Aion 2.0/3.0/3.0
//! Mini, Hermes 3 405B, Palmyra Vision 7B, Qwen 2.5 7B, GPT-OSS Safeguard
//! 20B/120B, Grok 4.20 Multi-Agent Beta, Venice rebrands, uncensored finetunes
//! such as GLM 4.7 Flash Heretic (Abliterated Large V2 above is the lone
//! exception — its listing names the base checkpoint, so it proxies instead
//! of sitting unrated), Seed 2.1 Turbo and Seed-2.0-Code, the
//! 2026-09-11 Sakana
//! listings (Fugu Max, Fugu Ultra v2), MiMo-V2.6-Flash (the Pro sibling is
//! rated; AA has no Flash page yet), TypeSafe's JEV 1.13 decisions model, and
//! Fireworks Ember-1 (a Kimi-K3-derived reasoning model; AA has no page, and
//! it deliberately does NOT proxy Kimi K3 — it is a derivative build, not the
//! same checkpoint). The 2026-10-18 listings join them: Qwen3.7 Flash (AA
//! rates the 3.7 Plus/Max tiers but has no Flash page, and no same-checkpoint
//! sibling to proxy), LiquidAI's LFM2.5-24B-A2B (AA's LFM2.5 pages cover only
//! the 1.2B–8B small models; the 24B-A2B is not the LFM2 24B A2B checkpoint),
//! Moondream 3 Preview and Moondream 3.1 (vision-language; no AA pages),
//! Pareto 26.9 (no AA page), and Elorian's Glimpse (no AA page).
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

pub(crate) const ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE: &str = "2026-10-23";
pub(crate) const ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION: &str = "v4.3.2";

/// `(model name, AA Intelligence score)` pairs, one representative row per
/// family. Names should mirror Surplus display names so exact-key joins hit
/// before fuzzy matching has to.
pub(crate) const SNAPSHOT_ROWS: &[(&str, f64)] = &[
    // Added 2026-10-23 for AA's 2026-10-22 release, read from the
    // still-v4.3.2 index (leader and scale unchanged). Ling 3.1 Flash 1019 —
    // the 1019 dated refresh of inclusionAI's Flash family, evaluated at 44,
    // three over the undated build the 2026-10-20 row reads (41, re-read
    // unchanged) — lands ahead of its Surplus listing (the catalog still
    // carries only the undated `ling-3.1-flash` SKU), the same way Gemini 4
    // Argon and Mistral Large 4 did; like the other dated refreshes the
    // release tag stays in the key so the undated row keeps its own
    // measurement. Muse Spark 1.2 Contributor is the contributor-licensed SKU
    // of the 1.2 weights (Surplus lists `muse-spark-1.2-contributor`); AA
    // rates no page of its own, so the row carries the 1.2 measurement and
    // SAME_WEIGHTS_SKU_ALIASES passes board evidence through — until now the
    // SKU joined the 1.2 rows only through the 0.80 fuzzy bar, one rename
    // away from the 1.3 Contributor split. Nemotron 3.5 Super VL was re-read
    // in the same bump: the page firmed at 26 (the 2026-10-17 entry read of
    // 25 had drifted past rounding, matching AA's 2026-10-15 changelog
    // number).
    ("inclusionAI: Ling 3.1 Flash 1019", 44.0),
    ("Meta: Muse Spark 1.2 Contributor", 39.8), // same weights; see SAME_WEIGHTS_SKU_ALIASES
    // Added 2026-10-20 for the Surplus listings of 2026-10-18..10-20, read
    // from the still-v4.3.2 index (leader and scale unchanged). MiniMax M3.1
    // Flash entered at 43 (single max-default measurement), thirteen over
    // MiniMax-M3 and MiniMax's strongest row. Qwen3.8 Max (0902) reads the
    // canonical page AA re-pointed at the 0902 snapshot (45.42, chart
    // default); the old undated 0803 build is deprecated at 40.15 and Surplus
    // delisted the undated SKU, but the undated row below stays so undated
    // leaderboard slugs keep joining. Qwen3.8 Max Prime (higher-throughput
    // same-checkpoint SKU) inherits through the stripped `prime` token, not a
    // row. Ling 3.1 Flash carries the `inclusionAI: ` display prefix because
    // that creator is not stripped (same as Cohere/Inception). Beam 1.0 is
    // Reflection's open-weights 501B A23B reasoning model. Muse Spark Open is
    // read at xhigh, the only effort AA measured for the open-weights coding
    // SKU. GLM 5.3 FlashX ~proxies GLM-5.3-Flash: it is the same checkpoint
    // served at up to 200 tok/s, but `flashx` cannot be a stripped deployment
    // token because that would collapse the key onto GLM-5.3 (44.9), the
    // wrong base. DeepSeek V4.1 Flash 1017 was re-read in the same bump: AA
    // firmed its max-effort measurement at 45.1, past rounding vs the 40 it
    // entered at three days earlier.
    ("MiniMax M3.1 Flash", 43.0),
    ("Qwen3.8 Max (0902)", 45.4),
    ("inclusionAI: Ling 3.1 Flash", 41.0),
    ("Reflection: Beam 1.0 501B A23B", 30.0),
    ("Muse Spark Open", 39.0),
    ("GLM-5.3-FlashX", 41.9), // same checkpoint; see SAME_WEIGHTS_SKU_ALIASES (AA rates no FlashX page)
    // Added 2026-10-17 for the Surplus listings of 2026-10-15..10-17, read
    // from the still-v4.3.2 index (leader and scale unchanged). DeepSeek V4.1
    // Flash 1017 is the 1017 dated refresh of the V4.1 Flash family; like the
    // 0731 build of V4 Flash it keeps its release tag in the key, so the
    // undated family row (39.5, the 0910 build) stays separate. NVIDIA's
    // Nemotron 3.5 Super VL is the 3.5 generation's Super tier with native
    // vision input, listed the evening of 2026-10-15 — hours after that day's
    // read — and enters past the Nemotron 3 Ultra 550B (23.4) as NVIDIA's
    // strongest row on the index; `vl` is part of the model name, not a
    // deployment token, and the `nvidia` creator prefix strips so the
    // "NVIDIA: " Surplus display name joins exactly. Mistral Large 4's matrix
    // priced at $0.0000 (100%-off launch window) and joins its existing
    // Preview-read row above unchanged.
    ("DeepSeek V4.1 Flash 1017", 45.0), // re-read 2026-10-20: max page firmed at 45.1 (was 40.0 at entry)
    ("NVIDIA Nemotron 3.5 Super VL", 26.0), // re-read 2026-10-23: page firmed at 26 (was 25.0 at the 2026-10-17 entry)
    // Added 2026-10-15 for the Surplus listings of 2026-10-07..10-15, read
    // from the still-v4.3.2 index (leader and scale unchanged). Claude Haiku
    // 5.5 entered at 43 read at max effort — the same page configuration its
    // Sonnet/Opus 5.5 siblings use — putting the Haiku tier 26 points over
    // the 4.5 sibling (15.4), between GPT-5.6 Sol and GPT-5.6 Terra and above
    // GPT-6 Luna (37); its batch SKU is the same weights on the Batch API at
    // half the interactive price, so it inherits through the stripped
    // `batch` deployment token rather than a row of its own. Abliteration
    // AI's Abliterated Large V2 is GLM-5.3 with the refusal directions
    // ablated out plus a post-ablation finetune; AA rates no page, and
    // because the listing names the base checkpoint (unlike derivative builds
    // such as Ember-1) the row ~proxies GLM-5.3 — the app also flags the
    // listing as uncensored so it can be filtered without reading the proxy
    // as a measurement. Meta's Muse Wonder is the API-available half of the
    // Wonder launch (the stronger Full sibling, 56, has no public API): 52 at
    // max, #9 of 216. Mistral Large 4 — the trillion-parameter launch AA's
    // tweet nicknamed "Le Chonk" — is read off its Preview release
    // (2026-10-06, the only version rated): 38, twenty-eight over Mistral
    // Large 3 and the strongest non-US/China model on the index; the Surplus
    // price matrix is still unpriced, so the row lands ahead of purchasable
    // asks. Gemini 4 Argon's general listing (2026-10-14) and the "Meta:
    // Muse Spark 1.3" SKU join existing rows via creator-prefix stripping.
    ("Claude Haiku 5.5", 43.0),
    ("Abliterated Large V2", 44.9), // ~proxy: GLM-5.3 with refusal directions removed; AA rates no page
    ("Muse Wonder", 52.0),
    ("Mistral Large 4", 38.0), // Preview read: the only version AA rates
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
    // execution mode of the same weights (see SAME_WEIGHTS_SKU_ALIASES).
    // Mercury 2.5 is Inception's diffusion-LLM refresh of Mercury 2; the name
    // carries the `Inception: ` prefix so it exact-joins the Surplus display
    // name (`inception` is not a stripped creator prefix, same as Cohere).
    // Seed-2.0-Code has no AA page and is unrated — see the header notes.
    ("GPT-6.1 Sol", 52.0),
    ("GPT-6.1 Sol Pro", 52.0), // same weights as GPT-6.1 Sol; see SAME_WEIGHTS_SKU_ALIASES
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
    // weights (see SAME_WEIGHTS_SKU_ALIASES). Grok 4.7 has no max-effort
    // measurement, so its row is the xhigh run AA labels on the model page.
    // MiMo-V2.6-Pro is Xiaomi's new 1T flagship at AA 46; the UltraSpeed SKU
    // and the unrated V2.6-Flash are covered by the header notes.
    ("Claude Opus 5.5", 58.0),
    ("GPT-6 Sol", 48.0),
    ("GPT-6 Sol Pro", 48.0), // same weights as GPT-6 Sol; see SAME_WEIGHTS_SKU_ALIASES
    ("Grok 4.7", 46.0),
    ("MiMo-V2.6-Pro", 46.0),
    ("GPT-6 Luna", 37.0),
    ("GPT-6 Luna Pro", 37.0), // same weights as GPT-6 Luna; see SAME_WEIGHTS_SKU_ALIASES
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
    ("Muse Spark 1.3", 48.0), // re-read 2026-10-15: the Wonder launch re-measured max at 48 (was 45.2 v4.3)
    ("Meta: Muse Spark 1.3 Contributor", 48.0), // same weights; see SAME_WEIGHTS_SKU_ALIASES (AA rates no Contributor page)
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
    // Surplus SKUs read the same v4.3 measurement (see
    // SAME_WEIGHTS_SKU_ALIASES).
    ("DeepSeek R1 0528", 13.1),
    ("DeepSeek R1", 13.1),
    ("Qwen3.6 27B", 21.9),
    ("Qwen3.6 35B A3B", 18.8),
    ("Qwen3.5 35B A3B", 19.3),
    ("Qwen3 Next 80B A3B Instruct", 9.6),
    ("Qwen3 Coder Next", 10.1),
    ("Qwen3 Coder 480B", 11.9),
    ("Qwen3 Coder 480B Turbo", 11.9), // same weights served faster; see SAME_WEIGHTS_SKU_ALIASES
    ("Gemma 4 31B", 15.4),
    ("Gemma 4 26B A4B", 16.7),
    ("Claude 4.5 Haiku", 15.4),
    ("GPT-5.5 Instant", 26.8),
    ("gpt-oss-120b", 12.3),
    ("gpt-oss-20b", 9.0),
    ("Qwen3 235B A22B 2507", 12.0),
    ("Mistral Small 3.2", 7.0),
    ("Mistral Small 3.2 24B Instruct", 7.0), // same model, open-weights listing name; see SAME_WEIGHTS_SKU_ALIASES
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

/// Same-weights SKU aliases: `(SKU display name, base variant name, what the
/// SKU is)`. Rows Surplus sells as separate SKUs that are actually the SAME
/// weights as an already-rated variant — `reasoning.mode: "pro"` execution
/// modes, licensing tiers, same-checkpoint speed variants the key normalizer
/// cannot strip, and duplicate listing names. Each needs its own snapshot row
/// so Surplus prices join, but no leaderboard measures the SKU itself, so the
/// score is inherited from the base variant. The builder appends an
/// inheritance note to the rendered row name, and the composite's alias pass
/// (`build_agentic_composite`) resolves the SKU's canonical model key to the
/// base variant in every source so same-weights SKUs rate identically.
///
/// This is the mechanism for same-weights pairs whose canonical keys differ.
/// Wrappers the key normalizer strips (`prime`, `ultraspeed`, `batch`,
/// `e2ee`, `web`, `private`, …) collapse onto the base key by construction and
/// need no entry here. Same-checkpoint speed variants whose wrapper token
/// CANNOT be stripped because it would collapse onto the wrong base (FlashX
/// would fold onto GLM-5.3 instead of GLM-5.3-Flash) belong here instead.
pub(crate) const SAME_WEIGHTS_SKU_ALIASES: &[(&str, &str, &str)] = &[
    // `reasoning.mode: "pro"` execution modes of the same weights.
    ("GPT-6.1 Sol Pro", "GPT-6.1 Sol", "reasoning.mode: pro"),
    ("GPT-6 Sol Pro", "GPT-6 Sol", "reasoning.mode: pro"),
    ("GPT-6 Luna Pro", "GPT-6 Luna", "reasoning.mode: pro"),
    ("GPT-5.6 Sol Pro", "GPT-5.6 Sol", "reasoning.mode: pro"),
    ("GPT-5.6 Terra Pro", "GPT-5.6 Terra", "reasoning.mode: pro"),
    ("GPT-5.6 Luna Pro", "GPT-5.6 Luna", "reasoning.mode: pro"),
    // Added 2026-10-23 after the Muse Spark 1.3 regression: same-weights SKUs
    // whose keys differ from the base must inherit the base's live board
    // evidence, or the composite rates the same checkpoint twice — the
    // Contributor SKU sat at its AA prior (85.4) while the base joined the
    // LiveBench 2.0 boards and landed at 70.9. FlashX is the same-checkpoint
    // speed variant that cannot be a stripped token (see the header notes).
    // R1 0528 is the refresh AA folded into the base DeepSeek R1 page, so both
    // Surplus SKUs serve the same weights. Mistral Small 3.2 24B Instruct is
    // the open-weights listing name of Mistral Small 3.2, and Qwen3 Coder
    // 480B Turbo is the 480B checkpoint served faster (same weights, like the
    // prime/ultraspeed wrappers).
    (
        "Meta: Muse Spark 1.3 Contributor",
        "Muse Spark 1.3",
        "contributor-licensed SKU of the same weights",
    ),
    // Found 2026-10-23 in the same audit's Surplus catalog sweep: the 1.2
    // generation's Contributor SKU had no snapshot row, so it joined the 1.2
    // rows only through the 0.80 fuzzy bar.
    (
        "Meta: Muse Spark 1.2 Contributor",
        "Muse Spark 1.2",
        "contributor-licensed SKU of the same weights",
    ),
    (
        "GLM-5.3-FlashX",
        "GLM-5.3-Flash",
        "same checkpoint at up to 200 tok/s",
    ),
    (
        "DeepSeek R1 0528",
        "DeepSeek R1",
        "refresh AA folded into the base page",
    ),
    (
        "Mistral Small 3.2 24B Instruct",
        "Mistral Small 3.2",
        "open-weights listing name of the same model",
    ),
    (
        "Qwen3 Coder 480B Turbo",
        "Qwen3 Coder 480B",
        "same weights served faster",
    ),
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
            let inherited = SAME_WEIGHTS_SKU_ALIASES.iter()
                .find(|(sku, _, _)| *sku == *model)
                .map(|(_, base, what)| (*base, *what));
            row.name = match inherited {
                Some((base, what)) => format!(
                    "{} [AA Intelligence {} · snapshot {} · inherited from {} — {}; same weights, not separately benchmarked]",
                    model,
                    ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION,
                    ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE,
                    base,
                    what,
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
    fn same_weights_alias_pairs_reference_real_rows_and_disclose_in_name() {
        let names: std::collections::HashSet<_> =
            SNAPSHOT_ROWS.iter().map(|(model, _)| *model).collect();
        assert!(!SAME_WEIGHTS_SKU_ALIASES.is_empty());
        for (sku, base, _) in SAME_WEIGHTS_SKU_ALIASES {
            assert!(names.contains(sku), "pair references unknown SKU `{sku}`");
            assert!(
                names.contains(base),
                "pair references unknown base variant `{base}`"
            );
            assert_ne!(
                benchmark_model_key(sku),
                benchmark_model_key(base),
                "{sku} must normalize differently from {base} (equal keys need no alias: \
                 the wrapper token should strip instead)"
            );
        }
        let rows = artificial_analysis_snapshot();
        for (sku, base, _) in SAME_WEIGHTS_SKU_ALIASES {
            let row = rows
                .iter()
                .find(|row| benchmark_model_key(&row.slug) == benchmark_model_key(sku))
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
            // Re-read 2026-10-15 (see the refresh test): the Wonder launch
            // moved the max-effort page from the 45.2 v4.3 read to 48.
            ("Muse Spark 1.3", 48.0),
            ("Gemini 3.8 Flash", 41.2),
            ("DeepSeek V4.1 Flash", 39.5),
            ("Meta: Muse Spark 1.3 Contributor", 48.0),
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
        // stay distinct keys resolved through SAME_WEIGHTS_SKU_ALIASES.
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
    fn aa_snapshot_covers_the_2026_10_15_refresh() {
        // Rows added 2026-10-15 under the still-v4.3.2 index for the Surplus
        // listings of 2026-10-07..10-15: Claude Haiku 5.5 (read at max, with
        // the batch SKU inheriting through the stripped `batch` token),
        // Abliterated Large V2 (~proxied onto GLM-5.3 because the listing
        // names the base checkpoint even though AA rates no page), Muse
        // Wonder (the API-available half of the Wonder launch), and Mistral
        // Large 4 (read off the Preview release, the only version rated).
        // Muse Spark 1.3 was re-measured at max for the Wonder launch, so its
        // rows moved 45.2 -> 48 without collapsing onto Spark 1.2 (39.8).
        let rows = artificial_analysis_snapshot();
        let score_of = |name: &str| {
            rows.iter()
                .find(|b| benchmark_model_key(&b.slug) == benchmark_model_key(name))
                .and_then(|b| b.agentic_coding)
        };
        for (name, expected) in [
            ("Claude Haiku 5.5", 43.0),
            ("Abliterated Large V2", 44.9),
            ("Muse Wonder", 52.0),
            ("Mistral Large 4", 38.0),
            ("Muse Spark 1.3", 48.0),
            ("Meta: Muse Spark 1.3 Contributor", 48.0),
        ] {
            assert_eq!(
                score_of(name),
                Some(expected),
                "{name} missing or wrong in snapshot"
            );
        }
        // The batch SKU inherits the base row via key equality, not a row.
        assert_eq!(
            benchmark_model_key("Claude Haiku 5.5 (batch)"),
            benchmark_model_key("Claude Haiku 5.5")
        );
        // The abliterated build keeps its own key: the GLM-5.3 proxy is
        // disclosed, but the SKU never joins the GLM row itself.
        assert_ne!(
            benchmark_model_key("Abliterated Large V2"),
            benchmark_model_key("GLM-5.3")
        );
        assert_eq!(
            score_of("GLM-5.3"),
            Some(44.9),
            "the proxy source row itself is unchanged"
        );
        // Family keys stay distinct where versions must not bleed.
        assert_ne!(
            benchmark_model_key("Mistral Large 4"),
            benchmark_model_key("Mistral Large 3")
        );
        assert_ne!(
            benchmark_model_key("Muse Wonder"),
            benchmark_model_key("Muse Spark 1.3")
        );
        assert_ne!(
            benchmark_model_key("Claude Haiku 5.5"),
            benchmark_model_key("Claude 4.5 Haiku")
        );
        // Muse Wonder Full (56) has no public API and no Surplus listing, so
        // it must not have a row.
        assert!(
            score_of("Muse Wonder Full").is_none(),
            "Muse Wonder Full needs no row"
        );
    }

    #[test]
    fn aa_snapshot_covers_the_2026_10_17_refresh() {
        // Rows added 2026-10-17 under the still-v4.3.2 index for the Surplus
        // listings of 2026-10-15..10-17: DeepSeek V4.1 Flash 1017 (the dated
        // refresh, a point over the undated family row's 39.5 v4.3 read) and
        // NVIDIA's Nemotron 3.5 Super VL (the 3.5-gen Super tier, past the
        // Nemotron 3 Ultra 550B as NVIDIA's strongest row). Mistral Large 4's
        // listing priced its matrix at $0.0000 — a 100%-off launch window —
        // and must join the existing Preview-read row unchanged rather than
        // getting a row of its own.
        let rows = artificial_analysis_snapshot();
        let score_of = |name: &str| {
            rows.iter()
                .find(|b| benchmark_model_key(&b.slug) == benchmark_model_key(name))
                .and_then(|b| b.agentic_coding)
        };
        // The Surplus display name carries the "NVIDIA: " creator prefix the
        // key strips; assert through it so the exact join is what's covered.
        // The 1017 entered at 40 and was re-read to 45 on 2026-10-20, and the
        // Nemotron row was re-read to 26 on 2026-10-23 (see those days' tests);
        // this test pins the family separations, not the entry values.
        for (name, expected) in [
            ("DeepSeek V4.1 Flash 1017", 45.0),
            ("NVIDIA: Nemotron 3.5 Super VL", 26.0),
            ("Mistral Large 4", 38.0),
            ("DeepSeek V4.1 Flash", 39.5),
        ] {
            assert_eq!(
                score_of(name),
                Some(expected),
                "{name} missing or wrong in snapshot"
            );
        }
        // The dated refresh keeps its release tag, so it never collapses onto
        // the undated family row (or the previous generation's 0731 build).
        assert_ne!(
            benchmark_model_key("DeepSeek V4.1 Flash 1017"),
            benchmark_model_key("DeepSeek V4.1 Flash"),
            "the 1017 refresh must not collapse onto the undated family row"
        );
        assert_ne!(
            benchmark_model_key("DeepSeek V4.1 Flash 1017"),
            benchmark_model_key("DeepSeek V4 Flash 0731"),
            "the 1017 refresh must not collapse onto the V4 generation"
        );
        // The Super tier stays distinct from both rated Nemotron siblings;
        // `vl` names the model, so it survives key normalization.
        assert_ne!(
            benchmark_model_key("NVIDIA: Nemotron 3.5 Super VL"),
            benchmark_model_key("NVIDIA Nemotron 3.5 Lightning 30B"),
            "Super VL must not collapse onto the Lightning tier"
        );
        assert_ne!(
            benchmark_model_key("NVIDIA: Nemotron 3.5 Super VL"),
            benchmark_model_key("NVIDIA Nemotron 3 Super 120B"),
            "the 3.5 generation must not collapse onto the 3 generation"
        );
    }

    #[test]
    fn aa_snapshot_covers_the_2026_10_23_refresh() {
        // Rows added 2026-10-23 under the still-v4.3.2 index for AA's
        // 2026-10-22 release: Ling 3.1 Flash 1019 (the dated refresh, three
        // over the undated build's 41) lands ahead of its Surplus listing,
        // and Muse Spark 1.2 Contributor (the contributor-licensed SKU of the
        // 1.2 weights, listed on Surplus) carries the 1.2 measurement and
        // joins through SAME_WEIGHTS_SKU_ALIASES instead of the fuzzy bar.
        // Nemotron 3.5 Super VL was re-read 25 -> 26 in the same bump.
        let rows = artificial_analysis_snapshot();
        let score_of = |name: &str| {
            rows.iter()
                .find(|b| benchmark_model_key(&b.slug) == benchmark_model_key(name))
                .and_then(|b| b.agentic_coding)
        };
        for (name, expected) in [
            ("inclusionAI: Ling 3.1 Flash 1019", 44.0),
            ("inclusionAI: Ling 3.1 Flash", 41.0),
            ("Meta: Muse Spark 1.2 Contributor", 39.8),
            ("Muse Spark 1.2", 39.8),
            ("NVIDIA: Nemotron 3.5 Super VL", 26.0),
        ] {
            assert_eq!(
                score_of(name),
                Some(expected),
                "{name} missing or wrong in snapshot"
            );
        }
        // The dated refresh keeps its release tag, so it never collapses onto
        // the undated family row.
        assert_ne!(
            benchmark_model_key("inclusionAI: Ling 3.1 Flash 1019"),
            benchmark_model_key("inclusionAI: Ling 3.1 Flash"),
            "the 1019 refresh must not collapse onto the undated family row"
        );
        // The 1.2 Contributor SKU joins the 1.2 rows through the alias table,
        // not a fuzzy accident.
        assert!(
            SAME_WEIGHTS_SKU_ALIASES
                .iter()
                .any(|(sku, base, _)| sku == &"Meta: Muse Spark 1.2 Contributor"
                    && base == &"Muse Spark 1.2"),
            "Muse Spark 1.2 Contributor must be registered in SAME_WEIGHTS_SKU_ALIASES"
        );
    }

    #[test]
    fn aa_snapshot_covers_the_2026_10_20_refresh() {
        // Rows added 2026-10-20 under the still-v4.3.2 index for the Surplus
        // listings of 2026-10-18..10-20: MiniMax M3.1 Flash (43, MiniMax's
        // strongest row), Qwen3.8 Max (0902) (reading the canonical page AA
        // re-pointed at the 0902 snapshot; same 45.4 the undated row carries),
        // inclusionAI's Ling 3.1 Flash (41), Reflection's Beam 1.0 (30), and
        // Meta's Muse Spark Open (39 at xhigh, the only measured effort).
        // GLM 5.3 FlashX ~proxies GLM-5.3-Flash (same checkpoint at speed)
        // because stripping `flashx` would collapse onto GLM-5.3, the wrong
        // base. DeepSeek V4.1 Flash 1017 was re-read 40 -> 45 when AA firmed
        // its max measurement. Qwen3.8 Max Prime inherits via the stripped
        // `prime` token rather than a row, and the unrated 2026-10-18
        // listings (Qwen3.7 Flash, LFM2.5-24B-A2B, Moondream 3 Preview/3.1,
        // Pareto 26.9, Elorian Glimpse) must have no rows.
        let rows = artificial_analysis_snapshot();
        let score_of = |name: &str| {
            rows.iter()
                .find(|b| benchmark_model_key(&b.slug) == benchmark_model_key(name))
                .and_then(|b| b.agentic_coding)
        };
        for (name, expected) in [
            ("MiniMax M3.1 Flash", 43.0),
            ("Qwen3.8 Max (0902)", 45.4),
            ("inclusionAI: Ling 3.1 Flash", 41.0),
            ("Reflection: Beam 1.0 501B A23B", 30.0),
            ("Muse Spark Open", 39.0),
            ("GLM-5.3-FlashX", 41.9),
            ("DeepSeek V4.1 Flash 1017", 45.0),
            ("Qwen3.8 Max", 45.4),
        ] {
            assert_eq!(
                score_of(name),
                Some(expected),
                "{name} missing or wrong in snapshot"
            );
        }
        // The dated Max reads the same canonical page as the undated row but
        // keeps its own key; Prime collapses onto the undated key through the
        // stripped `prime` token and inherits 45.4, not a row of its own.
        assert_ne!(
            benchmark_model_key("Qwen3.8 Max (0902)"),
            benchmark_model_key("Qwen3.8 Max"),
            "the dated snapshot must not collapse onto the undated row"
        );
        assert_eq!(
            benchmark_model_key("Qwen3.8 Max Prime"),
            benchmark_model_key("Qwen3.8 Max"),
            "the higher-throughput Prime SKU inherits the base row"
        );
        // FlashX keeps its own key (distinct from both GLM bases) and the
        // proxy source row itself is unchanged.
        assert_ne!(
            benchmark_model_key("GLM-5.3-FlashX"),
            benchmark_model_key("GLM-5.3"),
            "FlashX must not collapse onto GLM-5.3 (the wrong base)"
        );
        assert_ne!(
            benchmark_model_key("GLM-5.3-FlashX"),
            benchmark_model_key("GLM-5.3-Flash"),
            "FlashX keeps its own key; the proxy is disclosed, not joined"
        );
        assert_eq!(
            score_of("GLM-5.3-Flash"),
            Some(41.9),
            "the proxy source row itself is unchanged"
        );
        // Muse Spark Open stays distinct from the purchasable Spark 1.3.
        assert_ne!(
            benchmark_model_key("Muse Spark Open"),
            benchmark_model_key("Muse Spark 1.3")
        );
        // The unrated 2026-10-18 listings have no rows and no accidental
        // fuzzy-join partners.
        for unrated in [
            "Qwen3.7 Flash",
            "LFM2.5-24B-A2B",
            "Moondream 3 Preview",
            "Moondream 3.1",
            "Pareto 26.9",
            "Glimpse",
        ] {
            assert!(
                score_of(unrated).is_none(),
                "{unrated} is unrated and must not have a row"
            );
        }
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
