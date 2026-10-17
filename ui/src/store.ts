import { create } from 'zustand'
import { api, asCmdError, onCloseBlocked, onHubEvent } from './api'
import type { Action, AppView, Bootstrap, CatalogStatus, CmdError, HistoryEntry, Page, Settings, TaskView } from './types'

export interface Toast {
  id: number
  tone: 'ok' | 'error' | 'info'
  text: string
}

interface HubState {
  booted: boolean
  bootError: CmdError | null
  boot: Bootstrap | null
  status: CatalogStatus | null
  settings: Settings | null
  apps: AppView[]
  appsLoaded: boolean
  tasks: TaskView[]
  history: HistoryEntry[]
  refreshing: boolean
  page: Page
  selected: string | null
  confirm: { appId: string; action: Action } | null
  toasts: Toast[]

  init: () => Promise<void>
  go: (p: Page) => void
  select: (id: string | null) => void
  askConfirm: (c: { appId: string; action: Action } | null) => void
  reloadApps: () => Promise<void>
  reloadHistory: () => Promise<void>
  refresh: () => Promise<void>
  run: (appId: string, action: Action) => Promise<void>
  open: (appId: string) => Promise<void>
  cancel: (taskId: string) => Promise<void>
  dismiss: (taskId: string) => Promise<void>
  retry: (t: TaskView) => Promise<void>
  saveSettings: (s: Settings) => Promise<void>
  toast: (tone: Toast['tone'], text: string) => void
}

let toastSeq = 0
let started = false

export const useHub = create<HubState>((set, get) => ({
  booted: false,
  bootError: null,
  boot: null,
  status: null,
  settings: null,
  apps: [],
  appsLoaded: false,
  tasks: [],
  history: [],
  refreshing: false,
  page: 'home',
  selected: null,
  confirm: null,
  toasts: [],

  async init() {
    if (started) return
    started = true
    try {
      const boot = await api.bootstrap()
      set({ boot, status: boot.status, settings: boot.settings, booted: true })
      applyTheme(boot.settings.theme)
      await Promise.all([get().reloadApps(), get().reloadHistory(), api.listTasks().then((tasks) => set({ tasks }))])
    } catch (e) {
      set({ bootError: asCmdError(e), booted: true })
      return
    }

    await onHubEvent((ev) => {
      if (ev.kind === 'task') {
        const t = ev.task
        set((s) => {
          const i = s.tasks.findIndex((x) => x.id === t.id)
          const tasks = i >= 0 ? s.tasks.map((x) => (x.id === t.id ? t : x)) : [...s.tasks, t]
          return { tasks }
        })
        if (t.phase === 'done') {
          const verb = { install: 'instalada', update: 'actualizada', repair: 'reparada', uninstall: 'desinstalada' }[t.action]
          get().toast('ok', `${t.app_name} ${verb}${t.reboot_required ? '. Hace falta reiniciar el equipo para terminar.' : '.'}`)
        }
        if (t.phase === 'done' || t.phase === 'failed' || t.phase === 'cancelled') {
          void get().reloadApps()
          void get().reloadHistory()
        } else if (t.phase === 'queued') {
          void get().reloadApps()
        }
      } else if (ev.kind === 'catalog') {
        void api.catalogStatus().then((status) => set({ status }))
        void get().reloadApps()
      } else if (ev.kind === 'installed') {
        void get().reloadApps()
      }
    })
    await onCloseBlocked(() => get().toast('info', 'Hay una instalación en curso. Espera a que termine para cerrar ASTERION Hub.'))
    // The user may install or remove things outside the Hub; look again when the window comes back.
    window.addEventListener('focus', () => void get().reloadApps())
    window
      .matchMedia('(prefers-color-scheme: dark)')
      .addEventListener('change', () => applyTheme(get().settings?.theme ?? 'system'))
  },

  go: (page) => set({ page, selected: null }),
  select: (selected) => set({ selected }),
  askConfirm: (confirm) => set({ confirm }),

  async reloadApps() {
    try {
      set({ apps: await api.listApps(), appsLoaded: true })
    } catch (e) {
      get().toast('error', asCmdError(e).detail)
    }
  },

  async reloadHistory() {
    try {
      set({ history: await api.listHistory(500) })
    } catch {
      /* history is informative; failing to read it must not break the screen */
    }
  },

  async refresh() {
    if (get().refreshing) return
    set({ refreshing: true })
    try {
      const status = await api.refreshCatalog()
      set({ status })
      await get().reloadApps()
    } catch (e) {
      get().toast('error', asCmdError(e).detail)
    } finally {
      set({ refreshing: false })
    }
  },

  async run(appId, action) {
    try {
      await api.startTask(appId, action)
    } catch (e) {
      const err = asCmdError(e)
      get().toast('error', err.code === 'installer_busy' ? 'Ya hay una operación en curso para esta aplicación.' : err.detail)
    }
  },

  async open(appId) {
    try {
      await api.openApp(appId)
    } catch (e) {
      get().toast('error', asCmdError(e).detail)
    }
  },

  async cancel(taskId) {
    await api.cancelTask(taskId)
  },

  async dismiss(taskId) {
    await api.dismissTask(taskId)
    set((s) => ({ tasks: s.tasks.filter((t) => t.id !== taskId) }))
  },

  async retry(t) {
    await get().dismiss(t.id)
    await get().run(t.app_id, t.action)
  },

  async saveSettings(settings) {
    const prev = get().settings
    set({ settings })
    applyTheme(settings.theme)
    try {
      await api.saveSettings(settings)
      if (prev && prev.channel !== settings.channel) await get().refresh()
    } catch (e) {
      set({ settings: prev })
      applyTheme(prev?.theme ?? 'system')
      get().toast('error', asCmdError(e).detail)
    }
  },

  toast(tone, text) {
    const id = ++toastSeq
    set((s) => ({ toasts: [...s.toasts.slice(-3), { id, tone, text }] }))
    setTimeout(() => set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) })), tone === 'error' ? 9000 : 5000)
  },
}))

export function applyTheme(theme: Settings['theme']) {
  const dark = theme === 'dark' || (theme === 'system' && window.matchMedia('(prefers-color-scheme: dark)').matches)
  document.documentElement.dataset.theme = dark ? 'dark' : 'light'
}

export const updatesOf = (apps: AppView[]) => apps.filter((a) => a.state === 'update_available')
export const installedOf = (apps: AppView[]) => apps.filter((a) => a.installed)
export const activeTasks = (tasks: TaskView[]) => tasks.filter((t) => !['done', 'failed', 'cancelled'].includes(t.phase))
