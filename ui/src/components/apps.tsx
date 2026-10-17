import { ArrowRight, ArrowUpCircle, Download, MinusCircle, MoreHorizontal, Play, Trash2, Wrench } from 'lucide-react'
import { actionLabel, describeError } from '../copy'
import { initials, percent } from '../format'
import { categoryOf } from '../catalog'
import { useHub } from '../store'
import type { Action, AppView } from '../types'
import { Button, Menu, Pill, Progress } from './ui'

export interface Primary {
  kind: Action | 'open'
  label: string
  disabled?: boolean
  note?: string
}

export function primaryFor(a: AppView): Primary | null {
  if (a.active_task) return null
  switch (a.state) {
    case 'update_available':
      return a.installed?.running ? { kind: 'update', label: actionLabel.update, disabled: true, note: `Cierra ${a.name} para actualizar` } : { kind: 'update', label: actionLabel.update }
    case 'installed':
      if (a.installed?.running) return { kind: 'open', label: 'En ejecución', disabled: true }
      if (!a.installed?.can_open) return { kind: 'open', label: actionLabel.open, disabled: true, note: 'No se encontró el ejecutable' }
      return { kind: 'open', label: actionLabel.open }
    case 'not_installed':
      return { kind: 'install', label: actionLabel.install }
    case 'unavailable':
      return null
  }
}

const icons = { install: Download, update: ArrowUpCircle, open: Play, repair: Wrench, uninstall: Trash2 }

export function useDo() {
  const { run, open, askConfirm } = useHub()
  return (a: AppView, kind: Action | 'open') => {
    if (kind === 'open') return void open(a.id)
    if (kind === 'uninstall') return askConfirm({ appId: a.id, action: kind })
    return void run(a.id, kind)
  }
}

export function Tile({ app, size = 44 }: { app: Pick<AppView, 'name' | 'state' | 'installed'>; size?: number }) {
  const cls = app.state === 'update_available' ? 'tile--update' : app.state === 'installed' ? 'tile--ok' : app.state === 'unavailable' ? 'tile--dim' : ''
  return (
    <div className={`tile ${cls}`} style={{ width: size, height: size, fontSize: size * 0.34 }} aria-hidden>
      {initials(app.name)}
    </div>
  )
}

export function NameTile({ name, size = 40 }: { name: string; size?: number }) {
  return (
    <div className="tile" style={{ width: size, height: size, fontSize: size * 0.34 }} aria-hidden>
      {initials(name)}
    </div>
  )
}

export function StatePill({ app }: { app: AppView }) {
  switch (app.state) {
    case 'installed':
      return <Pill tone="ok">Instalada</Pill>
    case 'update_available':
      return <Pill tone="marca">Actualización</Pill>
    case 'not_installed':
      return <Pill>Disponible</Pill>
    case 'unavailable':
      return <Pill dot={false}>No disponible aquí</Pill>
  }
}

export function PrimaryButton({ app, small, variant }: { app: AppView; small?: boolean; variant?: 'primary' | 'accent' | 'inverse' }) {
  const doIt = useDo()
  const p = primaryFor(app)
  if (!p) return null
  const Icon = icons[p.kind]
  return (
    <Button variant={variant ?? (p.kind === 'install' ? 'accent' : 'primary')} small={small} disabled={p.disabled} icon={<Icon size={16} aria-hidden />} onClick={(e) => (e.stopPropagation(), doIt(app, p.kind))} title={p.note}>
      {p.label}
    </Button>
  )
}

/** Repair and uninstall live behind "···": rare, and never next to the one thing most people want. */
export function MoreMenu({ app, small = true }: { app: AppView; small?: boolean }) {
  const doIt = useDo()
  const items = app.actions.filter((x) => x === 'repair' || x === 'uninstall')
  if (items.length === 0) return null
  const running = !!app.installed?.running
  return (
    <Menu
      trigger={(p) => (
        <Button {...p} small={small} variant="ghost" aria-label={`Más acciones de ${app.name}`} icon={<MoreHorizontal size={18} aria-hidden />} />
      )}
    >
      {(close) => (
        <>
          {items.map((act) => {
            const Icon = icons[act]
            return (
              <button
                key={act}
                role="menuitem"
                className={act === 'uninstall' ? 'danger' : ''}
                disabled={running}
                title={running ? `Cierra ${app.name} primero` : undefined}
                onClick={() => {
                  close()
                  doIt(app, act)
                }}
              >
                <Icon size={16} aria-hidden /> {actionLabel[act]}
              </button>
            )
          })}
        </>
      )}
    </Menu>
  )
}

function Availability({ app }: { app: AppView }) {
  if (app.state === 'unavailable' && app.unavailable) {
    return (
      <p className="card__note">
        <MinusCircle size={14} aria-hidden /> {describeError(app.unavailable).title}
      </p>
    )
  }
  if (app.newer_blocked?.blocked) {
    return (
      <p className="card__note">
        <MinusCircle size={14} aria-hidden /> {app.newer_blocked.version}: {describeError(app.newer_blocked.blocked).title.toLowerCase()}
      </p>
    )
  }
  return null
}

export function Ver({ from, to }: { from?: string | null; to?: string | null }) {
  if (from && to && from !== to)
    return (
      <span className="ver">
        {from}
        <ArrowRight size={13} aria-hidden />
        <strong>{to}</strong>
      </span>
    )
  return <span className="ver">{to ?? from ?? '—'}</span>
}

