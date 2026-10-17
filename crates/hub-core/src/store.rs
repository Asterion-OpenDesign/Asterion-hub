//! Everything the Hub remembers, in one directory: settings, install records, update-check bookkeeping,
//! history, the last verified catalog (so the Hub still works offline) and the downloaded installers.
//!
//! Writes are atomic (temp file, then rename) so a crash or power loss never leaves half a JSON file.
//! A file that cannot be read is moved aside as `*.corrupt` and replaced by defaults: the Hub must
//! never refuse to start because of its own bookkeeping, and it must never touch the applications.

use crate::error::{HubError, Result};
use crate::manifest::{Channel, Detect};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoCheck {
    Never,
    OnStart,
    EverySixHours,
    Daily,
}

impl AutoCheck {
    pub fn interval_hours(self) -> Option<u64> {
        match self {
            AutoCheck::EverySixHours => Some(6),
            AutoCheck::Daily => Some(24),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallScope {
    /// Current user only: no administrator rights, no UAC prompt. The default.
    User,
    /// All users: needs elevation.
    Machine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub channel: Channel,
    pub auto_check: AutoCheck,
    pub install_scope: InstallScope,
    pub theme: Theme,
    /// Keep verified installers after installing (useful for repair without downloading again).
    pub keep_installers: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { channel: Channel::Stable, auto_check: AutoCheck::OnStart, install_scope: InstallScope::User, theme: Theme::Dark, keep_installers: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstallRecord {
    pub app_id: String,
    pub version: String,
    pub scope: InstallScope,
    /// Directory the Hub installed into (absent for installs that were only detected).
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub product_code: Option<String>,
    pub installed_at: String,
    /// Executable (Windows) or `.app` (macOS) to open, relative to `location`.
    #[serde(default)]
    pub launch: Option<String>,
    /// Copied from the catalog so the app is still recognised if it later leaves the catalog.
    #[serde(default)]
    pub detect: Detect,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// Highest catalog sequence ever accepted, per channel. Older catalogs are refused.
    pub catalog_sequence: BTreeMap<String, u64>,
    pub keyset_sequence: u64,
    pub installs: BTreeMap<String, InstallRecord>,
    pub last_check: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    Failed,
    Cancelled,
    /// The Hub was closed (or crashed) while the operation was running.
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub id: String,
    pub at: String,
    pub app_id: String,
    pub app_name: String,
    pub action: String,
    pub from_version: Option<String>,
    pub to_version: Option<String>,
    pub outcome: Outcome,
    pub error_code: Option<String>,
    pub message: Option<String>,
    pub duration_ms: u64,
}

/// An operation that has started changing the system. Written before the installer runs and removed
/// after, so that a crash in between is visible on the next start.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Journal {
    pub task_id: String,
    pub app_id: String,
    pub app_name: String,
    pub action: String,
    pub from_version: Option<String>,
    pub to_version: Option<String>,
    pub started_at: String,
}

#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

const HISTORY_MAX: usize = 1000;

impl Store {
    pub fn open(dir: impl Into<PathBuf>) -> Result<Store> {
        let s = Store { dir: dir.into() };
        for d in [s.dir.clone(), s.downloads_dir(), s.cache_dir(), s.logs_dir()] {
            std::fs::create_dir_all(&d).map_err(|e| HubError::io("create data directory", e))?;
        }
        Ok(s)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
    pub fn downloads_dir(&self) -> PathBuf {
        self.dir.join("downloads")
    }
    pub fn cache_dir(&self) -> PathBuf {
        self.dir.join("cache")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.dir.join("logs")
    }

    fn read_json<T: DeserializeOwned + Default>(&self, name: &str) -> T {
        let path = self.dir.join(name);
        match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("{name} unreadable ({e}); moving it aside and starting from defaults");
                    let _ = std::fs::rename(&path, self.dir.join(format!("{name}.corrupt")));
                    T::default()
                }
            },
            Err(_) => T::default(),
        }
    }

    pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
        let tmp = path.with_extension(format!("{}.tmp", path.extension().and_then(|e| e.to_str()).unwrap_or("")));
        let mut f = std::fs::File::create(&tmp).map_err(|e| HubError::io("write temp file", e))?;
        f.write_all(bytes).map_err(|e| HubError::io("write temp file", e))?;
        f.sync_all().map_err(|e| HubError::io("flush temp file", e))?;
        drop(f);
        std::fs::rename(&tmp, path).map_err(|e| HubError::io("replace file", e))
    }

    fn write_json<T: Serialize>(&self, name: &str, v: &T) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(v).map_err(|e| HubError::Internal(e.to_string()))?;
        Self::write_atomic(&self.dir.join(name), &bytes)
    }

