//! `asterion-publish`: everything the publisher side needs, and nothing the Hub ships.
//!
//! Typical flow (see docs/PUBLISHING.md):
//!   keygen            once: a root key (kept offline) and a release key (CI secret)
//!   keyset            once a year or on rotation/revocation: root signs the list of release keys
//!   release-add       per build: hash the package, read its MSI facts, write the release source file
//!   promote           beta -> stable, only after the gates passed
//!   build             assemble and sign the channel catalog from the source files
//!   verify            exactly what a Hub does, plus (--online) fetching every artifact and re-hashing it
//!   serve             local static server with Range support, for testing the Hub end to end

use hub_core::download::{download_verified, DownloadSpec};
use hub_core::manifest::*;
use hub_core::net::{build_client, UrlPolicy};
use hub_core::trust::{self, publisher as pubkeys, DelegatedKey, KeySet, RootSet};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use time::{Duration, OffsetDateTime};

type R<T> = std::result::Result<T, String>;

#[derive(Parser)]
#[command(name = "asterion-publish", version, about = "ASTERION catalog publisher")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate an Ed25519 key pair. The secret file must live OUTSIDE any git repository.
    Keygen {
        #[arg(long)]
        id: String,
        #[arg(long)]
        secret_out: PathBuf,
        #[arg(long)]
        public_out: PathBuf,
    },
    /// Sign the list of delegated release keys with the root key (writes keys.json and keys.json.sig).
    Keyset {
        #[arg(long)]
        root_key: PathBuf,
        /// Public key file(s) produced by `keygen --public-out`, one per delegated release key.
        #[arg(long = "delegate", required = true)]
        delegates: Vec<PathBuf>,
        /// Key ids to revoke.
        #[arg(long = "revoke")]
        revoke: Vec<String>,
        #[arg(long)]
        sequence: Option<u64>,
        #[arg(long, default_value_t = 365)]
        valid_days: i64,
        #[arg(long)]
        out_dir: PathBuf,
    },
    /// Print the facts of an MSI package.
    MsiFacts { file: PathBuf },
    /// Add a package to a release source file (computes SHA-256 and size; reads MSI facts).
    ReleaseAdd {
        #[arg(long)]
        src: PathBuf,
        #[arg(long)]
        app: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        file: PathBuf,
        /// Public download URL of the file.
        #[arg(long)]
        url: String,
        #[arg(long, default_value = "windows")]
        os: String,
        #[arg(long, default_value = "x86_64")]
        arch: String,
        #[arg(long)]
        os_min: Option<String>,
        #[arg(long)]
        launch: Option<String>,
        #[arg(long)]
        installed_size_mb: Option<u64>,
        #[arg(long)]
        install_dir_property: Option<String>,
        #[arg(long = "note")]
        notes: Vec<String>,
        /// Channel(s) the release starts in. New releases should start in beta.
        #[arg(long = "channel", default_value = "beta")]
        channels: Vec<String>,
        #[arg(long)]
        released_at: Option<String>,
    },
    /// Make a release visible in another channel (typically beta -> stable).
    Promote {
        #[arg(long)]
        src: PathBuf,
        #[arg(long)]
        app: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        to: String,
    },
    /// Pull a release: it stops being offered as an install or update target.
    Withdraw {
        #[arg(long)]
        src: PathBuf,
        #[arg(long)]
        app: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        reason: String,
    },
    /// Assemble, validate and sign <out>/<channel>/catalog.json from the source tree.
    Build {
        #[arg(long)]
        src: PathBuf,
        #[arg(long)]
        channel: String,
        #[arg(long)]
        release_key: PathBuf,
        #[arg(long)]
        out: PathBuf,
        #[arg(long, default_value_t = 30)]
        valid_days: i64,
        /// Defaults to the current Unix time, which only ever increases and needs no stored counter.
        #[arg(long)]
        sequence: Option<u64>,
    },
    /// Verify a published (or local) catalog the way a Hub does.
    Verify {
        /// A directory or an https URL holding keys.json and <channel>/catalog.json.
        #[arg(long)]
        site: String,
        #[arg(long)]
        roots: PathBuf,
        #[arg(long)]
        channel: String,
        /// The Hub's own config (hub.config.json): every artifact URL must pass the Hub's allow-list.
        #[arg(long)]
        hub_config: Option<PathBuf>,
        /// Download every artifact and check size, SHA-256 and MSI facts.
        #[arg(long)]
        online: bool,
        /// Debug-only escape hatch for local tests: allow http loopback artifacts.
        #[arg(long)]
        allow_loopback: bool,
    },
    /// Serve a directory over http://127.0.0.1 with Range support (testing only).
    Serve {
        #[arg(long)]
        dir: PathBuf,
        #[arg(long, default_value_t = 8787)]
        port: u16,
    },
}

