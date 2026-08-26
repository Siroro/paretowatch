//! Message protocol shared between the UI thread and the background fetch
//! worker.

use crate::types::{Benchmark, BenchmarkSource, PriceSnapshot, Settings};

#[derive(Debug)]
pub(crate) enum WorkerCommand {
    Refresh,
    UpdateSettings(Settings),
    Quit,
}

#[derive(Debug)]
pub(crate) enum WorkerMessage {
    Prices(Result<PriceSnapshot, String>),
    Benchmarks(BenchmarkSource, Result<Vec<Benchmark>, String>),
}
