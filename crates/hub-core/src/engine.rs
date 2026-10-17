//! The Hub's brain. It owns the catalog, the installed-state view and every operation (install, update,
//! repair, uninstall), and reports everything to the UI through plain serialisable views and events.
//! The UI paints; it never decides. That is also why every number it shows is read from here.

use crate::catalog::{system_clock, CatalogManager, CatalogSnapshot, Clock, Origin};
use crate::download::{clean_downloads, download_verified, verify_file, DownloadSpec};
use crate::error::{HubError, Incompat, Result};
use crate::manifest::{parse_installed_version, AppEntry, Catalog, Channel, HostInfo, PackageKind, Release};
use crate::msi_info;
use crate::net::{build_client, HttpFetcher, UrlPolicy};
use crate::platform::{self, InstallPlan, InstalledInfo};
use crate::store::{HistoryEntry, InstallRecord, Journal, Outcome, Settings, Store};
use crate::trust::RootSet;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use time::OffsetDateTime;
use tokio_util::sync::CancellationToken;
use url::Url;

// ---------------------------------------------------------------------------------------------------------
// configuration, events, views
// ---------------------------------------------------------------------------------------------------------

pub struct HubConfig {
    pub hub_version: semver::Version,
    pub catalog_base: Url,
    pub roots: RootSet,
    pub allowed_hosts: Vec<String>,
    /// Debug builds only.
    pub allow_loopback_http: bool,
    pub data_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Task { task: TaskView },
    Catalog,
    Installed,
}

pub type Emitter = Arc<dyn Fn(Event) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    Install,
    Update,
    Repair,
    Uninstall,
}