fn main() {
    let cli = Cli::parse();
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("runtime");
    if let Err(e) = rt.block_on(run(cli)) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> R<()> {
    match cli.cmd {
        Cmd::Keygen { id, secret_out, public_out } => keygen(&id, &secret_out, &public_out),
        Cmd::Keyset { root_key, delegates, revoke, sequence, valid_days, out_dir } => keyset(&root_key, &delegates, &revoke, sequence, valid_days, &out_dir),
        Cmd::MsiFacts { file } => {
            let p = hub_core::msi_info::read_msi(&file).map_err(|e| e.to_string())?;
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                "product_name": p.product_name, "product_version": p.product_version,
                "upgrade_code": p.upgrade_code, "product_code": p.product_code,
            })).unwrap());
            Ok(())
        }
        Cmd::ReleaseAdd { src, app, version, file, url, os, arch, os_min, launch, installed_size_mb, install_dir_property, notes, channels, released_at } => {
            release_add(&src, &app, &version, &file, &url, &os, &arch, os_min, launch, installed_size_mb, install_dir_property, notes, channels, released_at)
        }
        Cmd::Promote { src, app, version, to } => edit_release(&src, &app, &version, |r| {
            let ch = parse_channel(&to)?;
            if !r.channels.contains(&ch) {
                r.channels.push(ch);
            }
            Ok(())
        }),
        Cmd::Withdraw { src, app, version, reason } => edit_release(&src, &app, &version, |r| {
            r.withdrawn = Some(reason.clone());
            Ok(())
        }),
        Cmd::Build { src, channel, release_key, out, valid_days, sequence } => build(&src, &channel, &release_key, &out, valid_days, sequence),
        Cmd::Verify { site, roots, channel, hub_config, online, allow_loopback } => verify(&site, &roots, &channel, hub_config.as_deref(), online, allow_loopback).await,
        Cmd::Serve { dir, port } => serve(&dir, port),
    }
}

// ---------------------------------------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------------------------------------

fn now() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

fn rfc3339(t: OffsetDateTime) -> String {
    t.format(&time::format_description::well_known::Rfc3339).unwrap()
}

