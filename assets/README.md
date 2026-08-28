# Alert audio assets

These WAV files are embedded into ParetoWatch at compile time with `include_bytes!`.
They are sourced from Robin Lamb's "UI Sound Effects (Button Clicks, User Feedback, Notifications)" pack on OpenGameArt.

Source: https://opengameart.org/content/ui-sound-effects-button-clicks-user-feedback-notifications
License: CC0 1.0 / public domain

Mapping:
- `alert-soft.wav` <- `Ding.wav`
- `alert-chime.wav` <- `chimes.wav`
- `alert-urgent.wav` <- `alarm.wav`

# Fonts

`fonts/DejaVuSans.ttf` is embedded at compile time and installed as a
last-resort fallback font in `ParetoWatchApp::new` (`src/app/mod.rs`).
egui's default Proportional fonts lack the symbol glyphs the UI uses
(↑ ↓ → ↔ ● ◆ ◇ ▽ …), so DejaVu Sans supplies them.

Source: https://github.com/dejavu-fonts/dejavu-fonts/releases (version 2.37, `DejaVuSans.ttf`)
License: Bitstream Vera / public domain (see `fonts/DejaVuSans-LICENSE.txt`)
