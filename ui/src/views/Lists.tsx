import { useMemo, useState } from 'react'
import { CheckCircle2, Download, History as HistoryIcon, MinusCircle, PackageSearch, RefreshCw, Trash2 } from 'lucide-react'
import { api } from '../api'
import { matchText, sortApps } from '../catalog'
import { MoreMenu, NameTile, PrimaryButton, StatePill, Tile, UpdateSlab, Ver } from '../components/apps'
import { TaskCard } from '../components/TaskCard'
import { Button, Empty, Head, Pill, Section, Segmented } from '../components/ui'
import { actionPast, describeError, outcomeLabel } from '../copy'
import { ago, dateTime, duration } from '../format'
import { activeTasks, finishedTasks, installedOf, updatesOf, useHub } from '../store'
import { Loading, StatusBanners } from './common'

// ---- Instaladas ---------------------------------------------------------------------------------------

export function Installed() {
  const { apps, appsLoaded, openApp, go } = useHub()
  const [filter, setFilter] = useState<'all' | 'updates' | 'running'>('all')
  const all = installedOf(apps)
  const list = sortApps(
    all.filter((a) => (filter === 'updates' ? a.state === 'update_available' : filter === 'running' ? a.installed?.running : true)),
    'status',
  )
  return (
    <>
      <Head eyebrow={`Este equipo · ${all.length} instaladas`} title="Instaladas" lead="Lo que hay instalado, tal como lo detecta el sistema, tanto si lo instaló el Hub como si no." />
      <StatusBanners />
      {all.length > 0 && (
        <div className="toolbar">
          <Segmented
            label="Filtrar"
            value={filter}
            onChange={setFilter}
            options={[
              { value: 'all', label: 'Todas', count: all.length },
              { value: 'updates', label: 'Con actualización', count: all.filter((a) => a.state === 'update_available').length },
              { value: 'running', label: 'En ejecución', count: all.filter((a) => a.installed?.running).length },
            ]}
          />
        </div>
      )}
      {!appsLoaded ? (
        <Loading rows={3} h={72} />
      ) : all.length === 0 ? (
        <Empty icon={<PackageSearch size={28} />} title="Todavía no hay aplicaciones instaladas" action={<Button variant="primary" onClick={() => go('apps')}>Explorar el catálogo</Button>}>
          Instala una desde «Todas las aplicaciones».
        </Empty>
      ) : list.length === 0 ? (
        <Empty icon={<PackageSearch size={28} />} title="Nada con ese filtro" />
      ) : (
        <div className="lamina">
          {list.map((a) => (
            <div key={a.id} className="row" role="button" tabIndex={0} onClick={() => openApp(a.id)} onKeyDown={(e) => e.key === 'Enter' && openApp(a.id)}>
              <Tile app={a} size={40} />
              <div className="row__main">
                <h3>
                  {a.name}
                  <Ver from={a.installed?.version} to={a.state === 'update_available' ? a.latest?.version : a.installed?.version} />
                </h3>
                <p className="selectable path" style={{ whiteSpace: 'nowrap' }}>
                  {a.installed?.scope === 'machine' ? 'Todos los usuarios' : 'Solo tu usuario'}
                  {a.installed?.location ? ` · ${a.installed.location}` : ''}
                </p>
              </div>
              <div className="row__meta">
                <StatePill app={a} />
                {a.installed?.running && <Pill tone="ok">En ejecución</Pill>}
              </div>
              <div className="row__actions">
                <PrimaryButton app={a} small />
                <MoreMenu app={a} />
              </div>
            </div>
          ))}
        </div>
      )}
    </>
  )
}

// ---- Actualizaciones ----------------------------------------------------------------------------------

