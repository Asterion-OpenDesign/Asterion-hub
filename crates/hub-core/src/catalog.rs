//! Fetch, authenticate and cache the catalog.
//!
//! Order on every refresh: key set (root-signed) -> catalog (release-key-signed) -> validation ->
//! anti-rollback -> cache. If the network is down the last verified copy is re-verified from disk and
//! served, flagged as cached, so the Hub keeps working offline (installed apps never depended on it).

use crate::error::{HubError, Result};
use crate::manifest::{parse_rfc3339, Catalog, Channel};
use crate::net::Fetch;
use crate::store::Store;
use crate::trust::{verify_catalog, verify_keyset, KeySet, RootSet};
use serde::Serialize;
use std::sync::Arc;
use time::OffsetDateTime;
use url::Url;

pub type Clock = Arc<dyn Fn() -> OffsetDateTime + Send + Sync>;

pub fn system_clock() -> Clock {
    Arc::new(OffsetDateTime::now_utc)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Network,
    Cache,
}

#[derive(Debug, Clone)]
pub struct CatalogSnapshot {
    pub catalog: Catalog,
    pub origin: Origin,
    /// When this exact document was last fetched and verified.
    pub fetched_at: OffsetDateTime,
    /// The signed document is past its expiry: fine to show, not fine to install from.
    pub expired: bool,
    /// Days left on the publishing key set; the publisher must re-sign before it runs out.
    pub keyset_days_left: i64,
}

pub struct CatalogManager<F: Fetch> {
    fetcher: F,
    roots: RootSet,
    base: Url,
    store: Store,
    clock: Clock,
}

impl<F: Fetch> CatalogManager<F> {
    pub fn new(fetcher: F, roots: RootSet, base: Url, store: Store, clock: Clock) -> Self {
        let mut base = base;
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        CatalogManager { fetcher, roots, base, store, clock }
    }

    fn url(&self, rel: &str) -> Result<Url> {
        self.base.join(rel).map_err(|e| HubError::Internal(format!("bad catalog URL: {e}")))
    }

    fn now(&self) -> OffsetDateTime {
        (self.clock)()
    }

    async fn pair(&self, rel: &str) -> Result<(Vec<u8>, Vec<u8>)> {
        let doc = self.fetcher.get(&self.url(rel)?).await?;
        let sig = self.fetcher.get(&self.url(&format!("{rel}.sig"))?).await?;
        Ok((doc, sig))
    }

    /// Contact the server. On success the verified documents replace the cache.
    pub async fn refresh(&self, channel: Channel) -> Result<CatalogSnapshot> {
        let now = self.now();
        let mut state = self.store.state();

        let (ks_bytes, ks_sig) = self.pair("keys.json").await?;
        let keyset = verify_keyset(&self.roots, &ks_bytes, &ks_sig, now)?;
        if keyset.sequence < state.keyset_sequence {
            return Err(HubError::Rollback(format!("key set sequence {} is older than the {} already seen", keyset.sequence, state.keyset_sequence)));
        }

        let (cat_bytes, cat_sig) = self.pair(&format!("{}/catalog.json", channel.as_str())).await?;
        let catalog = verify_catalog(&keyset, &cat_bytes, &cat_sig, now)?;
        if catalog.channel != channel {
            return Err(HubError::InvalidMetadata(format!("asked for the {} channel, received {}", channel.as_str(), catalog.channel.as_str())));
        }
        let seen = state.catalog_sequence.get(channel.as_str()).copied().unwrap_or(0);
        if catalog.sequence < seen {
            return Err(HubError::Rollback(format!("catalog sequence {} is older than the {seen} already seen", catalog.sequence)));
        }
        if catalog.sequence == seen {
            // Same sequence must mean the same document; anything else is a replayed or forked catalog.
            if let Some(prev) = self.store.read_cache(&format!("{}.catalog.json", channel.as_str())) {
                if prev != cat_bytes {
                    return Err(HubError::Rollback("a different catalog was served under the same sequence number".into()));
                }
            }
        }

        let ch = channel.as_str();
        self.store.write_cache("keys.json", &ks_bytes)?;
        self.store.write_cache("keys.json.sig", &ks_sig)?;
        self.store.write_cache("keys.fetched", fmt(now).as_bytes())?;
        self.store.write_cache(&format!("{ch}.catalog.json"), &cat_bytes)?;
        self.store.write_cache(&format!("{ch}.catalog.json.sig"), &cat_sig)?;
        self.store.write_cache(&format!("{ch}.fetched"), fmt(now).as_bytes())?;

        state.keyset_sequence = keyset.sequence;
        state.catalog_sequence.insert(ch.to_string(), catalog.sequence);
        state.last_check = Some(fmt(now));
        self.store.save_state(&state)?;

        Ok(CatalogSnapshot { expired: false, keyset_days_left: keyset.days_left(now), catalog, origin: Origin::Network, fetched_at: now })
    }

