//! Shared HTTP helpers for the board fetchers: capped-body GET/POST with
//! consistent error context.

use std::io::Read;

use anyhow::{Context, Result, anyhow};
use reqwest::blocking::Client;
use serde_json::Value;

/// Hard cap on buffered response bodies. The largest fetch (the AA
/// leaderboard page) is a few megabytes, so anything bigger means the
/// endpoint drifted or lied about its content: error out instead of
/// ballooning memory.
const MAX_RESPONSE_BYTES: u64 = 25 * 1024 * 1024;

pub(crate) fn fetch_json(client: &Client, url: &str, label: &str) -> Result<Value> {
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("{label} request failed ({url})"))?
        .error_for_status()
        .with_context(|| format!("{label} returned an HTTP error ({url})"))?;
    if let Some(len) = response.content_length() {
        ensure_body_within_cap(len, label, url)?;
    }
    let body = read_capped_body(response, label, url)?;
    let text = decode_body(body, label, url)?;
    serde_json::from_str(&text).with_context(|| format!("{label} returned invalid JSON ({url})"))
}

pub(crate) fn fetch_text(client: &Client, url: &str, label: &str) -> Result<String> {
    let response = client
        .get(url)
        .send()
        .with_context(|| format!("{label} request failed ({url})"))?
        .error_for_status()
        .with_context(|| format!("{label} returned an HTTP error ({url})"))?;
    if let Some(len) = response.content_length() {
        ensure_body_within_cap(len, label, url)?;
    }
    let body = read_capped_body(response, label, url)?;
    decode_body(body, label, url)
}

pub(crate) fn fetch_json_post(
    client: &Client,
    url: &str,
    body: &Value,
    label: &str,
) -> Result<Value> {
    let response = client
        .post(url)
        .json(body)
        .send()
        .with_context(|| format!("{label} request failed ({url})"))?
        .error_for_status()
        .with_context(|| format!("{label} returned an HTTP error ({url})"))?;
    if let Some(len) = response.content_length() {
        ensure_body_within_cap(len, label, url)?;
    }
    let bytes = read_capped_body(response, label, url)?;
    let text = decode_body(bytes, label, url)?;
    serde_json::from_str(&text).with_context(|| format!("{label} returned invalid JSON ({url})"))
}

fn ensure_body_within_cap(len: u64, label: &str, url: &str) -> Result<()> {
    if len > MAX_RESPONSE_BYTES {
        return Err(anyhow!(
            "{label} response of {len} bytes exceeds the {MAX_RESPONSE_BYTES}-byte cap ({url})"
        ));
    }
    Ok(())
}

/// Buffer the response body under [`MAX_RESPONSE_BYTES`]. The stream is read
/// one byte past the cap so a body that lies about (or omits) its
/// Content-Length is detected rather than silently truncated.
pub(super) fn read_capped_body<R: Read>(response: R, label: &str, url: &str) -> Result<Vec<u8>> {
    let mut limited = response.take(MAX_RESPONSE_BYTES + 1);
    let mut body = Vec::new();
    limited
        .read_to_end(&mut body)
        .with_context(|| format!("Could not read {label} response ({url})"))?;
    ensure_body_within_cap(body.len() as u64, label, url)?;
    Ok(body)
}

/// Decode a capped body the way reqwest's `text()` does for the UTF-8
/// responses these feeds serve, including stripping a leading BOM.
fn decode_body(body: Vec<u8>, label: &str, url: &str) -> Result<String> {
    String::from_utf8(body)
        .map(|text| {
            text.strip_prefix('\u{feff}')
                .map(str::to_owned)
                .unwrap_or(text)
        })
        .with_context(|| format!("Could not read {label} response ({url})"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_bodies_are_rejected_with_source_named() {
        let url = "https://example.test/feed";
        assert!(ensure_body_within_cap(MAX_RESPONSE_BYTES, "Feed", url).is_ok());
        let err =
            ensure_body_within_cap(MAX_RESPONSE_BYTES + 1, "Feed", url).expect_err("over the cap");
        let message = format!("{err:#}");
        assert!(message.contains("Feed") && message.contains(url));
    }

    #[test]
    fn body_reads_stop_at_the_cap_even_when_the_stream_does_not() {
        // A stream with no end: the Take limit must stop the read and the
        // cap check must then reject it instead of truncating silently.
        let stream = std::io::repeat(0u8).take(MAX_RESPONSE_BYTES + 2);
        let err = read_capped_body(stream, "Feed", "https://example.test/feed")
            .expect_err("unbounded stream");
        let message = format!("{err:#}");
        assert!(message.contains("Feed") && message.contains("cap"));
    }

    #[test]
    fn bodies_within_the_cap_are_buffered_whole() {
        let stream = std::io::Cursor::new(b"payload".to_vec());
        let body =
            read_capped_body(stream, "Feed", "https://example.test/feed").expect("small body");
        assert_eq!(body, b"payload");
    }
}
