//! Live-source validation: fetch every benchmark board from its real
//! upstream and check the parse still produces sane rows. These tests hit
//! the network, so they are `#[ignore]`d out of normal `cargo test` runs —
//! run them on demand with:
//!
//! ```text
//! cargo test live_ -- --ignored --nocapture --test-threads 4
//! ```
//!
//! Beyond "the fetch succeeded", each check enforces what actually breaks
//! in practice: rows are non-empty, scores are finite and inside the board's
//! plausible scale (percent boards 0–100, Elo boards 500–3000), and the AA
//! boards' values are reconciled against the payload objects their pages
//! embed. (The last board with a silent bundled fallback, SWE-rebench, was
//! retired in 0.32.0 once its task window froze mid-2026.)

use std::time::Duration;

use reqwest::blocking::Client;

use super::terminal_bench::{
    TERMINAL_BENCH_4_LEADERBOARD, fetch_terminal_bench_version, parse_terminal_bench_harbor,
};
use super::{
    fetch_aa_3d, fetch_aa_briefcase, fetch_aa_index_and_speed, fetch_automationbench_aa,
    fetch_deepswe, fetch_design_arena, fetch_frontiercode, fetch_frontiercode2, fetch_livebench,
    fetch_swebench_pro, fetch_text, fetch_vals_sre_bench,
};
use crate::types::Benchmark;

/// Same client shape the worker uses, with a little more headroom for slow
/// board pages under a test run.
fn validation_client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("ParetoWatch/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("HTTP client")
}

/// Assert a board's rows exist and sit in the board's plausible score range,
/// printing the podium so a human can eyeball the numbers themselves.
fn validate_board(label: &str, rows: &[Benchmark], min_rows: usize, lo: f64, hi: f64) {
    assert!(
        rows.len() >= min_rows,
        "{label}: only {} rows, expected at least {min_rows}",
        rows.len()
    );
    let mut podium: Vec<&Benchmark> = rows.iter().collect();
    podium.sort_by(|a, b| {
        b.agentic_coding
            .or(b.overall)
            .unwrap_or(f64::NEG_INFINITY)
            .total_cmp(&a.agentic_coding.or(a.overall).unwrap_or(f64::NEG_INFINITY))
    });
    println!("{label}: {} rows", rows.len());
    for row in podium.iter().take(3) {
        println!(
            "  {:>8.2}  {}",
            row.agentic_coding.or(row.overall).unwrap_or(f64::NAN),
            row.name
        );
    }
    for row in rows {
        for score in [row.overall, row.coding, row.agentic_coding]
            .into_iter()
            .flatten()
        {
            assert!(
                score.is_finite(),
                "{label}: non-finite score on row {:?}",
                row.name
            );
            assert!(
                (lo..=hi).contains(&score),
                "{label}: score {score} outside {lo}..={hi} on row {:?}",
                row.name
            );
        }
    }
}

#[test]
#[ignore = "fetches live upstreams"]
fn live_aa_intelligence_index_and_speed() {
    let client = validation_client();
    let (index, speed) = fetch_aa_index_and_speed(&client);

    let rows = index.expect("AA Intelligence Index fetch failed");
    validate_board("AA Intelligence Index", &rows, 30, 0.0, 100.0);
    // The fetch never fails (the bundled snapshot is the fallback), so prove
    // the live page actually parsed: overlay re-tags matched rows "live".
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let live_rows = rows
        .iter()
        .filter(|row| row.name.contains(&format!("live {today}")))
        .count();
    assert!(
        live_rows >= 10,
        "only {live_rows} rows carry today's live tag — the page parse failed \
         and the bundled snapshot was returned unchanged"
    );
    println!("AA Intelligence Index: {live_rows} rows re-tagged live today");

    let speed = speed.expect("AA standardized-speed parse failed");
    assert!(
        !speed.entries.is_empty(),
        "AA speed snapshot carried no entries"
    );
    let with_tps = speed
        .entries
        .iter()
        .filter(|e| e.median_tps.is_some_and(|v| v > 0.0))
        .count();
    println!(
        "AA standardized speed: {} entries, {with_tps} with median tok/s",
        speed.entries.len()
    );
    assert!(
        with_tps >= 10,
        "only {with_tps} AA speed entries carry a median tok/s"
    );
    for entry in &speed.entries {
        for value in [entry.median_tps, entry.median_ttft_s, entry.median_e2e_s]
            .into_iter()
            .flatten()
        {
            assert!(
                value.is_finite() && value >= 0.0,
                "AA speed entry {:?} carries a negative or non-finite telemetry value ({value})",
                entry.name
            );
        }
    }
}

#[test]
#[ignore = "fetches live upstreams"]
fn live_terminal_bench_4() {
    let client = validation_client();
    let value = fetch_terminal_bench_version(&client, TERMINAL_BENCH_4_LEADERBOARD, "4.0")
        .expect("Terminal-Bench 4.0 Harbor fetch failed");
    let rows = parse_terminal_bench_harbor(&value).expect("Terminal-Bench 4.0 parse failed");
    validate_board("Terminal-Bench 4.0 (live Harbor)", &rows, 3, 0.0, 100.0);
}