function TaskMini({ app }: { app: AppView }) {
  const task = useHub((s) => s.tasks.find((t) => t.id === app.active_task))
  if (!task) return <span className="muted small">En curso…</span>
  const downloading = task.phase === 'downloading' && task.bytes_total > 0
  return (
    <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 6 }}>
      <Progress label={task.phase} value={percent(task.bytes_done, task.bytes_total)} indeterminate={!downloading} />
      <span className="muted small">{downloading ? `${percent(task.bytes_done, task.bytes_total)} %` : 'Instalando…'}</span>
    </div>
  )
}

export function AppCard({ app }: { app: AppView }) {
  const openApp = useHub((s) => s.openApp)
  return (
    <article className="card card--link" tabIndex={0} onClick={() => openApp(app.id)} onKeyDown={(e) => e.key === 'Enter' && openApp(app.id)} aria-label={app.name}>
      <div className="card__top">
        <Tile app={app} />
        <StatePill app={app} />
      </div>
      <h3>{app.name}</h3>
      <p className="card__sub">
        {app.publisher}
        {app.category ? ` · ${categoryOf(app)}` : ''}
      </p>
      <p className="card__summary">{app.summary}</p>
      <Availability app={app} />
      <div className="card__foot">
        {app.active_task ? <TaskMini app={app} /> : <PrimaryButton app={app} small />}
        {!app.active_task && (app.installed || app.latest) && <Ver from={app.installed?.version} to={app.state === 'update_available' ? app.latest?.version : (app.installed?.version ?? app.latest?.version)} />}
      </div>
    </article>
  )
}

export function CatalogRow({ app }: { app: AppView }) {
  const openApp = useHub((s) => s.openApp)
  return (
    <div className="row" role="button" tabIndex={0} onClick={() => openApp(app.id)} onKeyDown={(e) => e.key === 'Enter' && openApp(app.id)}>
      <Tile app={app} size={40} />
      <div className="row__main">
        <h3>
          {app.name}
          {app.category && <span className="chip hide-narrow">{categoryOf(app)}</span>}
        </h3>
        <p>{app.summary || app.publisher}</p>
      </div>
      <div className="row__meta">
        <Ver from={app.installed?.version} to={app.state === 'update_available' ? app.latest?.version : (app.installed?.version ?? app.latest?.version)} />
        <StatePill app={app} />
      </div>
      <div className="row__actions">{app.active_task ? <TaskMini app={app} /> : <PrimaryButton app={app} small />}</div>
    </div>
  )
}

/** An update waiting: a slab of blue, because it is the one thing in the Hub that asks for a decision. */
export function UpdateSlab({ app, notes = 2 }: { app: AppView; notes?: number }) {
  const openApp = useHub((s) => s.openApp)
  return (
    <article className="losa losa--row" tabIndex={0} onClick={() => openApp(app.id, 'novedades')} onKeyDown={(e) => e.key === 'Enter' && openApp(app.id, 'novedades')} aria-label={`Actualizar ${app.name}`}>
      <div className="tile" style={{ width: 44, height: 44, fontSize: 15, background: 'var(--marca-honda)', borderColor: 'var(--marca-canto)', color: 'var(--marca-tinta)' }} aria-hidden>
        {initials(app.name)}
      </div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <h3>{app.name}</h3>
        <p className="sub small" style={{ display: 'flex', gap: 10, alignItems: 'center' }}>
          <Ver from={app.installed?.version} to={app.latest?.version} />
          {app.latest && app.latest.notes.length > 0 && <span style={{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{app.latest.notes.slice(0, notes).join(' · ')}</span>}
        </p>
      </div>
      {app.active_task ? (
        <div style={{ width: 160 }}>
          <TaskMini app={app} />
        </div>
      ) : (
        <PrimaryButton app={app} small />
      )}
    </article>
  )
}

export function OpenButton({ app }: { app: AppView }) {
  const open = useHub((s) => s.open)
  const running = !!app.installed?.running
  const can = !!app.installed?.can_open && !running
  return (
    <Button variant="primary" small disabled={!can} icon={<Play size={16} aria-hidden />} onClick={(e) => (e.stopPropagation(), void open(app.id))} title={!app.installed?.can_open ? 'No se encontró el ejecutable' : undefined}>
      {running ? 'En ejecución' : actionLabel.open}
    </Button>
  )
}

/** Installed apps as a launcher: the Hub is also where you start them. */
export function LaunchCard({ app }: { app: AppView }) {
  const openApp = useHub((s) => s.openApp)
  return (
    <article className="card card--link" tabIndex={0} onClick={() => openApp(app.id)} onKeyDown={(e) => e.key === 'Enter' && openApp(app.id)} aria-label={app.name}>
      <div className="card__top">
        <Tile app={app} size={40} />
        {app.installed?.running ? <Pill tone="ok">En ejecución</Pill> : <Ver from={app.installed?.version} to={app.installed?.version} />}
      </div>
      <h3>{app.name}</h3>
      <p className="card__sub">{app.state === 'update_available' && app.latest ? `Actualización a ${app.latest.version}` : 'Lista para abrir'}</p>
      <div className="card__foot">
        {app.active_task ? <TaskMini app={app} /> : <OpenButton app={app} />}
      </div>
    </article>
  )
}