    /// Re-verify what is on disk. Nothing from the cache is trusted just because the Hub wrote it: a
    /// local process could have edited it, so the signatures are checked again against the compiled-in
    /// roots, at the moment the documents were fetched (so a document that has since expired still
    /// verifies, and is reported as expired instead of vanishing).
    pub fn load_cached(&self, channel: Channel) -> Result<CatalogSnapshot> {
        let ch = channel.as_str();
        let missing = || HubError::Offline("no saved catalog yet".into());
        let ks_bytes = self.store.read_cache("keys.json").ok_or_else(missing)?;
        let ks_sig = self.store.read_cache("keys.json.sig").ok_or_else(missing)?;
        let cat_bytes = self.store.read_cache(&format!("{ch}.catalog.json")).ok_or_else(missing)?;
        let cat_sig = self.store.read_cache(&format!("{ch}.catalog.json.sig")).ok_or_else(missing)?;
        let at = |name: &str| -> Result<OffsetDateTime> {
            let raw = self.store.read_cache(name).ok_or_else(missing)?;
            parse_rfc3339(String::from_utf8_lossy(&raw).trim())
        };
        let ks_at = at("keys.fetched")?;
        let cat_at = at(&format!("{ch}.fetched"))?;
        let keyset: KeySet = verify_keyset(&self.roots, &ks_bytes, &ks_sig, ks_at)?;
        let catalog = verify_catalog(&keyset, &cat_bytes, &cat_sig, cat_at)?;
        let now = self.now();
        let expired = parse_rfc3339(&catalog.expires_at)? <= now;
        Ok(CatalogSnapshot { expired, keyset_days_left: keyset.days_left(now), catalog, origin: Origin::Cache, fetched_at: cat_at })
    }

    /// Network first, cache as the fallback. The error from the network attempt is returned alongside
    /// the cached data so the UI can say *why* it is showing old information.
    pub async fn refresh_or_cached(&self, channel: Channel) -> (Result<CatalogSnapshot>, Option<HubError>) {
        match self.refresh(channel).await {
            Ok(s) => (Ok(s), None),
            Err(e) => {
                tracing::warn!("catalog refresh failed: {} ({})", e, e.code());
                (self.load_cached(channel), Some(e))
            }
        }
    }
}