fn read_json<T: for<'de> Deserialize<'de>>(p: &Path) -> R<T> {
    let b = std::fs::read(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?;
    serde_json::from_slice(&b).map_err(|e| format!("{} is not valid: {e}", p.display()))
}

fn write_json<T: Serialize>(p: &Path, v: &T) -> R<()> {
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let mut b = serde_json::to_vec_pretty(v).map_err(|e| e.to_string())?;
    b.push(b'\n');
    std::fs::write(p, b).map_err(|e| format!("cannot write {}: {e}", p.display()))
}

fn parse_channel(s: &str) -> R<Channel> {
    match s {
        "stable" => Ok(Channel::Stable),
        "beta" => Ok(Channel::Beta),
        other => Err(format!("unknown channel {other:?} (stable | beta)")),
    }
}

fn inside_git_repo(path: &Path) -> bool {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    abs.ancestors().skip(1).any(|a| a.join(".git").exists())
}

// ---------------------------------------------------------------------------------------------------------
// keys
// ---------------------------------------------------------------------------------------------------------

fn keygen(id: &str, secret_out: &Path, public_out: &Path) -> R<()> {
    if inside_git_repo(secret_out) {
        return Err(format!("{} is inside a git repository; a private key must never be next to source code", secret_out.display()));
    }
    if secret_out.exists() {
        return Err(format!("{} already exists; refusing to overwrite a key", secret_out.display()));
    }
    let (secret, public) = pubkeys::generate(id);
    if let Some(d) = secret_out.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(secret_out).map_err(|e| e.to_string())?;
    std::io::Write::write_all(&mut f, serde_json::to_string_pretty(&secret).unwrap().as_bytes()).map_err(|e| e.to_string())?;
    drop(f);
    let mut perm = std::fs::metadata(secret_out).map_err(|e| e.to_string())?.permissions();
    perm.set_readonly(true);
    std::fs::set_permissions(secret_out, perm).map_err(|e| e.to_string())?;
    write_json(public_out, &public)?;
    println!("secret key  : {}  (keep it private; back it up offline)", secret_out.display());
    println!("public key  : {}", public_out.display());
    println!("fingerprint : {}", fingerprint(&public.public));
    Ok(())
}

fn fingerprint(public_b64: &str) -> String {
    let h = Sha256::digest(public_b64.as_bytes());
    hex::encode(&h[..8])
}

fn keyset(root_key: &Path, delegates: &[PathBuf], revoke: &[String], sequence: Option<u64>, valid_days: i64, out_dir: &Path) -> R<()> {
    let (root_id, root) = pubkeys::load_secret(&std::fs::read(root_key).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let t = now();
    let mut keys = Vec::new();
    for d in delegates {
        let pk: trust::PublicKeyEntry = read_json(d)?;
        keys.push(DelegatedKey {
            id: pk.id,
            alg: pk.alg,
            public: pk.public,
            roles: vec!["catalog".into()],
            not_before: rfc3339(t - Duration::hours(1)),
            not_after: rfc3339(t + Duration::days(valid_days)),
        });
    }
    let ks = KeySet {
        schema: trust::KEYSET_SCHEMA,
        sequence: sequence.unwrap_or(t.unix_timestamp() as u64),
        generated_at: rfc3339(t),
        expires_at: rfc3339(t + Duration::days(valid_days)),
        keys,
        revoked: revoke.to_vec(),
    };
    let bytes = serde_json::to_vec_pretty(&ks).unwrap();
    let sig = pubkeys::sign_keyset(&root_id, &root, &bytes);
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    std::fs::write(out_dir.join("keys.json"), &bytes).map_err(|e| e.to_string())?;
    write_json(&out_dir.join("keys.json.sig"), &sig)?;
    println!("wrote keys.json (sequence {}, expires {}) signed by {root_id}", ks.sequence, ks.expires_at);
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------
// source tree
// ---------------------------------------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct AppSource {
    id: String,
    name: String,
    summary: String,
    #[serde(default)]
    description: String,
    publisher: String,
    #[serde(default)]
    homepage: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    detect: Detect,
}

#[derive(Serialize, Deserialize)]
struct ReleaseSource {
    version: String,
    released_at: String,
    #[serde(default)]
    notes: Vec<String>,
    #[serde(default)]
    withdrawn: Option<String>,
    #[serde(default)]
    min_hub_version: Option<String>,
    #[serde(default)]
    requires_apps: Vec<AppDependency>,
    channels: Vec<Channel>,
    artifacts: Vec<Artifact>,
}

impl ReleaseSource {
    fn into_release(self) -> Release {
        Release { version: self.version, released_at: self.released_at, notes: self.notes, withdrawn: self.withdrawn, min_hub_version: self.min_hub_version, requires_apps: self.requires_apps, artifacts: self.artifacts }
    }
}

fn release_path(src: &Path, app: &str, version: &str) -> R<PathBuf> {
    if !valid_app_id(app) {
        return Err(format!("invalid app id {app:?}"));
    }
    semver::Version::parse(version).map_err(|_| format!("{version:?} is not a semantic version"))?;
    Ok(src.join("apps").join(app).join("releases").join(format!("{version}.json")))
}

#[allow(clippy::too_many_arguments)]
fn release_add(src: &Path, app: &str, version: &str, file: &Path, url: &str, os: &str, arch: &str, os_min: Option<String>, launch: Option<String>, installed_size_mb: Option<u64>, install_dir_property: Option<String>, notes: Vec<String>, channels: Vec<String>, released_at: Option<String>) -> R<()> {
    if !src.join("apps").join(app).join("app.json").exists() {
        return Err(format!("no app.json for {app} under {}", src.display()));
    }
    let bytes = std::fs::read(file).map_err(|e| format!("cannot read {}: {e}", file.display()))?;
    let sha = hex::encode(Sha256::digest(&bytes));
    let os_e: Os = serde_json::from_value(serde_json::json!(os)).map_err(|e| e.to_string())?;
    let arch_e: Arch = serde_json::from_value(serde_json::json!(arch)).map_err(|e| e.to_string())?;
    let kind = match file.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("msi") => PackageKind::Msi,
        Some("dmg") => PackageKind::Dmg,
        Some("zip") => PackageKind::Zip,
        other => return Err(format!("unsupported package extension {other:?}")),
    };
    let msi = if kind == PackageKind::Msi {
        let p = hub_core::msi_info::read_msi(file).map_err(|e| e.to_string())?;
        if p.product_version != version {
            return Err(format!("the MSI says ProductVersion {}, but you are publishing {version}", p.product_version));
        }
        Some(MsiFacts { product_name: p.product_name, upgrade_code: p.upgrade_code, product_version: p.product_version, install_dir_property })
    } else {
        None
    };
    let artifact = Artifact { os: os_e, arch: arch_e, os_min, kind, url: url.to_string(), size: bytes.len() as u64, sha256: sha, installed_size_mb, launch, msi };

    let path = release_path(src, app, version)?;
    let mut rel: ReleaseSource = if path.exists() {
        read_json(&path)?
    } else {
        ReleaseSource {
            version: version.to_string(),
            released_at: released_at.unwrap_or_else(|| rfc3339(now())),
            notes: notes.clone(),
            withdrawn: None,
            min_hub_version: None,
            requires_apps: vec![],
            channels: channels.iter().map(|c| parse_channel(c)).collect::<R<_>>()?,
            artifacts: vec![],
        }
    };
    rel.artifacts.retain(|a| !(a.os == artifact.os && a.arch == artifact.arch));
    println!("{} {:?}/{:?}: {} bytes, sha256 {}", file.file_name().unwrap().to_string_lossy(), artifact.os, artifact.arch, artifact.size, artifact.sha256);
    rel.artifacts.push(artifact);
    write_json(&path, &rel)?;
    println!("wrote {}", path.display());
    Ok(())
}

fn edit_release(src: &Path, app: &str, version: &str, f: impl FnOnce(&mut ReleaseSource) -> R<()>) -> R<()> {
    let path = release_path(src, app, version)?;
    let mut rel: ReleaseSource = read_json(&path)?;
    f(&mut rel)?;
    write_json(&path, &rel)?;
    println!("updated {}", path.display());
    Ok(())
}

fn build(src: &Path, channel: &str, release_key: &Path, out: &Path, valid_days: i64, sequence: Option<u64>) -> R<()> {
    let channel = parse_channel(channel)?;
    let (key_id, key) = pubkeys::load_secret(&std::fs::read(release_key).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut apps = Vec::new();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(src.join("apps")).map_err(|e| format!("no apps directory in {}: {e}", src.display()))?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    dirs.sort();
    for d in dirs {
        let a: AppSource = read_json(&d.join("app.json"))?;
        if d.file_name().and_then(|n| n.to_str()) != Some(a.id.as_str()) {
            return Err(format!("{}: the directory name must equal the app id {}", d.display(), a.id));
        }
        let mut releases = Vec::new();
        if let Ok(rd) = std::fs::read_dir(d.join("releases")) {
            for f in rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")) {
                let r: ReleaseSource = read_json(&f)?;
                if r.channels.contains(&channel) {
                    releases.push(r.into_release());
                }
            }
        }
        if releases.is_empty() {
            println!("skipping {} (no release in {})", a.id, channel.as_str());
            continue;
        }
        releases.sort_by_key(|r| std::cmp::Reverse(semver::Version::parse(&r.version).ok()));
        apps.push(AppEntry { id: a.id, name: a.name, summary: a.summary, description: a.description, publisher: a.publisher, homepage: a.homepage, license: a.license, category: a.category, tags: a.tags, detect: a.detect, releases });
    }
    let t = now();
    let cat = Catalog {
        schema: CATALOG_SCHEMA,
        channel,
        sequence: sequence.unwrap_or(t.unix_timestamp() as u64),
        generated_at: rfc3339(t),
        expires_at: rfc3339(t + Duration::days(valid_days)),
        min_hub_version: None,
        apps,
    };
    let bytes = serde_json::to_vec_pretty(&cat).unwrap();
    // The very parser the Hub uses: if it would refuse this document, it is never signed.
    Catalog::parse(&bytes).map_err(|e| format!("the catalog does not validate: {e}"))?;
    let sig = pubkeys::sign_catalog(&key_id, &key, &bytes);
    let dir = out.join(channel.as_str());
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("catalog.json"), &bytes).map_err(|e| e.to_string())?;
    write_json(&dir.join("catalog.json.sig"), &sig)?;
    println!("built {}/catalog.json: {} app(s), sequence {}, signed by {key_id}", channel.as_str(), cat.apps.len(), cat.sequence);
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------
// verify
// ---------------------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct HubConfigFile {
    allowed_download_hosts: Vec<String>,
}

