//! Bundled Artificial Analysis Intelligence Index snapshot.
//!
//! This data lives apart from `main.rs` so the table can be bumped without
//! touching any fetch/parse code. Since 0.21.0 it is the OFFLINE BASE of a
//! hybrid: `fetch::aa_index` reads the public leaderboard page (no API key)
//! and overlays live values for every current model onto these rows, while
//! everything the live payload cannot see — legacy families, ~proxy rows,
//! AA "~est" values — keeps the bundled read below. The composite still
//! treats the source as a broad capability prior either way; row names
//! disclose which read they carry ("live <date>" vs "snapshot <date>").
//!
//! # Keeping the snapshot up to date
//!
//! The live overlay automates value freshness for current models, so manual
//! bumps now maintain what automation cannot: legacy-row re-reads, new-family
//! curation (proxy/alias/unrated policy), and the calibration anchors.
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
//! A second 2026-10-23 bump the same day did the first full-table re-read
//! since the v4.3 rebaseline: every row was fetched from its live model page
//! and compared against the stored value. The index stayed v4.3.2 and the
//! leader (Claude Opus 5.5, 58) is unchanged, so `AA_CALIBRATION_POINTS`
//! keeps its anchors. The leaderboard itself gained a Status filter defaulting
//! to Current (133 ranked): older models are hidden from the table but their
//! pages still live — GPT-6 Sol re-read unchanged at 48 (AA marks it "no
//! publicly available API"; its new "Daybreak Blue" variant is unevaluated
//! and has no Surplus listing, so no row). Thirty-four families had drifted
//! past rounding since their reads (mostly a single point: GPT-6 Luna
//! 37 → 38, MiniMax-M3 29.6 → 29, Qwen 3.7 Plus and Hy3 25.8 → 25, Qwen3.5
//! 397B A17B 19.1 → 18, Nemotron 3.5 Lightning 13.6 → 13, …) and moved to
//! their live integer displays. Three families moved for structural reasons:
//! AA folded the DeepSeek V4 Flash and V4 Pro canonical pages onto their
//! dated refresh builds (the undated pages now measure 0731/0813, the same
//! fold R1 0528 got, so both rows per family read one page — V4 Flash
//! 24.6 → 34, V4 Pro 30.9 → 36, and the dated rows re-read 34/36); Grok 4.1
//! Fast's reasoning evaluation is gone from AA (only the non-reasoning run
//! remains), so the row falls back to 11 per the no-reasoning-run policy;
//! and Qwen3 VL 235B A22B lost its reasoning page, reading the instruct-only
//! 10, while the Nemotron Nano 12B v2 row does the same at 6. Three rows
//! joined: DeepSeek V4.1 Flash Uncensored (listed 2026-10-21, dealignai's
//! uncensored finetune of the 1017 build — the listing names the base
//! checkpoint, so it ~proxies the 1017 row the way Abliterated Large V2
//! proxies GLM-5.3, and the app's uncensored flag applies), MiMo-V2.6-Flash
//! (AA finally published the Flash page: 38, eight under the Pro — it leaves
//! the unrated list), and Thinking Machines: Inkling (a Surplus listing
//! since 2026-07-24 — an auto-proposed SKU with live asks — that AA now
//! rates 25 at xhigh, the only effort measured; Inkling Small (26) is a
//! separate SKU Surplus does not list, so it gets no row). AA REMOVED the
//! undated `qwen3-8-flash` page (404; only the distinct Aug-26
//! `qwen3-8-flash-next` preview remains rated), so the Qwen3.8 Flash row and
//! the Qwen3.8 Omni Flash row that ~proxied it lost their rows rather than
//! keeping stale reads — the GLM-4.5 precedent — and the undated Surplus
//! SKUs fall back to the neutral prior. GLM-4.7-Thinking's page was folded
//! into the base GLM-4.7 page, so the SKU now inherits through
//! [`SAME_WEIGHTS_SKU_ALIASES`] exactly like DeepSeek R1 0528. Four pages
//! were retitled onto dated/renamed builds with unchanged values, verified
//! rather than moved: Gemini 3 Flash Preview → the GA reasoning page (26),
//! MiMo-V2.5 → its dated 0424 page, Grok Build 0.1 → 0616, and Grok 4.20
//! Beta → the 0309 v2 build.
//!
//! The 2026-10-24 Kimi K3.1 Pro launch bump stayed on v4.3.2: the leader
//! (Claude Opus 5.5, 58) and the scale are unchanged, so `AA_CALIBRATION_POINTS`
//! keeps its anchors. Moonshot's post-trained successor to Kimi K3 (2.8T total
//! / 104B active MoE, 1M context, text+image+video in) entered at 55, #4 on
//! the index — 11 over the K3 (max) row and trailing only Opus 5.5 (58),
//! Sonnet 5.5 (56), and Muse Wonder Full (56, no API). It is the leading
//! open-weights measurement the moment the weights land (promised by
//! 2026-11-04), narrowing the open-vs-proprietary gap from 12 points to 3.
//! The gains are agentic-led: #3 on GDPval-AA (1,846 Elo, +312 over K3),
//! Terminal-Bench 4.0 at 67% — the highest of any model, up from K3's 13 —
//! AutomationBench-AA 58% → 73%, HLE 47% → 52%, while AA-Omniscience went
//! 20 → 30 with hallucinations 53% → 44%. The launch was OpenRouter-first:
//! `moonshotai/kimi-k3.1-pro` listed at the first-party $2/$10/$0.20 per 1M
//! in/out/cache-hit rates, 33% under K3 on all three, holding K3's $1.98 Cost
//! per Task (55K output tokens per task, 14% more than K3's 48K). The row
//! therefore lands ahead of its Surplus listing (the Gemini 4 Argon pattern);
//! K3.1 Pro Highspeed (2-3x faster) is the same checkpoint and inherits
//! through the stripped `highspeed` token.
//!
//! The 2026-10-28 bump moved the snapshot to v5.0, AA's first major
//! rebaseline since v4.3: FrontierCode 2.0 and Terminal-Bench-Science joined
//! the index, HLE-Diamond replaced Humanity's Last Exam, AA-Omniscience and
//! CritPt were upgraded to v1.1/v2, GDPval-AA/SciCode/AA-LCR were removed,
//! and 60% of the index now runs on private test sets or held-out answers.
//! The scale re-compressed below the leader: Claude Opus 5.5 stayed at 58,
//! but the open-weights frontier fell hard (Kimi K3.1 Pro 55 → 46, GLM-5.3
//! 44.9 → 32, DeepSeek V4.1 Flash 1017 45 → 32, Qwen3.8 Max 0902 45.4 → 34),
//! widening AA's open-vs-proprietary gap from 3 to 12 points.
//! `AA_CALIBRATION_POINTS` was recalibrated in the same change. v5 publishes
//! scores for only ~75 models so far: every family with a v5 measurement was
//! re-read from the full leaderboard (integer displays; the live payload
//! keeps one-decimal precision underneath), while families the v5 board
//! shows as dashes — or hides entirely behind null model pages, spot-checked
//! on Kimi K3 and GPT-6 Sol — keep their last v4.3.2-scale read until AA
//! publishes (the live overlay replaces them automatically as it does), so
//! the snapshot is deliberately mixed-scale below the v5-rated frontier.
//! GPT-6.1 Terra — OpenAI's new cost-efficient tier, the cheapest top-10
//! model at $0.96 per index task — entered at 51 (max), between GPT-6.1 Sol
//! (55) and Gemini 4 Argon (50), ahead of its Surplus listing (the Gemini 4
//! Argon pattern). Grok 4.7 re-read at xhigh (34): v5 also measures a high
//! run (35), but xhigh stays the row's effort per the no-max policy. Muse
//! Wonder Full (50), Step 5 Preview (29), and K2 Horizon (18) joined the
//! rated set with no Surplus listing, so they still get no rows.
//!
//! Deliberately unrated (no AA presence as of 2026-10-23): Aion 2.0/3.0/3.0
//! Mini, Hermes 3 405B (AA rates only the Hermes 4 generation), Palmyra
//! Vision 7B, Qwen 2.5 7B, GPT-OSS Safeguard
//! 20B/120B, Grok 4.20 Multi-Agent Beta, Venice rebrands, uncensored finetunes
//! such as GLM 4.7 Flash Heretic (Abliterated Large V2 and DeepSeek V4.1
//! Flash Uncensored above are the lone exceptions — their listings name the
//! base checkpoint, so they proxy instead of sitting unrated), Seed 2.1
//! Turbo and Seed-2.0-Code, the 2026-09-11 Sakana
//! listings (Fugu Max, Fugu Ultra v2), TypeSafe's JEV 1.13 decisions model,
//! and Fireworks Ember-1 (a Kimi-K3-derived reasoning model; AA has no page,
//! and it deliberately does NOT proxy Kimi K3 — it is a derivative build, not
//! the same checkpoint). The 2026-10-18 listings join them: Qwen3.7 Flash (AA
//! rates the 3.7 Plus/Max tiers but has no Flash page, and no same-checkpoint
//! sibling to proxy), LiquidAI's LFM2.5-24B-A2B (AA's LFM2.5 pages cover only
//! the 1.2B–8B small models; the 24B-A2B is not the LFM2 24B A2B checkpoint),
//! Moondream 3 Preview and Moondream 3.1 (vision-language; no AA pages),
//! Pareto 26.9 (no AA page), and Elorian's Glimpse (no AA page). The undated
//! Qwen3.8 Flash and Qwen3.8 Omni Flash SKUs joined them on 2026-10-23 when
//! AA removed the undated `qwen3-8-flash` page: only the distinct Aug-26
//! `qwen3-8-flash-next` preview remains rated, and a different build is not
//! a proxy source.
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

