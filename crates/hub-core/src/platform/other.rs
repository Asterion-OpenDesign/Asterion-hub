//! Stubs for systems the Hub does not support yet (Linux builds exist only so the core's tests can run in CI).

use super::*;
use crate::error::HubError;

pub fn os_version() -> String {
    "0".into()
}
pub struct Snapshot;
pub fn capture() -> Snapshot {
    Snapshot
}
impl Snapshot {
    pub fn detect(&self, _: &AppEntry, _: Option<&InstallRecord>) -> Option<InstalledInfo> {
        None
    }
}
pub fn detect(_: &AppEntry, _: Option<&InstallRecord>) -> Option<InstalledInfo> {
    None
}
pub async fn install(_: &InstallPlan<'_>) -> Result<InstallResult> {
    Err(HubError::Unsupported("installing is only available on Windows and macOS".into()))
}
pub async fn uninstall(_: &InstalledInfo, _: &str, _: &Path) -> Result<()> {
    Err(HubError::Unsupported("uninstalling is only available on Windows and macOS".into()))
}
pub fn launch(_: &Path) -> Result<()> {
    Err(HubError::Unsupported("launching is only available on Windows and macOS".into()))
}
pub fn launch_target_fallback(_: &Path) -> Option<PathBuf> {
    None
}
pub fn default_install_dir(_: &str, _: InstallScope) -> Option<PathBuf> {
    None
}
pub fn open_url(_: &str) -> Result<()> {
    Err(HubError::Unsupported("opening links is only available on Windows and macOS".into()))
}