export function Updates() {
  const { apps, appsLoaded, updateAll, refresh, refreshing, status, openApp } = useHub()
  const list = updatesOf(apps)
  const stuck = apps.filter((a) => a.installed && a.state !== 'update_available' && a.newer_blocked?.blocked)
  return (
    <>
      <Head
        eyebrow={`Actualizaciones · ${list.length}`}
        title={list.length ? (list.length === 1 ? 'Hay 1 actualización lista.' : `Hay ${list.length} actualizaciones listas.`) : 'Todo está al día.'}
        lead={status?.loaded ? `Última comprobación ${ago(status.fetched_at)}${status.origin === 'cache' ? ' (datos guardados)' : ''}.` : 'Aún no se ha consultado el catálogo.'}
        actions={
          <>
            <Button busy={refreshing} onClick={() => void refresh()} icon={<RefreshCw size={16} aria-hidden />}>
              Buscar ahora
            </Button>
            {list.length > 1 && (
              <Button variant="primary" onClick={() => void updateAll()} icon={<Download size={16} aria-hidden />}>
                Actualizar todo ({list.length})
              </Button>
            )}
          </>
        }
      />
      <StatusBanners />
      {!appsLoaded ? (
        <Loading rows={2} h={80} />
      ) : list.length === 0 ? (
        <Empty icon={<CheckCircle2 size={28} />} title="No hay versiones nuevas">
          Las aplicaciones que tienes instaladas están en la última versión disponible para este equipo.
        </Empty>
      ) : (
        <div className="stack">
          {list.map((a) => (
            <UpdateSlab key={a.id} app={a} notes={3} />
          ))}
        </div>
      )}
      {stuck.length > 0 && (
        <Section title="Versiones que este equipo no puede instalar" count={stuck.length}>
          <div className="lamina">
            {stuck.map((a) => (
              <div key={a.id} className="row" role="button" tabIndex={0} onClick={() => openApp(a.id)} onKeyDown={(e) => e.key === 'Enter' && openApp(a.id)}>
                <Tile app={a} size={40} />
                <div className="row__main">
                  <h3>{a.name}</h3>
                  <p>
                    {a.newer_blocked?.version}: {a.newer_blocked?.blocked ? describeError(a.newer_blocked.blocked).title.toLowerCase() : ''}
                  </p>
                </div>
                <div className="row__meta">
                  <Pill tone="rev" dot={false}>
                    <MinusCircle size={13} aria-hidden /> Bloqueada
                  </Pill>
                </div>
                <div />
              </div>
            ))}
          </div>
        </Section>
      )}
    </>
  )
}

// ---- Descargas y tareas -------------------------------------------------------------------------------

export function Tasks() {
  const { tasks } = useHub()
  const active = activeTasks(tasks)
  const finished = finishedTasks(tasks).reverse()
  return (
    <>
      <Head
        eyebrow={`Tareas · ${active.length} activas`}
        title="Descargas y tareas"
        lead="Progreso real de cada operación: descarga, verificación e instalación."
        actions={
          finished.length > 0 ? (
            <Button variant="ghost" onClick={() => void api.dismissFinished().then(() => useHub.setState((s) => ({ tasks: s.tasks.filter((t) => !finished.includes(t)) })))}>
              Limpiar terminadas
            </Button>
          ) : undefined
        }
      />
      {tasks.length === 0 ? (
        <Empty icon={<Download size={28} />} title="No hay tareas">
          Cuando instales o actualices algo, verás aquí cada paso.
        </Empty>
      ) : (
        <>
          {active.length > 0 && (
            <Section title="En curso" count={active.length}>
              <div className="stack">
                {active.map((t) => (
                  <TaskCard key={t.id} task={t} />
                ))}
              </div>
            </Section>
          )}
          {finished.length > 0 && (
            <Section title="Terminadas" count={finished.length}>
              <div className="stack">
                {finished.map((t) => (
                  <TaskCard key={t.id} task={t} />
                ))}
              </div>
            </Section>
          )}
        </>
      )}
    </>
  )
}

// ---- Historial ----------------------------------------------------------------------------------------

function dayLabel(iso: string): string {
  const d = new Date(iso)
  const today = new Date()
  const diff = Math.round((new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime() - new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()) / 86400000)
  if (diff === 0) return 'Hoy'
  if (diff === 1) return 'Ayer'
  return d.toLocaleDateString('es', { weekday: 'long', day: 'numeric', month: 'long', year: 'numeric' })
}

