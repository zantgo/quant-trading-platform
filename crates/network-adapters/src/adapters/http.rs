// COLD PATH — shared HTTP helper for the venue REST probes (symbol checks,
// historical backfills, health checks). Nothing here runs on the hot path.
use std::time::Duration;

/// v11.12.19: per-attempt timeouts for the venue REST probes. Slow networks
/// (cold DNS can take seconds through a VPN) used to trip the old single
/// 10 s request before the venue ever answered.
pub const PROBE_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
pub const PROBE_TOTAL_TIMEOUT: Duration = Duration::from_secs(30);
pub const PROBE_ATTEMPTS: u32 = 3;

/// Backoff before the next attempt: 1 s, then 2 s.
pub fn probe_backoff(attempt_index: u32) -> Duration {
    Duration::from_millis(1000 << attempt_index.saturating_sub(1))
}

/// reqwest's `Display` hides the root cause behind the generic
/// "error sending request for url …". Walk the source chain and classify the
/// failure so the console (and the UI error) name the real problem —
/// dns / connect / timeout — instead of a vague transport line.
pub fn describe_reqwest_error(e: &reqwest::Error) -> String {
    let kind = if e.is_timeout() {
        "timeout"
    } else if e.is_connect() {
        "connect"
    } else if e.is_decode() {
        "decode"
    } else if e.is_body() {
        "body"
    } else if e.is_request() {
        "request"
    } else {
        "transport"
    };
    let mut root = String::new();
    let mut src: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(e);
    while let Some(s) = src {
        root = s.to_string();
        src = std::error::Error::source(s);
    }
    if root.is_empty() {
        format!("{}: {}", kind, e)
    } else {
        format!("{}: {} (cause: {})", kind, e, root)
    }
}
