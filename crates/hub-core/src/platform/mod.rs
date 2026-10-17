//! Everything that touches the operating system: finding installed apps, running installers,
//! uninstalling, launching, checking whether an app is running. One module per OS behind the same
//! small set of functions, so the engine above is OS-agnostic.
//!
//! Windows is implemented and has been exercised on a real machine. macOS is implemented but has NOT been
//! run on a Mac yet; see `docs/STATUS.md`. Other systems compile to stubs that report "unsupported".

use crate::error::Result;
use crate::manifest::{AppEntry, Arch, Artifact, HostInfo, Os, Release};
use crate::store::{InstallRecord, InstallScope};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
use windows as imp;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
mod other;
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
use other as imp;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InstalledInfo {
    pub version: String,
    pub scope: InstallScope,
    pub location: Option<PathBuf>,
    pub product_code: Option<String>,
    pub display_name: String,
}

pub struct InstallPlan<'a> {
    pub app: &'a AppEntry,
    pub release: &'a Release,
    pub artifact: &'a Artifact,
    pub package: &'a Path,
    pub scope: InstallScope,
    pub log_dir: &'a Path,
    /// Reinstall the same version over itself to restore missing or damaged files.
    pub repair: bool,
    pub existing: Option<&'a InstalledInfo>,
}

#[derive(Debug, Clone, Default)]
pub struct InstallResult {
    pub location: Option<PathBuf>,
    pub product_code: Option<String>,
    pub reboot_required: bool,
}

pub fn host_info(hub_version: semver::Version) -> HostInfo {
    let os = if cfg!(target_os = "windows") {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::Macos
    } else {
        Os::Unknown
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => Arch::X86_64,
        "aarch64" => Arch::Aarch64,
        _ => Arch::Unknown,
    };
    HostInfo { os, arch, os_version: imp::os_version(), hub_version }
}

/// What the system says is installed, read once. Use it when asking about many apps.
pub struct SystemApps(imp::Snapshot);

impl SystemApps {
    pub fn capture() -> SystemApps {
        SystemApps(imp::capture())
    }
    pub fn detect(&self, app: &AppEntry, record: Option<&InstallRecord>) -> Option<InstalledInfo> {
        self.0.detect(app, record)
    }
}

/// Is the app installed, by what the system itself says (not by what this Hub remembers)?
pub fn detect(app: &AppEntry, record: Option<&InstallRecord>) -> Option<InstalledInfo> {
    imp::detect(app, record)
}

pub async fn install(plan: &InstallPlan<'_>) -> Result<InstallResult> {
    imp::install(plan).await
}

pub async fn uninstall(info: &InstalledInfo, display_name: &str, log_dir: &Path) -> Result<()> {
    imp::uninstall(info, display_name, log_dir).await
}

pub fn launch(path: &Path) -> Result<()> {
    imp::launch(path)
}

/// Where the app's own executable (or bundle) is, if it can be determined.
pub fn launch_target(info: &InstalledInfo, launch: Option<&str>) -> Option<PathBuf> {
    let loc = info.location.as_ref()?;
    if let Some(bundle) = imp::launch_target_fallback(loc) {
        return Some(bundle); // macOS: the .app itself
    }
    match launch {
        Some(rel) if crate::manifest::is_safe_relative_path(rel) => {
            let p = loc.join(rel);
            p.exists().then_some(p)
        }
        _ => None,
    }
}

/// One look at the process list, reusable for many questions.
pub struct RunningSet(Vec<String>);

impl RunningSet {
    pub fn capture() -> RunningSet {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
        let mut sys = System::new();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::Always));
        RunningSet(sys.processes().values().filter_map(|p| p.exe().map(normalize)).collect())
    }

    /// Is a process running from this executable, or from inside this bundle directory?
    pub fn contains(&self, target: &Path) -> bool {
        let want = normalize(target);
        self.0.iter().any(|have| *have == want || have.starts_with(&format!("{want}/")))
    }
}

pub fn is_running(target: &Path) -> bool {
    RunningSet::capture().contains(target)
}
fn normalize(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/").trim_end_matches('/').to_lowercase()
}

pub fn default_install_dir(app_name: &str, scope: InstallScope) -> Option<PathBuf> {
    imp::default_install_dir(app_name, scope)
}

/// Free bytes on the volume that holds `path` (or the nearest existing parent).
pub fn free_space(path: &Path) -> Option<u64> {
    use sysinfo::Disks;
    let mut probe = Some(path);
    while let Some(p) = probe {
        if p.exists() {
            break;
        }
        probe = p.parent();
    }
    let probe = probe?.canonicalize().ok()?;
    let probe = normalize(&probe).trim_start_matches("//?/").to_string();
    let disks = Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| probe.starts_with(&normalize(d.mount_point())))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

pub fn open_url(url: &str) -> Result<()> {
    imp::open_url(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_space_is_found_for_an_existing_and_a_not_yet_existing_path() {
        let d = tempfile::tempdir().unwrap();
        assert!(free_space(d.path()).is_some_and(|b| b > 0));
        assert!(free_space(&d.path().join("not").join("yet")).is_some_and(|b| b > 0));
    }

    #[test]
    fn this_process_is_seen_running_and_a_made_up_path_is_not() {
        let me = std::env::current_exe().unwrap();
        assert!(is_running(&me));
        assert!(!is_running(Path::new("C:/definitely/not/running/anything.exe")));
    }

    #[test]
    fn the_host_reports_something_usable() {
        let h = host_info(semver::Version::new(0, 1, 0));
        assert_ne!(h.arch, Arch::Unknown);
        assert!(!h.os_version.is_empty());
        #[cfg(target_os = "windows")]
        assert!(h.os_version.starts_with("10.0."), "{}", h.os_version);
    }
}

