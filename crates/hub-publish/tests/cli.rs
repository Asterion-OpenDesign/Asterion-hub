//! The publisher CLI end to end, on temporary directories: keys, key set, release file, promotion,
//! signed catalog, verification (the Hub's own verifier), and the guards that keep secrets out of repositories.

use std::path::Path;
use std::process::Command;

fn run(args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_asterion-publish")).args(args).output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    (out.status.success(), text)
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

fn write_app(src: &Path) {
    let d = src.join("apps").join("demo");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("app.json"), r#"{"id":"demo","name":"Demo","summary":"s","publisher":"P"}"#).unwrap();
}

#[test]
fn full_publishing_flow_signs_and_verifies_and_stable_needs_promotion() {
    let t = tempfile::tempdir().unwrap();
    let k = t.path().join("keys");
    let (root, rootpub) = (k.join("root.key"), k.join("root.pub.json"));
    let (rel, relpub) = (k.join("rel.key"), k.join("rel.pub.json"));
    assert!(run(&["keygen", "--id", "root-t", "--secret-out", s(&root), "--public-out", s(&rootpub)]).0);
    assert!(run(&["keygen", "--id", "rel-t", "--secret-out", s(&rel), "--public-out", s(&relpub)]).0);
    let site = t.path().join("site");
    let (ok, o) = run(&["keyset", "--root-key", s(&root), "--delegate", s(&relpub), "--out-dir", s(&site)]);
    assert!(ok, "{o}");

    // roots.json as the Hub embeds it
    let pubkey: serde_json::Value = serde_json::from_slice(&std::fs::read(&rootpub).unwrap()).unwrap();
    let roots = t.path().join("roots.json");
    std::fs::write(&roots, serde_json::to_vec(&serde_json::json!({"schema":1,"roots":[pubkey]})).unwrap()).unwrap();

    let src = t.path().join("src");
    write_app(&src);
    let pkg = t.path().join("Demo-1.0.0.zip");
    std::fs::write(&pkg, b"not really a mac app, only bytes to hash").unwrap();
    let (ok, o) = run(&["release-add", "--src", s(&src), "--app", "demo", "--version", "1.0.0", "--file", s(&pkg), "--url", "https://example.test/Demo-1.0.0.zip", "--os", "macos", "--arch", "aarch64", "--note", "first"]);
    assert!(ok, "{o}");
    assert!(o.contains(&hex_sha256(b"not really a mac app, only bytes to hash")), "{o}");

    // New releases start in beta only: the stable catalog has nothing to publish yet.
    let (ok, o) = run(&["build", "--src", s(&src), "--channel", "stable", "--release-key", s(&rel), "--out", s(&site)]);
    assert!(ok, "{o}");
    assert!(o.contains("0 app(s)"), "{o}");
    let (ok, o) = run(&["build", "--src", s(&src), "--channel", "beta", "--release-key", s(&rel), "--out", s(&site)]);
    assert!(ok, "{o}");
    assert!(o.contains("1 app(s)"), "{o}");

    // the Hub's own verifier accepts it (without fetching the artifact)
    let (ok, o) = run(&["verify", "--site", s(&site), "--roots", s(&roots), "--channel", "beta"]);
    assert!(ok, "{o}");
    assert!(o.contains("key set") && o.contains("catalog"), "{o}");

    // promotion makes it appear in stable
    assert!(run(&["promote", "--src", s(&src), "--app", "demo", "--version", "1.0.0", "--to", "stable"]).0);
    let (ok, o) = run(&["build", "--src", s(&src), "--channel", "stable", "--release-key", s(&rel), "--out", s(&site)]);
    assert!(ok && o.contains("1 app(s)"), "{o}");

    // withdrawing keeps it in the document but marks it, so it can never be an install or update target
    assert!(run(&["withdraw", "--src", s(&src), "--app", "demo", "--version", "1.0.0", "--reason", "bad build"]).0);
    assert!(run(&["build", "--src", s(&src), "--channel", "stable", "--release-key", s(&rel), "--out", s(&site)]).0);
    let cat: serde_json::Value = serde_json::from_slice(&std::fs::read(site.join("stable").join("catalog.json")).unwrap()).unwrap();
    assert_eq!(cat["apps"][0]["releases"][0]["withdrawn"], "bad build");

    // a catalog tampered with after signing fails verification
    let path = site.join("stable").join("catalog.json");
    let mut bytes = std::fs::read(&path).unwrap();
    let at = bytes.windows(3).position(|w| w == b"Dem").unwrap();
    bytes[at] = b'X';
    std::fs::write(&path, bytes).unwrap();
    let (ok, o) = run(&["verify", "--site", s(&site), "--roots", s(&roots), "--channel", "stable"]);
    assert!(!ok && o.contains("signature"), "{o}");

    // a wrong root key does not verify the key set
    let (_, stranger) = (0, k.join("other.key"));
    assert!(run(&["keygen", "--id", "root-t", "--secret-out", s(&stranger), "--public-out", s(&k.join("other.pub.json"))]).0);
    let other: serde_json::Value = serde_json::from_slice(&std::fs::read(k.join("other.pub.json")).unwrap()).unwrap();
    let bad_roots = t.path().join("bad-roots.json");
    std::fs::write(&bad_roots, serde_json::to_vec(&serde_json::json!({"schema":1,"roots":[other]})).unwrap()).unwrap();
    let (ok, _) = run(&["verify", "--site", s(&site), "--roots", s(&bad_roots), "--channel", "beta"]);
    assert!(!ok);
}

#[test]
fn a_secret_key_is_never_written_inside_a_git_repository_or_over_another_key() {
    let t = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(t.path().join("repo").join(".git")).unwrap();
    let inside = t.path().join("repo").join("sub").join("k.key");
    let (ok, o) = run(&["keygen", "--id", "x", "--secret-out", s(&inside), "--public-out", s(&t.path().join("x.pub"))]);
    assert!(!ok && o.contains("git repository"), "{o}");
    assert!(!inside.exists());

    let outside = t.path().join("k.key");
    assert!(run(&["keygen", "--id", "x", "--secret-out", s(&outside), "--public-out", s(&t.path().join("x.pub"))]).0);
    let (ok, o) = run(&["keygen", "--id", "x", "--secret-out", s(&outside), "--public-out", s(&t.path().join("y.pub"))]);
    assert!(!ok && o.contains("already exists"), "{o}");
}

#[test]
fn invalid_sources_are_refused_before_anything_is_signed() {
    let t = tempfile::tempdir().unwrap();
    let k = t.path().join("rel.key");
    assert!(run(&["keygen", "--id", "rel-t", "--secret-out", s(&k), "--public-out", s(&t.path().join("rel.pub.json"))]).0);
    let src = t.path().join("src");
    write_app(&src);
    // directory name differs from the app id
    std::fs::rename(src.join("apps").join("demo"), src.join("apps").join("other")).unwrap();
    let (ok, o) = run(&["build", "--src", s(&src), "--channel", "beta", "--release-key", s(&k), "--out", s(&t.path().join("site"))]);
    assert!(!ok && o.contains("directory name"), "{o}");
    assert!(!t.path().join("site").join("beta").join("catalog.json").exists());
    // unknown channel, unsupported package type, bad version
    assert!(!run(&["build", "--src", s(&src), "--channel", "nightly", "--release-key", s(&k), "--out", s(&t.path().join("site"))]).0);
    let f = t.path().join("a.exe");
    std::fs::write(&f, b"x").unwrap();
    std::fs::rename(src.join("apps").join("other"), src.join("apps").join("demo")).unwrap();
    assert!(!run(&["release-add", "--src", s(&src), "--app", "demo", "--version", "1.0.0", "--file", s(&f), "--url", "https://e.test/a.exe"]).0);
    let z = t.path().join("a.zip");
    std::fs::write(&z, b"x").unwrap();
    assert!(!run(&["release-add", "--src", s(&src), "--app", "demo", "--version", "latest", "--file", s(&z), "--url", "https://e.test/a.zip", "--os", "macos"]).0);
}

fn hex_sha256(b: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(b))
}
