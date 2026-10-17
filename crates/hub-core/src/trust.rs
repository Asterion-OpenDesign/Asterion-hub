//! Authenticity of the catalog: who published it.
//!
//! This is NOT code signing (Authenticode / Apple notarization) and it does not make Windows or macOS
//! trust the installers. It answers one question: "was this catalog produced by someone holding the
//! ASTERION publishing key?". The file hashes inside the catalog then bind each installer to that
//! signed statement, so integrity (the bytes are what the publisher listed) and authenticity (the
//! publisher is us) are both checked before anything runs.
//!
//! Two tiers, so that a leaked CI key does not mean shipping a new Hub:
//!
//!   root key      Ed25519, kept OFFLINE. Its public half is compiled into the Hub (`trust/roots.json`).
//!                 It signs `keys.json`, the list of delegated publishing keys, rarely (about once a year).
//!   release key   Ed25519, lives in the CI secret store. Listed in `keys.json`; signs each `catalog.json`.
//!
//! A compromised release key is revoked by re-signing `keys.json` offline with the root key; every Hub
//! picks that up on its next refresh. A compromised root key is the one case that needs a new Hub.
//!
//! Signatures are over `DOMAIN || exact bytes of the file`, with a different DOMAIN for each document
//! type, so a signature for one kind of document can never be replayed as another.

use crate::error::{HubError, Result};
use crate::manifest::{parse_rfc3339, Catalog};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub const DOMAIN_KEYS: &[u8] = b"asterion-hub/keys/v1\n";
pub const DOMAIN_CATALOG: &[u8] = b"asterion-hub/catalog/v1\n";
pub const KEYSET_SCHEMA: u32 = 1;
const ALG: &str = "ed25519";

