//! Small display-formatting helpers shared by the UI and the fetch layer.

pub(crate) fn format_price_tick(price: f64) -> String {
    if !price.is_finite() {
        return String::new();
    }
    let abs = price.abs();
    if abs >= 100.0 {
        format!("${price:.0}")
    } else if abs >= 10.0 {
        format!("${price:.1}")
    } else if abs >= 1.0 {
        format!("${price:.2}")
    } else if abs >= 0.01 {
        format!("${price:.3}")
    } else {
        format!("${price:.5}")
    }
}

pub(crate) fn format_compact_number(value: f64) -> String {
    let abs = value.abs();
    if abs >= 1_000_000_000.0 {
        format!("{:.1}B", value / 1_000_000_000.0)
    } else if abs >= 1_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else if abs >= 1_000.0 {
        format!("{:.1}K", value / 1_000.0)
    } else {
        format!("{value:.0}")
    }
}
