# ParetoWatch v0.10.0

## Highlights

- Simplified persisted history to record price changes and once-per-day market telemetry only.
- Removed historical capability and deployment composite series from the History view; composite scores remain available as live-derived benchmark results.
- Decoupled history tracking from benchmark refreshes and composite data-version changes, reducing unnecessary history writes.
- Preserved decoding of legacy composite events for storage compatibility while intentionally ignoring them during replay; existing historical composite series will no longer be displayed.
- Updated the History UI and documentation to reflect the available blended, input, output, cache-read, and daily-volume metrics.

## Validation

The v0.10.0 history changes are validated with the repository's formatting, locked check, locked test, Clippy, and locked build commands. GitHub release packages are produced for Windows and Linux by the `v0.10.0` tag workflow.
