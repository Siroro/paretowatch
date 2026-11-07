//! The chart colour palette and the small badge widgets derived from it.
//! Creator identification (who a row belongs to) lives in
//! [`crate::model_id`]; this module only decides what it looks like.

use eframe::egui;

use crate::model_id::{canonical_group, normalize};

/// Brand-ish colors for the major labs so model groups read at a glance on
/// the chart. Everyone else gets a deterministic hue from a hash of the name.
pub(crate) fn creator_color(creator: &str) -> egui::Color32 {
    let n = normalize(canonical_group(creator));
    let known: &[(&str, u8, u8, u8)] = &[
        ("anthropic", 224, 122, 95),
        ("openai", 46, 204, 147),
        ("google", 93, 156, 246),
        ("deepseek", 124, 148, 255),
        ("alibaba", 255, 139, 22),
        ("xai", 176, 182, 194),
        ("mistral ai", 250, 200, 60),
        ("moonshot", 158, 134, 255),
        ("z ai", 92, 186, 255),
        ("zai", 92, 186, 255),
        ("nvidia", 132, 193, 30),
        ("minimax", 236, 112, 197),
        ("xiaomi", 255, 133, 51),
        ("arcee ai", 38, 202, 217),
        ("meta", 96, 148, 255),
        ("microsoft", 130, 181, 255),
        ("cohere", 255, 205, 184),
        ("ai21", 30, 200, 200),
        ("typesafe", 49, 120, 198),
    ];
    if let Some(&(_, r, g, b)) = known.iter().find(|(name, _, _, _)| *name == n) {
        return egui::Color32::from_rgb(r, g, b);
    }
    if n.is_empty() {
        return egui::Color32::from_rgb(148, 156, 170);
    }
    // Golden-angle hue spread from a stable hash keeps colors distinct across
    // refreshes without maintaining a central registry.
    let mut hash: u32 = 2166136261;
    for byte in n.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    let hue = (hash % 360) as f32;
    let (r, g, b) = hsl_to_rgb(hue, 0.62, 0.62);
    egui::Color32::from_rgb(r, g, b)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h / 60.0) % 6.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to_u8 = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    (to_u8(r1), to_u8(g1), to_u8(b1))
}

/// Semantic price-move colors (buyer's perspective): a price going up is
/// red, a price going down is green.
pub(crate) const PRICE_UP: egui::Color32 = egui::Color32::from_rgb(224, 86, 86);
pub(crate) const PRICE_DOWN: egui::Color32 = egui::Color32::from_rgb(54, 179, 126);

/// Feed-health colors, reusing the price palette: green healthy, red error.
pub(crate) const STATUS_OK: egui::Color32 = PRICE_DOWN;
pub(crate) const STATUS_BAD: egui::Color32 = PRICE_UP;

/// Market-discount label color by magnitude: deep discounts are green,
/// mid-range ones orange, shallow ones red.
pub(crate) fn discount_color(discount: f64) -> egui::Color32 {
    if discount >= 80.0 {
        egui::Color32::from_rgb(54, 179, 126)
    } else if discount >= 50.0 {
        egui::Color32::from_rgb(240, 160, 60)
    } else {
        egui::Color32::from_rgb(224, 86, 86)
    }
}

/// Freshness color by model age in days. A ripening ramp, not good/bad:
/// just-launched models glow green, then lime, amber, and orange as the
/// release recedes, and anything past a year settles into neutral gray.
/// Bucketed (not continuous) so the chart legend, card chip, and any other
/// age-colored element agree without sharing state.
pub(crate) fn age_color(age_days: f64) -> egui::Color32 {
    match age_days {
        d if d <= 31.0 => egui::Color32::from_rgb(74, 222, 128),
        d if d <= 92.0 => egui::Color32::from_rgb(163, 230, 53),
        d if d <= 183.0 => egui::Color32::from_rgb(234, 179, 8),
        d if d <= 366.0 => egui::Color32::from_rgb(249, 115, 22),
        _ => egui::Color32::from_rgb(138, 143, 152),
    }
}

/// Human age for a whole-day count: `3d`, `7 wks`, `5 mo`, `1.2 yr`.
pub(crate) fn age_label(age_days: f64) -> String {
    if age_days < 14.0 {
        format!("{}d", age_days.round() as i64)
    } else if age_days < 60.0 {
        format!("{:.0} wks", (age_days / 7.0).round())
    } else if age_days < 366.0 {
        format!("{:.0} mo", (age_days / 30.44).round())
    } else {
        format!("{:.1} yr", age_days / 365.25)
    }
}