    // ---- settings / state ---------------------------------------------------------------------------
    pub fn settings(&self) -> Settings {
        self.read_json("settings.json")
    }
    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        self.write_json("settings.json", s)
    }
    pub fn state(&self) -> State {
        self.read_json("state.json")
    }
    pub fn save_state(&self, s: &State) -> Result<()> {
        self.write_json("state.json", s)
    }

    // ---- journal ------------------------------------------------------------------------------------
    pub fn begin_journal(&self, j: &Journal) -> Result<()> {
        self.write_json("journal.json", j)
    }
    pub fn end_journal(&self) {
        let _ = std::fs::remove_file(self.dir.join("journal.json"));
    }
    pub fn take_orphan_journal(&self) -> Option<Journal> {
        let path = self.dir.join("journal.json");
        let j = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok());
        let _ = std::fs::remove_file(path);
        j
    }

    // ---- history ------------------------------------------------------------------------------------
    pub fn append_history(&self, e: &HistoryEntry) -> Result<()> {
        let path = self.dir.join("history.jsonl");
        let mut line = serde_json::to_vec(e).map_err(|x| HubError::Internal(x.to_string()))?;
        line.push(b'\n');
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path).map_err(|x| HubError::io("open history", x))?;
        f.write_all(&line).map_err(|x| HubError::io("append history", x))?;
        drop(f);
        let all = self.history(usize::MAX);
        if all.len() > HISTORY_MAX + HISTORY_MAX / 2 {
            let keep = &all[all.len() - HISTORY_MAX..];
            let mut out = Vec::new();
            for e in keep {
                out.extend(serde_json::to_vec(e).unwrap_or_default());
                out.push(b'\n');
            }
            Self::write_atomic(&path, &out)?;
        }
        Ok(())
    }

    /// Oldest first; `limit` keeps the most recent entries.
    pub fn history(&self, limit: usize) -> Vec<HistoryEntry> {
        let text = std::fs::read_to_string(self.dir.join("history.jsonl")).unwrap_or_default();
        let all: Vec<HistoryEntry> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
        let skip = all.len().saturating_sub(limit);
        all.into_iter().skip(skip).collect()
    }

    pub fn clear_history(&self) -> Result<()> {
        match std::fs::remove_file(self.dir.join("history.jsonl")) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(HubError::io("clear history", e)),
        }
    }

    // ---- verified metadata cache (raw bytes, re-verified on every load) -----------------------------
    pub fn cache_path(&self, name: &str) -> PathBuf {
        self.cache_dir().join(name)
    }
    pub fn read_cache(&self, name: &str) -> Option<Vec<u8>> {
        std::fs::read(self.cache_path(name)).ok()
    }
    pub fn write_cache(&self, name: &str, bytes: &[u8]) -> Result<()> {
        Self::write_atomic(&self.cache_path(name), bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(n: u32) -> HistoryEntry {
        HistoryEntry {
            id: format!("h{n}"),
            at: "2026-10-17T00:00:00Z".into(),
            app_id: "a".into(),
            app_name: "A".into(),
            action: "install".into(),
            from_version: None,
            to_version: Some("1.0.0".into()),
            outcome: Outcome::Success,
            error_code: None,
            message: None,
            duration_ms: 1,
        }
    }

    #[test]
    fn defaults_when_nothing_is_stored() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(d.path()).unwrap();
        assert_eq!(s.settings(), Settings::default());
        assert!(s.state().installs.is_empty());
        assert!(s.history(10).is_empty());
    }

    #[test]
    fn settings_and_state_roundtrip() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(d.path()).unwrap();
        let mut st = Settings::default();
        st.channel = Channel::Beta;
        st.auto_check = AutoCheck::Daily;
        s.save_settings(&st).unwrap();
        assert_eq!(s.settings(), st);
        let mut state = State::default();
        state.keyset_sequence = 7;
        state.catalog_sequence.insert("stable".into(), 42);
        s.save_state(&state).unwrap();
        let back = s.state();
        assert_eq!(back.keyset_sequence, 7);
        assert_eq!(back.catalog_sequence["stable"], 42);
    }

    #[test]
    fn a_corrupt_file_is_moved_aside_and_defaults_are_used() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(d.path()).unwrap();
        std::fs::write(d.path().join("state.json"), b"{ truncated").unwrap();
        assert!(s.state().installs.is_empty());
        assert!(d.path().join("state.json.corrupt").exists());
        assert!(!d.path().join("state.json").exists());
    }

    #[test]
    fn unknown_settings_fields_and_missing_fields_are_tolerated() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(d.path()).unwrap();
        std::fs::write(d.path().join("settings.json"), br#"{"channel":"beta","from_the_future":true}"#).unwrap();
        let st = s.settings();
        assert_eq!(st.channel, Channel::Beta);
        assert_eq!(st.auto_check, AutoCheck::OnStart);
    }

    #[test]
    fn history_appends_reads_back_and_is_bounded() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(d.path()).unwrap();
        for n in 0..(HISTORY_MAX + HISTORY_MAX / 2 + 5) as u32 {
            s.append_history(&entry(n)).unwrap();
        }
        let all = s.history(usize::MAX);
        assert!(all.len() <= HISTORY_MAX + HISTORY_MAX / 2);
        assert_eq!(all.last().unwrap().id, format!("h{}", HISTORY_MAX + HISTORY_MAX / 2 + 4));
        assert_eq!(s.history(3).len(), 3);
        s.clear_history().unwrap();
        assert!(s.history(10).is_empty());
        s.clear_history().unwrap();
    }

    #[test]
    fn a_torn_history_line_is_skipped() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(d.path()).unwrap();
        s.append_history(&entry(1)).unwrap();
        let mut f = std::fs::OpenOptions::new().append(true).open(d.path().join("history.jsonl")).unwrap();
        f.write_all(b"{\"id\":\"h2\",\"at\"").unwrap();
        assert_eq!(s.history(10).len(), 1);
    }

    #[test]
    fn an_orphan_journal_is_reported_once() {
        let d = tempfile::tempdir().unwrap();
        let s = Store::open(d.path()).unwrap();
        assert!(s.take_orphan_journal().is_none());
        let j = Journal { task_id: "t".into(), app_id: "a".into(), app_name: "A".into(), action: "update".into(), from_version: Some("1.0.0".into()), to_version: Some("1.1.0".into()), started_at: "2026-10-17T00:00:00Z".into() };
        s.begin_journal(&j).unwrap();
        assert_eq!(s.take_orphan_journal().unwrap().task_id, "t");
        assert!(s.take_orphan_journal().is_none());
        s.begin_journal(&j).unwrap();
        s.end_journal();
        assert!(s.take_orphan_journal().is_none());
    }
}
