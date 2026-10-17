//! Windows: detection through the Uninstall registry keys, installation through `msiexec`.
//!
//! Elevation is requested only when the user chose an all-users install (or when an existing all-users
//! install has to be updated or removed). The default is a per-user install that needs no administrator
//! rights and no UAC prompt. Nothing here disables SmartScreen, antivirus or any other protection.

use super::*;
use crate::error::HubError;
use crate::manifest::{PackageKind, WindowsDetect};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use winreg::enums::*;
use winreg::RegKey;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;
const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
const UNINSTALL_WOW: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

pub fn os_version() -> String {
    let nt = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    match nt {
        Ok(k) => {
            let major = k.get_value::<u32, _>("CurrentMajorVersionNumber").unwrap_or(10);
            let minor = k.get_value::<u32, _>("CurrentMinorVersionNumber").unwrap_or(0);
            let build = k.get_value::<String, _>("CurrentBuildNumber").unwrap_or_else(|_| "0".into());
            format!("{major}.{minor}.{build}")
        }
        Err(_) => "10.0.0".into(),
    }
}

pub fn default_install_dir(app_id: &str, scope: InstallScope) -> Option<PathBuf> {
    match scope {
        InstallScope::User => std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Programs").join("ASTERION").join(app_id)),
        // All-users installs use the package's own default (Program Files).
        InstallScope::Machine => None,
    }
}

fn is_guid(s: &str) -> bool {
    s.len() == 38 && s.starts_with('{') && s.ends_with('}')
}

pub fn detect(app: &AppEntry, record: Option<&InstallRecord>) -> Option<InstalledInfo> {
    let rule: &WindowsDetect = app.detect.windows.as_ref().or_else(|| record.and_then(|r| r.detect.windows.as_ref()))?;
    let roots = [
        (HKEY_CURRENT_USER, UNINSTALL, InstallScope::User),
        (HKEY_LOCAL_MACHINE, UNINSTALL, InstallScope::Machine),
        (HKEY_LOCAL_MACHINE, UNINSTALL_WOW, InstallScope::Machine),
    ];
    let mut best: Option<InstalledInfo> = None;
    for (hive, path, scope) in roots {
        let Ok(parent) = RegKey::predef(hive).open_subkey_with_flags(path, KEY_READ) else { continue };
        for name in parent.enum_keys().flatten() {
            let Ok(k) = parent.open_subkey_with_flags(&name, KEY_READ) else { continue };
            let Ok(display) = k.get_value::<String, _>("DisplayName") else { continue };
            if !display.eq_ignore_ascii_case(&rule.display_name) {
                continue;
            }
            if let Some(want) = &rule.publisher {
                if !k.get_value::<String, _>("Publisher").is_ok_and(|p| p.eq_ignore_ascii_case(want)) {
                    continue;
                }
            }
            let version = k.get_value::<String, _>("DisplayVersion").unwrap_or_default();
            let location = k
                .get_value::<String, _>("InstallLocation")
                .ok()
                .map(|s| PathBuf::from(s.trim().trim_matches('"')))
                .filter(|p| !p.as_os_str().is_empty() && p.exists())
                .or_else(|| record.and_then(|r| r.location.as_ref()).map(PathBuf::from).filter(|p| p.exists()))
                .or_else(|| search_install_dir(rule));
            let is_msi = k.get_value::<u32, _>("WindowsInstaller").unwrap_or(0) == 1;
            let product_code = (is_msi && is_guid(&name)).then(|| name.clone());
            // The hive an ARP entry lives in does not say who the install belongs to (a per-user MSI is
            // listed under HKLM too), so ask Windows Installer for the product's real context.
            let scope = product_code.as_deref().and_then(msi_scope).unwrap_or(scope);
            let info = InstalledInfo { version, scope, location, product_code, display_name: display };
            // If both a per-user and an all-users copy exist, the per-user one is what this user runs first.
            if best.is_none() || scope == InstallScope::User {
                best = Some(info);
            }
        }
    }
    best
}

/// Per-user or per-machine, as Windows Installer itself registered the product for the current user.
fn msi_scope(product_code: &str) -> Option<InstallScope> {
    use windows_sys::Win32::System::ApplicationInstallationAndServicing::{MsiGetProductInfoExW, MSIINSTALLCONTEXT_MACHINE, MSIINSTALLCONTEXT_USERMANAGED, MSIINSTALLCONTEXT_USERUNMANAGED};
    let code = wide(product_code.as_ref());
    let prop = wide("VersionString".as_ref());
    let known = |ctx| {
        let mut buf = [0u16; 128];
        let mut len = buf.len() as u32;
        // SAFETY: all pointers are valid for the call; a null user SID means "the current user".
        let rc = unsafe { MsiGetProductInfoExW(code.as_ptr(), std::ptr::null(), ctx, prop.as_ptr(), buf.as_mut_ptr(), &mut len) };
        rc == 0
    };
    if known(MSIINSTALLCONTEXT_USERUNMANAGED) {
        Some(InstallScope::User)
    } else if known(MSIINSTALLCONTEXT_MACHINE) || known(MSIINSTALLCONTEXT_USERMANAGED) {
        Some(InstallScope::Machine)
    } else {
        None
    }
}

