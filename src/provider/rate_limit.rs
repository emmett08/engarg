//! Classify bounded provider errors and calculate retry delays without exposing response text.
use reqwest::header::HeaderMap;
use serde_json::Value;
use std::{
    collections::hash_map::RandomState,
    hash::BuildHasher,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime},
};

pub(super) fn error_code(bytes: &[u8]) -> Option<&'static str> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    // Billing/quota takes precedence even when a contradictory retry code is supplied.
    for candidate in [
        value["error"]["code"].as_str(),
        value["error"]["type"].as_str(),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(code) = PERMANENT.iter().find(|&&code| code == candidate) {
            return Some(*code);
        }
    }
    ["rate_limit_exceeded", "rate_limit_error", "slow_down"]
        .into_iter()
        .find(|code| value["error"]["code"] == *code || value["error"]["type"] == *code)
}
const PERMANENT: &[&str] = &[
    "insufficient_quota",
    "credit_balance_exhausted",
    "organization_spend_limit_exceeded",
    "project_spend_limit_exceeded",
    "organization_usage_limit_exceeded",
    "billing_hard_limit_reached",
    "billing_not_active",
    "billing_limit_exceeded",
    "quota_exceeded",
];
pub(super) fn permanent(code: Option<&str>) -> bool {
    code.is_some_and(|code| PERMANENT.contains(&code))
}

pub(super) fn delay(
    headers: &HeaderMap,
    retry: u8,
    cap: Duration,
    now: SystemTime,
) -> Result<Duration, &'static str> {
    let server = headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| {
            let s = s.trim();
            if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
                Some(
                    s.parse::<u64>()
                        .map(Duration::from_secs)
                        .map_err(|_| "server_delay_exceeds_limit"),
                )
            } else {
                httpdate::parse_http_date(s)
                    .ok()
                    .map(|time| Ok(time.duration_since(now).unwrap_or_default()))
            }
        });
    let base = match server {
        Some(Ok(delay)) => delay,
        Some(Err(reason)) => return Err(reason),
        None => Duration::from_millis(
            (1000_u64.saturating_mul(1_u64 << u32::from(retry.saturating_sub(1).min(10))))
                .min((cap.as_millis() / 2) as u64),
        ),
    };
    if base > cap {
        return Err("server_delay_exceeds_limit");
    }
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let random = RandomState::new().hash_one((now, SEQUENCE.fetch_add(1, Ordering::Relaxed)));
    let jitter = Duration::from_millis(10 + random % 241).min(cap - base);
    Ok(base + jitter)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn permanent_codes_override_temporary_labels_and_unknown_text_stays_private() {
        assert_eq!(
            error_code(
                br#"{"error":{"type":"insufficient_quota","code":"slow_down","message":"secret"}}"#
            ),
            Some("insufficient_quota")
        );
        assert_eq!(error_code(br#"{"error":{"code":"secret"}}"#), None);
        assert!(permanent(error_code(
            br#"{"error":{"code":"project_spend_limit_exceeded"}}"#
        )));
    }
    #[test]
    fn seconds_dates_overflow_and_invalid_hints_preserve_bounds() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1700000000);
        let mut headers = HeaderMap::new();
        headers.insert(
            "retry-after",
            "999999999999999999999999999999999999".parse().unwrap(),
        );
        assert!(delay(&headers, 1, Duration::from_secs(120), now).is_err());
        headers.insert(
            "retry-after",
            httpdate::fmt_http_date(now + Duration::from_secs(10))
                .parse()
                .unwrap(),
        );
        let d = delay(&headers, 1, Duration::from_secs(120), now).unwrap();
        assert!(d >= Duration::from_secs(10) && d <= Duration::from_millis(10250));
        headers.insert("retry-after", "invalid".parse().unwrap());
        let d = delay(&headers, 1, Duration::from_secs(120), now).unwrap();
        assert!(d >= Duration::from_secs(1) && d <= Duration::from_millis(1250));
        headers.insert("retry-after", "121".parse().unwrap());
        assert!(delay(&headers, 1, Duration::from_secs(120), now).is_err());
    }
}
