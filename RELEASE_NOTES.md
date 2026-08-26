# ParetoWatch v0.8.0

## Highlights

- Added a configurable pinned-price font size in Settings, persisted with the rest of the application preferences.
- Made pinned-price windows responsive to the selected font size and measured content, while preserving capped visible rows and scrolling for larger watchlists.
- Improved pinned-price state propagation, row sizing, and refresh behavior, with coverage for responsive dimensions and window-size estimates.
- Reduced history UI work by reusing cached ordering, borrowing model data where possible, and windowing metric series rendering.
- Added safer history behavior when a previously selected model is no longer available.
- Refined settings presentation, feed-status display, and pricing-mix controls.

## Validation

The v0.8.0 changes are validated with the repository's formatting, check, test, and release-build commands. GitHub release packages are produced for Windows and Linux by the `v0.8.0` tag workflow.
