// Copyright (c) 2026 Sandy McArthur, Jr.
// SPDX-License-Identifier: MIT

pub(crate) mod api;
pub(crate) mod format;
pub(crate) mod lookup;
pub(crate) mod types;

use std::sync::{Arc, RwLock};

use anyhow::anyhow;
use reqwest::cookie::CookieStore;
use reqwest::header::HeaderValue;
use serde::Deserialize;
use url::Url;

pub const BASE_URL: &str = "https://www.myanonamouse.net";

/// Cookie store that tracks only the `mam_id` session cookie.
///
/// MAM rotates `mam_id` via `Set-Cookie` on responses; reqwest feeds those
/// through this store so subsequent requests carry the rotated value. The
/// current value can be read back at any time via [`SessionJar::current`]
/// for persistence across restarts.
pub struct SessionJar {
    mam_id: RwLock<String>,
}

impl SessionJar {
    fn new(initial: String) -> Self {
        Self {
            mam_id: RwLock::new(initial),
        }
    }

    /// The current (possibly rotated) `mam_id` value.
    pub fn current(&self) -> String {
        self.mam_id.read().unwrap().clone()
    }
}

/// Only ever send or accept the session cookie for MAM hosts — guards against
/// leaking the cookie on a redirect to another domain.
fn is_mam_host(url: &Url) -> bool {
    url.host_str()
        .is_some_and(|h| h == "myanonamouse.net" || h.ends_with(".myanonamouse.net"))
}

impl CookieStore for SessionJar {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        if !is_mam_host(url) {
            return;
        }
        for header in cookie_headers {
            let Ok(s) = header.to_str() else { continue };
            let Some(rest) = s.strip_prefix("mam_id=") else { continue };
            let value = rest.split(';').next().unwrap_or("").trim();
            if value.is_empty() {
                continue;
            }
            *self.mam_id.write().unwrap() = value.to_string();
        }
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        if !is_mam_host(url) {
            return None;
        }
        HeaderValue::from_str(&format!("mam_id={}", self.mam_id.read().unwrap())).ok()
    }
}

/// Build a reqwest Client wired to a [`SessionJar`] seeded with the mam_id
/// session cookie, plus a browser User-Agent. The jar is returned so callers
/// can observe cookie rotation.
pub fn build_client(mam_session: &str) -> anyhow::Result<(reqwest::Client, Arc<SessionJar>)> {
    // Validate up front — the value is sent as an HTTP header on every request.
    HeaderValue::from_str(&format!("mam_id={mam_session}"))
        .map_err(|_| anyhow!("Invalid mam_session value — contains non-ASCII characters"))?;

    let jar = Arc::new(SessionJar::new(mam_session.to_string()));
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0")
        .cookie_provider(jar.clone())
        .build()
        .map_err(|e| anyhow!("Failed to build HTTP client: {e}"))?;
    Ok((client, jar))
}

/// Response from `/json/jsonIp.php`
#[derive(Debug, Deserialize)]
pub struct IpInfo {
    pub ip: String,
    #[serde(rename = "ASN")]
    pub asn: serde_json::Value,
    #[serde(rename = "AS")]
    pub as_org: String,
}

impl IpInfo {
    pub fn asn_string(&self) -> String {
        match &self.asn {
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => s.clone(),
            _ => String::new(),
        }
    }
}

/// Fetch current IP info — used by `--test-connection` and the `get_ip_info` tool.
pub async fn get_ip_info(client: &reqwest::Client) -> anyhow::Result<IpInfo> {
    let resp = client
        .get(format!("{BASE_URL}/json/jsonIp.php"))
        .send()
        .await?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        return Err(anyhow!(enrich_error(status.as_u16(), &body)));
    }

    serde_json::from_str::<IpInfo>(&body)
        .map_err(|e| anyhow!("Failed to parse IP info response: {e}\nBody: {body}"))
}

/// Produce a human-readable error string for MAM HTTP errors, with LLM hints where useful.
pub fn enrich_error(status: u16, body: &str) -> String {
    let hint = match status {
        401 | 403 => Some(
            "The mam_id session cookie is invalid or expired. \
             Ask the user to go to Preferences → Security on MyAnonamouse and provide the updated mam_id value.",
        ),
        429 => Some("Rate limited by MyAnonamouse. Wait before retrying."),
        503 => Some("MyAnonamouse is temporarily unavailable. Try again later."),
        _ => None,
    };
    match hint {
        Some(h) => format!("HTTP {status}: {body}\n[Hint: {h}]"),
        None => format!("HTTP {status}: {body}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mam_url() -> Url {
        format!("{BASE_URL}/tor/js/loadSearchJSONbasic.php").parse().unwrap()
    }

    #[test]
    fn session_jar_sends_seeded_cookie() {
        let jar = SessionJar::new("abc123".into());
        let header = jar.cookies(&mam_url()).expect("cookie for MAM host");
        assert_eq!(header.to_str().unwrap(), "mam_id=abc123");
    }

    #[test]
    fn session_jar_tracks_rotation() {
        let jar = SessionJar::new("old-value".into());
        let headers = [HeaderValue::from_static(
            "mam_id=new-value; expires=Wed, 01 Jan 2031 00:00:00 GMT; path=/; secure; HttpOnly",
        )];
        jar.set_cookies(&mut headers.iter(), &mam_url());
        assert_eq!(jar.current(), "new-value");
    }

    #[test]
    fn session_jar_ignores_other_cookies_and_empty_values() {
        let jar = SessionJar::new("keep".into());
        let headers = [
            HeaderValue::from_static("uid=12345; path=/"),
            HeaderValue::from_static("mam_id=; path=/"),
        ];
        jar.set_cookies(&mut headers.iter(), &mam_url());
        assert_eq!(jar.current(), "keep");
    }

    #[test]
    fn session_jar_never_leaks_to_other_hosts() {
        let jar = SessionJar::new("secret".into());
        let other: Url = "https://example.com/".parse().unwrap();
        assert!(jar.cookies(&other).is_none());

        let headers = [HeaderValue::from_static("mam_id=evil; path=/")];
        jar.set_cookies(&mut headers.iter(), &other);
        assert_eq!(jar.current(), "secret");
    }
}
