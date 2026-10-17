//! The catalog: a signed, versioned, static JSON document that lists every application, its releases
//! and, per release, one artifact per operating system and architecture.
//!
//! Two rules keep the format evolvable without rebuilding the Hub:
//!   * unknown *fields* are ignored (additive changes inside schema 1 are free);
//!   * unknown *enum values* (a new OS, a new package kind) deserialize to `Unknown` and the artifact
//!     that uses them is simply not offered by an older Hub.
//! Structural nonsense, on the other hand, rejects the whole document: a signed catalog that fails
//! validation means the publisher or the Hub is out of step and nothing in it can be trusted to act on.

use crate::error::{HubError, Incompat, Result};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const CATALOG_SCHEMA: u32 = 1;
pub const MAX_ARTIFACT_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Beta,
}

impl Channel {
    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Beta => "beta",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Windows,
    Macos,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Arch {
    #[serde(rename = "x86_64")]
    X86_64,
    #[serde(rename = "aarch64")]
    Aarch64,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageKind {
    /// Windows Installer package.
    Msi,
    /// macOS disk image holding one `.app` bundle.
    Dmg,
    /// macOS zip archive holding one `.app` bundle.
    Zip,
    #[serde(other)]
    Unknown,
}

impl PackageKind {
    pub fn extension(self) -> &'static str {
        match self {
            PackageKind::Msi => "msi",
            PackageKind::Dmg => "dmg",
            PackageKind::Zip => "zip",
            PackageKind::Unknown => "bin",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Catalog {
    pub schema: u32,
    pub channel: Channel,
    /// Strictly increasing per channel. The Hub refuses to go backwards (rollback protection).
    pub sequence: u64,
    pub generated_at: String,
    pub expires_at: String,
    #[serde(default)]
    pub min_hub_version: Option<String>,
    pub apps: Vec<AppEntry>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    pub summary: String,
    #[serde(default)]
    pub description: String,
    pub publisher: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    /// Display name of the group the app belongs to ("BIM", "Utilidades"...): lets the Hub browse a large catalog.
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub detect: Detect,
    pub releases: Vec<Release>,
}

/// How to recognise an installation that this Hub did not make (data, never code).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Detect {
    #[serde(default)]
    pub windows: Option<WindowsDetect>,
    #[serde(default)]
    pub macos: Option<MacosDetect>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowsDetect {
    /// Exact `DisplayName` of the entry under `...\Windows\CurrentVersion\Uninstall`.
    pub display_name: String,
    #[serde(default)]
    pub publisher: Option<String>,
    /// Some installers do not record where they put the files. When the registry has no install
    /// location, the Hub looks for `executable` in these directories (only `%LOCALAPPDATA%\...` and
    /// `%PROGRAMFILES%\...` prefixes are accepted).
    #[serde(default)]
    pub search_dirs: Vec<String>,
    #[serde(default)]
    pub executable: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacosDetect {
    pub bundle_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Release {
    pub version: String,
    pub released_at: String,
    #[serde(default)]
    pub notes: Vec<String>,
    /// Set (to a reason) when a release was pulled. Never offered as an install or update target.
    #[serde(default)]
    pub withdrawn: Option<String>,
    #[serde(default)]
    pub min_hub_version: Option<String>,
    #[serde(default)]
    pub requires_apps: Vec<AppDependency>,
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppDependency {
    pub id: String,
    #[serde(default)]
    pub min_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artifact {
    pub os: Os,
    pub arch: Arch,
    #[serde(default)]
    pub os_min: Option<String>,
    pub kind: PackageKind,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    /// Approximate size once installed, for the free-space check.
    #[serde(default)]
    pub installed_size_mb: Option<u64>,
    /// Relative path of the executable inside the install directory (Windows) or the `.app` name (macOS).
    #[serde(default)]
    pub launch: Option<String>,
    /// Facts about the MSI, extracted by the publisher and re-checked by the Hub against the file.
    #[serde(default)]
    pub msi: Option<MsiFacts>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MsiFacts {
    pub product_name: String,
    pub upgrade_code: String,
    pub product_version: String,
    /// Name of the public MSI property that selects the install directory (e.g. `INSTALLFOLDER`).
    /// Absent when the package has no such property: the Hub then lets the package choose.
    #[serde(default)]
    pub install_dir_property: Option<String>,
}

// ---------------------------------------------------------------------------------------------------------
// validation
// ---------------------------------------------------------------------------------------------------------

fn bad<T>(msg: impl Into<String>) -> Result<T> {
    Err(HubError::InvalidMetadata(msg.into()))
}

pub fn valid_app_id(id: &str) -> bool {
    let b = id.as_bytes();
    !b.is_empty()
        && b.len() <= 40
        && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
        && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

pub fn valid_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn valid_guid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 38
        && b[0] == b'{'
        && b[37] == b'}'
        && s[1..37].bytes().enumerate().all(|(i, c)| match i {
            8 | 13 | 18 | 23 => c == b'-',
            _ => c.is_ascii_hexdigit(),
        })
}

pub fn parse_rfc3339(s: &str) -> Result<time::OffsetDateTime> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339)
        .map_err(|_| HubError::InvalidMetadata(format!("not an RFC 3339 timestamp: {s:?}")))
}

impl Catalog {
    /// Parse and validate. `bytes` must already have been authenticated by [`crate::trust`].
    pub fn parse(bytes: &[u8]) -> Result<Catalog> {
        // The schema number is read first so that a future incompatible schema is reported as such
        // rather than as a confusing missing-field error.
        #[derive(Deserialize)]
        struct Peek {
            schema: u32,
        }
        let peek: Peek = serde_json::from_slice(bytes).map_err(|e| HubError::InvalidMetadata(format!("catalog is not valid JSON: {e}")))?;
        if peek.schema != CATALOG_SCHEMA {
            return Err(HubError::UnsupportedSchema { found: peek.schema, supported: CATALOG_SCHEMA });
        }
        let cat: Catalog = serde_json::from_slice(bytes).map_err(|e| HubError::InvalidMetadata(format!("catalog does not match schema 1: {e}")))?;
        cat.validate()?;
        Ok(cat)
    }

    pub fn validate(&self) -> Result<()> {
        parse_rfc3339(&self.generated_at)?;
        parse_rfc3339(&self.expires_at)?;
        if let Some(v) = &self.min_hub_version {
            Version::parse(v).map_err(|_| HubError::InvalidMetadata(format!("min_hub_version {v:?} is not semver")))?;
        }
        let mut ids = HashSet::new();
        for app in &self.apps {
            if !ids.insert(app.id.as_str()) {
                return bad(format!("duplicate app id {:?}", app.id));
            }
            app.validate()?;
        }
        for app in &self.apps {
            for rel in &app.releases {
                for dep in &rel.requires_apps {
                    if dep.id == app.id {
                        return bad(format!("{} depends on itself", app.id));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn app(&self, id: &str) -> Option<&AppEntry> {
        self.apps.iter().find(|a| a.id == id)
    }
}

impl AppEntry {
    fn validate(&self) -> Result<()> {
        let id = &self.id;
        if !valid_app_id(id) {
            return bad(format!("app id {id:?} must match [a-z0-9][a-z0-9-]{{0,39}}"));
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 80 {
            return bad(format!("{id}: name must be 1-80 characters"));
        }
        if self.summary.chars().count() > 280 {
            return bad(format!("{id}: summary longer than 280 characters"));
        }
        if self.description.chars().count() > 4000 {
            return bad(format!("{id}: description longer than 4000 characters"));
        }
        if self.publisher.trim().is_empty() || self.publisher.chars().count() > 80 {
            return bad(format!("{id}: publisher must be 1-80 characters"));
        }
        if self.category.as_ref().is_some_and(|c| c.trim().is_empty() || c.chars().count() > 40) {
            return bad(format!("{id}: category must be 1-40 characters"));
        }
        if self.tags.len() > 8 || self.tags.iter().any(|t| t.trim().is_empty() || t.chars().count() > 24) {
            return bad(format!("{id}: at most 8 tags of 1-24 characters"));
        }
        if let Some(h) = &self.homepage {
            let u = url::Url::parse(h).map_err(|_| HubError::InvalidMetadata(format!("{id}: homepage is not a URL")))?;
            if u.scheme() != "https" {
                return bad(format!("{id}: homepage must be https"));
            }
        }
        if let Some(w) = &self.detect.windows {
            for d in &w.search_dirs {
                let rest = d.strip_prefix("%LOCALAPPDATA%\\").or_else(|| d.strip_prefix("%PROGRAMFILES%\\"));
                if !rest.is_some_and(is_safe_relative_path) {
                    return bad(format!("{id}: detect.search_dirs entry {d:?} must be %LOCALAPPDATA%\\... or %PROGRAMFILES%\\... without \"..\""));
                }
            }
            if let Some(e) = &w.executable {
                if !is_safe_relative_path(e) {
                    return bad(format!("{id}: detect.executable must be a safe relative path"));
                }
            }
        }
        let mut versions = HashSet::new();
        for rel in &self.releases {
            let v = Version::parse(&rel.version).map_err(|_| HubError::InvalidMetadata(format!("{id}: version {:?} is not semver", rel.version)))?;
            if !versions.insert(v) {
                return bad(format!("{id}: duplicate release {}", rel.version));
            }
            rel.validate(id)?;
        }
        Ok(())
    }
}

impl Release {
    fn validate(&self, app: &str) -> Result<()> {
        let v = &self.version;
        parse_rfc3339(&self.released_at)?;
        if self.notes.len() > 50 || self.notes.iter().any(|n| n.chars().count() > 500) {
            return bad(format!("{app} {v}: notes limited to 50 entries of 500 characters"));
        }
        if let Some(m) = &self.min_hub_version {
            Version::parse(m).map_err(|_| HubError::InvalidMetadata(format!("{app} {v}: min_hub_version is not semver")))?;
        }
        for dep in &self.requires_apps {
            if !valid_app_id(&dep.id) {
                return bad(format!("{app} {v}: dependency id {:?} is invalid", dep.id));
            }
        }
        let mut seen = HashSet::new();
        for a in &self.artifacts {
            a.validate(app, v)?;
            if !seen.insert((a.os, a.arch)) {
                return bad(format!("{app} {v}: two artifacts for the same OS and architecture"));
            }
        }
        Ok(())
    }

    pub fn semver(&self) -> Version {
        Version::parse(&self.version).expect("validated at parse time")
    }
}

impl Artifact {
    fn validate(&self, app: &str, v: &str) -> Result<()> {
        if self.os == Os::Unknown || self.arch == Arch::Unknown || self.kind == PackageKind::Unknown {
            // Not an error: a newer publisher, an older Hub. The artifact is skipped when selecting.
            return Ok(());
        }
        let ctx = format!("{app} {v} ({:?}/{:?})", self.os, self.arch);
        match (self.os, self.kind) {
            (Os::Windows, PackageKind::Msi) | (Os::Macos, PackageKind::Dmg) | (Os::Macos, PackageKind::Zip) => {}
            _ => return bad(format!("{ctx}: package kind {:?} does not belong to this OS", self.kind)),
        }
        let u = url::Url::parse(&self.url).map_err(|_| HubError::InvalidMetadata(format!("{ctx}: url is not a URL")))?;
        if !matches!(u.scheme(), "https" | "http") || u.host_str().is_none() {
            return bad(format!("{ctx}: url must be http(s) with a host"));
        }
        if !u.username().is_empty() || u.password().is_some() {
            return bad(format!("{ctx}: url must not carry credentials"));
        }
        if self.size == 0 || self.size > MAX_ARTIFACT_BYTES {
            return bad(format!("{ctx}: size out of range"));
        }
        if !valid_sha256(&self.sha256) {
            return bad(format!("{ctx}: sha256 must be 64 lowercase hex characters"));
        }
        if let Some(m) = &self.os_min {
            if !is_dotted_numeric(m) {
                return bad(format!("{ctx}: os_min must be dotted numbers"));
            }
        }
        if let Some(l) = &self.launch {
            if !is_safe_relative_path(l) {
                return bad(format!("{ctx}: launch path must be relative and stay inside the install directory"));
            }
        }
        if self.kind == PackageKind::Msi {
            let m = self.msi.as_ref().ok_or_else(|| HubError::InvalidMetadata(format!("{ctx}: an MSI artifact needs its msi facts")))?;
            if !valid_guid(&m.upgrade_code) {
                return bad(format!("{ctx}: msi.upgrade_code is not a braced GUID"));
            }
            if let Some(p) = &m.install_dir_property {
                let b = p.as_bytes();
                if b.is_empty() || b.len() > 64 || !b[0].is_ascii_uppercase() || !b.iter().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == b'_') {
                    return bad(format!("{ctx}: msi.install_dir_property must be an upper-case MSI property name"));
                }
            }
        }
        Ok(())
    }
}

pub fn is_dotted_numeric(s: &str) -> bool {
    !s.is_empty() && s.split('.').all(|p| !p.is_empty() && p.len() <= 9 && p.bytes().all(|c| c.is_ascii_digit()))
}

/// A path that cannot escape the directory it is joined to.
pub fn is_safe_relative_path(p: &str) -> bool {
    if p.is_empty() || p.len() > 260 || p.contains(':') || p.starts_with(['/', '\\']) || p.contains('\0') {
        return false;
    }
    p.split(['/', '\\']).all(|seg| !seg.is_empty() && seg != "." && seg != ".." && !seg.ends_with('.') && !seg.ends_with(' '))
}

// ---------------------------------------------------------------------------------------------------------
// versions
// ---------------------------------------------------------------------------------------------------------

/// Compare dotted numeric versions ("10.0.19045" vs "10.0.17763") part by part.
pub fn cmp_dotted(a: &str, b: &str) -> std::cmp::Ordering {
    let pa: Vec<u64> = a.split('.').map(|p| p.parse().unwrap_or(0)).collect();
    let pb: Vec<u64> = b.split('.').map(|p| p.parse().unwrap_or(0)).collect();
    for i in 0..pa.len().max(pb.len()) {
        let (x, y) = (pa.get(i).copied().unwrap_or(0), pb.get(i).copied().unwrap_or(0));
        if x != y {
            return x.cmp(&y);
        }
    }
    std::cmp::Ordering::Equal
}

/// Versions reported by installers are not always semver ("1.2", "1.2.3.4"): read the first three
/// numeric parts and ignore the rest. `None` when there is no leading number at all.
pub fn parse_installed_version(s: &str) -> Option<Version> {
    let mut nums = s.trim().trim_start_matches(['v', 'V']).split('.').map(|p| {
        let digits: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse::<u64>().ok()
    });
    let major = nums.next()??;
    let minor = nums.next().flatten().unwrap_or(0);
    let patch = nums.next().flatten().unwrap_or(0);
    Some(Version::new(major, minor, patch))
}

// ---------------------------------------------------------------------------------------------------------
// choosing what this machine can install
// ---------------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostInfo {
    pub os: Os,
    pub arch: Arch,
    /// Dotted OS version: Windows "10.0.<build>", macOS "14.5".
    pub os_version: String,
    pub hub_version: Version,
}

#[derive(Debug, Clone, Copy)]
pub struct Selected<'a> {
    pub release: &'a Release,
    pub artifact: &'a Artifact,
}

#[derive(Debug, Clone)]
pub struct Selection<'a> {
    /// The newest release this machine can install.
    pub best: Option<Selected<'a>>,
    /// The newest release overall when it is NOT the one in `best`, with the reason.
    pub newer_blocked: Option<(&'a Release, Incompat)>,
    /// Why nothing at all is installable (set only when `best` is `None`).
    pub why_none: Option<Incompat>,
}

fn check_release<'a>(rel: &'a Release, host: &HostInfo) -> std::result::Result<&'a Artifact, Incompat> {
    if let Some(min) = &rel.min_hub_version {
        let min_v = Version::parse(min).expect("validated");
        if host.hub_version < min_v {
            return Err(Incompat::HubTooOld { needs: min.clone(), have: host.hub_version.to_string() });
        }
    }
    let for_os: Vec<&Artifact> = rel.artifacts.iter().filter(|a| a.os == host.os && a.kind != PackageKind::Unknown).collect();
    if for_os.is_empty() {
        return Err(Incompat::NoBuildForOs);
    }
    let Some(art) = for_os.into_iter().find(|a| a.arch == host.arch) else {
        return Err(Incompat::NoBuildForArch);
    };
    if let Some(min) = &art.os_min {
        if cmp_dotted(&host.os_version, min).is_lt() {
            return Err(Incompat::OsTooOld { needs: min.clone(), have: host.os_version.clone() });
        }
    }
    Ok(art)
}

impl AppEntry {
    pub fn select<'a>(&'a self, host: &HostInfo) -> Selection<'a> {
        let mut rels: Vec<&Release> = self.releases.iter().filter(|r| r.withdrawn.is_none()).collect();
        rels.sort_by_key(|r| std::cmp::Reverse(r.semver()));
        let Some(newest) = rels.first().copied() else {
            return Selection { best: None, newer_blocked: None, why_none: Some(Incompat::NoStableRelease) };
        };
        let mut first_err: Option<Incompat> = None;
        for (i, rel) in rels.iter().enumerate() {
            match check_release(rel, host) {
                Ok(artifact) => {
                    let newer_blocked = if i > 0 { first_err.take().map(|e| (newest, e)) } else { None };
                    return Selection { best: Some(Selected { release: rel, artifact }), newer_blocked, why_none: None };
                }
                Err(e) => {
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
            }
        }
        Selection { best: None, newer_blocked: None, why_none: first_err }
    }

    pub fn release(&self, version: &str) -> Option<&Release> {
        self.releases.iter().find(|r| r.version == version)
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    pub fn artifact(os: Os, arch: Arch, kind: PackageKind) -> Artifact {
        Artifact {
            os,
            arch,
            os_min: None,
            kind,
            url: "https://example.test/f.msi".into(),
            size: 10,
            sha256: "a".repeat(64),
            installed_size_mb: None,
            launch: Some("app.exe".into()),
            msi: (kind == PackageKind::Msi).then(|| MsiFacts {
                product_name: "App".into(),
                upgrade_code: "{6C3E2B6A-4F0D-4B8E-9A61-2D7F5C1E8B43}".into(),
                product_version: "1.0.0".into(),
                install_dir_property: Some("INSTALLFOLDER".into()),
            }),
        }
    }

    pub fn release(version: &str, artifacts: Vec<Artifact>) -> Release {
        Release {
            version: version.into(),
            released_at: "2026-10-01T00:00:00Z".into(),
            notes: vec![],
            withdrawn: None,
            min_hub_version: None,
            requires_apps: vec![],
            artifacts,
        }
    }

    pub fn app(id: &str, releases: Vec<Release>) -> AppEntry {
        AppEntry {
            id: id.into(),
            name: "App".into(),
            summary: "s".into(),
            description: String::new(),
            publisher: "P".into(),
            releases,
            ..Default::default()
        }
    }

    pub fn catalog(apps: Vec<AppEntry>) -> Catalog {
        Catalog {
            schema: 1,
            channel: Channel::Stable,
            sequence: 1,
            generated_at: "2026-10-01T00:00:00Z".into(),
            expires_at: "2026-11-01T00:00:00Z".into(),
            min_hub_version: None,
            apps,
        }
    }

    pub fn host() -> HostInfo {
        HostInfo { os: Os::Windows, arch: Arch::X86_64, os_version: "10.0.19045".into(), hub_version: Version::new(0, 1, 0) }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;

    fn win() -> Artifact {
        artifact(Os::Windows, Arch::X86_64, PackageKind::Msi)
    }

    #[test]
    fn a_well_formed_catalog_roundtrips_and_validates() {
        let c = catalog(vec![app("asterion-bim", vec![release("0.1.0", vec![win()])])]);
        let bytes = serde_json::to_vec(&c).unwrap();
        let back = Catalog::parse(&bytes).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn unknown_fields_are_ignored_but_a_wrong_schema_is_refused() {
        let c = catalog(vec![]);
        let mut v = serde_json::to_value(&c).unwrap();
        v["something_new"] = serde_json::json!({"a": 1});
        assert!(Catalog::parse(&serde_json::to_vec(&v).unwrap()).is_ok());
        v["schema"] = serde_json::json!(2);
        match Catalog::parse(&serde_json::to_vec(&v).unwrap()) {
            Err(HubError::UnsupportedSchema { found: 2, supported: 1 }) => {}
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unknown_enum_values_skip_the_artifact_instead_of_failing() {
        let c = catalog(vec![app("a", vec![release("1.0.0", vec![win()])])]);
        let mut v = serde_json::to_value(&c).unwrap();
        let arts = &mut v["apps"][0]["releases"][0]["artifacts"];
        let mut extra = arts[0].clone();
        extra["os"] = "linux".into();
        extra["kind"] = "appimage".into();
        arts.as_array_mut().unwrap().push(extra);
        let parsed = Catalog::parse(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert_eq!(parsed.apps[0].releases[0].artifacts[1].os, Os::Unknown);
        let sel = parsed.apps[0].select(&host());
        assert_eq!(sel.best.unwrap().artifact.os, Os::Windows);
    }

    fn invalid(mutate: impl FnOnce(&mut serde_json::Value)) -> HubError {
        let c = catalog(vec![app("asterion-bim", vec![release("0.1.0", vec![win()])])]);
        let mut v = serde_json::to_value(&c).unwrap();
        mutate(&mut v);
        Catalog::parse(&serde_json::to_vec(&v).unwrap()).unwrap_err()
    }

    #[test]
    fn invalid_metadata_is_rejected() {
        macro_rules! art {
            ($v:expr) => {
                $v["apps"][0]["releases"][0]["artifacts"][0]
            };
        }
        let cases: Vec<(&str, Box<dyn FnOnce(&mut serde_json::Value)>)> = vec![
            ("bad app id", Box::new(|v| v["apps"][0]["id"] = "../evil".into())),
            ("uppercase app id", Box::new(|v| v["apps"][0]["id"] = "Asterion".into())),
            ("version not semver", Box::new(|v| v["apps"][0]["releases"][0]["version"] = "latest".into())),
            ("short hash", Box::new(|v| art!(v)["sha256"] = "abc".into())),
            ("uppercase hash", Box::new(|v| art!(v)["sha256"] = "A".repeat(64).into())),
            ("zero size", Box::new(|v| art!(v)["size"] = 0.into())),
            ("huge size", Box::new(|v| art!(v)["size"] = (MAX_ARTIFACT_BYTES + 1).into())),
            ("url with credentials", Box::new(|v| art!(v)["url"] = "https://u:p@example.test/a.msi".into())),
            ("ftp url", Box::new(|v| art!(v)["url"] = "ftp://example.test/a.msi".into())),
            ("launch escapes the install dir", Box::new(|v| art!(v)["launch"] = "..\\..\\evil.exe".into())),
            ("launch absolute", Box::new(|v| art!(v)["launch"] = "C:\\evil.exe".into())),
            ("dmg on windows", Box::new(|v| art!(v)["kind"] = "dmg".into())),
            ("msi without facts", Box::new(|v| art!(v)["msi"] = serde_json::Value::Null)),
            ("msi dir property injection", Box::new(|v| art!(v)["msi"]["install_dir_property"] = "X /qn REMOVE=ALL".into())),
            ("search dir escapes", Box::new(|v| v["apps"][0]["detect"]["windows"] = serde_json::json!({"display_name":"X","search_dirs":["%LOCALAPPDATA%\\..\\x"]}))),
            ("search dir absolute", Box::new(|v| v["apps"][0]["detect"]["windows"] = serde_json::json!({"display_name":"X","search_dirs":["C:\\Windows"]}))),
            ("category too long", Box::new(|v| v["apps"][0]["category"] = "x".repeat(41).into())),
            ("too many tags", Box::new(|v| v["apps"][0]["tags"] = serde_json::json!(["a", "b", "c", "d", "e", "f", "g", "h", "i"]))),
            ("homepage not https", Box::new(|v| v["apps"][0]["homepage"] = "http://example.test".into())),
            ("bad timestamp", Box::new(|v| v["expires_at"] = "tomorrow".into())),
        ];
        for (name, f) in cases {
            match invalid(f) {
                HubError::InvalidMetadata(_) => {}
                other => panic!("{name}: expected InvalidMetadata, got {other:?}"),
            }
        }
    }

    #[test]
    fn duplicate_apps_releases_and_platforms_are_rejected() {
        let a = app("a", vec![release("1.0.0", vec![win()])]);
        assert!(catalog(vec![a.clone(), a.clone()]).validate().is_err());
        let dup_rel = app("a", vec![release("1.0.0", vec![win()]), release("1.0.0", vec![win()])]);
        assert!(catalog(vec![dup_rel]).validate().is_err());
        let dup_art = app("a", vec![release("1.0.0", vec![win(), win()])]);
        assert!(catalog(vec![dup_art]).validate().is_err());
    }

    #[test]
    fn selection_picks_the_newest_compatible_release() {
        let a = app("a", vec![release("1.0.0", vec![win()]), release("1.2.0", vec![win()]), release("1.1.0", vec![win()])]);
        assert_eq!(a.select(&host()).best.unwrap().release.version, "1.2.0");
    }

    #[test]
    fn withdrawn_releases_are_never_selected() {
        let mut r = release("2.0.0", vec![win()]);
        r.withdrawn = Some("broken".into());
        let a = app("a", vec![release("1.0.0", vec![win()]), r]);
        let sel = a.select(&host());
        assert_eq!(sel.best.unwrap().release.version, "1.0.0");
        assert!(sel.newer_blocked.is_none());
    }

    #[test]
    fn incompatible_os_arch_and_version_are_reported_with_a_reason() {
        let mac_only = app("a", vec![release("1.0.0", vec![artifact(Os::Macos, Arch::Aarch64, PackageKind::Dmg)])]);
        assert_eq!(mac_only.select(&host()).why_none, Some(Incompat::NoBuildForOs));

        let arm_only = app("a", vec![release("1.0.0", vec![artifact(Os::Windows, Arch::Aarch64, PackageKind::Msi)])]);
        assert_eq!(arm_only.select(&host()).why_none, Some(Incompat::NoBuildForArch));

        let mut old_os = win();
        old_os.os_min = Some("10.0.22000".into());
        let a = app("a", vec![release("1.0.0", vec![old_os])]);
        assert!(matches!(a.select(&host()).why_none, Some(Incompat::OsTooOld { .. })));

        let mut needs_hub = release("1.0.0", vec![win()]);
        needs_hub.min_hub_version = Some("9.0.0".into());
        let a = app("a", vec![needs_hub]);
        assert!(matches!(a.select(&host()).why_none, Some(Incompat::HubTooOld { .. })));

        let none = app("a", vec![]);
        assert_eq!(none.select(&host()).why_none, Some(Incompat::NoStableRelease));
    }

    #[test]
    fn a_newer_release_needing_a_newer_hub_falls_back_and_says_so() {
        let mut newest = release("2.0.0", vec![win()]);
        newest.min_hub_version = Some("9.0.0".into());
        let a = app("a", vec![release("1.0.0", vec![win()]), newest]);
        let sel = a.select(&host());
        assert_eq!(sel.best.unwrap().release.version, "1.0.0");
        let (rel, why) = sel.newer_blocked.unwrap();
        assert_eq!(rel.version, "2.0.0");
        assert!(matches!(why, Incompat::HubTooOld { .. }));
    }

    #[test]
    fn dotted_and_installed_versions_compare_sensibly() {
        use std::cmp::Ordering::*;
        assert_eq!(cmp_dotted("10.0.19045", "10.0.17763"), Greater);
        assert_eq!(cmp_dotted("10.0", "10.0.0"), Equal);
        assert_eq!(cmp_dotted("14.5", "15"), Less);
        assert_eq!(parse_installed_version("1.2"), Some(Version::new(1, 2, 0)));
        assert_eq!(parse_installed_version("1.2.3.4"), Some(Version::new(1, 2, 3)));
        assert_eq!(parse_installed_version("v0.1.0"), Some(Version::new(0, 1, 0)));
        assert_eq!(parse_installed_version("abc"), None);
        assert_eq!(parse_installed_version(""), None);
    }

    #[test]
    fn relative_paths_cannot_escape() {
        for ok in ["app.exe", "bin/app.exe", "bin\\app.exe", "My App.app"] {
            assert!(is_safe_relative_path(ok), "{ok}");
        }
        for bad in ["", "/etc/passwd", "\\\\host\\share", "..", "a/../b", "C:\\x", "a//b", "a/./b", "x.", "a\0b"] {
            assert!(!is_safe_relative_path(bad), "{bad:?}");
        }
    }

    #[test]
    fn app_ids() {
        assert!(valid_app_id("asterion-bim"));
        assert!(valid_app_id("a1"));
        assert!(!valid_app_id(""));
        assert!(!valid_app_id("-a"));
        assert!(!valid_app_id("A"));
        assert!(!valid_app_id("a_b"));
        assert!(!valid_app_id(&"a".repeat(41)));
    }
}


