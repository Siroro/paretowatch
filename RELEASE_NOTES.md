# ParetoWatch v0.16.0

## Features

- Added Terminal-Bench 4.0 as a benchmark source. It reads the live Harbor leaderboard (model + agent rows, accuracy metric, per-task token estimates) and joins the composites at the same weight as Terminal-Bench 3.0. Unlike 3.0 it has no published snapshot fallback, so a failed read surfaces as an error instead of stale-generation data.
- Pareto legend chips now toggle creator groups on and off. Hidden groups leave both the chart and the frontier — a hidden run can no longer hold a frontier spot while its orbs are off the chart — and remain re-enactable from the legend. Group labels are canonicalized so catalog spelling variants cannot split a hidden family back onto the chart, and hidden groups participate in the Pareto cache key.

## Validation

Formatting, locked check, locked test, Clippy with warnings denied, and locked build all pass. The Terminal-Bench 4.0 source was additionally verified against the live Harbor leaderboard. GitHub release packages continue to be produced for Windows and Linux by the tag workflow.
