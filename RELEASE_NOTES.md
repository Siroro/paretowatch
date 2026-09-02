# ParetoWatch v0.17.2

## Fixes

- Composite ranking corrected for the AA leader when it has thin board coverage. Claude Fable 5.1 (AA Intelligence 66) was showing 90.6 under Opus 5's 97.1 even after the v0.17.1 fix, because LiveBench had already published a Fable 5.1 row: a single thin board put it on the partial-coverage path, whose confidence ramp shrank the score toward population-neutral. Thin coverage now reverts toward the model's PRIOR standing — weak evidence moves a model partway from what the AA scale already established toward the measurement — plus a small ordinal uncertainty discount so near-tied models still order the broader-evidenced one first. Verified against the live leaderboards: Fable 5.1 tops the composite at 99.5, Opus 5 at 97.1.
- A new live-data smoke test (`cargo test -- --ignored`) runs the real board fetchers through the composite and pins the leader, so ranking rulings can be checked against production data before shipping.

## Validation

Formatting, locked check, locked test (120 tests), Clippy with warnings denied, and locked build all pass. All prior ranking regressions are preserved: the mid-AA newcomer enters mid-pack below measured leaders, the hot two-board model stays below broad top coverage, and the GLM-5.3/Sol thin-evidence ordering keeps its ordering via the uncertainty discount.
