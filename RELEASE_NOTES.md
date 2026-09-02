# ParetoWatch v0.17.0

## Features

- Bundled Artificial Analysis snapshot extended for the Surplus listings of 2026-08-27..09-01: Claude Fable 5.1 (AA 66, the new snapshot leader), Gemini 3.6 Flash, Gemini 3.5 Flash Lite, Qwen3.8 Flash, MiMo-V2.5, Muse Glimmer 30B, and NVIDIA Nemotron 3.5 Lightning 30B now carry AA Intelligence scores. The Gemini Flash Lite row also stops the previous fuzzy join onto the full Gemini 3.5 Flash score (52 vs its measured 37), and the MiMo-V2.5 row keeps the non-Pro variant from nearly matching V2.5-Pro. Aion 3.0, Aion 3.0 Mini, and Seed 2.1 Turbo remain unrated (no AA presence) and are documented as such.
- MiMo models now reliably share the Xiaomi brand color. Surplus price rows carry bare `xiaomi-mimo-*` style IDs without a provider field and some boards label the creator "MiMo"; both now canonicalize into the Xiaomi legend group instead of falling back to a hash color or gray "Unknown".

## Validation

Formatting, locked check, locked test (118 tests), Clippy with warnings denied, and locked build all pass. New snapshot rows are covered by a dedicated regression test, and the MiMo/Xiaomi canonicalization is pinned by theme tests. GitHub release packages continue to be produced for Windows and Linux by the tag workflow.
