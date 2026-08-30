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

/// Dollar amounts for cost readouts: keeps enough decimals that a cheap
/// model's cents still show, without float noise.
pub(crate) fn format_usd(value: f64) -> String {
    if !value.is_finite() {
        return String::new();
    }
    let abs = value.abs();
    if abs >= 1_000.0 {
        format!("${value:.0}")
    } else if abs >= 10.0 {
        format!("${value:.2}")
    } else if abs >= 0.01 {
        format!("${value:.3}")
    } else if abs >= 0.001 {
        format!("${value:.4}")
    } else {
        format!("${value:.6}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_usd_steps_decimals_by_magnitude() {
        assert_eq!(format_usd(1234.0), "$1234");
        assert_eq!(format_usd(45.678), "$45.68");
        assert_eq!(format_usd(1.2345), "$1.234");
        assert_eq!(format_usd(0.01234), "$0.012");
        assert_eq!(format_usd(0.0001234), "$0.000123");
        assert_eq!(format_usd(f64::NAN), "");
    }
}
