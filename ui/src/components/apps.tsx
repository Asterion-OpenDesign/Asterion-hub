import { ArrowUpCircle, CheckCircle2, Download, Play, Wrench, Trash2, MinusCircle } from 'lucide-react'
import { actionLabel, describeError, stateLabel } from '../copy'
import { initials } from '../format'
import { useHub } from '../store'
import type { Action, AppView } from '../types'
import { Button, Chip, type Tone } from './ui'

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
      return a.installed?.running
        ? { kind: 'update', label: actionLabel.update, disabled: true, note: `Cierra ${a.name} para actualizar` }
        : { kind: 'update', label: actionLabel.update }
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

export function AppAvatar({ name, size = 44 }: { name: string; size?: number }) {
  return (
    <div className="avatar" style={{ width: size, height: size, fontSize: size * 0.36 }} aria-hidden>
      {initials(name)}
    </div>
  )
}

export function StateChip({ app }: { app: AppView }) {
  const tone: Tone = app.state === 'installed' ? 'ok' : app.state === 'update_available' ? 'marca' : 'neutral'
  const text = app.state === 'installed' && app.installed ? `Instalada · ${app.installed.version}` : app.state === 'update_available' && app.latest ? `Actualización · ${app.latest.version}` : stateLabel(app)
  return <Chip tone={tone}>{text}</Chip>
}

export function PrimaryButton({ app, small }: { app: AppView; small?: boolean }) {
  const doIt = useDo()
  const p = primaryFor(app)
  if (!p) return null
  const Icon = icons[p.kind]
  return (
    <Button variant="primary" small={small} disabled={p.disabled} icon={<Icon size={16} aria-hidden />} onClick={(e) => (e.stopPropagation(), doIt(app, p.kind))} title={p.note}>
      {p.label}
    </Button>
  )
}

export function SecondaryButtons({ app, small }: { app: AppView; small?: boolean }) {
  const doIt = useDo()
  const running = app.installed?.running
  const items = app.actions.filter((x) => x !== 'install' && x !== 'update')
  return (
    <>
      {items.map((act) => {
        const Icon = icons[act]
        return (
          <Button key={act} small={small} variant={act === 'uninstall' ? 'ghost' : 'secondary'} disabled={!!running} title={running ? `Cierra ${app.name} primero` : undefined} icon={<Icon size={15} aria-hidden />} onClick={(e) => (e.stopPropagation(), doIt(app, act))}>
            {actionLabel[act]}
          </Button>
        )
      })}
    </>
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
        <MinusCircle size={14} aria-hidden /> {app.newer_blocked.version} no disponible aquí: {describeError(app.newer_blocked.blocked).title.toLowerCase()}
      </p>
    )
  }
  return null
}

export function AppCard({ app }: { app: AppView }) {
  const { select, tasks } = useHub()
  const task = tasks.find((t) => t.id === app.active_task)
  return (
    <article className="card card--app" tabIndex={0} onClick={() => select(app.id)} onKeyDown={(e) => e.key === 'Enter' && select(app.id)} aria-label={app.name}>
      <div className="card__top">
        <AppAvatar name={app.name} />
        <StateChip app={app} />
      </div>
      <h3>{app.name}</h3>
      <p className="card__publisher">{app.publisher}</p>
      <p className="card__summary">{app.summary}</p>
      <Availability app={app} />
      <div className="card__foot">
        {task ? (
          <span className="card__busy">
            {task.phase === 'downloading' && task.bytes_total > 0 ? `${Math.floor((task.bytes_done / task.bytes_total) * 100)} %` : 'En curso…'}
          </span>
        ) : (
          <PrimaryButton app={app} small />
        )}
        {app.state === 'installed' && !task && <CheckCircle2 size={18} className="card__ok" aria-label="Instalada" />}
      </div>
    </article>
  )
}

export function AppRow({ app, children }: { app: AppView; children?: React.ReactNode }) {
  const { select } = useHub()
  return (
    <div className="row" onClick={() => select(app.id)} tabIndex={0} onKeyDown={(e) => e.key === 'Enter' && select(app.id)}>
      <AppAvatar name={app.name} size={40} />
      <div className="row__main">
        <h3>{app.name}</h3>
        <p>{app.summary || app.publisher}</p>
      </div>
      <div className="row__meta">{children ?? <StateChip app={app} />}</div>
      <div className="row__actions">
        <PrimaryButton app={app} small />
      </div>
    </div>
  )
}