/// The `*` marking a market that also lists a zero-priced ask. Priced rows
/// keep their real prices; the tooltip explains what was excluded.
pub(crate) fn free_offer_badge(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("*").strong()).on_hover_text(
        "This market also has a free (100% off) offer. Prices shown exclude it \
             and are the cheapest ask that actually costs money.",
    );
}

/// The model's Surplus slug in monospace with a one-click copy button — the
/// identifier to hand to the Surplus API. The button flips to a checkmark for
/// a couple of seconds so the copy is visibly acknowledged. The clipboard and
/// check glyphs are emoji: only egui's bundled Noto Emoji / emoji-icon-font
/// faces cover them (plain symbol code points like U+29C9 render as tofu).
pub(crate) fn copyable_slug(ui: &mut egui::Ui, slug: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(slug).monospace().weak())
            .on_hover_text("Model slug as listed on Surplus — click 📋 to copy");
        let id = egui::Id::new("copyable-slug").with(slug);
        let copied_at = ui
            .ctx()
            .data_mut(|data| data.get_temp::<std::time::Instant>(id));
        let acknowledged =
            copied_at.is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(2));
        if acknowledged {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(250));
        }
        let button = if acknowledged { "✔" } else { "📋" };
        if ui.small_button(button).clicked() {
            ui.ctx().copy_text(slug.to_owned());
            ui.ctx()
                .data_mut(|data| data.insert_temp(id, std::time::Instant::now()));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_id::{canonical_group, group_label};

    #[test]
    fn catalog_creator_spelling_variants_share_one_group_and_color() {
        // Real strings observed in the live Surplus catalog: the same lab is
        // spelled differently across model rows.
        for variants in [
            ["xAI", "SpaceXAI", "X.AI"],
            ["Z.ai", "Zhipu AI", "Zhipu"],
            ["Alibaba", "Qwen", "Alibaba Cloud"],
            ["Moonshot", "Moonshot AI", "Kimi"],
            ["OpenAI", "Open AI", "openai"],
            ["Venice AI", "Venice", "api.venice.ai"],
            ["Xiaomi", "MiMo", "MiMo AI"],
        ] {
            let canonical = canonical_group(variants[0]);
            for variant in variants {
                assert_eq!(
                    creator_color(variant),
                    creator_color(canonical),
                    "{variant}"
                );
            }
        }
        // Unknown labs get hash colors, never a known lab's brand color.
        assert_ne!(creator_color("Kuaishou"), creator_color("ByteDance"));
    }

    #[test]
    fn creator_colors_are_stable_and_distinguish_known_groups() {
        // Known labs get fixed brand colors; empty groups get the neutral gray.
        assert_eq!(creator_color("Anthropic"), creator_color("anthropic"));
        assert_ne!(creator_color("Anthropic"), creator_color("OpenAI"));
        assert_ne!(creator_color("Anthropic"), creator_color(""));
        // Unknown creators hash deterministically.
        assert_eq!(creator_color("Some New Lab"), creator_color("Some New Lab"));
    }

    #[test]
    fn mimo_models_color_as_xiaomi() {
        // Some boards label the creator `MiMo`: it must land in the Xiaomi
        // group with its brand color, never a hash color.
        assert_eq!(creator_color("MiMo"), creator_color("Xiaomi"));
        assert_eq!(group_label("MiMo"), "Xiaomi");
    }

    #[test]
    fn discount_colors_bucket_by_magnitude() {
        assert_eq!(discount_color(95.0), discount_color(80.0));
        assert_eq!(discount_color(79.9), discount_color(50.0));
        assert_eq!(discount_color(49.9), discount_color(0.0));
        assert_ne!(discount_color(80.0), discount_color(79.9));
        assert_ne!(discount_color(50.0), discount_color(49.9));
    }

    #[test]
    fn age_colors_and_labels_follow_the_documented_buckets() {
        assert_eq!(age_color(0.0), age_color(31.0));
        assert_eq!(age_color(31.5), age_color(92.0));
        assert_ne!(age_color(31.0), age_color(31.5));
        assert_ne!(age_color(92.5), age_color(92.0));
        assert_ne!(age_color(183.5), age_color(183.0));
        assert_eq!(age_color(400.0), age_color(1_000.0));
        assert_eq!(age_label(0.0), "0d");
        assert_eq!(age_label(6.0), "6d");
        assert_eq!(age_label(49.0), "7 wks");
        assert_eq!(age_label(152.0), "5 mo");
        assert_eq!(age_label(438.0), "1.2 yr");
    }
}
