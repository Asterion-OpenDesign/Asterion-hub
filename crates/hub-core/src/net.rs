//! Network policy and the HTTP client. Every URL the Hub is willing to touch goes through `UrlPolicy`,
//! including each hop of a redirect.

use crate::error::{HubError, Result};
use std::future::Future;
use std::net::IpAddr;
use std::time::Duration;
use url::{Host, Url};

pub const MAX_METADATA_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct UrlPolicy {
    /// Exact host names, or `*.example.com` for any subdomain (not the bare domain).
    allowed_hosts: Vec<String>,
    /// Debug builds and tests only: allow plain `http://` to loopback so the whole pipeline can be
    /// exercised against a local server. Release builds construct the policy with `false`.
    allow_loopback_http: bool,
    /// Publisher tool only: any https host (the publisher checks the catalog against the Hub's own
    /// allow-list separately, with `new`).
    any_https: bool,
}

impl UrlPolicy {
    pub fn new(allowed_hosts: Vec<String>, allow_loopback_http: bool) -> UrlPolicy {
        UrlPolicy { allowed_hosts: allowed_hosts.into_iter().map(|h| h.to_ascii_lowercase()).collect(), allow_loopback_http, any_https: false }
    }

    #[cfg(feature = "publisher")]
    pub fn any_https() -> UrlPolicy {
        UrlPolicy { allowed_hosts: vec![], allow_loopback_http: true, any_https: true }
    }

    fn is_loopback(url: &Url) -> bool {
        match url.host() {
            Some(Host::Domain(d)) => d.eq_ignore_ascii_case("localhost"),
            Some(Host::Ipv4(ip)) => IpAddr::V4(ip).is_loopback(),
            Some(Host::Ipv6(ip)) => IpAddr::V6(ip).is_loopback(),
            None => false,
        }
    }

    pub fn check(&self, url: &Url) -> Result<()> {
        let refuse = |why: &str| Err(HubError::UrlRefused(format!("{why}: {}", redact_url(url))));
        if !url.username().is_empty() || url.password().is_some() {
            return refuse("credentials in URL");
        }
        if self.allow_loopback_http && Self::is_loopback(url) && matches!(url.scheme(), "http" | "https") {
            return Ok(());
        }
        if url.scheme() != "https" {
            return refuse("only https is allowed");
        }
        if url.port().is_some_and(|p| p != 443) {
            return refuse("non-standard port");
        }
        let host = match url.host() {
            Some(Host::Domain(d)) => d.to_ascii_lowercase(),
            _ => return refuse("host must be a DNS name"),
        };
        let ok = self.any_https || self.allowed_hosts.iter().any(|a| match a.strip_prefix("*.") {
            Some(suffix) => host.len() > suffix.len() + 1 && host.ends_with(suffix) && host.as_bytes()[host.len() - suffix.len() - 1] == b'.',
            None => *a == host,
        });
        if ok {
            Ok(())
        } else {
            refuse("host is not in the download allow-list")
        }
    }
}

/// Never put a query string (which could carry a token) into logs or errors.
pub fn redact_url(url: &Url) -> String {
    format!("{}://{}{}", url.scheme(), url.host_str().unwrap_or("?"), url.path())
}

