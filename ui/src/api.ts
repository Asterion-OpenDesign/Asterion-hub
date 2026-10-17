import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { Action, AppView, Bootstrap, CatalogStatus, CmdError, HistoryEntry, HubEvent, Settings, TaskView } from './types'

/** True only inside the Hub window. In a plain browser there is no engine and nothing is invented. */
export const hasHost = (): boolean => typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

export function asCmdError(e: unknown): CmdError {
  if (e && typeof e === 'object' && 'code' in e && 'detail' in e) return e as CmdError
  return { code: 'internal', detail: String(e), retryable: false }
}

export const api = {
  bootstrap: () => invoke<Bootstrap>('bootstrap'),
  refreshCatalog: () => invoke<CatalogStatus>('refresh_catalog'),
  catalogStatus: () => invoke<CatalogStatus>('catalog_status'),
  listApps: () => invoke<AppView[]>('list_apps'),
  listTasks: () => invoke<TaskView[]>('list_tasks'),
  startTask: (appId: string, action: Action) => invoke<string>('start_task', { appId, action }),
  cancelTask: (taskId: string) => invoke<boolean>('cancel_task', { taskId }),
  dismissTask: (taskId: string) => invoke<void>('dismiss_task', { taskId }),
  dismissFinished: () => invoke<void>('dismiss_finished_tasks'),
  openApp: (appId: string) => invoke<void>('open_app', { appId }),
  listHistory: (limit: number) => invoke<HistoryEntry[]>('list_history', { limit }),
  clearHistory: () => invoke<void>('clear_history'),
  getSettings: () => invoke<Settings>('get_settings'),
  saveSettings: (settings: Settings) => invoke<void>('save_settings', { settings }),
  openLink: (url: string) => invoke<void>('open_link', { url }),
  openLogsFolder: () => invoke<void>('open_logs_folder'),
}

export function onHubEvent(handler: (e: HubEvent) => void): Promise<UnlistenFn> {
  return listen<HubEvent>('hub://event', (ev) => handler(ev.payload))
}

export function onCloseBlocked(handler: () => void): Promise<UnlistenFn> {
  return listen('hub://close-blocked', handler)
}