#[test]
#[ignore = "fetches live upstreams"]
fn live_deepswe() {
    let client = validation_client();
    let rows = fetch_deepswe(&client).expect("DeepSWE v1.1 fetch failed");
    validate_board("DeepSWE v1.1", &rows, 3, 0.0, 100.0);
}

#[test]
#[ignore = "fetches live upstreams"]
fn live_livebench() {
    let client = validation_client();
    let rows = fetch_livebench(&client).expect("LiveBench fetch failed");
    validate_board("LiveBench", &rows, 3, 0.0, 100.0);
}

#[test]
#[ignore = "fetches live upstreams"]
fn live_design_arena() {
    let client = validation_client();
    let rows = fetch_design_arena(&client).expect("Design Arena fetch failed");
    validate_board(
        "Design Arena (Frontend Design Elo)",
        &rows,
        5,
        500.0,
        3000.0,
    );
}

#[test]
#[ignore = "fetches live upstreams"]
fn live_frontiercode_1_1() {
    let client = validation_client();
    let rows = fetch_frontiercode(&client).expect("FrontierCode 1.1 fetch failed");
    validate_board("FrontierCode 1.1", &rows, 3, 0.0, 100.0);
}

#[test]
#[ignore = "fetches live upstreams"]
fn live_frontiercode_2_0() {
    let client = validation_client();
    let rows = fetch_frontiercode2(&client).expect("FrontierCode 2.0 fetch failed");
    validate_board("FrontierCode 2.0", &rows, 3, 0.0, 100.0);
}

#[test]
#[ignore = "fetches live upstreams"]
fn live_aa_briefcase() {
    let client = validation_client();
    let rows = fetch_aa_briefcase(&client).expect("AA-Briefcase fetch failed");
    validate_board("AA-Briefcase (Elo)", &rows, 3, 500.0, 3000.0);
}

#[test]
#[ignore = "fetches live upstreams"]
fn live_automationbench_aa() {
    let client = validation_client();
    let rows = fetch_automationbench_aa(&client).expect("AutomationBench-AA fetch failed");
    validate_board("AutomationBench-AA (partial score)", &rows, 3, 0.0, 100.0);
}

#[test]
#[ignore = "fetches live upstreams"]
fn live_aa_3d() {
    let client = validation_client();
    let rows = fetch_aa_3d(&client).expect("AA-3D fetch failed");
    validate_board("AA-3D (Elo)", &rows, 3, 500.0, 3000.0);
}

#[test]
#[ignore = "fetches live upstreams"]
fn live_swebench_pro() {
    let client = validation_client();
    let rows = fetch_swebench_pro(&client).expect("SWE-Bench Pro V2 fetch failed");
    validate_board("SWE-Bench Pro V2 HARD", &rows, 3, 0.0, 100.0);
}

#[test]
#[ignore = "fetches live upstreams"]
fn live_vals_sre_bench() {
    let client = validation_client();
    let rows = fetch_vals_sre_bench(&client).expect("SRE Bench (Vals) fetch failed");
    validate_board("SRE Bench (Vals)", &rows, 3, 0.0, 100.0);
}

/// The worker dispatches every remote source through one function; a new
/// source that forgets a match arm would break at compile time, but a source
/// wired to the wrong fetcher would not. Fetch one cheap JSON board through
/// the dispatcher to prove the routing works end to end.
#[test]
#[ignore = "fetches live upstreams"]
fn live_worker_dispatch_routes_sources() {
    let client = validation_client();
    let rows = super::fetch_benchmark_source(&client, crate::types::BenchmarkSource::DeepSWE11)
        .expect("dispatched DeepSWE fetch failed");
    validate_board("Worker dispatch (DeepSWE11)", &rows, 3, 0.0, 100.0);
    // The AA snapshot dispatch must not error either (it never does — the
    // bundled snapshot is its fallback).
    super::fetch_benchmark_source(
        &client,
        crate::types::BenchmarkSource::ArtificialAnalysisSnapshot,
    )
    .expect("dispatched AA snapshot fetch failed");
}