/// Installers that do not record their location: look for the known executable in the declared places.
fn search_install_dir(rule: &WindowsDetect) -> Option<PathBuf> {
    let exe = rule.executable.as_ref()?;
    rule.search_dirs.iter().find_map(|d| {
        let (var, rest) = if let Some(r) = d.strip_prefix("%LOCALAPPDATA%\\") {
            ("LOCALAPPDATA", r)
        } else if let Some(r) = d.strip_prefix("%PROGRAMFILES%\\") {
            ("ProgramW6432", r)
        } else {
            return None;
        };
        let base = std::env::var_os(var).or_else(|| (var == "ProgramW6432").then(|| std::env::var_os("ProgramFiles")).flatten())?;
        let dir = PathBuf::from(base).join(rest.replace('/', "\\"));
        dir.join(exe).is_file().then_some(dir)
    })
}

fn system32(exe: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    root.join("System32").join(exe)
}

fn quote(p: &Path) -> String {
    format!("\"{}\"", p.display())
}

/// Run msiexec with an exact command line. `elevate` shows the Windows UAC prompt.
async fn run_msiexec(cmdline: String, elevate: bool) -> Result<i32> {
    let exe = system32("msiexec.exe");
    if elevate {
        return tokio::task::spawn_blocking(move || shell_execute_runas(&exe, &cmdline))
            .await
            .map_err(|e| HubError::Internal(e.to_string()))?;
    }
    let status = tokio::process::Command::new(&exe)
        .raw_arg(cmdline)
        .creation_flags(CREATE_NO_WINDOW)
        .kill_on_drop(false)
        .status()
        .await
        .map_err(|e| HubError::io("start msiexec", e))?;
    Ok(status.code().unwrap_or(-1))
}

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

fn shell_execute_runas(exe: &Path, params: &str) -> Result<i32> {
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    let verb = wide("runas".as_ref());
    let file = wide(exe.as_os_str());
    let args = wide(params.as_ref());
    // SAFETY: the structure is zero-initialised and filled with pointers that outlive the call; the
    // process handle returned through SEE_MASK_NOCLOSEPROCESS is closed below.
    unsafe {
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = args.as_ptr();
        info.nShow = 0; // SW_HIDE: msiexec shows its own basic UI
        if ShellExecuteExW(&mut info) == 0 {
            let err = GetLastError();
            return if err == 1223 {
                // ERROR_CANCELLED: the user declined the UAC prompt.
                Err(HubError::InstallCancelled)
            } else {
                Err(HubError::Internal(format!("could not start the elevated installer (Windows error {err})")))
            };
        }
        let h = info.hProcess;
        if h.is_null() {
            return Err(HubError::Internal("the elevated installer did not return a process".into()));
        }
        let waited = WaitForSingleObject(h, INFINITE);
        let mut code = 0u32;
        let got = GetExitCodeProcess(h, &mut code);
        CloseHandle(h);
        if waited != WAIT_OBJECT_0 || got == 0 {
            return Err(HubError::Internal("lost track of the elevated installer".into()));
        }
        Ok(code as i32)
    }
}

fn map_exit(code: i32) -> Result<bool> {
    match code {
        0 => Ok(false),
        3010 | 1641 => Ok(true),
        1602 => Err(HubError::InstallCancelled),
        1618 => Err(HubError::InstallerBusy),
        1638 => Err(HubError::NewerInstalled),
        other => Err(HubError::InstallerFailed(other)),
    }
}

pub async fn install(plan: &InstallPlan<'_>) -> Result<InstallResult> {
    if plan.artifact.kind != PackageKind::Msi {
        return Err(HubError::Unsupported(format!("{:?} packages cannot be installed on Windows", plan.artifact.kind)));
    }
    let facts = plan.artifact.msi.as_ref().ok_or_else(|| HubError::UnexpectedPackage("no MSI facts for an MSI artifact".into()))?;
    std::fs::create_dir_all(plan.log_dir).map_err(|e| HubError::io("create log directory", e))?;
    let log = plan.log_dir.join(format!("msi-{}-{}.log", plan.app.id, plan.release.version));

    // Keep an existing install where it is; otherwise choose the per-user default.
    let target_dir = plan
        .existing
        .and_then(|e| e.location.clone())
        .or_else(|| default_install_dir(&plan.app.id, plan.scope));

    let mut line = format!("/i {} /norestart /l*v {}", quote(plan.package), quote(&log));
    let elevate = plan.scope == InstallScope::Machine;
    match plan.scope {
        InstallScope::User => line.push_str(" /qn MSIINSTALLPERUSER=1 ALLUSERS=2"),
        InstallScope::Machine => line.push_str(" /qb! ALLUSERS=1"),
    }
    let mut location = None;
    if let (Some(prop), Some(dir)) = (&facts.install_dir_property, &target_dir) {
        // The package's own property name (validated as an upper-case identifier by the catalog parser).
        line.push_str(&format!(" {prop}=\"{}\\\"", dir.display().to_string().trim_end_matches('\\')));
        location = Some(dir.clone());
    }
    if plan.repair {
        line.push_str(" REINSTALL=ALL REINSTALLMODE=vomus");
    }

    tracing::info!(app = %plan.app.id, version = %plan.release.version, scope = ?plan.scope, repair = plan.repair, "running msiexec");
    let code = run_msiexec(line, elevate).await?;
    tracing::info!(exit = code, "msiexec finished");
    let reboot_required = map_exit(code)?;
    Ok(InstallResult { location, product_code: None, reboot_required })
}