/// A detached signature file (`catalog.json.sig`, `keys.json.sig`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigFile {
    pub keyid: String,
    pub alg: String,
    /// Base64 of the 64-byte Ed25519 signature.
    pub sig: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicKeyEntry {
    pub id: String,
    pub alg: String,
    /// Base64 of the 32-byte Ed25519 public key.
    pub public: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RootsFile {
    schema: u32,
    roots: Vec<PublicKeyEntry>,
}

/// The trust anchors compiled into the Hub.
#[derive(Debug, Clone)]
pub struct RootSet {
    roots: Vec<(String, VerifyingKey)>,
}

impl RootSet {
    pub fn from_json(bytes: &[u8]) -> Result<RootSet> {
        let f: RootsFile = serde_json::from_slice(bytes).map_err(|e| HubError::InvalidMetadata(format!("roots.json: {e}")))?;
        if f.schema != 1 {
            return Err(HubError::InvalidMetadata("roots.json: unsupported schema".into()));
        }
        let roots = f.roots.iter().map(|r| Ok((r.id.clone(), parse_public(r)?))).collect::<Result<Vec<_>>>()?;
        if roots.is_empty() {
            return Err(HubError::InvalidMetadata("roots.json lists no root key".into()));
        }
        Ok(RootSet { roots })
    }

    pub fn from_keys(roots: Vec<(String, VerifyingKey)>) -> RootSet {
        RootSet { roots }
    }

    fn get(&self, id: &str) -> Option<&VerifyingKey> {
        self.roots.iter().find(|(i, _)| i == id).map(|(_, k)| k)
    }
}

fn parse_public(e: &PublicKeyEntry) -> Result<VerifyingKey> {
    if e.alg != ALG {
        return Err(HubError::InvalidMetadata(format!("key {}: unsupported algorithm {:?}", e.id, e.alg)));
    }
    let raw = B64.decode(&e.public).map_err(|_| HubError::InvalidMetadata(format!("key {}: public key is not base64", e.id)))?;
    let arr: [u8; 32] = raw.try_into().map_err(|_| HubError::InvalidMetadata(format!("key {}: public key is not 32 bytes", e.id)))?;
    VerifyingKey::from_bytes(&arr).map_err(|_| HubError::InvalidMetadata(format!("key {}: not a valid Ed25519 key", e.id)))
}

fn verify_detached(key: &VerifyingKey, domain: &[u8], bytes: &[u8], sig: &SigFile) -> Result<()> {
    if sig.alg != ALG {
        return Err(HubError::BadSignature(format!("unsupported signature algorithm {:?}", sig.alg)));
    }
    let raw = B64.decode(&sig.sig).map_err(|_| HubError::BadSignature("signature is not base64".into()))?;
    let arr: [u8; 64] = raw.try_into().map_err(|_| HubError::BadSignature("signature is not 64 bytes".into()))?;
    let signature = Signature::from_bytes(&arr);
    let mut msg = Vec::with_capacity(domain.len() + bytes.len());
    msg.extend_from_slice(domain);
    msg.extend_from_slice(bytes);
    key.verify_strict(&msg, &signature).map_err(|_| HubError::BadSignature("the signature does not match the content".into()))
}

fn parse_sig(bytes: &[u8]) -> Result<SigFile> {
    serde_json::from_slice(bytes).map_err(|_| HubError::BadSignature("signature file is not valid".into()))
}

// ---------------------------------------------------------------------------------------------------------
// key set (signed by a root key)
// ---------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegatedKey {
    pub id: String,
    pub alg: String,
    pub public: String,
    /// What the key may sign. Today only `"catalog"`.
    pub roles: Vec<String>,
    pub not_before: String,
    pub not_after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeySet {
    pub schema: u32,
    /// Strictly increasing; the Hub refuses an older key set (otherwise a revoked key could be revived).
    pub sequence: u64,
    pub generated_at: String,
    pub expires_at: String,
    pub keys: Vec<DelegatedKey>,
    /// Ids of delegated keys that must no longer be accepted, whatever their validity window says.
    #[serde(default)]
    pub revoked: Vec<String>,
}

pub fn verify_keyset(roots: &RootSet, bytes: &[u8], sig_bytes: &[u8], now: OffsetDateTime) -> Result<KeySet> {
    let sig = parse_sig(sig_bytes)?;
    let root = roots.get(&sig.keyid).ok_or_else(|| HubError::BadSignature(format!("key set signed by unknown root {:?}", sig.keyid)))?;
    verify_detached(root, DOMAIN_KEYS, bytes, &sig)?;
    let ks: KeySet = serde_json::from_slice(bytes).map_err(|e| HubError::InvalidMetadata(format!("keys.json: {e}")))?;
    if ks.schema != KEYSET_SCHEMA {
        return Err(HubError::UnsupportedSchema { found: ks.schema, supported: KEYSET_SCHEMA });
    }
    parse_rfc3339(&ks.generated_at)?;
    if parse_rfc3339(&ks.expires_at)? <= now {
        return Err(HubError::Expired("the publishing key set has expired".into()));
    }
    Ok(ks)
}

impl KeySet {
    fn catalog_key(&self, id: &str, now: OffsetDateTime) -> Result<VerifyingKey> {
        if self.revoked.iter().any(|r| r == id) {
            return Err(HubError::BadSignature(format!("publishing key {id:?} has been revoked")));
        }
        let k = self
            .keys
            .iter()
            .find(|k| k.id == id)
            .ok_or_else(|| HubError::BadSignature(format!("catalog signed by a key that is not in the key set: {id:?}")))?;
        if !k.roles.iter().any(|r| r == "catalog") {
            return Err(HubError::BadSignature(format!("publishing key {id:?} may not sign catalogs")));
        }
        if parse_rfc3339(&k.not_before)? > now || parse_rfc3339(&k.not_after)? <= now {
            return Err(HubError::BadSignature(format!("publishing key {id:?} is outside its validity window")));
        }
        parse_public(&PublicKeyEntry { id: k.id.clone(), alg: k.alg.clone(), public: k.public.clone() })
    }

    /// Days left before this key set stops being accepted (negative once expired).
    pub fn days_left(&self, now: OffsetDateTime) -> i64 {
        parse_rfc3339(&self.expires_at).map(|e| (e - now).whole_days()).unwrap_or(-1)
    }
}

// ---------------------------------------------------------------------------------------------------------
// catalog (signed by a delegated key)
// ---------------------------------------------------------------------------------------------------------

/// Authenticate, then parse and validate. The order matters: nothing in `bytes` is interpreted until the
/// signature has been checked.
pub fn verify_catalog(keys: &KeySet, bytes: &[u8], sig_bytes: &[u8], now: OffsetDateTime) -> Result<Catalog> {
    let sig = parse_sig(sig_bytes)?;
    let key = keys.catalog_key(&sig.keyid, now)?;
    verify_detached(&key, DOMAIN_CATALOG, bytes, &sig)?;
    let cat = Catalog::parse(bytes)?;
    if parse_rfc3339(&cat.expires_at)? <= now {
        return Err(HubError::Expired("the catalog has expired".into()));
    }
    Ok(cat)
}

// ---------------------------------------------------------------------------------------------------------
// publisher side (never compiled into the Hub)
// ---------------------------------------------------------------------------------------------------------

#[cfg(feature = "publisher")]
pub mod publisher {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    /// On-disk private key. The file must live outside the repository and outside any shared folder.
    #[derive(Serialize, Deserialize)]
    pub struct SecretKeyFile {
        pub keyid: String,
        pub alg: String,
        /// Base64 of the 32-byte Ed25519 seed.
        pub secret: String,
    }

    pub fn generate(keyid: &str) -> (SecretKeyFile, PublicKeyEntry) {
        let sk = SigningKey::generate(&mut rand_core::OsRng);
        let public = PublicKeyEntry { id: keyid.into(), alg: ALG.into(), public: B64.encode(sk.verifying_key().to_bytes()) };
        (SecretKeyFile { keyid: keyid.into(), alg: ALG.into(), secret: B64.encode(sk.to_bytes()) }, public)
    }

    pub fn load_secret(json: &[u8]) -> Result<(String, SigningKey)> {
        let f: SecretKeyFile = serde_json::from_slice(json).map_err(|_| HubError::InvalidMetadata("not a secret key file".into()))?;
        if f.alg != ALG {
            return Err(HubError::InvalidMetadata("unsupported key algorithm".into()));
        }
        let raw = B64.decode(&f.secret).map_err(|_| HubError::InvalidMetadata("secret key is not base64".into()))?;
        let arr: [u8; 32] = raw.try_into().map_err(|_| HubError::InvalidMetadata("secret key is not 32 bytes".into()))?;
        Ok((f.keyid, SigningKey::from_bytes(&arr)))
    }

    pub fn public_of(keyid: &str, sk: &SigningKey) -> PublicKeyEntry {
        PublicKeyEntry { id: keyid.into(), alg: ALG.into(), public: B64.encode(sk.verifying_key().to_bytes()) }
    }

    fn sign(keyid: &str, sk: &SigningKey, domain: &[u8], bytes: &[u8]) -> SigFile {
        let mut msg = Vec::with_capacity(domain.len() + bytes.len());
        msg.extend_from_slice(domain);
        msg.extend_from_slice(bytes);
        SigFile { keyid: keyid.into(), alg: ALG.into(), sig: B64.encode(sk.sign(&msg).to_bytes()) }
    }

    pub fn sign_keyset(keyid: &str, root: &SigningKey, bytes: &[u8]) -> SigFile {
        sign(keyid, root, DOMAIN_KEYS, bytes)
    }

    pub fn sign_catalog(keyid: &str, key: &SigningKey, bytes: &[u8]) -> SigFile {
        sign(keyid, key, DOMAIN_CATALOG, bytes)
    }
}

#[cfg(test)]
pub(crate) mod testkit {
    //! Ephemeral keys for tests. These are generated per test run and never stored.
    use super::publisher::*;
    use super::*;
    use crate::manifest::fixtures;
    use ed25519_dalek::SigningKey;
    use time::macros::datetime;

    pub const NOW: OffsetDateTime = datetime!(2026-10-17 12:00 UTC);

    pub struct Pki {
        pub root: SigningKey,
        pub release: SigningKey,
        pub roots: RootSet,
        pub keyset_bytes: Vec<u8>,
        pub keyset_sig: Vec<u8>,
    }

    pub fn keyset_json(release: &SigningKey, revoked: Vec<String>, sequence: u64, expires: &str) -> Vec<u8> {
        let pk = public_of("rel-1", release);
        serde_json::to_vec(&KeySet {
            schema: 1,
            sequence,
            generated_at: "2026-10-01T00:00:00Z".into(),
            expires_at: expires.into(),
            keys: vec![DelegatedKey {
                id: pk.id,
                alg: pk.alg,
                public: pk.public,
                roles: vec!["catalog".into()],
                not_before: "2026-09-01T00:00:00Z".into(),
                not_after: "2027-09-01T00:00:00Z".into(),
            }],
            revoked,
        })
        .unwrap()
    }

    pub fn pki() -> Pki {
        let (root_secret, root_pub) = generate("root-1");
        let (_, root) = load_secret(&serde_json::to_vec(&root_secret).unwrap()).unwrap();
        let (rel_secret, _) = generate("rel-1");
        let (_, release) = load_secret(&serde_json::to_vec(&rel_secret).unwrap()).unwrap();
        let roots = RootSet::from_json(&serde_json::to_vec(&serde_json::json!({"schema":1,"roots":[root_pub]})).unwrap()).unwrap();
        let keyset_bytes = keyset_json(&release, vec![], 1, "2027-06-01T00:00:00Z");
        let keyset_sig = serde_json::to_vec(&sign_keyset("root-1", &root, &keyset_bytes)).unwrap();
        Pki { root, release, roots, keyset_bytes, keyset_sig }
    }

    pub fn signed_catalog(pki: &Pki, cat: &Catalog) -> (Vec<u8>, Vec<u8>) {
        let bytes = serde_json::to_vec(cat).unwrap();
        let sig = serde_json::to_vec(&sign_catalog("rel-1", &pki.release, &bytes)).unwrap();
        (bytes, sig)
    }

    pub fn sample_catalog() -> Catalog {
        use crate::manifest::*;
        fixtures::catalog(vec![fixtures::app(
            "asterion-bim",
            vec![fixtures::release("0.1.0", vec![fixtures::artifact(Os::Windows, Arch::X86_64, PackageKind::Msi)])],
        )])
    }
}

#[cfg(test)]
mod tests {
    use super::publisher::*;
    use super::testkit::*;
    use super::*;

    #[test]
    fn a_catalog_signed_by_a_delegated_key_verifies() {
        let p = pki();
        let ks = verify_keyset(&p.roots, &p.keyset_bytes, &p.keyset_sig, NOW).unwrap();
        let (bytes, sig) = signed_catalog(&p, &sample_catalog());
        let cat = verify_catalog(&ks, &bytes, &sig, NOW).unwrap();
        assert_eq!(cat.apps[0].id, "asterion-bim");
    }

    #[test]
    fn a_modified_catalog_fails_even_by_one_byte() {
        let p = pki();
        let ks = verify_keyset(&p.roots, &p.keyset_bytes, &p.keyset_sig, NOW).unwrap();
        let (mut bytes, sig) = signed_catalog(&p, &sample_catalog());
        let at = bytes.iter().position(|b| *b == b'0').unwrap();
        bytes[at] = b'1';
        assert!(matches!(verify_catalog(&ks, &bytes, &sig, NOW), Err(HubError::BadSignature(_))));
    }

    #[test]
    fn a_catalog_signed_by_a_stranger_is_refused() {
        let p = pki();
        let ks = verify_keyset(&p.roots, &p.keyset_bytes, &p.keyset_sig, NOW).unwrap();
        let (_, stranger) = load_secret(&serde_json::to_vec(&generate("rel-1").0).unwrap()).unwrap();
        let bytes = serde_json::to_vec(&sample_catalog()).unwrap();
        // Same key id as the delegated one, but a different key: the id alone proves nothing.
        let sig = serde_json::to_vec(&sign_catalog("rel-1", &stranger, &bytes)).unwrap();
        assert!(matches!(verify_catalog(&ks, &bytes, &sig, NOW), Err(HubError::BadSignature(_))));
        // And an id that is not in the key set at all.
        let sig = serde_json::to_vec(&sign_catalog("rel-9", &stranger, &bytes)).unwrap();
        assert!(matches!(verify_catalog(&ks, &bytes, &sig, NOW), Err(HubError::BadSignature(_))));
    }

    #[test]
    fn a_keyset_not_signed_by_a_trusted_root_is_refused() {
        let p = pki();
        let (_, rogue) = load_secret(&serde_json::to_vec(&generate("root-1").0).unwrap()).unwrap();
        let sig = serde_json::to_vec(&sign_keyset("root-1", &rogue, &p.keyset_bytes)).unwrap();
        assert!(matches!(verify_keyset(&p.roots, &p.keyset_bytes, &sig, NOW), Err(HubError::BadSignature(_))));
        let sig = serde_json::to_vec(&sign_keyset("root-7", &p.root, &p.keyset_bytes)).unwrap();
        assert!(matches!(verify_keyset(&p.roots, &p.keyset_bytes, &sig, NOW), Err(HubError::BadSignature(_))));
    }

    #[test]
    fn signatures_do_not_cross_document_types() {
        let p = pki();
        // A catalog signature presented as a key-set signature (and the reverse) must fail: the domains differ.
        let as_catalog = serde_json::to_vec(&sign_catalog("root-1", &p.root, &p.keyset_bytes)).unwrap();
        assert!(matches!(verify_keyset(&p.roots, &p.keyset_bytes, &as_catalog, NOW), Err(HubError::BadSignature(_))));
    }

    #[test]
    fn a_revoked_release_key_is_refused_even_with_a_valid_signature() {
        let p = pki();
        let revoked = keyset_json(&p.release, vec!["rel-1".into()], 2, "2027-06-01T00:00:00Z");
        let sig = serde_json::to_vec(&sign_keyset("root-1", &p.root, &revoked)).unwrap();
        let ks = verify_keyset(&p.roots, &revoked, &sig, NOW).unwrap();
        let (bytes, csig) = signed_catalog(&p, &sample_catalog());
        match verify_catalog(&ks, &bytes, &csig, NOW) {
            Err(HubError::BadSignature(m)) => assert!(m.contains("revoked"), "{m}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn expired_documents_and_keys_outside_their_window_are_refused() {
        let p = pki();
        let ks = verify_keyset(&p.roots, &p.keyset_bytes, &p.keyset_sig, NOW).unwrap();
        let (bytes, sig) = signed_catalog(&p, &sample_catalog());
        let late = NOW + time::Duration::days(60); // catalog expires 2026-11-01
        assert!(matches!(verify_catalog(&ks, &bytes, &sig, late), Err(HubError::Expired(_))));
        let way_late = time::macros::datetime!(2028-01-01 0:00 UTC); // past the key's not_after
        assert!(matches!(verify_keyset(&p.roots, &p.keyset_bytes, &p.keyset_sig, way_late), Err(HubError::Expired(_))));
        let early = time::macros::datetime!(2026-08-01 0:00 UTC); // before the key's not_before
        assert!(matches!(verify_catalog(&ks, &bytes, &sig, early), Err(HubError::BadSignature(_))));
    }

    #[test]
    fn a_key_without_the_catalog_role_cannot_sign_catalogs() {
        let p = pki();
        let mut ks: KeySet = serde_json::from_slice(&p.keyset_bytes).unwrap();
        ks.keys[0].roles = vec!["something-else".into()];
        let bytes = serde_json::to_vec(&sample_catalog()).unwrap();
        let sig = serde_json::to_vec(&sign_catalog("rel-1", &p.release, &bytes)).unwrap();
        assert!(matches!(verify_catalog(&ks, &bytes, &sig, NOW), Err(HubError::BadSignature(_))));
    }

    #[test]
    fn garbage_signature_files_are_bad_signatures_not_panics() {
        let p = pki();
        let ks = verify_keyset(&p.roots, &p.keyset_bytes, &p.keyset_sig, NOW).unwrap();
        let bytes = serde_json::to_vec(&sample_catalog()).unwrap();
        for sig in [&b""[..], b"{}", b"not json", br#"{"keyid":"rel-1","alg":"rsa","sig":"AA=="}"#, br#"{"keyid":"rel-1","alg":"ed25519","sig":"AA=="}"#] {
            assert!(matches!(verify_catalog(&ks, &bytes, sig, NOW), Err(HubError::BadSignature(_))));
        }
    }

    #[test]
    fn a_signed_but_invalid_catalog_is_still_rejected() {
        // The publisher signed something that does not validate: authentic is not the same as acceptable.
        let p = pki();
        let ks = verify_keyset(&p.roots, &p.keyset_bytes, &p.keyset_sig, NOW).unwrap();
        let mut cat = sample_catalog();
        cat.apps[0].releases[0].artifacts[0].sha256 = "nope".into();
        let (bytes, sig) = signed_catalog(&p, &cat);
        assert!(matches!(verify_catalog(&ks, &bytes, &sig, NOW), Err(HubError::InvalidMetadata(_))));
    }
}

