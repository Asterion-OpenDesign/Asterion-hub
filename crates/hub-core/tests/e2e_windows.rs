//! End-to-end test of the real engine against a real MSI on this machine: catalog over HTTP, signature
//! verification, download with progress, SHA-256 + MSI-facts check, `msiexec` install, detection through the
//! registry, update, refusal while the app is running, repair, a corrupted release that must not touch the
//! working install, and uninstall.
//!
//! It changes the machine (a per-user install of a throw-away test product named "ASTERION Hub Test App",
//! removed at the end), so it is ignored by default:
//!
//!   powershell -File tests/fixtures/build.ps1            # builds .dev/fixtures/testapp-1.0.0.msi and 1.1.0
//!   cargo test -p asterion-hub-core --test e2e_windows -- --ignored --nocapture
//!
//! It never touches any real ASTERION application: the test product has its own name and UpgradeCode.
#![cfg(windows)]

use hub_core::engine::{Action, AppState, Emitter, Engine, Event, HubConfig, Phase};
use hub_core::manifest::*;
use hub_core::trust::publisher::{generate, load_secret, sign_catalog, sign_keyset};
use hub_core::trust::{DelegatedKey, KeySet, RootSet};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type Files = Arc<Mutex<HashMap<String, Vec<u8>>>>;

fn serve(files: Files) -> String {
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr().to_ip().unwrap());
    std::thread::spawn(move || {
        for req in server.incoming_requests() {
            let path = req.url().split('?').next().unwrap().trim_start_matches('/').to_string();
            let data = files.lock().unwrap().get(&path).cloned();
            let Some(data) = data else {
                let _ = req.respond(tiny_http::Response::empty(404));
                continue;
            };
            let from = req.headers().iter().find(|h| h.field.equiv("Range")).and_then(|h| h.value.as_str().strip_prefix("bytes=").and_then(|s| s.trim_end_matches('-').parse::<usize>().ok()));
            let resp = match from {
                Some(f) if f < data.len() => tiny_http::Response::from_data(data[f..].to_vec())
                    .with_status_code(206)
                    .with_header(tiny_http::Header::from_bytes("Content-Range", format!("bytes {}-{}/{}", f, data.len() - 1, data.len())).unwrap()),
                _ => tiny_http::Response::from_data(data),
            };
            // A little throttling so that progress is observable on a fast loopback.
            let _ = req.respond(resp);
        }
    });
    base
}

fn fixture(name: &str) -> Vec<u8> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.dev/fixtures").join(name);
    std::fs::read(&p).unwrap_or_else(|_| panic!("missing {} - run tests/fixtures/build.ps1 first", p.display()))
}

fn artifact(base: &str, file: &str, bytes: &[u8], version: &str, hash_override: Option<String>) -> Artifact {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), bytes).unwrap();
    let p = hub_core::msi_info::read_msi(tmp.path()).unwrap();
    Artifact {
        os: Os::Windows,
        arch: Arch::X86_64,
        os_min: Some("10.0.17763".into()),
        kind: PackageKind::Msi,
        url: format!("{base}/files/{file}"),
        size: bytes.len() as u64,
        sha256: hash_override.unwrap_or_else(|| hex::encode(Sha256::digest(bytes))),
        installed_size_mb: Some(1),
        launch: Some("testapp.exe".into()),
        msi: Some(MsiFacts { product_name: p.product_name, upgrade_code: p.upgrade_code, product_version: version.into(), install_dir_property: Some("INSTALLFOLDER".into()) }),
    }
}

fn app(releases: Vec<Release>) -> AppEntry {
    AppEntry {
        id: "hub-test-app".into(),
        name: "ASTERION Hub Test App".into(),
        summary: "Throw-away product used by the Hub's own end-to-end test.".into(),
        description: String::new(),
        publisher: "ASTERION Hub Tests".into(),
        detect: Detect { windows: Some(WindowsDetect { display_name: "ASTERION Hub Test App".into(), publisher: Some("ASTERION Hub Tests".into()), search_dirs: vec![], executable: Some("testapp.exe".into()) }), macos: None },
        releases,
        ..Default::default()
    }
}