export function History() {
  const { history, reloadHistory } = useHub()
  const [open, setOpen] = useState<string | null>(null)
  const [outcome, setOutcome] = useState<'all' | 'success' | 'problem'>('all')
  const [app, setApp] = useState('')
  const [q, setQ] = useState('')
  const names = useMemo(() => [...new Map(history.map((h) => [h.app_id, h.app_name])).entries()].sort((a, b) => a[1].localeCompare(b[1], 'es')), [history])
  const shown = history.filter((h) => (outcome === 'all' ? true : outcome === 'success' ? h.outcome === 'success' : h.outcome !== 'success') && (!app || h.app_id === app) && matchText(`${h.app_name} ${actionPast[h.action] ?? h.action} ${h.error_code ?? ''}`, q))
  const groups: { day: string; items: typeof shown }[] = []
  for (const h of shown) {
    const day = dayLabel(h.at)
    const g = groups.find((x) => x.day === day)
    if (g) g.items.push(h)
    else groups.push({ day, items: [h] })
  }
  return (
    <>
      <Head
        eyebrow={`Historial · ${history.length} operaciones`}
        title="Historial"
        lead="Qué se hizo, cuándo y cómo terminó. Se guarda solo en este equipo."
        actions={
          history.length > 0 ? (
            <Button variant="ghost" icon={<Trash2 size={15} aria-hidden />} onClick={() => void api.clearHistory().then(reloadHistory)}>
              Borrar historial
            </Button>
          ) : undefined
        }
      />
      {history.length > 0 && (
        <div className="toolbar">
          <label className="field-search">
            <HistoryIcon size={16} aria-hidden />
            <input type="search" placeholder="Buscar en el historial" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Buscar en el historial" />
          </label>
          <Segmented
            label="Resultado"
            value={outcome}
            onChange={setOutcome}
            options={[
              { value: 'all', label: 'Todo' },
              { value: 'success', label: 'Completadas' },
              { value: 'problem', label: 'Con problemas' },
            ]}
          />
          {names.length > 1 && (
            <select value={app} onChange={(e) => setApp(e.target.value)} aria-label="Aplicación">
              <option value="">Todas las aplicaciones</option>
              {names.map(([id, name]) => (
                <option key={id} value={id}>
                  {name}
                </option>
              ))}
            </select>
          )}
        </div>
      )}
      {history.length === 0 ? (
        <Empty icon={<HistoryIcon size={28} />} title="Aún no hay historial">
          Las instalaciones, actualizaciones, reparaciones y desinstalaciones quedarán registradas aquí.
        </Empty>
      ) : shown.length === 0 ? (
        <Empty icon={<HistoryIcon size={28} />} title="Nada coincide" />
      ) : (
        <div className="table" role="table" aria-label="Historial de operaciones">
          {groups.map((g) => (
            <div key={g.day}>
              <div className="day">{g.day}</div>
              {g.items.map((h) => {
                const err = h.error_code ? describeError({ code: h.error_code, detail: h.message ?? '', retryable: false }) : null
                return (
                  <div key={h.id} className="table__row" role="row">
                    <button className="table__line" onClick={() => setOpen(open === h.id ? null : h.id)} aria-expanded={open === h.id}>
                      <span className="muted">{dateTime(h.at).replace(/^\d+ \w+\.? \d{4},? /, '')}</span>
                      <span style={{ display: 'flex', gap: 10, alignItems: 'center' }}>
                        <NameTile name={h.app_name} size={28} />
                        <span>
                          {actionPast[h.action] ?? h.action} · <strong>{h.app_name}</strong>
                        </span>
                      </span>
                      <Ver from={h.from_version} to={h.to_version} />
                      <span>
                        <Pill tone={h.outcome === 'success' ? 'ok' : h.outcome === 'failed' ? 'fdr' : 'rev'}>{outcomeLabel[h.outcome]}</Pill>
                      </span>
                    </button>
                    {open === h.id && (
                      <div className="table__detail">
                        {err && (
                          <p>
                            <strong>{err.title}.</strong> {err.hint}
                          </p>
                        )}
                        <p className="muted">Duración: {duration(h.duration_ms)}</p>
                        {h.message && <pre className="selectable">{h.message}</pre>}
                      </div>
                    )}
                  </div>
                )
              })}
            </div>
          ))}
        </div>
      )}
    </>
  )
}
