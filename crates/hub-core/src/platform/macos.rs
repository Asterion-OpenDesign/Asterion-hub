//! macOS: apps are `.app` bundles installed into `~/Applications` (no administrator rights).
//!
//! STATUS: written against Apple's documented tools (`hdiutil`, `ditto`, `plutil`, `open`) and
//! compile-checked, but NOT yet run on a Mac. Do not treat it as working until the macOS checklist in
//! `docs/STATUS.md` has been executed on real hardware.
//!
//! Gatekeeper: this module neither sets nor removes the `com.apple.quarantine` attribute and never calls
//! `spctl`/`xattr`. Whatever the system decides about an unsigned app stays the system's decision.

use super::*;
use crate::error::HubError;
use crate::manifest::PackageKind;
use std::process::Command;

pub fn os_version() -> String {
    Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "0".into())
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub fn default_install_dir(_app_id: &str, scope: InstallScope) -> Option<PathBuf> {
    match scope {
        InstallScope::User => home().map(|h| h.join("Applications")),
        InstallScope::Machine => Some(PathBuf::from("/Applications")),
    }
}

fn read_info(app: &Path) -> Option<plist::Dictionary> {
    plist::Value::from_file(app.join("Contents").join("Info.plist")).ok()?.into_dictionary()
}

fn plist_value(app: &Path, key: &str) -> Option<String> {
    read_info(app)?.get(key)?.as_string().map(str::to_string).filter(|s| !s.is_empty())
}

fn search_dirs() -> Vec<(PathBuf, InstallScope)> {
    let mut v = vec![(PathBuf::from("/Applications"), InstallScope::Machine)];
    if let Some(h) = home() {
        v.insert(0, (h.join("Applications"), InstallScope::User));
    }
    v
}

/// The bundles in the Applications folders, read once and reused for every app.
pub struct Snapshot(Vec<(PathBuf, InstallScope, String, String)>);

pub fn capture() -> Snapshot {
    let mut out = Vec::new();
    for (dir, scope) in search_dirs() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "app") {
                if let Some(info) = read_info(&p) {
                    let get = |k: &str| info.get(k).and_then(|v| v.as_string()).unwrap_or_default().to_string();
                    out.push((p, scope, get("CFBundleIdentifier"), get("CFBundleShortVersionString")));
                }
            }
        }
    }
    Snapshot(out)
}

impl Snapshot {
    pub fn detect(&self, app: &AppEntry, record: Option<&InstallRecord>) -> Option<InstalledInfo> {
        let rule = app.detect.macos.as_ref().or_else(|| record.and_then(|r| r.detect.macos.as_ref()))?;
        self.0.iter().find(|(_, _, id, _)| *id == rule.bundle_id).map(|(path, scope, _, version)| InstalledInfo {
            version: version.clone(),
            scope: *scope,
            location: Some(path.clone()),
            product_code: None,
            display_name: app.name.clone(),
        })
    }
}

pub fn detect(app: &AppEntry, record: Option<&InstallRecord>) -> Option<InstalledInfo> {
    capture().detect(app, record)
}

fn run(cmd: &mut Command, what: &'static str) -> Result<std::process::Output> {
    let out = cmd.output().map_err(|e| HubError::io(what, e))?;
    if out.status.success() {
        Ok(out)
    } else {
        tracing::warn!(tool = what, code = ?out.status.code(), "tool failed");
        Err(HubError::InstallerFailed(out.status.code().unwrap_or(-1)))
    }
}

fn single_app_in(dir: &Path) -> Result<PathBuf> {
    let apps: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| HubError::io("read package contents", e))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "app") && p.is_dir())
        .collect();
    match apps.as_slice() {
        [one] => Ok(one.clone()),
        _ => Err(HubError::UnexpectedPackage(format!("expected exactly one .app in the package, found {}", apps.len()))),
    }
}