fn release(version: &str, a: Artifact) -> Release {
    Release { version: version.into(), released_at: "2026-10-17T00:00:00Z".into(), notes: vec![format!("test release {version}")], withdrawn: None, min_hub_version: None, requires_apps: vec![], artifacts: vec![a] }
}

async fn wait_final(engine: &Engine, id: &str) -> hub_core::engine::TaskView {
    let t0 = Instant::now();
    loop {
        if let Some(t) = engine.tasks().into_iter().find(|t| t.id == id) {
            if t.phase.is_final() {
                return t;
            }
        }
        assert!(t0.elapsed() < Duration::from_secs(180), "task {id} did not finish");
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

#[tokio::test]
#[ignore = "installs and removes a throw-away MSI on this machine"]
async fn install_update_repair_failed_update_and_uninstall() {
    // ---- the test product must not be installed already ----
    let probe = app(vec![]);
    assert!(hub_core::platform::detect(&probe, None).is_none(), "the test product is already installed; remove it first");

    let m100 = fixture("testapp-1.0.0.msi");
    let m110 = fixture("testapp-1.1.0.msi");
    let files: Files = Arc::new(Mutex::new(HashMap::new()));
    files.lock().unwrap().insert("files/testapp-1.0.0.msi".into(), m100.clone());
    files.lock().unwrap().insert("files/testapp-1.1.0.msi".into(), m110.clone());
    let base = serve(files.clone());

    // ---- a real two-tier PKI, in memory ----
    let (root_secret, root_pub) = generate("root-e2e");
    let (_, root) = load_secret(&serde_json::to_vec(&root_secret).unwrap()).unwrap();
    let (rel_secret, _) = generate("rel-e2e");
    let (_, release_key) = load_secret(&serde_json::to_vec(&rel_secret).unwrap()).unwrap();
    let rel_pub = hub_core::trust::publisher::public_of("rel-e2e", &release_key);
    let now = time::OffsetDateTime::now_utc();
    let rfc = |t: time::OffsetDateTime| t.format(&time::format_description::well_known::Rfc3339).unwrap();
    let ks = serde_json::to_vec(&KeySet {
        schema: 1,
        sequence: 1,
        generated_at: rfc(now),
        expires_at: rfc(now + time::Duration::days(365)),
        keys: vec![DelegatedKey { id: rel_pub.id, alg: rel_pub.alg, public: rel_pub.public, roles: vec!["catalog".into()], not_before: rfc(now - time::Duration::hours(1)), not_after: rfc(now + time::Duration::days(365)) }],
        revoked: vec![],
    })
    .unwrap();
    files.lock().unwrap().insert("keys.json".into(), ks.clone());
    files.lock().unwrap().insert("keys.json.sig".into(), serde_json::to_vec(&sign_keyset("root-e2e", &root, &ks)).unwrap());
    let roots = RootSet::from_json(&serde_json::to_vec(&serde_json::json!({"schema":1,"roots":[root_pub]})).unwrap()).unwrap();

    let publish = |seq: u64, releases: Vec<Release>| {
        let cat = Catalog { schema: 1, channel: Channel::Stable, sequence: seq, generated_at: rfc(now), expires_at: rfc(now + time::Duration::days(30)), min_hub_version: None, apps: vec![app(releases)] };
        let bytes = serde_json::to_vec(&cat).unwrap();
        files.lock().unwrap().insert("stable/catalog.json".into(), bytes.clone());
        files.lock().unwrap().insert("stable/catalog.json.sig".into(), serde_json::to_vec(&sign_catalog("rel-e2e", &release_key, &bytes)).unwrap());
    };
    publish(1, vec![release("1.0.0", artifact(&base, "testapp-1.0.0.msi", &m100, "1.0.0", None))]);

    // ---- the engine, with every event recorded ----
    let events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(vec![]));
    let sink = events.clone();
    let emit: Emitter = Arc::new(move |e| sink.lock().unwrap().push(e));
    let data = tempfile::tempdir().unwrap();
    let engine = Engine::new(
        HubConfig { hub_version: semver::Version::new(0, 1, 0), catalog_base: url::Url::parse(&format!("{base}/")).unwrap(), roots, allowed_hosts: vec![], allow_loopback_http: true, data_dir: data.path().to_path_buf() },
        emit,
    )
    .unwrap();

    // Whatever happens below, the throw-away product is removed when this guard drops.
    struct Cleanup(AppEntry);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            if let Some(info) = hub_core::platform::detect(&self.0, None) {
                if let Some(code) = info.product_code {
                    let _ = std::process::Command::new("msiexec.exe").args(["/x", &code, "/qn", "/norestart"]).status();
                }
            }
        }
    }
    let _cleanup = Cleanup(probe.clone());

    // 1. the catalog arrives, verified
    let status = engine.refresh().await;
    assert!(status.loaded, "{status:?}");
    assert_eq!(status.origin, Some(hub_core::catalog::Origin::Network));
    assert!(status.refresh_error.is_none());
    let v = engine.apps();
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].state, AppState::NotInstalled);
    assert_eq!(v[0].actions, vec![Action::Install]);
    println!("catalog verified; app offered: {} {}", v[0].name, v[0].latest.as_ref().unwrap().version);

    // 2. install, with real progress
    let id = engine.start("hub-test-app", Action::Install).unwrap();
    let t = wait_final(&engine, &id).await;
    assert_eq!(t.phase, Phase::Done, "{:?}", t.error);
    let progress: Vec<(u64, u64)> = events.lock().unwrap().iter().filter_map(|e| match e { Event::Task { task } if task.phase == Phase::Downloading => Some((task.bytes_done, task.bytes_total)), _ => None }).collect();
    assert!(progress.iter().any(|(d, t)| *d == *t && *t == m100.len() as u64), "the download reached 100 %: {progress:?}");
    assert!(progress.windows(2).all(|w| w[0].0 <= w[1].0));
    let phases: Vec<Phase> = events.lock().unwrap().iter().filter_map(|e| match e { Event::Task { task } if task.id == id => Some(task.phase), _ => None }).collect();
    for p in [Phase::Preparing, Phase::Downloading, Phase::Verifying, Phase::Installing, Phase::Finishing, Phase::Done] {
        assert!(phases.contains(&p), "phase {p:?} was reported: {phases:?}");
    }
    let info = hub_core::platform::detect(&probe, None).expect("the system reports the install");
    assert_eq!(info.version, "1.0.0");
    assert_eq!(info.scope, hub_core::store::InstallScope::User, "per-user: no elevation was needed");
    let v = engine.apps();
    assert_eq!(v[0].state, AppState::Installed);
    let loc = PathBuf::from(v[0].installed.as_ref().unwrap().location.clone().expect("location known"));
    assert!(loc.join("testapp.exe").is_file(), "{}", loc.display());
    assert!(v[0].installed.as_ref().unwrap().can_open && v[0].installed.as_ref().unwrap().installed_by_hub);
    println!("installed 1.0.0 per-user at {}", loc.display());

    // 3. a newer version is published: the Hub notices
    publish(2, vec![release("1.0.0", artifact(&base, "testapp-1.0.0.msi", &m100, "1.0.0", None)), release("1.1.0", artifact(&base, "testapp-1.1.0.msi", &m110, "1.1.0", None))]);
    engine.refresh().await;
    let v = engine.apps();
    assert_eq!(v[0].state, AppState::UpdateAvailable);
    assert_eq!(v[0].latest.as_ref().unwrap().version, "1.1.0");
    assert!(v[0].actions.contains(&Action::Update));

    // 4. the app is running: the Hub refuses to replace files in use, and changes nothing
    let mut running = std::process::Command::new(loc.join("testapp.exe")).args(["-n", "120", "127.0.0.1"]).stdout(std::process::Stdio::null()).spawn().unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(engine.apps()[0].installed.as_ref().unwrap().running, "running state is detected");
    let id = engine.start("hub-test-app", Action::Update).unwrap();
    let t = wait_final(&engine, &id).await;
    assert_eq!(t.phase, Phase::Failed);
    assert_eq!(t.error.as_ref().unwrap().code, "app_running");
    assert_eq!(hub_core::platform::detect(&probe, None).unwrap().version, "1.0.0");
    let _ = running.kill();
    let _ = running.wait();
    tokio::time::sleep(Duration::from_millis(300)).await;

    // 5. update
    let id = engine.start("hub-test-app", Action::Update).unwrap();
    let t = wait_final(&engine, &id).await;
    assert_eq!(t.phase, Phase::Done, "{:?}", t.error);
    assert_eq!(hub_core::platform::detect(&probe, None).unwrap().version, "1.1.0");
    assert_eq!(engine.apps()[0].state, AppState::Installed);
    println!("updated to 1.1.0");

    // 6. repair: delete a file, repair restores it
    std::fs::remove_file(loc.join("readme.txt")).unwrap();
    let id = engine.start("hub-test-app", Action::Repair).unwrap();
    let t = wait_final(&engine, &id).await;
    assert_eq!(t.phase, Phase::Done, "{:?}", t.error);
    assert!(loc.join("readme.txt").is_file(), "repair restored the deleted file");
    println!("repaired");

    // 7. a bad release (hash does not match the bytes served): refused, working install untouched
    let mut evil = release("1.2.0", artifact(&base, "testapp-1.1.0.msi", &m110, "1.2.0", Some("0".repeat(64))));
    evil.notes = vec!["tampered".into()];
    publish(3, vec![release("1.1.0", artifact(&base, "testapp-1.1.0.msi", &m110, "1.1.0", None)), evil]);
    engine.refresh().await;
    assert_eq!(engine.apps()[0].state, AppState::UpdateAvailable);
    let id = engine.start("hub-test-app", Action::Update).unwrap();
    let t = wait_final(&engine, &id).await;
    assert_eq!(t.phase, Phase::Failed);
    assert_eq!(t.error.as_ref().unwrap().code, "hash_mismatch");
    assert!(t.error.as_ref().unwrap().retryable);
    assert_eq!(hub_core::platform::detect(&probe, None).unwrap().version, "1.1.0", "the installed app is untouched");
    assert!(loc.join("testapp.exe").is_file());
    println!("tampered release refused, 1.1.0 intact");

    // 8. a MSI that is not what the (signed) catalog says: right hash, wrong facts
    let mut wrong = artifact(&base, "testapp-1.0.0.msi", &m100, "1.0.0", None);
    wrong.msi.as_mut().unwrap().product_version = "9.9.9".into();
    publish(4, vec![release("9.9.9", wrong)]);
    engine.refresh().await;
    let id = engine.start("hub-test-app", Action::Update).unwrap();
    let t = wait_final(&engine, &id).await;
    assert_eq!(t.phase, Phase::Failed);
    assert_eq!(t.error.as_ref().unwrap().code, "unexpected_package");
    assert_eq!(hub_core::platform::detect(&probe, None).unwrap().version, "1.1.0");

    // 9. uninstall; user data and the Hub's own folder are not involved
    let id = engine.start("hub-test-app", Action::Uninstall).unwrap();
    let t = wait_final(&engine, &id).await;
    assert_eq!(t.phase, Phase::Done, "{:?}", t.error);
    assert!(hub_core::platform::detect(&probe, None).is_none());
    assert!(!loc.join("testapp.exe").exists());
    assert!(engine.apps().iter().all(|a| a.installed.is_none()));
    println!("uninstalled");

    // 10. history tells the story
    let h = engine.history(50);
    let story: Vec<(String, String)> = h.iter().rev().map(|e| (e.action.clone(), format!("{:?}", e.outcome))).collect();
    println!("history (oldest first): {story:?}");
    assert_eq!(story.len(), 7);
    assert_eq!(story.first().unwrap(), &("install".to_string(), "Success".to_string()));
    assert_eq!(story.last().unwrap(), &("uninstall".to_string(), "Success".to_string()));
}