impl Action {
    fn as_str(self) -> &'static str {
        match self {
            Action::Install => "install",
            Action::Update => "update",
            Action::Repair => "repair",
            Action::Uninstall => "uninstall",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Queued,
    Preparing,
    Downloading,
    Verifying,
    WaitingForInstaller,
    Installing,
    Finishing,
    Done,
    Failed,
    Cancelled,
}

impl Phase {
    pub fn is_final(self) -> bool {
        matches!(self, Phase::Done | Phase::Failed | Phase::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorView {
    pub code: String,
    /// English, for the "details" expander and the logs. The UI shows its own localised copy for `code`.
    pub detail: String,
    pub retryable: bool,
}

impl From<&HubError> for ErrorView {
    fn from(e: &HubError) -> Self {
        ErrorView { code: e.code().into(), detail: e.to_string(), retryable: e.retryable() }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskView {
    pub id: String,
    pub app_id: String,
    pub app_name: String,
    pub action: Action,
    pub version: Option<String>,
    pub phase: Phase,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub bytes_per_sec: u64,
    pub error: Option<ErrorView>,
    pub reboot_required: bool,
    pub started_at: String,
    pub finished_at: Option<String>,
    /// Cancelling is refused while the installer itself is running: stopping it half-way is what corrupts installs.
    pub cancellable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogStatus {
    pub loaded: bool,
    pub origin: Option<Origin>,
    pub fetched_at: Option<String>,
    pub expired: bool,
    pub channel: Channel,
    pub sequence: Option<u64>,
    pub keyset_days_left: Option<i64>,
    /// Why the Hub is showing saved data (or nothing): the failure of the last network attempt.
    pub refresh_error: Option<ErrorView>,
    pub last_check: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InstalledView {
    pub version: String,
    pub scope: crate::store::InstallScope,
    pub location: Option<String>,
    pub installed_by_hub: bool,
    pub running: bool,
    pub can_open: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseView {
    pub version: String,
    pub released_at: String,
    pub notes: Vec<String>,
    pub installable: bool,
    pub blocked: Option<ErrorView>,
    pub size: Option<u64>,
    pub kind: Option<PackageKind>,
    pub os_min: Option<String>,
    pub installed_size_mb: Option<u64>,
    pub requires: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppState {
    NotInstalled,
    Installed,
    UpdateAvailable,
    /// Not installed and nothing installable on this machine (wrong OS, wrong architecture, too old...).
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppView {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub description: String,
    pub publisher: String,
    pub homepage: Option<String>,
    pub license: Option<String>,
    pub state: AppState,
    pub installed: Option<InstalledView>,
    /// The newest release this machine can install.
    pub latest: Option<ReleaseView>,
    /// Newest release overall when this machine cannot take it, with the reason.
    pub newer_blocked: Option<ReleaseView>,
    pub unavailable: Option<ErrorView>,
    pub releases: Vec<ReleaseView>,
    /// What can be done right now (empty while a task for this app is active).
    pub actions: Vec<Action>,
    pub active_task: Option<String>,
    /// Present only for an app that is installed but no longer in the catalog.
    pub orphaned: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct HostView {
    pub os: crate::manifest::Os,
    pub arch: crate::manifest::Arch,
    pub os_version: String,
    pub hub_version: String,
    pub data_dir: String,
}

// ---------------------------------------------------------------------------------------------------------
// engine
// ---------------------------------------------------------------------------------------------------------

struct TaskEntry {
    view: TaskView,
    cancel: CancellationToken,
}

struct Inner {
    store: Store,
    catalog: CatalogManager<HttpFetcher>,
    client: reqwest::Client,
    policy: UrlPolicy,
    host: HostInfo,
    data_dir: PathBuf,
    emit: Emitter,
    clock: Clock,
    snapshot: RwLock<Option<CatalogSnapshot>>,
    refresh_error: RwLock<Option<ErrorView>>,
    tasks: Mutex<Vec<TaskEntry>>,
    counter: AtomicU64,
    /// Windows Installer runs one installation at a time; so does the Hub.
    install_lock: tokio::sync::Mutex<()>,
}

#[derive(Clone)]
pub struct Engine {
    inner: Arc<Inner>,
}

fn rfc3339(t: OffsetDateTime) -> String {
    t.format(&time::format_description::well_known::Rfc3339).expect("rfc3339")
}

impl Engine {
    pub fn new(cfg: HubConfig, emit: Emitter) -> Result<Engine> {
        Self::with_clock(cfg, emit, system_clock())
    }

    pub fn with_clock(cfg: HubConfig, emit: Emitter, clock: Clock) -> Result<Engine> {
        let store = Store::open(&cfg.data_dir)?;
        let policy = UrlPolicy::new(cfg.allowed_hosts.clone(), cfg.allow_loopback_http);
        policy.check(&cfg.catalog_base)?;
        let client = build_client(policy.clone())?;
        let fetcher = HttpFetcher::new(client.clone(), policy.clone());
        let catalog = CatalogManager::new(fetcher, cfg.roots, cfg.catalog_base, store.clone(), clock.clone());
        let host = platform::host_info(cfg.hub_version);

        // A journal that is still there means the Hub (or the machine) stopped in the middle of an
        // installer run. Say so in the history; the next detection shows what state the app really is in.
        if let Some(j) = store.take_orphan_journal() {
            let _ = store.append_history(&HistoryEntry {
                id: j.task_id,
                at: rfc3339(clock()),
                app_id: j.app_id,
                app_name: j.app_name,
                action: j.action,
                from_version: j.from_version,
                to_version: j.to_version,
                outcome: Outcome::Interrupted,
                error_code: Some("interrupted".into()),
                message: Some("ASTERION Hub was closed while this operation was running.".into()),
                duration_ms: 0,
            });
        }
        // Partial downloads are kept for resuming; anything else in the folder is leftovers.
        let engine = Engine {
            inner: Arc::new(Inner {
                store,
                catalog,
                client,
                policy,
                host,
                data_dir: cfg.data_dir,
                emit,
                clock,
                snapshot: RwLock::new(None),
                refresh_error: RwLock::new(None),
                tasks: Mutex::new(Vec::new()),
                counter: AtomicU64::new(0),
                install_lock: tokio::sync::Mutex::new(()),
            }),
        };
        // Whatever was verified last time is available immediately, before any network call.
        let channel = engine.inner.store.settings().channel;
        if let Ok(s) = engine.inner.catalog.load_cached(channel) {
            *engine.inner.snapshot.write().unwrap() = Some(s);
        }
        Ok(engine)
    }

    fn emit(&self, e: Event) {
        (self.inner.emit)(e);
    }

    fn now(&self) -> OffsetDateTime {
        (self.inner.clock)()
    }

    pub fn host(&self) -> HostView {
        let h = &self.inner.host;
        HostView { os: h.os, arch: h.arch, os_version: h.os_version.clone(), hub_version: h.hub_version.to_string(), data_dir: self.inner.data_dir.display().to_string() }
    }

    pub fn settings(&self) -> Settings {
        self.inner.store.settings()
    }

    pub fn save_settings(&self, s: Settings) -> Result<()> {
        let old = self.inner.store.settings();
        self.inner.store.save_settings(&s)?;
        if old.channel != s.channel {
            *self.inner.snapshot.write().unwrap() = self.inner.catalog.load_cached(s.channel).ok();
            self.emit(Event::Catalog);
        }
        Ok(())
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.inner.store.logs_dir()
    }

    // ---- catalog ------------------------------------------------------------------------------------

    pub async fn refresh(&self) -> CatalogStatus {
        let channel = self.settings().channel;
        let (res, net_err) = self.inner.catalog.refresh_or_cached(channel).await;
        match res {
            Ok(snap) => *self.inner.snapshot.write().unwrap() = Some(snap),
            Err(e) => {
                // Nothing usable at all (first run offline, or the saved copy no longer verifies).
                *self.inner.snapshot.write().unwrap() = None;
                *self.inner.refresh_error.write().unwrap() = Some(ErrorView::from(&net_err.unwrap_or(e)));
                self.emit(Event::Catalog);
                return self.status();
            }
        }
        *self.inner.refresh_error.write().unwrap() = net_err.as_ref().map(ErrorView::from);
        self.emit(Event::Catalog);
        self.emit(Event::Installed);
        self.status()
    }

    pub fn status(&self) -> CatalogStatus {
        let channel = self.settings().channel;
        let snap = self.inner.snapshot.read().unwrap();
        CatalogStatus {
            loaded: snap.is_some(),
            origin: snap.as_ref().map(|s| s.origin),
            fetched_at: snap.as_ref().map(|s| rfc3339(s.fetched_at)),
            expired: snap.as_ref().is_some_and(|s| s.expired),
            channel,
            sequence: snap.as_ref().map(|s| s.catalog.sequence),
            keyset_days_left: snap.as_ref().map(|s| s.keyset_days_left),
            refresh_error: self.inner.refresh_error.read().unwrap().clone(),
            last_check: self.inner.store.state().last_check,
        }
    }

    /// Hub-level compatibility note from the catalog (it may require a newer Hub as a whole).
    pub fn catalog_requires_newer_hub(&self) -> Option<String> {
        let snap = self.inner.snapshot.read().unwrap();
        let min = snap.as_ref()?.catalog.min_hub_version.clone()?;
        let min_v = semver::Version::parse(&min).ok()?;
        (self.inner.host.hub_version < min_v).then_some(min)
    }

    fn catalog(&self) -> Option<Catalog> {
        self.inner.snapshot.read().unwrap().as_ref().map(|s| s.catalog.clone())
    }

    // ---- views --------------------------------------------------------------------------------------

    pub fn apps(&self) -> Vec<AppView> {
        let catalog = self.catalog();
        let state = self.inner.store.state();
        let running = platform::RunningSet::capture();
        let tasks = self.inner.tasks.lock().unwrap();
        let active = |id: &str| tasks.iter().find(|t| t.view.app_id == id && !t.view.phase.is_final()).map(|t| t.view.id.clone());
        let mut out = Vec::new();
        let mut seen = Vec::new();
        if let Some(cat) = &catalog {
            for app in &cat.apps {
                seen.push(app.id.clone());
                out.push(self.app_view(app, state.installs.get(&app.id), &running, active(&app.id), false));
            }
        }
        // Installed through the Hub, then dropped from the catalog: still manageable (open, uninstall).
        for (id, rec) in &state.installs {
            if seen.contains(id) {
                continue;
            }
            let stub = AppEntry {
                id: id.clone(),
                name: if rec.name.is_empty() { id.clone() } else { rec.name.clone() },
                summary: String::new(),
                description: String::new(),
                publisher: String::new(),
                homepage: None,
                license: None,
                detect: rec.detect.clone(),
                releases: vec![],
            };
            let v = self.app_view(&stub, Some(rec), &running, active(id), true);
            if v.installed.is_some() {
                out.push(v);
            }
        }
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        out
    }

    fn release_view(&self, rel: &Release, host: &HostInfo, app: &AppEntry) -> ReleaseView {
        let sel = app.select(host);
        let _ = sel;
        let art = rel.artifacts.iter().find(|a| a.os == host.os && a.arch == host.arch && a.kind != PackageKind::Unknown);
        let blocked = self.release_block(rel, host).map(|i| ErrorView::from(&HubError::Incompatible(i)));
        ReleaseView {
            version: rel.version.clone(),
            released_at: rel.released_at.clone(),
            notes: rel.notes.clone(),
            installable: blocked.is_none(),
            blocked,
            size: art.map(|a| a.size),
            kind: art.map(|a| a.kind),
            os_min: art.and_then(|a| a.os_min.clone()),
            installed_size_mb: art.and_then(|a| a.installed_size_mb),
            requires: rel.requires_apps.iter().map(|d| match &d.min_version {
                Some(v) => format!("{} {v}", d.id),
                None => d.id.clone(),
            }).collect(),
        }
    }

    fn release_block(&self, rel: &Release, host: &HostInfo) -> Option<Incompat> {
        let probe = AppEntry { id: "probe".into(), name: String::new(), summary: String::new(), description: String::new(), publisher: String::new(), homepage: None, license: None, detect: Default::default(), releases: vec![rel.clone()] };
        let sel = probe.select(host);
        if sel.best.is_some() {
            None
        } else {
            sel.why_none
        }
    }

    fn app_view(&self, app: &AppEntry, record: Option<&InstallRecord>, running: &platform::RunningSet, active_task: Option<String>, orphaned: bool) -> AppView {
        let host = &self.inner.host;
        let info = platform::detect(app, record);
        let selection = app.select(host);
        let mut releases: Vec<&Release> = app.releases.iter().filter(|r| r.withdrawn.is_none()).collect();
        releases.sort_by_key(|r| std::cmp::Reverse(r.semver()));
        let release_views: Vec<ReleaseView> = releases.iter().map(|r| self.release_view(r, host, app)).collect();

        let latest = selection.best.map(|s| self.release_view(s.release, host, app));
        let newer_blocked = selection.newer_blocked.as_ref().map(|(r, why)| {
            let mut v = self.release_view(r, host, app);
            v.blocked = Some(ErrorView::from(&HubError::Incompatible(why.clone())));
            v.installable = false;
            v
        });
        let unavailable = selection.why_none.as_ref().map(|w| ErrorView::from(&HubError::Incompatible(w.clone())));

        let installed = info.as_ref().map(|i| {
            let target = platform::launch_target(i, selection.best.and_then(|s| s.artifact.launch.as_deref()).or_else(|| record.and_then(|r| r.launch.as_deref())));
            InstalledView {
                version: i.version.clone(),
                scope: i.scope,
                location: i.location.as_ref().map(|p| p.display().to_string()),
                installed_by_hub: record.is_some(),
                running: target.as_ref().is_some_and(|t| running.contains(t)),
                can_open: target.is_some(),
            }
        });

        let update_available = match (&info, &selection.best) {
            (Some(i), Some(best)) => match parse_installed_version(&i.version) {
                Some(have) => best.release.semver() > have,
                None => true,
            },
            _ => false,
        };
        let state = match (&info, update_available, &selection.best) {
            (Some(_), true, _) => AppState::UpdateAvailable,
            (Some(_), false, _) => AppState::Installed,
            (None, _, Some(_)) => AppState::NotInstalled,
            (None, _, None) => AppState::Unavailable,
        };

        let mut actions = Vec::new();
        if active_task.is_none() {
            match state {
                AppState::NotInstalled => actions.push(Action::Install),
                AppState::UpdateAvailable => {
                    actions.push(Action::Update);
                    actions.push(Action::Repair);
                    actions.push(Action::Uninstall);
                }
                AppState::Installed => {
                    if selection.best.is_some() {
                        actions.push(Action::Repair);
                    }
                    actions.push(Action::Uninstall);
                }
                AppState::Unavailable => {}
            }
        }

        AppView {
            id: app.id.clone(),
            name: app.name.clone(),
            summary: app.summary.clone(),
            description: app.description.clone(),
            publisher: app.publisher.clone(),
            homepage: app.homepage.clone(),
            license: app.license.clone(),
            state,
            installed,
            latest,
            newer_blocked,
            unavailable,
            releases: release_views,
            actions,
            active_task,
            orphaned,
        }
    }

    // ---- tasks --------------------------------------------------------------------------------------

    pub fn tasks(&self) -> Vec<TaskView> {
        self.inner.tasks.lock().unwrap().iter().map(|t| t.view.clone()).collect()
    }

    pub fn has_active_install(&self) -> bool {
        self.inner.tasks.lock().unwrap().iter().any(|t| matches!(t.view.phase, Phase::Installing | Phase::WaitingForInstaller))
    }

    pub fn has_active_tasks(&self) -> bool {
        self.inner.tasks.lock().unwrap().iter().any(|t| !t.view.phase.is_final())
    }

    pub fn cancel(&self, task_id: &str) -> bool {
        let tasks = self.inner.tasks.lock().unwrap();
        match tasks.iter().find(|t| t.view.id == task_id) {
            Some(t) if t.view.cancellable && !t.view.phase.is_final() => {
                t.cancel.cancel();
                true
            }
            _ => false,
        }
    }

    pub fn dismiss(&self, task_id: &str) {
        self.inner.tasks.lock().unwrap().retain(|t| !(t.view.id == task_id && t.view.phase.is_final()));
    }

    pub fn dismiss_finished(&self) {
        self.inner.tasks.lock().unwrap().retain(|t| !t.view.phase.is_final());
    }

    fn update_task(&self, id: &str, f: impl FnOnce(&mut TaskView)) {
        let view = {
            let mut tasks = self.inner.tasks.lock().unwrap();
            let Some(t) = tasks.iter_mut().find(|t| t.view.id == id) else { return };
            f(&mut t.view);
            t.view.clone()
        };
        self.emit(Event::Task { task: view });
    }

    /// Validate what can be validated now, register the task, and run it in the background.
    pub fn start(&self, app_id: &str, action: Action) -> Result<String> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| HubError::Internal("start() must be called from inside the async runtime".into()))?;
        let name = {
            let state = self.inner.store.state();
            let cat = self.catalog();
            match (cat.as_ref().and_then(|c| c.app(app_id)), state.installs.get(app_id)) {
                (Some(a), _) => a.name.clone(),
                (None, Some(r)) => r.name.clone(),
                (None, None) => return Err(HubError::UnknownApp(app_id.to_string())),
            }
        };
        let id = {
            let mut tasks = self.inner.tasks.lock().unwrap();
            if tasks.iter().any(|t| t.view.app_id == app_id && !t.view.phase.is_final()) {
                return Err(HubError::InstallerBusy);
            }
            let id = format!("t{}-{}", self.now().unix_timestamp(), self.inner.counter.fetch_add(1, Ordering::SeqCst));
            let view = TaskView {
                id: id.clone(),
                app_id: app_id.to_string(),
                app_name: name,
                action,
                version: None,
                phase: Phase::Queued,
                bytes_done: 0,
                bytes_total: 0,
                bytes_per_sec: 0,
                error: None,
                reboot_required: false,
                started_at: rfc3339(self.now()),
                finished_at: None,
                cancellable: true,
            };
            tasks.push(TaskEntry { view, cancel: CancellationToken::new() });
            // Keep the list bounded: drop the oldest finished ones.
            while tasks.len() > 50 {
                if let Some(i) = tasks.iter().position(|t| t.view.phase.is_final()) {
                    tasks.remove(i);
                } else {
                    break;
                }
            }
            id
        };
        let view = self.inner.tasks.lock().unwrap().iter().find(|t| t.view.id == id).unwrap().view.clone();
        self.emit(Event::Task { task: view });

        let me = self.clone();
        let (tid, app) = (id.clone(), app_id.to_string());
        runtime.spawn(async move { me.run(tid, app, action).await });
        Ok(id)
    }

    async fn run(&self, task_id: String, app_id: String, action: Action) {
        let started = Instant::now();
        let cancel = {
            let tasks = self.inner.tasks.lock().unwrap();
            tasks.iter().find(|t| t.view.id == task_id).map(|t| t.cancel.clone()).unwrap_or_default()
        };
        let before = self.inner.store.state().installs.get(&app_id).map(|r| r.version.clone());
        let result = self.run_inner(&task_id, &app_id, action, &cancel).await;
        self.inner.store.end_journal();

        let (outcome, err_view, to_version, reboot) = match &result {
            Ok(r) => (Outcome::Success, None, r.version.clone(), r.reboot_required),
            Err(e) if matches!(e, HubError::Cancelled | HubError::InstallCancelled) => (Outcome::Cancelled, Some(ErrorView::from(e)), None, false),
            Err(e) => (Outcome::Failed, Some(ErrorView::from(e)), None, false),
        };
        if let Err(e) = &result {
            tracing::warn!(app = %app_id, action = action.as_str(), code = e.code(), "operation did not complete: {e}");
        }
        let (app_name, version) = {
            let tasks = self.inner.tasks.lock().unwrap();
            tasks.iter().find(|t| t.view.id == task_id).map(|t| (t.view.app_name.clone(), t.view.version.clone())).unwrap_or_default()
        };
        let _ = self.inner.store.append_history(&HistoryEntry {
            id: task_id.clone(),
            at: rfc3339(self.now()),
            app_id: app_id.clone(),
            app_name,
            action: action.as_str().into(),
            from_version: before,
            to_version: to_version.or(version),
            outcome,
            error_code: err_view.as_ref().map(|e| e.code.clone()),
            message: err_view.as_ref().map(|e| e.detail.clone()),
            duration_ms: started.elapsed().as_millis() as u64,
        });
        let now = rfc3339(self.now());
        self.update_task(&task_id, |t| {
            t.phase = match outcome {
                Outcome::Success => Phase::Done,
                Outcome::Cancelled => Phase::Cancelled,
                _ => Phase::Failed,
            };
            t.error = err_view;
            t.reboot_required = reboot;
            t.cancellable = false;
            t.finished_at = Some(now);
        });
        self.emit(Event::Installed);
    }

    fn set_phase(&self, id: &str, phase: Phase) {
        self.update_task(id, |t| {
            t.phase = phase;
            t.cancellable = !matches!(phase, Phase::Installing | Phase::Finishing);
        });
    }

    async fn run_inner(&self, task_id: &str, app_id: &str, action: Action, cancel: &CancellationToken) -> Result<RunOutcome> {
        let inner = &self.inner;
        self.set_phase(task_id, Phase::Preparing);
        let state = inner.store.state();
        let record = state.installs.get(app_id).cloned();
        let catalog = self.catalog();
        let snapshot_expired = inner.snapshot.read().unwrap().as_ref().is_some_and(|s| s.expired);

        // The entry to work with: the catalog's, or a stub built from the install record (uninstall only).
        let app: AppEntry = match (catalog.as_ref().and_then(|c| c.app(app_id)).cloned(), &record) {
            (Some(a), _) => a,
            (None, Some(r)) if action == Action::Uninstall => AppEntry {
                id: app_id.into(),
                name: r.name.clone(),
                summary: String::new(),
                description: String::new(),
                publisher: String::new(),
                homepage: None,
                license: None,
                detect: r.detect.clone(),
                releases: vec![],
            },
            _ => return Err(HubError::UnknownApp(app_id.into())),
        };
        let existing = platform::detect(&app, record.as_ref());

        if action == Action::Uninstall {
            let info = existing.ok_or_else(|| HubError::NotInstalled(app.name.clone()))?;
            if let Some(target) = platform::launch_target(&info, record.as_ref().and_then(|r| r.launch.as_deref())) {
                if platform::is_running(&target) {
                    return Err(HubError::AppRunning(app.name.clone()));
                }
            }
            self.set_phase(task_id, Phase::WaitingForInstaller);
            let _guard = tokio::select! {
                g = inner.install_lock.lock() => g,
                _ = cancel.cancelled() => return Err(HubError::Cancelled),
            };
            self.begin_journal(task_id, &app, action, Some(info.version.clone()), None)?;
            self.set_phase(task_id, Phase::Installing);
            platform::uninstall(&info, &app.name, &inner.store.logs_dir()).await?;
            // User data and settings live outside the install directory and are not touched.
            let mut st = inner.store.state();
            st.installs.remove(app_id);
            inner.store.save_state(&st)?;
            return Ok(RunOutcome::default());
        }

        // ---- install / update / repair: needs a trustworthy catalog ----
        if catalog.is_none() {
            return Err(HubError::Offline("no catalog is available yet".into()));
        }
        if snapshot_expired {
            return Err(HubError::Expired("the saved catalog has expired; connect to the internet and refresh before installing".into()));
        }
        let selection = app.select(&inner.host);
        let best = match selection.best {
            Some(b) => b,
            None => return Err(HubError::Incompatible(selection.why_none.unwrap_or(Incompat::NoStableRelease))),
        };

        // Which release to put on the machine.
        let (release, artifact, repair) = match action {
            Action::Install => {
                if existing.is_some() {
                    return Err(HubError::Unsupported(format!("{} is already installed", app.name)));
                }
                (best.release, best.artifact, false)
            }
            Action::Update => {
                let info = existing.as_ref().ok_or_else(|| HubError::NotInstalled(app.name.clone()))?;
                if let Some(have) = parse_installed_version(&info.version) {
                    if best.release.semver() <= have {
                        return Err(HubError::Unsupported(format!("{} is already up to date", app.name)));
                    }
                }
                (best.release, best.artifact, false)
            }
            Action::Repair => {
                let info = existing.as_ref().ok_or_else(|| HubError::NotInstalled(app.name.clone()))?;
                // Reinstall the very version that is installed when the catalog still has it.
                let same = app
                    .releases
                    .iter()
                    .filter(|r| r.withdrawn.is_none() && Some(r.semver()) == parse_installed_version(&info.version))
                    .find_map(|r| {
                        let probe = AppEntry { releases: vec![r.clone()], ..app.clone() };
                        probe.select(&inner.host).best.map(|s| (r.version.clone(), s.artifact.clone()))
                    });
                match same {
                    Some((v, _)) => {
                        let rel = app.release(&v).unwrap();
                        let art = rel.artifacts.iter().find(|a| a.os == inner.host.os && a.arch == inner.host.arch).unwrap();
                        (rel, art, true)
                    }
                    None => (best.release, best.artifact, false),
                }
            }
            Action::Uninstall => unreachable!(),
        };
        self.update_task(task_id, |t| t.version = Some(release.version.clone()));

        // Dependencies on other ASTERION apps.
        let cat = catalog.as_ref().unwrap();
        for dep in &release.requires_apps {
            let dep_app = cat.app(&dep.id).ok_or_else(|| HubError::DependencyMissing(format!("{} (not in the catalog)", dep.id)))?;
            let dep_rec = state.installs.get(&dep.id);
            let have = platform::detect(dep_app, dep_rec).ok_or_else(|| HubError::DependencyMissing(dep_app.name.clone()))?;
            if let Some(min) = &dep.min_version {
                let (Some(h), Ok(m)) = (parse_installed_version(&have.version), semver::Version::parse(min)) else {
                    return Err(HubError::DependencyMissing(format!("{} {min} or newer", dep_app.name)));
                };
                if h < m {
                    return Err(HubError::DependencyMissing(format!("{} {min} or newer", dep_app.name)));
                }
            }
        }

        // Disk space: the download, and the installed size (or twice the download when unknown).
        let need_download = artifact.size;
        let free_dl = platform::free_space(&inner.store.downloads_dir()).unwrap_or(u64::MAX);
        let target_dir = existing.as_ref().and_then(|e| e.location.clone()).or_else(|| platform::default_install_dir(&app.id, self.settings().install_scope)).unwrap_or_else(|| inner.store.downloads_dir());
        let free_target = platform::free_space(&target_dir).unwrap_or(u64::MAX);
        let need_install = artifact.installed_size_mb.map(|m| m * 1024 * 1024).unwrap_or(artifact.size * 2);
        let mb = |b: u64| b / (1024 * 1024);
        if free_dl < need_download + 64 * 1024 * 1024 {
            return Err(HubError::DiskSpace { need_mb: mb(need_download) + 64, free_mb: mb(free_dl) });
        }
        if free_target < need_install + 64 * 1024 * 1024 {
            return Err(HubError::DiskSpace { need_mb: mb(need_install) + 64, free_mb: mb(free_target) });
        }

        // Never replace files an application is using.
        if let Some(info) = &existing {
            if let Some(target) = platform::launch_target(info, artifact.launch.as_deref()) {
                if platform::is_running(&target) {
                    return Err(HubError::AppRunning(app.name.clone()));
                }
            }
        }

        // ---- download (resumable, verified) ----
        self.set_phase(task_id, Phase::Downloading);
        let spec = DownloadSpec { url: &artifact.url, size: artifact.size, sha256: &artifact.sha256, kind: artifact.kind, dest_dir: &inner.store.downloads_dir() };
        let me = self.clone();
        let tid = task_id.to_string();
        let mut last_emit = Instant::now() - Duration::from_secs(1);
        let mut window = (Instant::now(), 0u64, 0f64);
        let package = download_verified(&inner.client, &inner.policy, &spec, cancel, move |done, total| {
            let now = Instant::now();
            let dt = now.duration_since(window.0).as_secs_f64();
            if dt >= 0.5 {
                let inst = (done.saturating_sub(window.1)) as f64 / dt;
                window = (now, done, if window.2 == 0.0 { inst } else { window.2 * 0.6 + inst * 0.4 });
            }
            let speed = window.2 as u64;
            if now.duration_since(last_emit) >= Duration::from_millis(120) || done == total {
                last_emit = now;
                me.update_task(&tid, |t| {
                    t.bytes_done = done;
                    t.bytes_total = total;
                    t.bytes_per_sec = speed;
                });
            }
        })
        .await?;

        // ---- verify again, right before running ----
        self.set_phase(task_id, Phase::Verifying);
        verify_file(&package, artifact.size, &artifact.sha256).await?;
        if artifact.kind == PackageKind::Msi {
            let facts = artifact.msi.clone().ok_or_else(|| HubError::UnexpectedPackage("no MSI facts".into()))?;
            let p = package.clone();
            tokio::task::spawn_blocking(move || msi_info::check_against_catalog(&p, &facts)).await.map_err(|e| HubError::Internal(e.to_string()))??;
        }
        if cancel.is_cancelled() {
            return Err(HubError::Cancelled);
        }

        // ---- install ----
        self.set_phase(task_id, Phase::WaitingForInstaller);
        let _guard = tokio::select! {
            g = inner.install_lock.lock() => g,
            _ = cancel.cancelled() => return Err(HubError::Cancelled),
        };
        // The wait may have been long; the package on disk is checked one last time.
        verify_file(&package, artifact.size, &artifact.sha256).await?;
        let settings = self.settings();
        let scope = existing.as_ref().map(|e| e.scope).unwrap_or(settings.install_scope);
        self.begin_journal(task_id, &app, action, existing.as_ref().map(|e| e.version.clone()), Some(release.version.clone()))?;
        self.set_phase(task_id, Phase::Installing);
        let result = platform::install(&InstallPlan {
            app: &app,
            release,
            artifact,
            package: &package,
            scope,
            log_dir: &inner.store.logs_dir(),
            repair,
            existing: existing.as_ref(),
        })
        .await?;

        // ---- confirm what the system now says, and remember it ----
        self.set_phase(task_id, Phase::Finishing);
        let after: Option<InstalledInfo> = platform::detect(&app, record.as_ref());
        let info = after.ok_or_else(|| HubError::InstallerFailed(0))?;
        let mut st = inner.store.state();
        st.installs.insert(
            app_id.to_string(),
            InstallRecord {
                app_id: app_id.to_string(),
                version: info.version.clone(),
                scope: info.scope,
                location: info.location.as_ref().or(result.location.as_ref()).map(|p| p.display().to_string()),
                product_code: info.product_code.clone(),
                installed_at: rfc3339(self.now()),
                launch: artifact.launch.clone(),
                detect: app.detect.clone(),
                name: app.name.clone(),
            },
        );
        inner.store.save_state(&st)?;
        if !settings.keep_installers {
            clean_downloads(&inner.store.downloads_dir(), None);
        }
        Ok(RunOutcome { version: Some(info.version), reboot_required: result.reboot_required })
    }

    fn begin_journal(&self, task_id: &str, app: &AppEntry, action: Action, from: Option<String>, to: Option<String>) -> Result<()> {
        self.inner.store.begin_journal(&Journal {
            task_id: task_id.into(),
            app_id: app.id.clone(),
            app_name: app.name.clone(),
            action: action.as_str().into(),
            from_version: from,
            to_version: to,
            started_at: rfc3339(self.now()),
        })
    }

    // ---- misc ---------------------------------------------------------------------------------------

    pub fn open(&self, app_id: &str) -> Result<()> {
        let state = self.inner.store.state();
        let catalog = self.catalog();
        let record = state.installs.get(app_id);
        let app = catalog.as_ref().and_then(|c| c.app(app_id)).cloned().or_else(|| {
            record.map(|r| AppEntry { id: app_id.into(), name: r.name.clone(), summary: String::new(), description: String::new(), publisher: String::new(), homepage: None, license: None, detect: r.detect.clone(), releases: vec![] })
        });
        let app = app.ok_or_else(|| HubError::UnknownApp(app_id.into()))?;
        let info = platform::detect(&app, record).ok_or_else(|| HubError::NotInstalled(app.name.clone()))?;
        let launch = app.select(&self.inner.host).best.and_then(|s| s.artifact.launch.clone()).or_else(|| record.and_then(|r| r.launch.clone()));
        let target = platform::launch_target(&info, launch.as_deref()).ok_or_else(|| HubError::NotInstalled(format!("{} (its executable was not found)", app.name)))?;
        platform::launch(&target)
    }

    pub fn history(&self, limit: usize) -> Vec<HistoryEntry> {
        let mut h = self.inner.store.history(limit);
        h.reverse();
        h
    }

    pub fn clear_history(&self) -> Result<()> {
        self.inner.store.clear_history()
    }

    /// Is a periodic check due, given the user's setting?
    pub fn auto_check_due(&self) -> bool {
        use crate::store::AutoCheck;
        let s = self.settings();
        let last = self.inner.store.state().last_check.and_then(|t| crate::manifest::parse_rfc3339(&t).ok());
        match s.auto_check {
            AutoCheck::Never => false,
            AutoCheck::OnStart => true,
            other => {
                let hours = other.interval_hours().unwrap_or(24) as i64;
                last.map(|l| (self.now() - l).whole_hours() >= hours).unwrap_or(true)
            }
        }
    }
}

#[derive(Default)]
struct RunOutcome {
    version: Option<String>,
    reboot_required: bool,
}

