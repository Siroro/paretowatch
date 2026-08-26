//! Creator identification, the chart colour palette, and the small badge
//! widgets derived from both.

use eframe::egui;

use crate::bench::normalize;

pub(crate) fn infer_creator(model: &str) -> String {
    let n = normalize(model);
    if n.starts_with("claude ")
        || n.starts_with("opus ")
        || n.starts_with("sonnet ")
        || n.starts_with("haiku ")
        || n.starts_with("fable ")
    {
        "Anthropic".into()
    } else if n.starts_with("gpt ")
        || n.starts_with("o1 ")
        || n.starts_with("o3 ")
        || n.starts_with("o4 ")
    {
        "OpenAI".into()
    } else if n.starts_with("gemini ") {
        "Google".into()
    } else if n.starts_with("grok ") {
        "xAI".into()
    } else if n.starts_with("deepseek ") {
        "DeepSeek".into()
    } else if n.starts_with("qwen") {
        "Alibaba".into()
    } else if n.starts_with("kimi ") {
        "Moonshot".into()
    } else if n.starts_with("glm ") {
        "Z.AI".into()
    } else if n.starts_with("minimax ") {
        "MiniMax".into()
    } else if n.starts_with("mimo ") {
        "Xiaomi".into()
    } else if n.starts_with("nemotron ") || n.starts_with("nvidia ") {
        "NVIDIA".into()
    } else if n.starts_with("mistral ") {
        "Mistral AI".into()
    } else if n.starts_with("arcee ") || n.starts_with("trinity ") {
        "Arcee AI".into()
    } else {
        String::new()
    }
}

/// Brand-ish colors for the major labs so model groups read at a glance on
/// the chart. Everyone else gets a deterministic hue from a hash of the name.
pub(crate) fn creator_color(creator: &str) -> egui::Color32 {
    let n = normalize(creator);
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

pub(crate) fn group_label(creator: &str) -> &str {
    if creator.is_empty() {
        "Unknown"
    } else {
        creator
    }
}

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

/// The `*` marking a market that also lists a zero-priced ask. Priced rows
/// keep their real prices; the tooltip explains what was excluded.
pub(crate) fn free_offer_badge(ui: &mut egui::Ui) {
    ui.label(egui::RichText::new("*").strong()).on_hover_text(
        "This market also has a free (100% off) offer. Prices shown exclude it \
             and are the cheapest ask that actually costs money.",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn discount_colors_bucket_by_magnitude() {
        assert_eq!(discount_color(95.0), discount_color(80.0));
        assert_eq!(discount_color(79.9), discount_color(50.0));
        assert_eq!(discount_color(49.9), discount_color(0.0));
        assert_ne!(discount_color(80.0), discount_color(79.9));
        assert_ne!(discount_color(50.0), discount_color(49.9));
    }
}