pub async fn uninstall(info: &InstalledInfo, display_name: &str, log_dir: &Path) -> Result<()> {
    let code = info.product_code.as_ref().ok_or_else(|| HubError::Unsupported(format!("{display_name} was not installed by Windows Installer; remove it from Windows Settings")))?;
    std::fs::create_dir_all(log_dir).map_err(|e| HubError::io("create log directory", e))?;
    let log = log_dir.join("msi-uninstall.log");
    let elevate = info.scope == InstallScope::Machine;
    let ui = if elevate { "/qb!" } else { "/qn" };
    let line = format!("/x {code} {ui} /norestart /l*v {}", quote(&log));
    let exit = run_msiexec(line, elevate).await?;
    // 1605: "this action is only valid for products that are currently installed" - already gone.
    if exit == 1605 {
        return Ok(());
    }
    map_exit(exit).map(|_| ())
}

pub fn launch(path: &Path) -> Result<()> {
    if !path.is_file() {
        return Err(HubError::NotInstalled(format!("{} is missing", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())));
    }
    std::process::Command::new(path)
        .current_dir(path.parent().unwrap_or(Path::new(".")))
        .creation_flags(DETACHED_PROCESS)
        .spawn()
        .map(|_| ())
        .map_err(|e| HubError::io("start application", e))
}

pub fn launch_target_fallback(_install_dir: &Path) -> Option<PathBuf> {
    None
}

pub fn open_url(url: &str) -> Result<()> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    if !url.starts_with("https://") {
        return Err(HubError::UrlRefused("only https links can be opened".into()));
    }
    let verb = wide("open".as_ref());
    let file = wide(url.as_ref());
    // SAFETY: both strings are NUL-terminated and outlive the call; the remaining pointers are null.
    let r = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), file.as_ptr(), std::ptr::null(), std::ptr::null(), 1) };
    if (r as isize) > 32 {
        Ok(())
    } else {
        Err(HubError::Internal("Windows could not open the link".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msiexec_exit_codes_map_to_the_right_errors() {
        assert_eq!(map_exit(0).unwrap(), false);
        assert_eq!(map_exit(3010).unwrap(), true);
        assert_eq!(map_exit(1602).unwrap_err().code(), "install_cancelled");
        assert_eq!(map_exit(1618).unwrap_err().code(), "installer_busy");
        assert_eq!(map_exit(1638).unwrap_err().code(), "newer_installed");
        assert_eq!(map_exit(1603).unwrap_err().code(), "installer_failed");
    }

    #[test]
    fn detection_of_an_app_that_is_not_installed_is_none() {
        let mut app = crate::manifest::fixtures::app("x", vec![]);
        app.detect.windows = Some(WindowsDetect { display_name: "ASTERION Hub Test Product That Does Not Exist 9f3a".into(), publisher: None, ..Default::default() });
        assert!(detect(&app, None).is_none());
    }

    /// Read-only: finds an ASTERION BIM that was installed by something other than this Hub, if there is one.
    #[test]
    #[ignore = "depends on what is installed on this machine"]
    fn detects_an_existing_asterion_bim_install() {
        let mut app = crate::manifest::fixtures::app("asterion-bim", vec![]);
        app.detect.windows = Some(WindowsDetect {
            display_name: "ASTERION BIM".into(),
            publisher: Some("ASTERION Open Design".into()),
            search_dirs: vec!["%LOCALAPPDATA%\\Programs\\ASTERION".into(), "%PROGRAMFILES%\\ASTERION".into()],
            executable: Some("asterion-shell.exe".into()),
        });
        let found = detect(&app, None);
        println!("{found:#?}");
        if let Some(i) = found {
            assert!(i.location.is_some_and(|l| l.join("asterion-shell.exe").is_file()), "location resolved through search_dirs");
        }
    }

    #[test]
    fn a_non_http_url_is_never_handed_to_the_shell() {
        assert!(open_url("file:///C:/Windows/System32/calc.exe").is_err());
        assert!(open_url("calc.exe").is_err());
    }
}
