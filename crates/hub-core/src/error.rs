//! One error type for the whole core. Every variant has a stable machine `code()` that the UI maps to
//! user-facing copy; the `Display` text is English, for logs, and never contains secrets or full
//! user paths beyond what is needed to diagnose.

use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum HubError {
    // ---- network ------------------------------------------------------------------------------------
    #[error("network unavailable: {0}")]
    Offline(String),
    #[error("server answered HTTP {status} for {what}")]
    HttpStatus { status: u16, what: &'static str },
    #[error("download interrupted: {0}")]
    DownloadInterrupted(String),
    #[error("download URL refused: {0}")]
    UrlRefused(String),

    // ---- trust --------------------------------------------------------------------------------------
    #[error("signature verification failed: {0}")]
    BadSignature(String),
    #[error("metadata expired: {0}")]
    Expired(String),
    #[error("metadata rollback refused: {0}")]
    Rollback(String),
    #[error("invalid metadata: {0}")]
    InvalidMetadata(String),
    #[error("unsupported catalog schema {found} (this Hub reads {supported})")]
    UnsupportedSchema { found: u32, supported: u32 },

    // ---- artifacts ----------------------------------------------------------------------------------
    #[error("size mismatch: expected {expected} bytes, got {actual}")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("SHA-256 mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },
    #[error("unexpected package: {0}")]
    UnexpectedPackage(String),

    // ---- installation -------------------------------------------------------------------------------
    #[error("no compatible build: {0}")]
    Incompatible(Incompat),
    #[error("missing dependency: {0}")]
    DependencyMissing(String),
    #[error("not enough disk space: need {need_mb} MB, {free_mb} MB free")]
    DiskSpace { need_mb: u64, free_mb: u64 },
    #[error("{0} is running; close it and try again")]
    AppRunning(String),
    #[error("another installation is in progress")]
    InstallerBusy,
    #[error("installation cancelled by the user")]
    InstallCancelled,
    #[error("the installer failed with exit code {0}")]
    InstallerFailed(i32),
    #[error("not installed: {0}")]
    NotInstalled(String),
    #[error("operation not supported on this system: {0}")]
    Unsupported(String),
    #[error("a newer version is already installed")]
    NewerInstalled,

    // ---- generic ------------------------------------------------------------------------------------
    #[error("cancelled")]
    Cancelled,
    #[error("unknown app: {0}")]
    UnknownApp(String),
    #[error("i/o error ({context}): {source}")]
    Io {
        context: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("internal error: {0}")]
    Internal(String),
}

/// Why a release has no installable build for this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incompat {
    NoBuildForOs,
    NoBuildForArch,
    OsTooOld { needs: String, have: String },
    HubTooOld { needs: String, have: String },
    NoStableRelease,
}

impl fmt::Display for Incompat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Incompat::NoBuildForOs => write!(f, "no build for this operating system"),
            Incompat::NoBuildForArch => write!(f, "no build for this processor architecture"),
            Incompat::OsTooOld { needs, have } => write!(f, "needs OS {needs} or newer (this system: {have})"),
            Incompat::HubTooOld { needs, have } => write!(f, "needs ASTERION Hub {needs} or newer (this Hub: {have})"),
            Incompat::NoStableRelease => write!(f, "no published release"),
        }
    }
}

impl Incompat {
    pub fn code(&self) -> &'static str {
        match self {
            Incompat::NoBuildForOs => "no_build_for_os",
            Incompat::NoBuildForArch => "no_build_for_arch",
            Incompat::OsTooOld { .. } => "os_too_old",
            Incompat::HubTooOld { .. } => "hub_too_old",
            Incompat::NoStableRelease => "no_release",
        }
    }
}

impl HubError {
    /// Stable identifier for the UI. Never changes once published.
    pub fn code(&self) -> &'static str {
        match self {
            HubError::Offline(_) => "offline",
            HubError::HttpStatus { .. } => "http_status",
            HubError::DownloadInterrupted(_) => "download_interrupted",
            HubError::UrlRefused(_) => "url_refused",
            HubError::BadSignature(_) => "bad_signature",
            HubError::Expired(_) => "metadata_expired",
            HubError::Rollback(_) => "metadata_rollback",
            HubError::InvalidMetadata(_) => "invalid_metadata",
            HubError::UnsupportedSchema { .. } => "unsupported_schema",
            HubError::SizeMismatch { .. } => "size_mismatch",
            HubError::HashMismatch { .. } => "hash_mismatch",
            HubError::UnexpectedPackage(_) => "unexpected_package",
            HubError::Incompatible(i) => i.code(),
            HubError::DependencyMissing(_) => "dependency_missing",
            HubError::DiskSpace { .. } => "disk_space",
            HubError::AppRunning(_) => "app_running",
            HubError::InstallerBusy => "installer_busy",
            HubError::InstallCancelled => "install_cancelled",
            HubError::InstallerFailed(_) => "installer_failed",
            HubError::NotInstalled(_) => "not_installed",
            HubError::Unsupported(_) => "unsupported",
            HubError::NewerInstalled => "newer_installed",
            HubError::Cancelled => "cancelled",
            HubError::UnknownApp(_) => "unknown_app",
            HubError::Io { .. } => "io",
            HubError::Internal(_) => "internal",
        }
    }

    /// Worth offering "Retry" for: the same operation may succeed later without any change.
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            HubError::Offline(_)
                | HubError::DownloadInterrupted(_)
                | HubError::InstallerBusy
                | HubError::AppRunning(_)
                | HubError::HttpStatus { .. }
                | HubError::Cancelled
                | HubError::InstallCancelled
                | HubError::HashMismatch { .. }
                | HubError::SizeMismatch { .. }
        )
    }

    pub fn io(context: &'static str, source: std::io::Error) -> Self {
        HubError::Io { context, source }
    }
}

pub type Result<T> = std::result::Result<T, HubError>;