pub async fn install(plan: &InstallPlan<'_>) -> Result<InstallResult> {
    let bundle_id = plan.app.detect.macos.as_ref().map(|m| m.bundle_id.clone()).ok_or_else(|| HubError::UnexpectedPackage("the catalog gives no bundle id for this app".into()))?;
    let dest_dir = default_install_dir(&plan.app.id, plan.scope).ok_or_else(|| HubError::Unsupported("no applications folder".into()))?;
    let package = plan.package.to_path_buf();
    let kind = plan.artifact.kind;
    let version = plan.release.version.clone();
    let existing = plan.existing.and_then(|e| e.location.clone());
    tokio::task::spawn_blocking(move || -> Result<InstallResult> {
        let work = tempfile_dir()?;
        let result = (|| {
            let (source_root, mounted) = match kind {
                PackageKind::Dmg => {
                    let mnt = work.join("mnt");
                    std::fs::create_dir_all(&mnt).map_err(|e| HubError::io("create mount point", e))?;
                    run(Command::new("/usr/bin/hdiutil").args(["attach", "-nobrowse", "-readonly", "-noautoopen", "-mountpoint"]).arg(&mnt).arg(&package), "hdiutil attach")?;
                    (mnt, true)
                }
                PackageKind::Zip => {
                    let out = work.join("unzipped");
                    std::fs::create_dir_all(&out).map_err(|e| HubError::io("create extraction directory", e))?;
                    run(Command::new("/usr/bin/ditto").args(["-x", "-k"]).arg(&package).arg(&out), "ditto extract")?;
                    (out, false)
                }
                other => return Err(HubError::Unsupported(format!("{other:?} packages cannot be installed on macOS"))),
            };
            let inner = (|| {
                let app = single_app_in(&source_root)?;
                // The bundle must be the product the catalog promised, at the version it promised.
                let got_id = plist_value(&app, "CFBundleIdentifier").unwrap_or_default();
                if got_id != bundle_id {
                    return Err(HubError::UnexpectedPackage(format!("the bundle id is {got_id:?}, the catalog says {bundle_id:?}")));
                }
                let got_v = plist_value(&app, "CFBundleShortVersionString").unwrap_or_default();
                if got_v != version {
                    return Err(HubError::UnexpectedPackage(format!("the bundle version is {got_v:?}, the catalog says {version:?}")));
                }
                std::fs::create_dir_all(&dest_dir).map_err(|e| HubError::io("create applications folder", e))?;
                let target = existing.clone().filter(|p| p.starts_with(&dest_dir)).unwrap_or_else(|| dest_dir.join(app.file_name().unwrap()));
                let staging = dest_dir.join(format!(".asterion-hub-{}.new", std::process::id()));
                let _ = std::fs::remove_dir_all(&staging);
                run(Command::new("/usr/bin/ditto").arg(&app).arg(&staging), "ditto copy")?;
                let backup = dest_dir.join(format!(".asterion-hub-{}.old", std::process::id()));
                let had_old = target.exists();
                if had_old {
                    std::fs::rename(&target, &backup).map_err(|e| HubError::io("move the old version aside", e))?;
                }
                if let Err(e) = std::fs::rename(&staging, &target) {
                    if had_old {
                        let _ = std::fs::rename(&backup, &target); // put the working version back
                    }
                    return Err(HubError::io("put the new version in place", e));
                }
                let _ = std::fs::remove_dir_all(&backup);
                Ok(InstallResult { location: Some(target), product_code: None, reboot_required: false })
            })();
            if mounted {
                let _ = Command::new("/usr/bin/hdiutil").args(["detach", "-quiet"]).arg(&source_root).output();
            }
            inner
        })();
        let _ = std::fs::remove_dir_all(&work);
        result
    })
    .await
    .map_err(|e| HubError::Internal(e.to_string()))?
}

fn tempfile_dir() -> Result<PathBuf> {
    let p = std::env::temp_dir().join(format!("asterion-hub-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
    std::fs::create_dir_all(&p).map_err(|e| HubError::io("create work directory", e))?;
    Ok(p)
}

pub async fn uninstall(info: &InstalledInfo, display_name: &str, _log_dir: &Path) -> Result<()> {
    let app = info.location.clone().ok_or_else(|| HubError::NotInstalled(display_name.to_string()))?;
    // Only ever remove a bundle that sits directly inside an Applications folder.
    let ok = app.extension().is_some_and(|x| x == "app") && search_dirs().iter().any(|(d, _)| app.parent() == Some(d.as_path()));
    if !ok {
        return Err(HubError::Unsupported("refusing to remove an application outside an Applications folder".into()));
    }
    std::fs::remove_dir_all(&app).map_err(|e| HubError::io("remove the application", e))
}

pub fn launch(path: &Path) -> Result<()> {
    run(Command::new("/usr/bin/open").arg(path), "open").map(|_| ())
}

pub fn launch_target_fallback(install_dir: &Path) -> Option<PathBuf> {
    install_dir.extension().is_some_and(|x| x == "app").then(|| install_dir.to_path_buf())
}

pub fn open_url(url: &str) -> Result<()> {
    if !url.starts_with("https://") {
        return Err(HubError::UrlRefused("only https links can be opened".into()));
    }
    run(Command::new("/usr/bin/open").arg(url), "open").map(|_| ())
}
