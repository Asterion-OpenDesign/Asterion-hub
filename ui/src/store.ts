import { create } from 'zustand'
import { api, asCmdError, onCloseBlocked, onHubEvent } from './api'
import type { Action, AppTab, AppView, Bootstrap, CatalogStatus, CmdError, HistoryEntry, Page, Route, Settings, TaskView } from './types'

export interface Toast {
  id: number
  tone: 'ok' | 'error' | 'info'
  text: string
}

export type CatalogView = 'grid' | 'list'
export type CatalogSort = 'name' | 'recent' | 'status' | 'category'
export type StatusFilter = 'all' | 'available' | 'installed' | 'updates'

/** What the catalog screen remembers between sessions. Pure presentation: nothing the engine decides. */
interface Prefs {
  view: CatalogView
  sort: CatalogSort
  showUnavailable: boolean
}

const PREFS_KEY = 'asterion-hub.ui.v1'

function loadPrefs(): Prefs {
  const d: Prefs = { view: 'grid', sort: 'name', showUnavailable: false }
  try {
    const raw = JSON.parse(localStorage.getItem(PREFS_KEY) ?? '{}')
    return {
      view: raw.view === 'list' ? 'list' : 'grid',
      sort: ['name', 'recent', 'status', 'category'].includes(raw.sort) ? raw.sort : d.sort,
      showUnavailable: raw.showUnavailable === true,
    }
  } catch {
    return d
  }
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

  route: Route
  trail: Route[]
  confirm: { appId: string; action: Action } | null
  palette: boolean
  toasts: Toast[]

  prefs: Prefs
  query: string
  statusFilter: StatusFilter
  category: string | null

  init: () => Promise<void>
  go: (p: Page) => void
  openApp: (id: string, tab?: AppTab) => void
  setTab: (tab: AppTab) => void
  back: () => void
  askConfirm: (c: { appId: string; action: Action } | null) => void
  setPalette: (open: boolean) => void
  setPrefs: (p: Partial<Prefs>) => void
  setQuery: (q: string) => void
  setStatusFilter: (s: StatusFilter) => void
  setCategory: (c: string | null) => void
  clearFilters: () => void
  reloadApps: () => Promise<void>
  reloadHistory: () => Promise<void>
  refresh: () => Promise<void>
  run: (appId: string, action: Action) => Promise<void>
  updateAll: () => Promise<void>
  open: (appId: string) => Promise<void>
  cancel: (taskId: string) => Promise<void>
  dismiss: (taskId: string) => Promise<void>
  retry: (t: TaskView) => Promise<void>
  saveSettings: (s: Settings) => Promise<void>
  toast: (tone: Toast['tone'], text: string) => void
}

let toastSeq = 0
let started = false
let reloadTimer: ReturnType<typeof setTimeout> | undefined

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

  route: { page: 'home' },
  trail: [],
  confirm: null,
  palette: false,
  toasts: [],

  prefs: loadPrefs(),
  query: '',
  statusFilter: 'all',
  category: null,

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

    // Events can arrive in bursts (a task emits several per second); the apps list is read once per burst.
    const scheduleReload = () => {
      clearTimeout(reloadTimer)
      reloadTimer = setTimeout(() => void get().reloadApps(), 150)
    }

    await onHubEvent((ev) => {
      if (ev.kind === 'task') {
        const t = ev.task
        set((s) => {
          const i = s.tasks.findIndex((x) => x.id === t.id)
          return { tasks: i >= 0 ? s.tasks.map((x) => (x.id === t.id ? t : x)) : [...s.tasks, t] }
        })
        if (t.phase === 'done') {
          const verb = { install: 'instalada', update: 'actualizada', repair: 'reparada', uninstall: 'desinstalada' }[t.action]
          get().toast('ok', `${t.app_name} ${verb}${t.reboot_required ? '. Hace falta reiniciar el equipo para terminar.' : '.'}`)
        }
        if (t.phase === 'done' || t.phase === 'failed' || t.phase === 'cancelled') {
          scheduleReload()
          void get().reloadHistory()
        } else if (t.phase === 'queued') {
          scheduleReload()
        }
      } else if (ev.kind === 'catalog') {
        void api.catalogStatus().then((status) => set({ status }))
        scheduleReload()
      } else if (ev.kind === 'installed') {
        scheduleReload()
      }
    })
    await onCloseBlocked(() => get().toast('info', 'Hay una instalación en curso. Espera a que termine para cerrar ASTERION Hub.'))
    window.addEventListener('focus', () => scheduleReload())
    window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => applyTheme(get().settings?.theme ?? 'system'))
    window.addEventListener('keydown', (e) => {
      const target = e.target as HTMLElement | null
      const typing = target && (target.tagName === 'INPUT' || target.tagName === 'SELECT' || target.tagName === 'TEXTAREA')
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault()
        get().setPalette(!get().palette)
      } else if (e.key === '/' && !typing && !get().palette) {
        e.preventDefault()
        get().setPalette(true)
      } else if (e.key === 'Escape' && get().palette) {
        get().setPalette(false)
      } else if (e.altKey && e.key === 'ArrowLeft') {
        get().back()
      }
    })
  },

  go: (page) => set({ route: { page }, trail: [] }),

  openApp: (id, tab = 'resumen') => set((s) => ({ trail: [...s.trail, s.route], route: { page: s.route.page === 'home' ? 'apps' : s.route.page, appId: id, tab }, palette: false })),

  setTab: (tab) => set((s) => ({ route: { ...s.route, tab } })),

  back: () =>
    set((s) => {
      if (s.trail.length === 0) return s.route.appId ? { route: { page: s.route.page } } : s
      return { route: s.trail[s.trail.length - 1], trail: s.trail.slice(0, -1) }
    }),

  askConfirm: (confirm) => set({ confirm }),
  setPalette: (palette) => set({ palette }),

  setPrefs: (p) =>
    set((s) => {
      const prefs = { ...s.prefs, ...p }
      try {
        localStorage.setItem(PREFS_KEY, JSON.stringify(prefs))
      } catch {
        /* a webview with storage disabled just forgets the choice */
      }
      return { prefs }
    }),
  setQuery: (query) => set({ query }),
  setStatusFilter: (statusFilter) => set({ statusFilter }),
  setCategory: (category) => set({ category }),
  clearFilters: () => set({ query: '', statusFilter: 'all', category: null }),

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

  async updateAll() {
    const todo = get().apps.filter((a) => a.actions.includes('update') && !a.installed?.running)
    for (const a of todo) await get().run(a.id, 'update')
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
export const finishedTasks = (tasks: TaskView[]) => tasks.filter((t) => ['done', 'failed', 'cancelled'].includes(t.phase))

/** The rule behind "show unavailable": an app this machine cannot install and does not have. */
export const unavailableOf = (apps: AppView[]) => apps.filter((a) => a.state === 'unavailable')
