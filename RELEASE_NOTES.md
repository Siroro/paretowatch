# ParetoWatch v0.17.1

## Fixes

- Composite ranking corrected for models no leaderboard has measured yet. Claude Fable 5.1 (AA Intelligence 66, the highest score in the snapshot) was landing at 90.6 under Opus 5's 97.1 because unmeasured models were shrunk toward the neutral midpoint and the AA insertion percentile overshot past 100 ((n+1)/n on a 24-model cohort, then discounted). Unmeasured models now enter exactly at their anchored AA standing — Fable 5.1 enters at the 100th percentile and tops the composite — with the standing disclosed honestly ("sparse evidence · 0 sources · prior AA 100th · N boards pending").
- The safeguards against thin evidence are unchanged: a mid-AA newcomer still enters mid-pack below measured leaders, a hot two-board model still cannot outrank broad top coverage, and partial coverage still gets a proportional confidence discount. Only the zero-rows-anywhere case changed.

## Validation

Formatting, locked check, locked test (119 tests), Clippy with warnings denied, and locked build all pass, including a new regression test pinning the AA-leader-tops-composite behavior and the preserved thin-evidence guards. CI green on the fix commit.
