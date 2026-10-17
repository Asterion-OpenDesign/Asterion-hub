import { ArrowLeft, ChevronRight, CheckCircle2, CircleAlert, CloudOff, Download, History, House, Info, LayoutGrid, RefreshCw, Search, Settings, SquareCheckBig } from 'lucide-react'
import type { ReactNode } from 'react'
import { actionPast, nav } from '../copy'
import { ago } from '../format'
import { activeTasks, installedOf, updatesOf, useHub } from '../store'
import type { Page } from '../types'
import { Wordmark } from './Logo'
import { Button, Modal } from './ui'

const icons: Record<Page, ReactNode> = {
  home: <House size={18} />,
  apps: <LayoutGrid size={18} />,
  installed: <SquareCheckBig size={18} />,
  updates: <RefreshCw size={18} />,
  tasks: <Download size={18} />,
  history: <History size={18} />,
  settings: <Settings size={18} />,
}

export function Rail() {
  const { route, go, apps, tasks } = useHub()
  const counts: Partial<Record<Page, number>> = {
    installed: installedOf(apps).length,
    updates: updatesOf(apps).length,
    tasks: activeTasks(tasks).length,
  }
  return (
    <nav className="rail plano" aria-label="Principal">
      <Wordmark />
      <ul>
        {nav.map((n) => (
          <li key={n.id}>
            <button className={`nav ${route.page === n.id ? 'nav--on' : ''}`} onClick={() => go(n.id)} aria-current={route.page === n.id ? 'page' : undefined}>
              {icons[n.id]}
              <span>{n.label}</span>
              {!!counts[n.id] && <span className={`count ${n.id === 'updates' ? 'count--marca' : ''}`}>{counts[n.id]}</span>}
            </button>
          </li>
        ))}
      </ul>
      <RailFoot />
    </nav>
  )
}

function RailFoot() {
  const boot = useHub((s) => s.boot)
  const status = useHub((s) => s.status)
  return (
    <div className="rail__foot">
      <div>
        <strong>ASTERION Hub</strong> <span className="maq">{boot?.host.hub_version}</span>
      </div>
      {status?.channel === 'beta' && <span className="pill pill--rev">Canal beta</span>}
    </div>
  )
}

export function TopBar() {
  const { route, trail, back, go, apps, setPalette, status, refreshing, refresh } = useHub()
  const label = nav.find((n) => n.id === route.page)?.label ?? ''
  const app = route.appId ? apps.find((a) => a.id === route.appId) : null
  const boot = useHub((s) => s.boot)
  const cached = status?.origin === 'cache' || (!status?.loaded && !!status?.refresh_error)
  return (
    <header className="topbar plano">
      <button className={`back ${route.appId || trail.length ? '' : 'back--off'}`} onClick={back} aria-label="Volver" title="Volver (Alt + ←)">
        <ArrowLeft size={18} />
      </button>
      <div className="crumbs" aria-label="Ruta">
        {route.appId ? (
          <>
            <button onClick={() => go(route.page)}>{label}</button>
            <ChevronRight size={14} className="sep" aria-hidden />
            <strong>{app?.name ?? route.appId}</strong>
          </>
        ) : (
          <strong>{label}</strong>
        )}
      </div>
      {boot?.dev_catalog && (
        <span className="pill pill--marca devpill" title={`Catálogo de desarrollo: ${boot.dev_catalog}`}>
          Catálogo local
        </span>
      )}
      <button className="search-pill" onClick={() => setPalette(true)} aria-label="Buscar aplicaciones y acciones">
        <Search size={15} aria-hidden />
        <span>Buscar aplicaciones y acciones</span>
        <kbd>Ctrl K</kbd>
      </button>
      <button className={`sync ${cached ? 'sync--off' : ''}`} onClick={() => void refresh()} disabled={refreshing} title={status?.loaded ? `Catálogo comprobado ${ago(status.fetched_at)}. Pulsa para buscar actualizaciones ahora.` : 'Buscar actualizaciones ahora'}>
        {refreshing ? <RefreshCw size={14} className="spin" aria-hidden /> : cached ? <CloudOff size={14} aria-hidden /> : <span className="dot" aria-hidden />}
        {refreshing ? 'Comprobando…' : cached ? 'Sin conexión' : !status?.loaded ? 'Sin catálogo' : 'Al día'}
      </button>
    </header>
  )
}

export function Toasts() {
  const { toasts } = useHub()
  return (
    <div className="toasts" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className={`toast toast--${t.tone} floating`}>
          {t.tone === 'ok' ? <CheckCircle2 size={18} aria-hidden /> : t.tone === 'error' ? <CircleAlert size={18} aria-hidden /> : <Info size={18} aria-hidden />}
          <span>{t.text}</span>
        </div>
      ))}
    </div>
  )
}

export function ConfirmDialog() {
  const { confirm, askConfirm, apps, run } = useHub()
  if (!confirm) return null
  const app = apps.find((a) => a.id === confirm.appId)
  if (!app) return null
  const close = () => askConfirm(null)
  return (
    <Modal
      title={`${actionPast[confirm.action]} de ${app.name}`}
      onClose={close}
      footer={
        <>
          <Button onClick={close} data-autofocus>
            Cancelar
          </Button>
          <Button
            variant="danger"
            onClick={() => {
              close()
              void run(app.id, confirm.action)
            }}
          >
            Desinstalar
          </Button>
        </>
      }
    >
      <p>
        Se quitará <strong>{app.name}</strong> de este equipo.
      </p>
      <p className="muted">ASTERION Hub no elimina tus proyectos, documentos ni ajustes personales: solo se retira la aplicación instalada.</p>
    </Modal>
  )
}