/// Exploratory survey for the v5 coverage gap: the RSC payload every AA page
/// embeds (`initialModels`) carries per-model scores for ALL of AA's boards —
/// including deprecated models the Intelligence Index no longer measures —
/// so one sweep over the pages measures how many v5-unrated families each AA
/// board would rate. The three wired AA boards are measured through their
/// real fetchers (full tables); the rest through the embedded score objects
/// (a lower bound — the chart subset of models each page preloads).
/// Temporary tooling for the 2026-10-28 investigation.
#[test]
#[ignore = "exploratory: surveys AA board coverage for the v5 gap"]
fn aa_v5_coverage_survey() {
    use crate::artificial_analysis_snapshot::artificial_analysis_snapshot;
    use crate::bench::matching::benchmark_model_key;
    use crate::fetch::aa_evals::{alias_display_name, split_aa_display_name};
    use std::collections::{BTreeMap, BTreeSet};
    use std::time::Instant;

    let started = Instant::now();
    let client = validation_client();

    // Rated vs unrated snapshot families, keyed the way board joins key.
    let snapshot = artificial_analysis_snapshot();
    let mut rated: BTreeSet<String> = BTreeSet::new();
    let mut unrated: BTreeSet<String> = BTreeSet::new();
    let mut display: BTreeMap<String, String> = BTreeMap::new();
    for row in &snapshot {
        let key = benchmark_model_key(&row.slug);
        if key.is_empty() {
            continue;
        }
        display
            .entry(key.clone())
            .or_insert_with(|| row.slug.clone());
        if row.overall.is_some() {
            rated.insert(key);
        } else {
            unrated.insert(key);
        }
    }
    println!(
        "snapshot families: {} rated / {} unrated",
        rated.len(),
        unrated.len()
    );

    // board label -> (rated keys scored, unrated keys scored)
    let mut covers: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)> = BTreeMap::new();
    let mut record =
        |board: &str, keys: Vec<String>, rated: &BTreeSet<String>, unrated: &BTreeSet<String>| {
            let entry = covers
                .entry(board.to_owned())
                .or_insert_with(|| (BTreeSet::new(), BTreeSet::new()));
            for key in keys {
                if rated.contains(&key) {
                    entry.0.insert(key);
                } else if unrated.contains(&key) {
                    entry.1.insert(key);
                }
            }
        };

    // Wired AA boards: measure through their real fetchers (full tables).
    type Fetcher = fn(&reqwest::blocking::Client) -> anyhow::Result<Vec<Benchmark>>;
    let wired: &[(&str, Fetcher)] = &[
        ("AA-Briefcase Elo", super::fetch_aa_briefcase),
        ("AutomationBench partial", super::fetch_automationbench_aa),
        ("AA-3D Elo", super::fetch_aa_3d),
    ];
    for (label, fetch) in wired {
        match fetch(&client) {
            Ok(rows) => {
                let keys = rows.iter().map(|r| benchmark_model_key(&r.slug)).collect();
                record(label, keys, &rated, &unrated);
                println!("{label}: {} rows via real fetcher", rows.len());
            }
            Err(err) => println!("{label}: FETCH FAILED {err:#}"),
        }
    }

    // Scalar per-board score fields the embedded model objects carry.
    let boards: &[(&str, &str)] = &[
        ("Intelligence Index v5", "intelligenceIndex"),
        ("AA-Briefcase Elo", "briefcaseElo"),
        ("AA-3D Elo", "aa3dElo"),
        ("AutomationBench partial", "automationBenchPartialScore"),
        ("Analyst Agent pass@1", "analystAgentPassAt1"),
        ("LiveCodeBench", "livecodebench"),
        ("FrontierCode 1.1", "frontierCode"),
        ("FrontierCode 2.0", "frontierCode20"),
        ("Terminal-Bench 2.1", "terminalBench21"),
        ("Terminal-Bench 4.0", "terminalBench40"),
        ("Terminal-Bench Science", "terminalBenchScience"),
        ("Terminal-Bench Hard", "terminalbenchHard"),
        ("GPQA Diamond", "gpqa"),
        ("HLE Diamond", "hleDiamond"),
        ("HLE", "hle"),
        ("SciCode", "scicode"),
        ("IFBench", "ifbench"),
        ("LCR", "lcr"),
        ("MMLU-Pro", "mmluPro"),
        ("Math 500", "math500"),
        ("AIME 25", "aime25"),
        ("Tau2", "tau2"),
        ("Tau3 Banking", "tauBanking"),
        ("EnterpriseOps Gym", "enterpriseOpsGym"),
        ("Omniscience v1.1", "omniscienceV1"),
        ("CritPt", "critpt"),
        ("GDPval", "gdpval"),
        ("Cyber Index", "cyberIndex"),
        ("HealthBench", "healthbench"),
        ("MLCR overall", "mlcrOverall"),
    ];

    let pages: &[(&str, &str)] = &[
        (
            "models-leaderboard",
            "https://artificialanalysis.ai/leaderboards/models",
        ),
        (
            "eval-livecodebench",
            "https://artificialanalysis.ai/evaluations/livecodebench",
        ),
        (
            "eval-aa-briefcase",
            "https://artificialanalysis.ai/evaluations/aa-briefcase",
        ),
        (
            "eval-aa-3d",
            "https://artificialanalysis.ai/evaluations/aa-3d",
        ),
        (
            "eval-automationbench-aa",
            "https://artificialanalysis.ai/evaluations/automationbench-aa",
        ),
        (
            "eval-aa-analyst-agent",
            "https://artificialanalysis.ai/evaluations/aa-analyst-agent",
        ),
        (
            "eval-ifbench",
            "https://artificialanalysis.ai/evaluations/ifbench",
        ),
        (
            "eval-gpqa-diamond",
            "https://artificialanalysis.ai/evaluations/gpqa-diamond",
        ),
        (
            "eval-hle-diamond",
            "https://artificialanalysis.ai/evaluations/hle-diamond",
        ),
        (
            "eval-scicode",
            "https://artificialanalysis.ai/evaluations/scicode",
        ),
        (
            "eval-tau3-banking",
            "https://artificialanalysis.ai/evaluations/tau3-banking",
        ),
    ];

    // A model object opens with id/slug/name/shortName — creator objects
    // (id/slug/name/color/logo) lack shortName, which keeps the chunking from
    // splitting a model at its embedded creator. Quotes appear both escaped
    // (RSC-in-JS-string) and bare (RSC proper) depending on the page.
    let object_start_re = regex::Regex::new(
        r#"\{\\?"id\\?":\\?"[0-9a-f-]{36}\\?",\\?"slug\\?":\\?"[a-z0-9][a-z0-9._-]*\\?",\\?"name\\?":\\?".{1,120}?\\?",\\?"shortName\\?":\\?""#,
    )
    .expect("object regex");
    let short_name_re =
        regex::Regex::new(r#"\\?"shortName\\?":\\?"(.{1,120}?)\\?""#).expect("name regex");

    for (page_label, url) in pages {
        let html = match fetch_text(&client, url, page_label) {
            Ok(html) => html,
            Err(err) => {
                println!("{page_label}: FETCH FAILED {err:#}");
                continue;
            }
        };
        let starts: Vec<usize> = object_start_re
            .find_iter(&html)
            .map(|m| m.start())
            .collect();
        let mut page_hits: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (idx, &start) in starts.iter().enumerate() {
            let end = starts.get(idx + 1).copied().unwrap_or(html.len());
            let chunk = &html[start..end.min(html.len())];
            let Some(name) = short_name_re
                .captures(chunk)
                .and_then(|c| c.get(1).map(|m| m.as_str().to_owned()))
            else {
                continue;
            };
            let (base, _effort) = split_aa_display_name(&name);
            let base = alias_display_name(&base).to_owned();
            let key = benchmark_model_key(&base);
            if key.is_empty() || (!rated.contains(&key) && !unrated.contains(&key)) {
                continue;
            }
            for (label, field) in boards {
                let pattern =
                    format!(r#"\\?"{field}\\?":(null|-?[0-9]+\.?[0-9]*(?:[eE][+-]?[0-9]+)?)"#);
                let Ok(re) = regex::Regex::new(&pattern) else {
                    continue;
                };
                let Some(caps) = re.captures(chunk) else {
                    continue;
                };
                if caps.extract::<1>().1[0] != "null" {
                    page_hits
                        .entry((*label).to_owned())
                        .or_default()
                        .push(key.clone());
                }
            }
        }
        for (label, keys) in page_hits {
            record(&label, keys, &rated, &unrated);
        }
        println!(
            "{page_label}: {} model objects ({:.1}s)",
            starts.len(),
            started.elapsed().as_secs_f32()
        );
    }

    println!(
        "\n=== board coverage of the {} unrated families ===",
        unrated.len()
    );
    let mut table: Vec<_> = covers.iter().collect();
    table.sort_by_key(|(_, (_, u))| std::cmp::Reverse(u.len()));
    for (label, (r, u)) in table {
        println!(
            "{:<26} unrated {}/{}  rated {}/{}",
            label,
            u.len(),
            unrated.len(),
            r.len(),
            rated.len()
        );
    }
    println!("\n=== unrated families each relevant board would rate ===");
    for field in [
        "Intelligence Index v5",
        "AA-Briefcase Elo",
        "LiveCodeBench",
        "GPQA Diamond",
        "IFBench",
        "Terminal-Bench 4.0",
        "FrontierCode 2.0",
        "AutomationBench partial",
        "AA-3D Elo",
    ] {
        if let Some((_, u)) = covers.get(field)
            && !u.is_empty()
        {
            let names: Vec<String> = u.iter().map(|k| display[k].clone()).collect();
            println!("{field} ({}): {}", u.len(), names.join(", "));
        }
    }
    let union: BTreeSet<String> = covers
        .values()
        .flat_map(|(_, u)| u.iter().cloned())
        .collect();
    let still: Vec<String> = unrated
        .difference(&union)
        .map(|k| display[k].clone())
        .collect();
    println!(
        "\nunion of every AA board rates {}/{} unrated; still unrated by all of AA: {}",
        union.len(),
        unrated.len(),
        still.join(", ")
    );
}

/// AA-3D correctness check: the page server-renders its leaderboard as an
/// HTML table AND embeds the same board as RSC objects. The app reads the
/// payload objects (nearest-preceding-name attribution, integer-only elo),
/// so parse the table independently and demand every family's Elo — and the
/// best-variant effort it was read at — agree with the payload parse.
#[test]
#[ignore = "fetches live upstreams"]
fn live_aa_3d_elo_matches_server_table() {
    use crate::bench::matching::benchmark_model_key;
    use crate::fetch::aa_evals::{alias_display_name, parse_aa_3d, split_aa_display_name};
    use std::collections::BTreeMap;

    let client = validation_client();
    let html = fetch_text(&client, crate::types::AA_3D_URL, "AA-3D leaderboard page")
        .expect("AA-3D page fetch failed");

    // Ground truth: the server-rendered table (rank | range | model | elo±CI).
    let tr = regex::Regex::new(r"(?s)<tr[^>]*>(.*?)</tr>").expect("tr regex");
    let td = regex::Regex::new(r"(?s)<td[^>]*>(.*?)</td>").expect("td regex");
    let mut table_rows = 0usize;
    // family key -> (best elo, effort of the best variant, display base)
    let mut table_best: BTreeMap<String, (f64, Option<String>, String)> = BTreeMap::new();
    for row_html in tr
        .captures_iter(&html)
        .map(|m| m.extract::<1>().1[0].to_string())
    {
        let cells: Vec<String> = td
            .captures_iter(&row_html)
            .map(|c| super::collapse_whitespace(&super::strip_html_tags(c.extract::<1>().1[0])))
            .collect();
        if cells.len() < 4 {
            continue;
        }
        let Some(elo) = cells[3]
            .split(['±', ' '])
            .next()
            .and_then(|v| v.parse::<f64>().ok())
        else {
            continue;
        };
        let (base, effort) = split_aa_display_name(&cells[2]);
        let base = alias_display_name(&base).to_owned();
        let key = benchmark_model_key(&base);
        if key.is_empty() || !(500.0..=3000.0).contains(&elo) {
            continue;
        }
        table_rows += 1;
        let replace = table_best.get(&key).is_none_or(|(old, _, _)| elo > *old);
        if replace {
            table_best.insert(key, (elo, effort, base));
        }
    }
    assert!(
        table_rows >= 20,
        "AA-3D table parse found only {table_rows} scored rows — table markup changed?"
    );

    // The app's payload parse, folded to one row per family (best variant).
    let parsed = parse_aa_3d(&html);
    let mut parsed_best: BTreeMap<String, (f64, Option<String>)> = BTreeMap::new();
    for row in &parsed {
        let key = benchmark_model_key(&row.slug);
        assert!(!key.is_empty(), "AA-3D row {:?} keyed empty", row.slug);
        let score = row.agentic_coding.expect("AA-3D row carries a score");
        let replace = parsed_best.get(&key).is_none_or(|(old, _)| score > *old);
        if replace {
            parsed_best.insert(key, (score, row.reasoning_effort.clone()));
        }
    }

    println!(
        "AA-3D: {table_rows} table rows, {} families; payload parse: {} rows, {} families",
        table_best.len(),
        parsed.len(),
        parsed_best.len()
    );
    for (key, (table_elo, table_effort, base)) in &table_best {
        match parsed_best.get(key) {
            Some((parsed_elo, parsed_effort)) => {
                println!(
                    "  {base:<34} table {table_elo:>5} ({}) · payload {parsed_elo:>5} ({})",
                    table_effort.as_deref().unwrap_or("-"),
                    parsed_effort.as_deref().unwrap_or("-")
                );
                assert_eq!(
                    parsed_elo, table_elo,
                    "AA-3D Elo mismatch for {base}: payload says {parsed_elo}, table says {table_elo}"
                );
            }
            None => panic!(
                "AA-3D family {base} is in the server table at {table_elo} but the payload parse missed it"
            ),
        }
    }
    for (key, (parsed_elo, _)) in &parsed_best {
        assert!(
            table_best.contains_key(key),
            "AA-3D payload parse produced a family the server table does not list (key {key}, elo {parsed_elo}) — likely captured another board's elo"
        );
    }
}

/// Full AA reconciliation: every AA board the app serves, re-derived from
/// channels independent of the app's parsers and compared family by family.
///
/// - Intelligence Index v5 + standardized speed: the models leaderboard's
///   server-rendered table (integer index, tok/s, TTFT per variant) vs the
///   app's live-overlay rows and speed snapshot, both folded with the app's
///   own variant-preference rules.
/// - AA-Briefcase / AutomationBench-AA / AA-3D: object-scoped payload fields
///   (`briefcaseElo` / `automationBenchPartialScore` / `aa3dElo` inside each
///   model object) vs the app's stream-attribution parses — two different
///   extraction methods over the same data must agree on every family.
#[test]
#[ignore = "fetches live upstreams"]
fn live_aa_boards_reconcile_with_published_values() {
    use crate::bench::matching::benchmark_model_key;
    use crate::fetch::aa_evals::{
        NAMED_EFFORTS, parse_aa_3d, parse_aa_briefcase, parse_automationbench_aa,
    };
    use crate::fetch::aa_index::{classify_aa_name, variant_preference};
    use crate::types::AaSpeedSnapshot;
    use std::collections::BTreeMap;

    /// The speed fold's variant ranking (aa_speed keeps it private): the
    /// calmest variant reads cleanest — bare > low > … > max > non-reasoning.
    fn speed_rank(effort: Option<&str>, non_reasoning: bool) -> usize {
        if non_reasoning {
            return 0;
        }
        let Some(effort) = effort else {
            return 6;
        };
        match NAMED_EFFORTS.iter().position(|t| *t == effort) {
            Some(position) => NAMED_EFFORTS.len() - position,
            None => NAMED_EFFORTS.len(),
        }
    }

    let client = validation_client();
    let tr = regex::Regex::new(r"(?s)<tr[^>]*>(.*?)</tr>").expect("tr regex");
    let td = regex::Regex::new(r"(?s)<td[^>]*>(.*?)</td>").expect("td regex");
    let table = |html: &str| -> Vec<Vec<String>> {
        tr.captures_iter(html)
            .map(|m| m.extract::<1>().1[0].to_string())
            .map(|row_html| {
                td.captures_iter(&row_html)
                    .map(|c| {
                        super::collapse_whitespace(&super::strip_html_tags(c.extract::<1>().1[0]))
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|cells| cells.len() >= 4)
            .collect()
    };

    // ------------------------------------------------------------------
    // A. Intelligence Index v5: leaderboard table vs the app's overlay rows.
    // ------------------------------------------------------------------
    let models_html = fetch_text(
        &client,
        "https://artificialanalysis.ai/leaderboards/models",
        "AA models leaderboard",
    )
    .expect("models leaderboard fetch failed");

    // family key -> (index value, preference rank of the variant read, name)
    let mut table_index: BTreeMap<String, (f64, usize, String)> = BTreeMap::new();
    let mut table_speed: BTreeMap<String, ((f64, f64), usize, String)> = BTreeMap::new();
    for cells in table(&models_html) {
        let Some((key, name, effort, non_reasoning)) = classify_aa_name(&cells[0]) else {
            continue;
        };
        if key.is_empty() {
            continue;
        }
        if let Ok(value) = cells[3].trim().parse::<f64>()
            && (0.0..=100.0).contains(&value)
        {
            let rank = variant_preference(effort.as_deref(), non_reasoning);
            if table_index.get(&key).is_none_or(|(_, old, _)| rank > *old) {
                table_index.insert(key.clone(), (value, rank, name.clone()));
            }
        }
        if cells.len() >= 7
            && let (Ok(tps), Ok(ttft)) = (
                cells[5].trim().parse::<f64>(),
                cells[6].trim().parse::<f64>(),
            )
        {
            let rank = speed_rank(effort.as_deref(), non_reasoning);
            if table_speed.get(&key).is_none_or(|(_, old, _)| rank > *old) {
                table_speed.insert(key.clone(), ((tps, ttft), rank, name));
            }
        }
    }
    println!("index table: {} scored families", table_index.len());

    let (index_rows, speed_result) = fetch_aa_index_and_speed(&client);
    let index_rows = index_rows.expect("AA index fetch failed");
    let mut mismatches = 0;
    let mut bundled = 0;
    let mut checked = 0;
    let mut scored_rows: Vec<&Benchmark> =
        index_rows.iter().filter(|r| r.overall.is_some()).collect();
    scored_rows.sort_by(|a, b| {
        b.overall
            .unwrap_or(f64::MIN)
            .total_cmp(&a.overall.unwrap_or(f64::MIN))
    });
    for row in &scored_rows {
        let key = benchmark_model_key(&row.slug);
        let app_value = row.overall.unwrap_or_default();
        match table_index.get(&key) {
            Some((table_value, _, name)) => {
                checked += 1;
                let diff = (app_value - table_value).abs();
                println!(
                    "  index {:<34} app {:>6.2} table {:>5.1} ({}){}",
                    row.slug,
                    app_value,
                    table_value,
                    name,
                    if diff > 0.51 { "  <-- MISMATCH" } else { "" }
                );
                if diff > 0.51 {
                    mismatches += 1;
                }
            }
            None => {
                bundled += 1;
                println!(
                    "  index {:<34} app {:>6.2} (bundled read, not on live table)",
                    row.slug, app_value
                );
            }
        }
    }
    let joined_keys: std::collections::BTreeSet<String> = scored_rows
        .iter()
        .map(|r| benchmark_model_key(&r.slug))
        .collect();
    let unjoined = table_index
        .keys()
        .filter(|k| !joined_keys.contains(*k))
        .count();
    println!(
        "index: {checked} families reconciled, {mismatches} mismatches, {bundled} bundled-only reads, {unjoined} table families the app does not score",
    );
    assert_eq!(
        mismatches, 0,
        "AA Intelligence Index disagrees with the published table"
    );
    assert!(
        checked >= 30,
        "index reconciliation covered only {checked} families"
    );

    // ------------------------------------------------------------------
    // B. Standardized speed: same table's tok/s + TTFT columns vs the
    //    app's speed snapshot, both folded to the calmest variant.
    // ------------------------------------------------------------------
    let speed: AaSpeedSnapshot = speed_result.expect("AA speed snapshot parse failed");
    let mut speed_checked = 0;
    let mut speed_mismatched = 0;
    for entry in &speed.entries {
        let Some(((table_tps, table_ttft), _, _)) = table_speed.get(&entry.key) else {
            continue;
        };
        if let Some(app_tps) = entry.median_tps {
            speed_checked += 1;
            if (app_tps - table_tps).abs() > 1.01 {
                speed_mismatched += 1;
                println!(
                    "  speed {:<30} app tps {app_tps:.1} table {table_tps:.1}  <-- MISMATCH",
                    entry.name
                );
            }
        }
        if let Some(app_ttft) = entry.median_ttft_s
            && (app_ttft - table_ttft).abs() > 0.6
        {
            speed_mismatched += 1;
            println!(
                "  speed {:<30} app ttft {app_ttft:.2}s table {table_ttft:.2}s  <-- MISMATCH",
                entry.name
            );
        }
    }
    println!(
        "speed: {speed_checked} families' median tok/s reconciled, {speed_mismatched} mismatches"
    );
    assert_eq!(
        speed_mismatched, 0,
        "AA speed snapshot disagrees with the published table"
    );
    assert!(
        speed_checked >= 25,
        "speed reconciliation covered only {speed_checked} families"
    );

    // ------------------------------------------------------------------
    // C–E. Object-scoped payload fields vs the app's stream parses for
    //      Briefcase, AutomationBench, and AA-3D.
    // ------------------------------------------------------------------
    let object_start_re = regex::Regex::new(
        r#"\{\\?"id\\?":\\?"[0-9a-f-]{36}\\?",\\?"slug\\?":\\?"[a-z0-9][a-z0-9._-]*\\?",\\?"name\\?":\\?".{1,120}?\\?",\\?"shortName\\?":\\?""#,
    )
    .expect("object regex");
    let short_name_re =
        regex::Regex::new(r#"\\?"shortName\\?":\\?"(.{1,120}?)\\?""#).expect("name regex");
    let field_re = |field: &str| {
        regex::Regex::new(&format!(
            r#"\\?"{field}\\?":(null|-?[0-9]+\.?[0-9]*(?:[eE][+-]?[0-9]+)?)"#
        ))
        .expect("field regex")
    };
    let brief_field = field_re("briefcaseElo");
    let auto_field = field_re("automationBenchPartialScore");
    let three_d_field = field_re("aa3dElo");

    let eval_pages = [
        (
            "https://artificialanalysis.ai/evaluations/aa-briefcase",
            "AA-Briefcase",
        ),
        (
            "https://artificialanalysis.ai/evaluations/automationbench-aa",
            "AutomationBench",
        ),
        ("https://artificialanalysis.ai/evaluations/aa-3d", "AA-3D"),
        (
            "https://artificialanalysis.ai/evaluations/aa-analyst-agent",
            "AA-Analyst",
        ),
        (
            "https://artificialanalysis.ai/evaluations/ifbench",
            "IFBench",
        ),
        (
            "https://artificialanalysis.ai/evaluations/gpqa-diamond",
            "GPQA",
        ),
        (
            "https://artificialanalysis.ai/evaluations/hle-diamond",
            "HLE-Diamond",
        ),
    ];
    // key -> best object-scoped value per board (max, matching the app's fold)
    let mut ks_brief: BTreeMap<String, f64> = BTreeMap::new();
    let mut ks_auto: BTreeMap<String, f64> = BTreeMap::new();
    let mut ks_3d: BTreeMap<String, f64> = BTreeMap::new();
    let mut briefcase_html = String::new();
    let mut auto_html = String::new();
    let mut aa3d_html = String::new();
    for (url, label) in eval_pages {
        let html = match fetch_text(&client, url, label) {
            Ok(html) => html,
            Err(err) => {
                println!("{label}: FETCH FAILED {err:#}");
                continue;
            }
        };
        match label {
            "AA-Briefcase" => briefcase_html = html.clone(),
            "AutomationBench" => auto_html = html.clone(),
            "AA-3D" => aa3d_html = html.clone(),
            _ => {}
        }
        let starts: Vec<usize> = object_start_re
            .find_iter(&html)
            .map(|m| m.start())
            .collect();
        for (idx, &start) in starts.iter().enumerate() {
            let end = starts.get(idx + 1).copied().unwrap_or(html.len());
            let chunk = &html[start..end.min(html.len())];
            let Some(name) = short_name_re
                .captures(chunk)
                .and_then(|c| c.get(1).map(|m| m.as_str().to_owned()))
            else {
                continue;
            };
            let Some((key, _, _, _)) = classify_aa_name(&name) else {
                continue;
            };
            if key.is_empty() {
                continue;
            }
            for (re, map) in [
                (&brief_field, &mut ks_brief),
                (&auto_field, &mut ks_auto),
                (&three_d_field, &mut ks_3d),
            ] {
                if let Some(caps) = re.captures(chunk)
                    && caps.extract::<1>().1[0] != "null"
                    && let Ok(v) = caps.extract::<1>().1[0].parse::<f64>()
                {
                    let better = map.get(&key).is_none_or(|old| v > *old);
                    if better {
                        map.insert(key.clone(), v);
                    }
                }
            }
        }
    }

    let reconcile = |board: &str,
                     app_rows: Vec<Benchmark>,
                     truth: &BTreeMap<String, f64>,
                     tolerance: f64|
     -> (usize, usize, usize) {
        let app: BTreeMap<String, f64> = app_rows
            .iter()
            .filter_map(|r| {
                let key = benchmark_model_key(&r.slug);
                let v = r.agentic_coding?;
                (!key.is_empty()).then_some((key, v))
            })
            .collect();
        let mut checked = 0;
        let mut mismatched = 0;
        let mut missed = 0;
        for (key, truth_v) in truth {
            match app.get(key) {
                Some(app_v) => {
                    checked += 1;
                    if (app_v - truth_v).abs() > tolerance {
                        mismatched += 1;
                        println!(
                            "  {board} {:<34} app {app_v:.2} object-scoped {truth_v:.2}  <-- MISMATCH",
                            key
                        );
                    }
                }
                None => {
                    missed += 1;
                    println!(
                        "  {board} {:<34} object-scoped {truth_v:.2} but the app parse missed it",
                        key
                    );
                }
            }
        }
        println!(
            "{board}: {checked} families reconciled, {mismatched} mismatches, {missed} object-scoped families the app parse missed"
        );
        (checked, mismatched, missed)
    };

    // AA's chart-preload objects store AutomationBench partial scores as
    // 0–1 fractions (the page headline and the app's parse use percent).
    let ks_auto_pct: BTreeMap<String, f64> = ks_auto
        .iter()
        .map(|(k, v)| (k.clone(), v * 100.0))
        .collect();
    let (b_checked, b_bad, _b_missed) = reconcile(
        "AA-Briefcase",
        parse_aa_briefcase(&briefcase_html),
        &ks_brief,
        // The chart-preload Elo drifts a few points behind the rendered
        // table AA headlines (observed up to ~12); gross errors still trip.
        15.0,
    );
    let (a_checked, a_bad, _a_missed) = reconcile(
        "AutomationBench",
        parse_automationbench_aa(&auto_html),
        &ks_auto_pct,
        0.5,
    );
    let (d_checked, d_bad, d_missed) = reconcile("AA-3D", parse_aa_3d(&aa3d_html), &ks_3d, 0.5);

    assert_eq!(
        b_bad, 0,
        "AA-Briefcase values disagree with the object-scoped payload fields"
    );
    assert_eq!(
        a_bad, 0,
        "AutomationBench values disagree with the object-scoped payload fields"
    );
    assert_eq!(
        d_bad, 0,
        "AA-3D values disagree with the object-scoped payload fields"
    );
    // Object-scoped "missed" families are channel name variants (e.g. the
    // preload names carry "(with fallback)" the table strips), so only AA-3D
    // — where both channels classify identical names — must miss none.
    assert_eq!(d_missed, 0, "AA-3D parse missed object-scoped families");
    assert!(
        b_checked >= 20 && a_checked >= 15 && d_checked >= 15,
        "reconciliation coverage too thin: briefcase {b_checked}, automation {a_checked}, 3d {d_checked}"
    );
}

/// Comprehensive model-join audit: fetch the live Surplus catalogue and every
/// benchmark board, join them exactly the way the chart does, and inspect
/// every cross-key (fuzzy) join. A join is SUSPICIOUS when the two keys'
/// version-number sequences differ and release tags cannot explain it — the
/// shape of the `Sonnet 5` ↔ `Sonnet 5.5` set-collapse the count-aware
/// matching now rejects. Dated releases, deployment-token drops, and alias
/// joins must all survive.
#[test]
#[ignore = "fetches live upstreams"]
fn live_model_join_audit() {
    use crate::bench::matching::{
        benchmark_model_key, best_benchmark_match, keys_differ_only_by_release_tag,
    };
    use crate::types::{BenchmarkSource, Settings};
    use std::collections::HashSet;

    fn version_seq(key: &str) -> Vec<&str> {
        key.split_whitespace()
            .filter(|t| !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()))
            .collect()
    }

    let client = validation_client();
    let settings = Settings::default();
    let snapshot = super::fetch_prices(&client, &settings).expect("Surplus price fetch failed");
    println!("catalogue: {} quotes", snapshot.quotes.len());

    let mut suspicious_total = 0usize;
    let mut quotes_with_any_match = HashSet::new();
    for source in BenchmarkSource::remote_sources() {
        let rows = match super::fetch_benchmark_source(&client, source) {
            Ok(rows) => rows,
            Err(err) => {
                println!("{}: FETCH FAILED {err:#}", source.label());
                continue;
            }
        };
        let mut fuzzy = 0usize;
        let mut suspicious = 0usize;
        let mut matched_quotes = 0usize;
        let mut matched_rows = HashSet::new();
        for quote in &snapshot.quotes {
            let Some(bench) = best_benchmark_match(quote, &rows) else {
                continue;
            };
            matched_quotes += 1;
            quotes_with_any_match.insert(benchmark_model_key(&quote.display_name));
            matched_rows.insert(benchmark_model_key(&bench.slug));
            // Judge the key pair that actually produced the join, mirroring
            // best_benchmark_match: exact branch (four key combinations) or
            // the fuzzy pair (max of name/slug jaccards).
            let q_model = benchmark_model_key(&quote.model);
            let q_name = benchmark_model_key(&quote.display_name);
            let b_slug = benchmark_model_key(&bench.slug);
            let b_name = benchmark_model_key(&bench.name);
            let exact =
                q_model == b_slug || q_name == b_slug || q_model == b_name || q_name == b_name;
            if exact {
                continue;
            }
            fuzzy += 1;
            let (pair_a, pair_b) = if crate::bench::matching::token_jaccard(&q_name, &b_name)
                >= crate::bench::matching::token_jaccard(&q_model, &b_slug)
            {
                (&q_name, &b_name)
            } else {
                (&q_model, &b_slug)
            };
            let explainable = keys_differ_only_by_release_tag(pair_a, pair_b)
                || version_seq(pair_a) == version_seq(pair_b);
            if !explainable {
                suspicious += 1;
                suspicious_total += 1;
                println!(
                    "  SUSPICIOUS  {} ({}) -> {} ({})   [{}]",
                    pair_a,
                    quote.display_name,
                    pair_b,
                    bench.slug,
                    source.label()
                );
            }
        }
        let unmatched_rows = rows.len() - matched_rows.len();
        println!(
            "{}: {} rows, {} quotes matched, {fuzzy} cross-key joins, {suspicious} suspicious, {unmatched_rows} rows with no listing",
            source.label(),
            rows.len(),
            matched_quotes
        );
    }
    println!(
        "catalogue models with no benchmark evidence on any board: {}",
        snapshot.quotes.len() - quotes_with_any_match.len()
    );
    assert_eq!(
        suspicious_total, 0,
        "{suspicious_total} board joins pair different model versions"
    );
}