pub fn build_client(policy: UrlPolicy) -> Result<reqwest::Client> {
    let redirect = reqwest::redirect::Policy::custom(move |attempt| {
        if attempt.previous().len() >= 5 {
            return attempt.error("too many redirects");
        }
        match policy.check(attempt.url()) {
            Ok(()) => attempt.follow(),
            Err(e) => attempt.error(e.to_string()),
        }
    });
    reqwest::Client::builder()
        .user_agent(concat!("ASTERION-Hub/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .redirect(redirect)
        .build()
        .map_err(|e| HubError::Internal(format!("http client: {e}")))
}

pub fn classify(e: reqwest::Error) -> HubError {
    if e.is_connect() || e.is_timeout() || e.is_request() {
        HubError::Offline(error_chain(&e))
    } else if e.is_redirect() {
        HubError::UrlRefused(error_chain(&e))
    } else {
        HubError::DownloadInterrupted(error_chain(&e))
    }
}

fn error_chain(e: &dyn std::error::Error) -> String {
    let mut s = e.to_string();
    let mut cur = e.source();
    while let Some(c) = cur {
        s.push_str(": ");
        s.push_str(&c.to_string());
        cur = c.source();
    }
    s
}

/// Small metadata documents (catalog, key set, signatures). Abstract so tests can inject failures.
pub trait Fetch: Send + Sync {
    fn get(&self, url: &Url) -> impl Future<Output = Result<Vec<u8>>> + Send;
}

#[derive(Clone)]
pub struct HttpFetcher {
    client: reqwest::Client,
    policy: UrlPolicy,
}

impl HttpFetcher {
    pub fn new(client: reqwest::Client, policy: UrlPolicy) -> HttpFetcher {
        HttpFetcher { client, policy }
    }
}

impl Fetch for HttpFetcher {
    async fn get(&self, url: &Url) -> Result<Vec<u8>> {
        use futures_util::StreamExt;
        self.policy.check(url)?;
        let resp = self.client.get(url.clone()).timeout(Duration::from_secs(20)).send().await.map_err(classify)?;
        let status = resp.status();
        if !status.is_success() {
            return Err(HubError::HttpStatus { status: status.as_u16(), what: "metadata" });
        }
        if resp.content_length().is_some_and(|l| l as usize > MAX_METADATA_BYTES) {
            return Err(HubError::InvalidMetadata("metadata document is too large".into()));
        }
        let mut body = Vec::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(classify)?;
            if body.len() + chunk.len() > MAX_METADATA_BYTES {
                return Err(HubError::InvalidMetadata("metadata document is too large".into()));
            }
            body.extend_from_slice(&chunk);
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(hosts: &[&str], loopback: bool) -> UrlPolicy {
        UrlPolicy::new(hosts.iter().map(|s| s.to_string()).collect(), loopback)
    }
    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn only_https_to_allowed_hosts() {
        let pol = p(&["releases.example.com", "*.cdn.example.net"], false);
        assert!(pol.check(&u("https://releases.example.com/a.msi")).is_ok());
        assert!(pol.check(&u("https://x.cdn.example.net/a.msi")).is_ok());
        assert!(pol.check(&u("https://a.b.cdn.example.net/a.msi")).is_ok());
        for bad in [
            "http://releases.example.com/a.msi",
            "https://evil.example.com/a.msi",
            "https://cdn.example.net/a.msi",
            "https://xcdn.example.net/a.msi",
            "https://releases.example.com.evil.io/a.msi",
            "https://user:pw@releases.example.com/a.msi",
            "https://releases.example.com:8443/a.msi",
            "https://203.0.113.9/a.msi",
            "ftp://releases.example.com/a.msi",
            "file:///C:/a.msi",
        ] {
            assert!(matches!(pol.check(&u(bad)), Err(HubError::UrlRefused(_))), "{bad}");
        }
    }

    #[test]
    fn loopback_http_exists_only_when_explicitly_enabled() {
        assert!(p(&[], false).check(&u("http://127.0.0.1:8787/a")).is_err());
        assert!(p(&[], false).check(&u("https://localhost/a")).is_err());
        let dev = p(&[], true);
        assert!(dev.check(&u("http://127.0.0.1:8787/a")).is_ok());
        assert!(dev.check(&u("http://localhost:9/a")).is_ok());
        assert!(dev.check(&u("http://[::1]:9/a")).is_ok());
        assert!(dev.check(&u("http://example.com/a")).is_err());
        assert!(dev.check(&u("http://u:p@127.0.0.1/a")).is_err());
    }

    #[test]
    fn urls_are_logged_without_their_query() {
        assert_eq!(redact_url(&u("https://h.example/a/b.msi?token=secret#frag")), "https://h.example/a/b.msi");
    }
}

