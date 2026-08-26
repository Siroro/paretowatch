# ParetoWatch v0.9.0

## Highlights

- Propagated the active liquidity filter into pinned-price windows so displayed prices follow the same provider-quality policy as the Pareto view.
- Re-select the cheapest qualifying provider for pinned rows and clearly show an unpriced state when no provider satisfies the selected filter.
- Corrected cache-inclusive market discounts to use the market-wide best cache-read ask when the selected provider does not publish a cache price.
- Kept discount labels visible by moving price-change details into hover text and clarified blended fresh-input presentation.
- Added regression coverage for filtered pinned rows, cache-inclusive discounts, provider fallback behavior, and market pricing edge cases.
- Refined Pareto and pricing presentation for consistent provider, discount, and workload-mix semantics.

## Validation

The v0.9.0 pricing and pinned-widget changes are validated with the repository's formatting, locked check, locked test, Clippy, and locked build commands. GitHub release packages are produced for Windows and Linux by the `v0.9.0` tag workflow.
