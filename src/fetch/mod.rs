//! Fetch layer: benchmark source dispatch plus the shared HTTP and parsing
//! helpers the board fetchers build on. The poll loop lives in
//! [`crate::worker`].

pub(crate) mod aa_evals;
pub(crate) mod aa_index;
pub(crate) mod aa_names;
pub(crate) mod aa_speed;
pub(crate) mod deepswe;
pub(crate) mod design_arena;
pub(crate) mod frontiercode;
pub(crate) mod http;
#[cfg(test)]
mod live_validation;
pub(crate) mod livebench;
pub(crate) mod parse;
pub(crate) mod performance;
pub(crate) mod prices;
pub(crate) mod swebench_pro;
pub(crate) mod terminal_bench;
pub(crate) mod vals_sre;

use anyhow::{Result, anyhow};
use reqwest::blocking::Client;

use crate::types::{Benchmark, BenchmarkSource};

use aa_evals::{fetch_aa_3d, fetch_aa_briefcase, fetch_automationbench_aa};
use aa_index::fetch_artificial_analysis_index;
use deepswe::fetch_deepswe;
use design_arena::fetch_design_arena;
use frontiercode::{fetch_frontiercode, fetch_frontiercode2};
use livebench::fetch_livebench;
use swebench_pro::fetch_swebench_pro;
use terminal_bench::fetch_terminal_bench_4;
use vals_sre::fetch_vals_sre_bench;

pub(crate) fn fetch_benchmark_source(
    client: &Client,
    source: BenchmarkSource,
) -> Result<Vec<Benchmark>> {
    match source {
        BenchmarkSource::ArtificialAnalysisSnapshot => fetch_artificial_analysis_index(client),
        BenchmarkSource::TerminalBench4 => fetch_terminal_bench_4(client),
        BenchmarkSource::DeepSWE11 => fetch_deepswe(client),
        BenchmarkSource::LiveBench => fetch_livebench(client),
        BenchmarkSource::DesignArena => fetch_design_arena(client),
        BenchmarkSource::FrontierCode => fetch_frontiercode(client),
        BenchmarkSource::FrontierCode2 => fetch_frontiercode2(client),
        BenchmarkSource::AABriefcase => fetch_aa_briefcase(client),
        BenchmarkSource::AutomationBenchAA => fetch_automationbench_aa(client),
        BenchmarkSource::AA3D => fetch_aa_3d(client),
        BenchmarkSource::SWEBenchPro => fetch_swebench_pro(client),
        BenchmarkSource::ValsSREBench => fetch_vals_sre_bench(client),
        BenchmarkSource::CompositeAgentic | BenchmarkSource::CompositeDeployment => {
            Err(anyhow!("Composite is derived locally and is not fetched"))
        }
    }
}