fn fmt(t: OffsetDateTime) -> String {
    t.format(&time::format_description::well_known::Rfc3339).expect("rfc3339")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::Fetch;
    use crate::trust::publisher::*;
    use crate::trust::testkit::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// An in-memory "server" whose documents and failures the test controls.
    #[derive(Clone, Default)]
    struct Fake {
        docs: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        down: Arc<Mutex<Option<fn() -> HubError>>>,
    }
    impl Fake {
        fn put(&self, path: &str, bytes: Vec<u8>) {
            self.docs.lock().unwrap().insert(path.to_string(), bytes);
        }
        fn serve(&self, p: &Pki, cat: &Catalog) {
            self.put("keys.json", p.keyset_bytes.clone());
            self.put("keys.json.sig", p.keyset_sig.clone());
            let (b, s) = signed_catalog(p, cat);
            self.put(&format!("{}/catalog.json", cat.channel.as_str()), b);
            self.put(&format!("{}/catalog.json.sig", cat.channel.as_str()), s);
        }
    }
    impl Fetch for Fake {
        async fn get(&self, url: &Url) -> Result<Vec<u8>> {
            if let Some(mk) = *self.down.lock().unwrap() {
                return Err(mk());
            }
            let path = url.path().trim_start_matches("/base/").to_string();
            self.docs.lock().unwrap().get(&path).cloned().ok_or(HubError::HttpStatus { status: 404, what: "metadata" })
        }
    }

    fn mgr(f: &Fake, p: &Pki, dir: &std::path::Path) -> CatalogManager<Fake> {
        CatalogManager::new(f.clone(), p.roots.clone(), Url::parse("https://h.example/base").unwrap(), Store::open(dir).unwrap(), Arc::new(|| NOW))
    }

    fn cat(seq: u64) -> Catalog {
        let mut c = sample_catalog();
        c.sequence = seq;
        c
    }

    #[tokio::test]
    async fn refresh_verifies_caches_and_remembers_the_sequence() {
        let (p, f, d) = (pki(), Fake::default(), tempfile::tempdir().unwrap());
        f.serve(&p, &cat(5));
        let m = mgr(&f, &p, d.path());
        let snap = m.refresh(Channel::Stable).await.unwrap();
        assert_eq!(snap.origin, Origin::Network);
        assert_eq!(snap.catalog.sequence, 5);
        assert_eq!(Store::open(d.path()).unwrap().state().catalog_sequence["stable"], 5);
        // …and the cache alone reproduces it.
        let cached = m.load_cached(Channel::Stable).unwrap();
        assert_eq!(cached.origin, Origin::Cache);
        assert_eq!(cached.catalog, snap.catalog);
    }

    #[tokio::test]
    async fn offline_serves_the_cache_and_says_why() {
        let (p, f, d) = (pki(), Fake::default(), tempfile::tempdir().unwrap());
        f.serve(&p, &cat(1));
        let m = mgr(&f, &p, d.path());
        m.refresh(Channel::Stable).await.unwrap();
        *f.down.lock().unwrap() = Some(|| HubError::Offline("dns".into()));
        let (res, err) = m.refresh_or_cached(Channel::Stable).await;
        assert_eq!(res.unwrap().origin, Origin::Cache);
        assert_eq!(err.unwrap().code(), "offline");
    }

    #[tokio::test]
    async fn offline_with_no_cache_is_an_offline_error() {
        let (p, f, d) = (pki(), Fake::default(), tempfile::tempdir().unwrap());
        *f.down.lock().unwrap() = Some(|| HubError::Offline("dns".into()));
        let (res, err) = mgr(&f, &p, d.path()).refresh_or_cached(Channel::Stable).await;
        assert_eq!(res.unwrap_err().code(), "offline");
        assert!(err.is_some());
    }

    #[tokio::test]
    async fn an_older_catalog_is_refused_as_a_rollback() {
        let (p, f, d) = (pki(), Fake::default(), tempfile::tempdir().unwrap());
        let m = mgr(&f, &p, d.path());
        f.serve(&p, &cat(10));
        m.refresh(Channel::Stable).await.unwrap();
        f.serve(&p, &cat(9)); // perfectly signed, but old
        assert_eq!(m.refresh(Channel::Stable).await.unwrap_err().code(), "metadata_rollback");
        // The good copy is still what the cache holds.
        assert_eq!(m.load_cached(Channel::Stable).unwrap().catalog.sequence, 10);
    }

    #[tokio::test]
    async fn the_same_sequence_with_different_content_is_refused() {
        let (p, f, d) = (pki(), Fake::default(), tempfile::tempdir().unwrap());
        let m = mgr(&f, &p, d.path());
        f.serve(&p, &cat(3));
        m.refresh(Channel::Stable).await.unwrap();
        let mut forked = cat(3);
        forked.apps[0].name = "Forked".into();
        f.serve(&p, &forked);
        assert_eq!(m.refresh(Channel::Stable).await.unwrap_err().code(), "metadata_rollback");
        f.serve(&p, &cat(3)); // identical bytes are fine (a plain re-check)
        assert!(m.refresh(Channel::Stable).await.is_ok());
    }

    #[tokio::test]
    async fn a_tampered_download_is_refused_and_does_not_replace_the_cache() {
        let (p, f, d) = (pki(), Fake::default(), tempfile::tempdir().unwrap());
        let m = mgr(&f, &p, d.path());
        f.serve(&p, &cat(1));
        m.refresh(Channel::Stable).await.unwrap();
        f.serve(&p, &cat(2));
        let mut evil = f.docs.lock().unwrap().get("stable/catalog.json").cloned().unwrap();
        let at = evil.iter().position(|b| *b == b'a').unwrap();
        evil[at] = b'b';
        f.put("stable/catalog.json", evil);
        assert_eq!(m.refresh(Channel::Stable).await.unwrap_err().code(), "bad_signature");
        assert_eq!(m.load_cached(Channel::Stable).unwrap().catalog.sequence, 1);
    }

    #[tokio::test]
    async fn the_wrong_channel_is_refused() {
        let (p, f, d) = (pki(), Fake::default(), tempfile::tempdir().unwrap());
        let mut beta = cat(1);
        beta.channel = Channel::Beta;
        f.serve(&p, &beta);
        // Serve the beta document where the stable one is expected.
        let (b, s) = signed_catalog(&p, &beta);
        f.put("stable/catalog.json", b);
        f.put("stable/catalog.json.sig", s);
        assert_eq!(mgr(&f, &p, d.path()).refresh(Channel::Stable).await.unwrap_err().code(), "invalid_metadata");
    }

    #[tokio::test]
    async fn a_tampered_cache_is_not_trusted() {
        let (p, f, d) = (pki(), Fake::default(), tempfile::tempdir().unwrap());
        let m = mgr(&f, &p, d.path());
        f.serve(&p, &cat(1));
        m.refresh(Channel::Stable).await.unwrap();
        let path = d.path().join("cache").join("stable.catalog.json");
        let mut bytes = std::fs::read(&path).unwrap();
        let at = bytes.iter().position(|b| *b == b'a').unwrap();
        bytes[at] = b'b';
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(m.load_cached(Channel::Stable).unwrap_err().code(), "bad_signature");
    }

    #[tokio::test]
    async fn an_expired_cached_catalog_is_shown_but_flagged() {
        let (p, f, d) = (pki(), Fake::default(), tempfile::tempdir().unwrap());
        f.serve(&p, &cat(1));
        mgr(&f, &p, d.path()).refresh(Channel::Stable).await.unwrap();
        let later: Clock = Arc::new(|| NOW + time::Duration::days(60)); // catalog expires after 15 days
        let m2 = CatalogManager::new(f.clone(), p.roots.clone(), Url::parse("https://h.example/base").unwrap(), Store::open(d.path()).unwrap(), later);
        let snap = m2.load_cached(Channel::Stable).unwrap();
        assert!(snap.expired);
    }

    #[tokio::test]
    async fn a_revoked_key_takes_effect_on_the_next_refresh() {
        let (p, f, d) = (pki(), Fake::default(), tempfile::tempdir().unwrap());
        let m = mgr(&f, &p, d.path());
        f.serve(&p, &cat(1));
        m.refresh(Channel::Stable).await.unwrap();
        // The root key (offline) re-signs the key set with the release key revoked.
        let ks = keyset_json(&p.release, vec!["rel-1".into()], 2, "2027-06-01T00:00:00Z");
        f.put("keys.json", ks.clone());
        f.put("keys.json.sig", serde_json::to_vec(&sign_keyset("root-1", &p.root, &ks)).unwrap());
        assert_eq!(m.refresh(Channel::Stable).await.unwrap_err().code(), "bad_signature");
    }

    #[tokio::test]
    async fn an_older_keyset_cannot_resurrect_a_revoked_key() {
        let (p, f, d) = (pki(), Fake::default(), tempfile::tempdir().unwrap());
        let m = mgr(&f, &p, d.path());
        let revoked = keyset_json(&p.release, vec!["rel-1".into()], 5, "2027-06-01T00:00:00Z");
        f.put("keys.json", revoked.clone());
        f.put("keys.json.sig", serde_json::to_vec(&sign_keyset("root-1", &p.root, &revoked)).unwrap());
        let (b, s) = signed_catalog(&p, &cat(1));
        f.put("stable/catalog.json", b.clone());
        f.put("stable/catalog.json.sig", s.clone());
        let _ = m.refresh(Channel::Stable).await; // refused (revoked), but the sequence 5 has not been stored yet…
        // …so first accept a good newer key set, then try to go back to the old one.
        let newer = keyset_json(&p.release, vec![], 6, "2027-06-01T00:00:00Z");
        f.put("keys.json", newer.clone());
        f.put("keys.json.sig", serde_json::to_vec(&sign_keyset("root-1", &p.root, &newer)).unwrap());
        m.refresh(Channel::Stable).await.unwrap();
        f.put("keys.json", p.keyset_bytes.clone()); // sequence 1
        f.put("keys.json.sig", p.keyset_sig.clone());
        assert_eq!(m.refresh(Channel::Stable).await.unwrap_err().code(), "metadata_rollback");
    }
}
