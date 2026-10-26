//! Message protocol shared between the UI thread and the background fetch
//! worker.

use crate::types::{Benchmark, BenchmarkSource, ModelPerformance, PriceSnapshot, Settings};

#[derive(Debug)]
pub(crate) enum WorkerCommand {
    Refresh,
    UpdateSettings(Settings),
    /// Poll the Provider Performance API for these models (on-demand
    /// selection or the slow background refresh of cached rows).
    FetchPerformance(Vec<String>),
    Quit,
}

#[derive(Debug)]
pub(crate) enum WorkerMessage {
    Prices(Result<PriceSnapshot, String>),
    Benchmarks(BenchmarkSource, Result<Vec<Benchmark>, String>),
    /// Provider breakdowns that fetched successfully, plus every model the
    /// pull attempted: an attempted model with no row failed, so its last
    /// cached row survives upstream and the caller can back off retrying.
    Performance(Vec<ModelPerformance>, Vec<String>),
}
