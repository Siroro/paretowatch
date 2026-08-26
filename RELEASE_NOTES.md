# ParetoWatch v0.7.0

## Highlights

- Refactored the application into focused modules for app state, tabs, alerts, Pareto analysis, settings, themes, tray integration, workers, and formatting.
- Fixed pinned pricing-window sizing by measuring rendered content, accounting for window chrome and separators, capping visible rows, and retaining scrolling for larger result sets.
- Improved provider-aware pricing with workload-blended rates, prompt-cache reads, catalog modality filtering, market overlays, healthy-seller qualification, and safe fallback behavior.
- Enforced the Surplus market cache interval when scheduling price refreshes.
- Expanded benchmark scoring, matching, diagnostics, and feed handling across current benchmark sources.
- Improved history tracking and comparison views for benchmark and price changes.
- Refined alert handling, persistence, summaries, and tray behavior while the main window is hidden.
- Continued the v0.6.0 cleanup of stale benchmark data and improved Pareto chart and market-detail interactions.

## Validation

The v0.7.0 source refactor and pricing-window fixes are validated with the repository's formatting, check, test, and release-build commands. GitHub release packages are produced for Windows and Linux by the `v0.7.0` tag workflow.
