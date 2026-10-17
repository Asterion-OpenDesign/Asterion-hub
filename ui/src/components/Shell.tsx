import { CheckCircle2, CircleAlert, Cloud, CloudOff, Download, History, House, Info, LayoutGrid, RefreshCw, Settings, SquareCheckBig, X } from 'lucide-react'
import type { ReactNode } from 'react'
import { actionPast, describeError, nav } from '../copy'
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

export function Sidebar() {
  const { page, go, apps, tasks, status, refreshing, refresh } = useHub()
  const counts: Partial<Record<Page, number>> = {
    installed: installedOf(apps).length,
    updates: updatesOf(apps).length,
    tasks: activeTasks(tasks).length,
  }
  const fromCache = status?.origin === 'cache'
  return (
    <nav className="sidebar" aria-label="Principal">
      <Wordmark />
      <ul>
        {nav.map((n) => (
          <li key={n.id}>
            <button className={`nav ${page === n.id ? 'nav--on' : ''}`} onClick={() => go(n.id)} aria-current={page === n.id ? 'page' : undefined}>
              {icons[n.id]}
              <span>{n.label}</span>
              {!!counts[n.id] && <span className={`badge ${n.id === 'updates' ? 'badge--marca' : ''}`}>{counts[n.id]}</span>}
            </button>
          </li>
        ))}
      </ul>
      <div className="sidebar__foot">
        <div className={`conn ${fromCache || status?.refresh_error ? 'conn--off' : ''}`}>
          {fromCache || !status?.loaded ? <CloudOff size={16} aria-hidden /> : <Cloud size={16} aria-hidden />}
          <div>
            <strong>{!status?.loaded ? 'Sin catálogo' : fromCache ? 'Sin conexión' : 'Catálogo al día'}</strong>
            <span>{status?.loaded ? `Comprobado ${ago(status.fetched_at)}` : status?.refresh_error ? describeError(status.refresh_error).title : 'Aún no consultado'}</span>
          </div>
        </div>
        <Button small variant="secondary" busy={refreshing} onClick={() => void refresh()} icon={<RefreshCw size={15} aria-hidden />}>
          Buscar actualizaciones
        </Button>
      </div>
    </nav>
  )
}

export function Toasts() {
  const { toasts } = useHub()
  return (
    <div className="toasts" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className={`toast toast--${t.tone}`}>
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

export function CloseIcon() {
  return <X size={16} />
}
