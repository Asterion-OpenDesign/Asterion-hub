// Mirrors the serialisable views of `hub-core::engine`. The web layer only paints these.

export type Action = 'install' | 'update' | 'repair' | 'uninstall'

export type Phase =
  | 'queued'
  | 'preparing'
  | 'downloading'
  | 'verifying'
  | 'waiting_for_installer'
  | 'installing'
  | 'finishing'
  | 'done'
  | 'failed'
  | 'cancelled'

export interface ErrorView {
  code: string
  detail: string
  retryable: boolean
}

export interface TaskView {
  id: string
  app_id: string
  app_name: string
  action: Action
  version: string | null
  phase: Phase
  bytes_done: number
  bytes_total: number
  bytes_per_sec: number
  error: ErrorView | null
  reboot_required: boolean
  started_at: string
  finished_at: string | null
  cancellable: boolean
}

export interface CatalogStatus {
  loaded: boolean
  origin: 'network' | 'cache' | null
  fetched_at: string | null
  expired: boolean
  channel: 'stable' | 'beta'
  sequence: number | null
  keyset_days_left: number | null
  refresh_error: ErrorView | null
  last_check: string | null
}

export interface InstalledView {
  version: string
  scope: 'user' | 'machine'
  location: string | null
  installed_by_hub: boolean
  running: boolean
  can_open: boolean
}

export interface ReleaseView {
  version: string
  released_at: string
  notes: string[]
  installable: boolean
  blocked: ErrorView | null
  size: number | null
  kind: string | null
  os_min: string | null
  installed_size_mb: number | null
  requires: string[]
}

export type AppState = 'not_installed' | 'installed' | 'update_available' | 'unavailable'

export interface AppView {
  id: string
  name: string
  summary: string
  description: string
  publisher: string
  homepage: string | null
  license: string | null
  state: AppState
  installed: InstalledView | null
  latest: ReleaseView | null
  newer_blocked: ReleaseView | null
  unavailable: ErrorView | null
  releases: ReleaseView[]
  actions: Action[]
  active_task: string | null
  orphaned: boolean
}

export interface HostView {
  os: 'windows' | 'macos' | 'unknown'
  arch: 'x86_64' | 'aarch64' | 'unknown'
  os_version: string
  hub_version: string
  data_dir: string
}

export type AutoCheck = 'never' | 'on_start' | 'every_six_hours' | 'daily'

export interface Settings {
  channel: 'stable' | 'beta'
  auto_check: AutoCheck
  install_scope: 'user' | 'machine'
  theme: 'system' | 'light' | 'dark'
  keep_installers: boolean
}

export interface HistoryEntry {
  id: string
  at: string
  app_id: string
  app_name: string
  action: string
  from_version: string | null
  to_version: string | null
  outcome: 'success' | 'failed' | 'cancelled' | 'interrupted'
  error_code: string | null
  message: string | null
  duration_ms: number
}

export interface Bootstrap {
  host: HostView
  settings: Settings
  status: CatalogStatus
  requires_newer_hub: string | null
  dev_catalog: string | null
}

export type HubEvent = { kind: 'task'; task: TaskView } | { kind: 'catalog' } | { kind: 'installed' }

export interface CmdError {
  code: string
  detail: string
  retryable: boolean
}

export type Page = 'home' | 'apps' | 'installed' | 'updates' | 'tasks' | 'history' | 'settings'