pub(crate) const ARTIFICIAL_ANALYSIS_SNAPSHOT_DATE: &str = "2026-10-28";
pub(crate) const ARTIFICIAL_ANALYSIS_SNAPSHOT_VERSION: &str = "v5.0";

/// `(model name, AA Intelligence score)` pairs, one representative row per
/// family. Names should mirror Surplus display names so exact-key joins hit
/// before fuzzy matching has to.
pub(crate) const SNAPSHOT_ROWS: &[(&str, f64)] = &[
    // Added 2026-10-28 for the v5.0 rebaseline (see the header notes).
    // GPT-6.1 Terra is OpenAI's new cost-efficient tier under GPT-6.1 Sol —
    // the cheapest top-10 model on the index at $0.96 per task — read at the
    // max effort the GPT-6 Surplus SKUs expose via `reasoning_effort`. It
    // enters at 51, between Sol (55) and Gemini 4 Argon (50), ahead of its
    // Surplus listing (the Gemini 4 Argon pattern); `terra` is a tier name,
    // not a deployment token, so the key joins nothing else. Kimi K3.1 Pro
    // re-read 55 → 46: the v5 scale pushed the open-weights frontier down
    // (now 12 points under the leader, up from 3), and its agentic gains
    // (Terminal-Bench 4.0 67%, AA-Briefcase 1,846 Elo) live on live boards.
    ("GPT-6.1 Terra", 51.0),
    ("Kimi K3.1 Pro", 46.0), // re-read 2026-10-28 (v5.0): was 55.0 at the 2026-10-24 entry
    // Added in the second 2026-10-23 bump (the full-table re-read; see the
    // header notes). DeepSeek V4.1 Flash Uncensored is dealignai's uncensored
    // finetune of the 1017 build (Surplus listed it 2026-10-21; "origin
    // Experiential Labs", FP8); AA rates no page, but unlike most uncensored
    // builds the listing names its base checkpoint, so the row ~proxies the
    // 1017 measurement exactly the way Abliterated Large V2 ~proxies GLM-5.3
    // — and the app's uncensored flag marks the listing independently of the
    // proxy. MiMo-V2.6-Flash finally got its AA page (the unrated note had
    // been waiting for it); re-read under v5.0 alongside its Pro sibling.
    // Inkling is Thinking Machines' model, listed on Surplus since
    // 2026-07-24 as an auto-proposed SKU ("from openrouter, venice. Awaiting
    // review.") that nonetheless carries live asks; AA rates it 25 at xhigh,
    // the only effort measured, so the row reads that. Inkling Small (26 →
    // 13 under v5.0) is a separate SKU with no Surplus listing and gets no
    // row.
    ("DeepSeek V4.1 Flash Uncensored", 32.0), // ~proxy: the 1017 build with refusals removed; AA rates no page
    ("MiMo-V2.6-Flash", 27.0),
    ("Thinking Machines: Inkling", 12.0), // xhigh, the only effort AA measured; `thinking machines` is not a stripped creator prefix
    // Added 2026-10-23 for AA's 2026-10-22 release; re-read under v5.0
    // (2026-10-28). Ling 3.1 Flash 1019 is the 1019 dated refresh of
    // inclusionAI's Flash family; like the other dated refreshes the release
    // tag stays in the key so the undated row keeps its own measurement, and
    // the v5 board's short name aliases onto this row in
    // `alias_display_name`. Muse Spark 1.2 Contributor is the
    // contributor-licensed SKU of the 1.2 weights (Surplus lists
    // `muse-spark-1.2-contributor`); AA rates no page of its own, so the row
    // carries the 1.2 measurement and SAME_WEIGHTS_SKU_ALIASES passes board
    // evidence through — until now the SKU joined the 1.2 rows only through
    // the 0.80 fuzzy bar, one rename away from the 1.3 Contributor split.
    // Both rows keep their v4.3.2 reads: v5.0 has no Muse Spark measurement.
    ("inclusionAI: Ling 3.1 Flash 1019", 29.0),
    ("Meta: Muse Spark 1.2 Contributor", 39.8), // same weights; see SAME_WEIGHTS_SKU_ALIASES
    // Added 2026-10-20 for the Surplus listings of 2026-10-18..10-20;
    // re-read 2026-10-28 under v5.0 except where noted. MiniMax M3.1 Flash
    // re-read 43 → 29 (single measurement). Qwen3.8 Max (0902) reads the
    // canonical page AA re-pointed at the 0902 snapshot; re-read 45.4 → 34.
    // The old undated 0803 build is deprecated and Surplus delisted the
    // undated SKU, but the undated row below stays so undated leaderboard
    // slugs keep joining. Qwen3.8 Max Prime (higher-throughput
    // same-checkpoint SKU) inherits through the stripped `prime` token, not a
    // row. Ling 3.1 Flash carries the `inclusionAI: ` display prefix because
    // that creator is not stripped (same as Cohere/Inception); its v5 board
    // short name aliases onto the row in `alias_display_name`. Beam 1.0 is
    // Reflection's open-weights 501B A23B reasoning model; re-read 30 → 18.
    // Muse Spark Open is read at xhigh, the only effort AA measured for the
    // open-weights coding SKU; re-read 39 → 25. GLM 5.3 FlashX ~proxies
    // GLM-5.3-Flash: it is the same checkpoint served at up to 200 tok/s,
    // but `flashx` cannot be a stripped deployment token because that would
    // collapse the key onto GLM-5.3, the wrong base — and GLM-5.3-Flash has
    // no v5 measurement, so both rows keep their v4.3.2 reads.
    ("MiniMax M3.1 Flash", 29.0),
    ("Qwen3.8 Max (0902)", 34.0),
    ("inclusionAI: Ling 3.1 Flash", 41.0), // v5.0 shows the undated build as a dash; read retained
    ("Reflection: Beam 1.0 501B A23B", 18.0),
    ("Muse Spark Open", 25.0),
    ("GLM-5.3-FlashX", 41.9), // same checkpoint; see SAME_WEIGHTS_SKU_ALIASES (AA rates no FlashX page; v5.0 dash on Flash)
    // Added 2026-10-17 for the Surplus listings of 2026-10-15..10-17;
    // re-read 2026-10-28 under v5.0. DeepSeek V4.1 Flash 1017 is the 1017
    // dated refresh of the V4.1 Flash family; like the 0731 build of V4
    // Flash it keeps its release tag in the key, so the undated family row
    // (the 0910 build) stays separate — and keeps its v4.3.2 read, since the
    // undated page is a v5 dash. NVIDIA's Nemotron 3.5 Super VL is the 3.5
    // generation's Super tier with native vision input; re-read 26 → 14.
    // Mistral Large 4's listing priced its matrix at $0.0000 (100%-off
    // launch window) and joins its Preview-read row below.
    ("DeepSeek V4.1 Flash 1017", 32.0),
    ("NVIDIA Nemotron 3.5 Super VL", 14.0),
    // Added 2026-10-15 for the Surplus listings of 2026-10-07..10-15;
    // re-read 2026-10-28 under v5.0. Claude Haiku 5.5 re-read 43 → 36 (max —
    // the same page configuration its Sonnet/Opus 5.5 siblings use); its
    // batch SKU is the same weights on the Batch API at half the interactive
    // price, so it inherits through the stripped `batch` deployment token
    // rather than a row of its own. Abliteration AI's Abliterated Large V2
    // is GLM-5.3 with the refusal directions ablated out plus a post-ablation
    // finetune; AA rates no page, and because the listing names the base
    // checkpoint (unlike derivative builds such as Ember-1) the row ~proxies
    // GLM-5.3's v5 read — the app also flags the listing as uncensored so it
    // can be filtered without reading the proxy as a measurement. Meta's
    // Muse Wonder re-read 52 → 43 (max; the stronger Full sibling, 50, has
    // no public API). Mistral Large 4 is read off its Preview release
    // (2026-10-06, the only version AA rates); re-read 38 → 25. Gemini 4
    // Argon's general listing (2026-10-14) and the "Meta: Muse Spark 1.3"
    // SKU join existing rows via creator-prefix stripping; Muse Spark 1.3
    // keeps its v4.3.2 read (no v5 measurement yet).
    ("Claude Haiku 5.5", 36.0),
    ("Abliterated Large V2", 32.0), // ~proxy: GLM-5.3 with refusal directions removed; AA rates no page
    ("Muse Wonder", 43.0),
    ("Mistral Large 4", 25.0), // Preview read: the only version AA rates
    // Added 2026-09-30 (second same-day bump), re-read 2026-10-28 under
    // v5.0. Gemini 4 Argon re-read 53 → 50 at high reasoning — still the
    // highest effort Argon exposes. `argon` is a codename, not a stripped
    // deployment token, so the key joins nothing else.
    ("Gemini 4 Argon", 50.0),
    // Added 2026-09-30 for the Surplus listings of 2026-09-29..09-30;
    // re-read 2026-10-28 under v5.0. GPT-6.1 Sol re-read 52 → 55 (max
    // effort) — the v5 scale now places it alongside GPT-6 Astra's 57, well
    // clear of GPT-6 Sol's retained 48. Its Pro SKU is a
    // `reasoning.mode: "pro"` execution mode of the same weights (see
    // SAME_WEIGHTS_SKU_ALIASES). Mercury 2.5 is Inception's diffusion-LLM
    // refresh of Mercury 2; the name carries the `Inception: ` prefix so it
    // exact-joins the Surplus display name (`inception` is not a stripped
    // creator prefix, same as Cohere); its v5 read is a dash, so the
    // v4.3.2 read is retained. Seed-2.0-Code has no AA page and is unrated —
    // see the header notes.
    ("GPT-6.1 Sol", 55.0),
    ("GPT-6.1 Sol Pro", 55.0), // same weights as GPT-6.1 Sol; see SAME_WEIGHTS_SKU_ALIASES
    ("Inception: Mercury 2.5", 12.0),
    // Added 2026-09-29 for the Surplus listings of 2026-09-23..09-29;
    // re-read 2026-10-28 under v5.0. Claude Sonnet 5.5 re-read 56 → 54
    // (adaptive reasoning at max effort with default fallback — the same
    // page configuration Opus 5.5 was read at). Command A+ re-read 13 → 6.
    // The Qwen3.8 Omni Flash row that ~proxied its text sibling was removed
    // on 2026-10-23 when AA deleted the sibling's page.
    ("Claude Sonnet 5.5", 54.0),
    ("Cohere: Command A+", 6.0),
    // Added 2026-09-22 for the Surplus listings of 2026-09-20..09-22; the
    // v5.0 rebaseline re-read these rows where AA measures them. Claude Opus
    // 5.5 stayed the AA leader at 58 through the v5.0 rebaseline. GPT-6 Sol
    // has no v5 measurement (its page reads null; AA marks it "no publicly
    // available API"), so its row keeps the v4.3.2 read. Grok 4.7 re-read
    // 46 → 34 at xhigh, the best effort it exposes (v5 also measures a high
    // run at 35; the row stays xhigh per the no-max policy). MiMo-V2.6-Pro
    // re-read 46 → 30; the UltraSpeed SKU and the V2.6-Flash row are covered
    // by the header notes. GPT-6 Luna re-read 38 → 28.
    ("Claude Opus 5.5", 58.0),
    ("GPT-6 Sol", 48.0),     // v5.0 dash (page reads null); v4.3.2 read retained
    ("GPT-6 Sol Pro", 48.0), // same weights as GPT-6 Sol; see SAME_WEIGHTS_SKU_ALIASES
    ("Grok 4.7", 34.0),      // xhigh; re-read 2026-10-28 (v5.0): was 46.0
    ("MiMo-V2.6-Pro", 30.0),
    ("GPT-6 Luna", 28.0),     // re-read 2026-10-28 (v5.0): was 38.0
    ("GPT-6 Luna Pro", 28.0), // same weights as GPT-6 Luna; see SAME_WEIGHTS_SKU_ALIASES
    // Added 2026-09-19 for the Surplus listings of 2026-09-03..09-11;
    // re-read 2026-10-28 under v5.0 where AA measures the family. GPT-6
    // Astra re-read 52.8 → 57 (max), closing to one point under the leader.
    // Claude Fable 5.1 re-read 53.4 → 49 (max with fallback). DeepSeek V4.1
    // Flash is the 09-10 refresh (no v5 measurement; read retained). Muse
    // Spark 1.3 Contributor is Meta's contributor-licensed SKU of the 1.3
    // weights, which AA does not rate separately, so it ~proxies the base
    // row's retained read like its base.
    ("Claude Fable 5.1", 49.0),
    ("GPT-6 Astra", 57.0),
    ("Claude Opus 5", 50.7),
    ("Claude Fable 5", 49.7),
    ("GPT-5.6 Sol", 47.1),
    ("Grok 4.6", 44.4),
    ("Kimi K3", 43.8),
    ("GLM-5.3", 32.0),
    ("GLM-5.3-Flash", 41.9), // v5.0 dash; v4.3.2 read retained (the FlashX proxy above follows it)
    ("Qwen3.8 Max", 45.4),
    ("Qwen3.8 2.4T A95B", 40.0),
    ("GPT-5.6 Terra", 42.3),
    ("Muse Spark 1.3", 48.0), // re-read 2026-10-15: the Wonder launch re-measured max at 48 (was 45.2 v4.3)
    ("Meta: Muse Spark 1.3 Contributor", 48.0), // same weights; see SAME_WEIGHTS_SKU_ALIASES (AA rates no Contributor page)
    ("Muse Spark 1.2", 39.8),
    ("DeepSeek V4.1 Flash", 39.0), // re-read 2026-10-23: page drifted a point under the 0910 build's 39.5 v4.3 read
    ("Gemini 3.8 Flash", 31.0),    // re-read 2026-10-28 (v5.0): was 41.2
    ("Gemini 3.7 Flash", 39.4),
    ("Gemini 3.6 Flash", 34.3),
    ("Grok 4.5", 39.1),
    ("Claude Sonnet 5", 38.4),
    ("GPT-5.5", 38.0),
    ("Claude Opus 4.8", 42.0),
    ("Claude Opus 4.7", 40.7),
    ("Muse Spark 1.1", 34.3),
    ("Muse Glimmer 30B", 6.0),      // re-read 2026-10-28 (v5.0): was 17.0
    ("DeepSeek V4 Pro 0813", 23.0), // re-read 2026-10-28 (v5.0): was 36.0
    ("GLM-5.2", 34.0),
    ("GPT-5.4", 39.0),
    ("GPT-5.4 Mini", 24.0),
    ("GPT-5.4 Nano", 21.2),
    ("GPT-5.2", 30.4),
    ("GPT-5 Mini", 17.4),
    ("DeepSeek V4 Flash 0731", 34.0), // re-read 2026-10-23: the canonical page's 0731 measurement firmed at 34 (was 34.5 v4.3)
    ("GPT-5.6 Luna", 37.5),
    ("Gemini 3.5 Flash", 33.0),
    ("Gemini 3.5 Flash Lite", 12.0), // re-read 2026-10-28 (v5.0): was 22.0
    ("Gemini 3.1 Pro Preview", 30.4),
    ("Gemini 3.1 Flash-Lite", 16.0),
    ("Gemini 3 Flash Preview", 26.3),
    ("Claude Sonnet 4.6", 24.7),
    ("MiniMax-M3", 19.0), // re-read 2026-10-28 (v5.0): was 29.0
    ("MiniMax-M2.7", 23.2),
    ("MiniMax-M2.5", 22.8),
    ("MiniMax-M2.1", 20.9),
    ("MiniMax-M2", 18.6),
    ("DeepSeek V4 Pro", 23.0), // re-read 2026-10-28 (v5.0): the canonical page measures the 0813 build, now at 23
    ("Kimi K2.6", 27.5),
    ("Claude Opus 4.6", 26.4),
    ("Qwen3.8 27B", 20.0), // re-read 2026-10-28 (v5.0, xhigh): was 33.9
    ("Kimi K2.7 Code", 26.3),
    ("Kimi K2 Thinking", 22.0),
    ("Kimi K2", 12.7),
    ("MiMo-V2.5-Pro", 26.4),
    ("MiMo-V2.5", 22.3),
    ("Hy3", 25.0),
    ("DeepSeek V4 Flash", 34.0), // re-read 2026-10-23: the canonical page now measures the 0731 build (was the undated read 24.6)
    ("GLM-5.1", 26.4),
    ("GLM-5", 27.9),
    ("GLM-4.7", 22.2),
    ("GLM-4.7-Flash", 14.9),
    ("GLM-4.6", 14.9),
    // GLM-4.5 had a v4.1.1-era row (20.0); AA removed its model page before
    // v4.3, so the family is unrated now and falls back to the neutral prior.
    ("GLM-4.5-Air", 11.1),
    ("Grok 4.3", 25.4),
    ("Grok 4.1 Fast", 11.0), // re-read 2026-10-23: AA retired the reasoning evaluation; only the non-reasoning run (11) remains, so the row falls back per the no-reasoning-run policy (was 17.9)
    ("Grok 4", 22.0),
    ("NVIDIA Nemotron 3 Ultra 550B A55B", 12.0), // re-read 2026-10-28 (v5.0): was 23.4; the board's short name aliases onto this row
    ("NVIDIA Nemotron 3.5 Lightning 30B", 13.0),
    ("Claude 4.5 Sonnet", 19.3),
    ("Claude Opus 4.5", 23.7),
    ("Kimi K2.5", 23.0),
    ("Qwen3.5 397B A17B", 18.0),
    ("Qwen3.5 122B A10B", 16.2),
    ("DeepSeek V3.2", 16.0),
    ("DeepSeek V3.1", 13.7),
    ("DeepSeek V3.1 Terminus", 13.9),
    // AA folded the R1 0528 refresh into the base DeepSeek R1 page, so both
    // Surplus SKUs read the same v4.3 measurement (see
    // SAME_WEIGHTS_SKU_ALIASES).
    ("DeepSeek R1 0528", 13.1),
    ("DeepSeek R1", 13.1),
    ("Qwen3.6 27B", 21.0),
    ("Qwen3.6 35B A3B", 18.0),
    ("Qwen3.5 35B A3B", 19.3),
    ("Qwen3 Next 80B A3B Instruct", 9.6),
    ("Qwen3 Coder Next", 9.0),
    ("Qwen3 Coder 480B", 11.9),
    ("Qwen3 Coder 480B Turbo", 11.9), // same weights served faster; see SAME_WEIGHTS_SKU_ALIASES
    ("Gemma 4 31B", 15.4),
    ("Gemma 4 26B A4B", 16.7),
    ("Claude 4.5 Haiku", 15.4),
    ("GPT-5.5 Instant", 26.0),
    ("gpt-oss-120b", 12.3),
    ("gpt-oss-20b", 9.0),
    ("Qwen3 235B A22B 2507", 12.0),
    ("Mistral Small 3.2", 7.0),
    ("Mistral Small 3.2 24B Instruct", 7.0), // same model, open-weights listing name; see SAME_WEIGHTS_SKU_ALIASES
    ("Mistral Small 4", 11.0),
    ("Mistral Large 3", 9.7),
    ("Llama 4 Scout", 6.0),
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
    ("GPT-5.6 Sol Pro", 47.1), // same weights as GPT-5.6 Sol; see SAME_WEIGHTS_SKU_ALIASES
    ("GPT-5.6 Terra Pro", 42.3), // same weights as GPT-5.6 Terra; see SAME_WEIGHTS_SKU_ALIASES
    ("GPT-5.6 Luna Pro", 37.5), // same weights as GPT-5.6 Luna; see SAME_WEIGHTS_SKU_ALIASES
    // Google
    ("Gemini 2.5 Pro", 16.0),
    ("Gemini 2.5 Flash", 9.9),
    // xAI
    ("Grok 4.20 Beta", 25.7),
    ("Grok Build 0.1", 27.2),
    ("Grok Code Fast 1", 14.1),
    // Z AI
    // GLM-4.7-Thinking's AA page was folded into the base GLM-4.7 page (see
    // SAME_WEIGHTS_SKU_ALIASES), so the SKU reads the base measurement.
    ("GLM-4.7-Thinking", 22.2),
    ("GLM-5 Turbo", 26.6),
    ("GLM 5V Turbo", 23.0),
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
    ("Ministral 3 8B", 5.0),
    ("Ministral 3B", 4.8),
    ("Devstral 2 123B", 9.4),
    // NVIDIA
    ("NVIDIA Nemotron 3 Super 120B", 13.0),
    ("NVIDIA Nemotron Cascade 2 30B", 11.7),
    ("NVIDIA Nemotron 3 Nano 30B A3B", 8.9),
    ("NVIDIA Nemotron Nano 12B v2", 6.0), // ~est: the VL Reasoning page is gone; the row reads the page's non-reasoning 6 (was 7.5)
    ("NVIDIA Nemotron Nano 9B v2", 6.8),
    // Qwen
    ("Qwen 3.7 Max", 29.0),
    ("Qwen 3.7 Plus", 25.0),
    ("Qwen3.5 Plus", 20.4),  // ~proxy: Qwen3.5 Omni Plus
    ("Qwen3.5 Flash", 12.0), // ~proxy: Qwen3.5 Omni Flash (re-read 2026-10-23: source page 12)
    ("Qwen 3.5 9B", 11.0),
    ("Qwen3 VL 235B A22B", 10.0), // re-read 2026-10-23: the reasoning page is gone; only the instruct run (10) remains (was 13.4)
    ("Qwen3 235B A22B Thinking 2507", 12.7),
    ("Qwen3 32B", 7.2),
    ("Qwen3 30B A3B", 7.6), // reasoning variant (instruct is lower)
    ("Qwen3 Coder 30B A3B Instruct", 9.6),
    // Others
    ("Mercury 2", 11.0),
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
        "GLM-4.7-Thinking",
        "GLM-4.7",
        "thinking-mode page AA folded into the base",
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
    fn aa_board_name_aliases_resolve_onto_snapshot_rows() {
        // The AA boards (Intelligence Index, AA-3D, speed) name several
        // families under short forms that would fall under the fuzzy bar or
        // duplicate rows in the live overlay; every `alias_display_name`
        // entry must resolve onto a real snapshot row's canonical key.
        let rows = artificial_analysis_snapshot();
        let keys: std::collections::HashSet<String> = rows
            .iter()
            .map(|row| benchmark_model_key(&row.slug))
            .collect();
        for board_name in [
            "Inkling",
            "Nemotron 3 Ultra",
            "Muse Glimmer",
            "Beam 1.0",
            "Ling 3.1 Flash 1019",
        ] {
            let aliased = crate::fetch::aa_evals::alias_display_name(board_name);
            let key = benchmark_model_key(aliased);
            assert!(
                !key.is_empty() && keys.contains(&key),
                "board name {board_name:?} aliases to {aliased:?}, which matches no snapshot row",
            );
        }
    }
}