async fn load(site: &str, rel: &str) -> R<Vec<u8>> {
    if site.starts_with("https://") || site.starts_with("http://") {
        let url = format!("{}/{rel}", site.trim_end_matches('/'));
        let c = build_client(UrlPolicy::any_https()).map_err(|e| e.to_string())?;
        let r = c.get(&url).send().await.map_err(|e| format!("{url}: {e}"))?;
        if !r.status().is_success() {
            return Err(format!("{url}: HTTP {}", r.status()));
        }
        Ok(r.bytes().await.map_err(|e| e.to_string())?.to_vec())
    } else {
        std::fs::read(Path::new(site).join(rel)).map_err(|e| format!("{}: {e}", Path::new(site).join(rel).display()))
    }
}

async fn verify(site: &str, roots: &Path, channel: &str, hub_config: Option<&Path>, online: bool, allow_loopback: bool) -> R<()> {
    let channel = parse_channel(channel)?;
    let roots = RootSet::from_json(&std::fs::read(roots).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let t = now();
    let ks = trust::verify_keyset(&roots, &load(site, "keys.json").await?, &load(site, "keys.json.sig").await?, t).map_err(|e| e.to_string())?;
    println!("key set      OK  sequence {}, {} day(s) left", ks.sequence, ks.days_left(t));
    if ks.days_left(t) < 30 {
        println!("WARNING      the key set expires in {} day(s): re-sign it with the root key", ks.days_left(t));
    }
    let rel = format!("{}/catalog.json", channel.as_str());
    let cat = trust::verify_catalog(&ks, &load(site, &rel).await?, &load(site, &format!("{rel}.sig")).await?, t).map_err(|e| e.to_string())?;
    println!("catalog      OK  channel {}, sequence {}, {} app(s), expires {}", channel.as_str(), cat.sequence, cat.apps.len(), cat.expires_at);

    let hub_policy = match hub_config {
        Some(p) => {
            let c: HubConfigFile = read_json(p)?;
            Some(UrlPolicy::new(c.allowed_download_hosts, allow_loopback))
        }
        None => None,
    };
    let any = UrlPolicy::any_https();
    let client = build_client(any.clone()).map_err(|e| e.to_string())?;
    let tmp = std::env::temp_dir().join(format!("asterion-publish-verify-{}", std::process::id()));
    let mut failures = 0;
    for app in &cat.apps {
        for r in &app.releases {
            for a in &r.artifacts {
                if a.os == Os::Unknown || a.arch == Arch::Unknown || a.kind == PackageKind::Unknown {
                    continue;
                }
                let tag = format!("{} {} {:?}/{:?}", app.id, r.version, a.os, a.arch);
                if let Some(p) = &hub_policy {
                    let u = url::Url::parse(&a.url).map_err(|e| e.to_string())?;
                    if let Err(e) = p.check(&u) {
                        println!("artifact     FAIL {tag}: {e}");
                        failures += 1;
                        continue;
                    }
                }
                if !online {
                    println!("artifact     --   {tag} (not fetched; use --online)");
                    continue;
                }
                let spec = DownloadSpec { url: &a.url, size: a.size, sha256: &a.sha256, kind: a.kind, dest_dir: &tmp };
                match download_verified(&client, &any, &spec, &tokio_util::sync::CancellationToken::new(), |_, _| {}).await {
                    Ok(path) => {
                        let facts = match (&a.msi, a.kind) {
                            (Some(f), PackageKind::Msi) => hub_core::msi_info::check_against_catalog(&path, f).map(|_| ()).map_err(|e| e.to_string()),
                            _ => Ok(()),
                        };
                        match facts {
                            Ok(()) => println!("artifact     OK   {tag}: {} bytes, sha256 matches", a.size),
                            Err(e) => {
                                println!("artifact     FAIL {tag}: {e}");
                                failures += 1;
                            }
                        }
                    }
                    Err(e) => {
                        println!("artifact     FAIL {tag}: {e}");
                        failures += 1;
                    }
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    if failures > 0 {
        return Err(format!("{failures} artifact check(s) failed; do not promote"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------
// serve
// ---------------------------------------------------------------------------------------------------------

fn serve(dir: &Path, port: u16) -> R<()> {
    let root = dir.canonicalize().map_err(|e| format!("{}: {e}", dir.display()))?;
    let server = tiny_http::Server::http(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    println!("serving {} on http://127.0.0.1:{port}/  (Ctrl+C to stop; testing only, loopback only)", root.display());
    for req in server.incoming_requests() {
        let url = req.url().split('?').next().unwrap_or("/").to_string();
        let rel = url.trim_start_matches('/');
        let path = root.join(rel);
        // Never serve anything outside the directory (".." and symlinks resolved).
        let ok = path.canonicalize().ok().filter(|p| p.starts_with(&root) && p.is_file());
        let Some(file) = ok else {
            let _ = req.respond(tiny_http::Response::empty(404));
            continue;
        };
        let data = match std::fs::read(&file) {
            Ok(d) => d,
            Err(_) => {
                let _ = req.respond(tiny_http::Response::empty(500));
                continue;
            }
        };
        let range = req.headers().iter().find(|h| h.field.equiv("Range")).and_then(|h| h.value.as_str().strip_prefix("bytes=").map(|s| s.to_string()));
        let from = range.as_deref().and_then(|r| r.trim_end_matches('-').split('-').next()).and_then(|n| n.parse::<usize>().ok());
        let resp = match from {
            Some(f) if f < data.len() => {
                let tail = data[f..].to_vec();
                tiny_http::Response::from_data(tail)
                    .with_status_code(206)
                    .with_header(tiny_http::Header::from_bytes("Content-Range", format!("bytes {}-{}/{}", f, data.len() - 1, data.len())).unwrap())
            }
            Some(_) => tiny_http::Response::from_data(Vec::new()).with_status_code(416),
            None => tiny_http::Response::from_data(data),
        };
        let _ = req.respond(resp.with_header(tiny_http::Header::from_bytes("Accept-Ranges", "bytes").unwrap()));
    }
    Ok(())
}

