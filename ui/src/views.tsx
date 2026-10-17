import { useMemo, useState } from 'react'
import { ArrowRight, CheckCircle2, CircleAlert, CloudOff, Download, FolderOpen, History as HistoryIcon, PackageSearch, RefreshCw, Search, ShieldAlert, Trash2, WifiOff, FlaskConical } from 'lucide-react'
import { api } from './api'
import { AppAvatar, AppCard, PrimaryButton, SecondaryButtons, StateChip } from './components/apps'
import { TaskCard } from './components/TaskCard'
import { Banner, Button, Chip, Empty, PageHeader, Skeleton } from './components/ui'
import { actionPast, describeError, outcomeLabel } from './copy'
import { ago, dateTime, duration } from './format'
import { activeTasks, installedOf, updatesOf, useHub } from './store'
import type { AppView, AutoCheck, Settings } from './types'

// ---- shared ----------------------------------------------------------------------------------------------

export function StatusBanners() {
  const { status, boot, refresh, refreshing } = useHub()
  if (!status || !boot) return null
  const retry = (
    <Button small variant="secondary" busy={refreshing} onClick={() => void refresh()} icon={<RefreshCw size={15} aria-hidden />}>
      Reintentar
    </Button>
  )
  return (
    <>
      {boot.dev_catalog && (
        <Banner tone="marca" icon={<FlaskConical size={18} />} title="Catálogo de desarrollo">
          Esta compilación de depuración lee <span className="selectable">{boot.dev_catalog}</span>, no el catálogo oficial.
        </Banner>
      )}
      {boot.requires_newer_hub && (
        <Banner tone="rev" icon={<CircleAlert size={18} />} title="Hay un ASTERION Hub más reciente">
          El catálogo pide la versión {boot.requires_newer_hub} o posterior para mostrar todas las novedades.
        </Banner>
      )}
      {!status.loaded && status.refresh_error && (
        <Banner tone="fdr" icon={<WifiOff size={18} />} title={describeError(status.refresh_error).title} action={retry}>
          {describeError(status.refresh_error).hint} Aún no hay datos guardados que mostrar.
        </Banner>
      )}
      {status.loaded && status.origin === 'cache' && (
        <Banner tone="rev" icon={<CloudOff size={18} />} title="Mostrando datos guardados" action={retry}>
          {status.refresh_error ? `${describeError(status.refresh_error).title}. ` : ''}La información es de {ago(status.fetched_at)}. Las aplicaciones instaladas funcionan con normalidad.
        </Banner>
      )}
      {status.loaded && status.expired && (
        <Banner tone="rev" icon={<ShieldAlert size={18} />} title="El catálogo guardado ha caducado" action={retry}>
          Conéctate a internet y actualiza: sin un catálogo vigente el Hub no instala ni actualiza.
        </Banner>
      )}
      {status.keyset_days_left != null && status.keyset_days_left < 30 && status.keyset_days_left >= 0 && (
        <Banner tone="rev" icon={<ShieldAlert size={18} />} title="Las claves de publicación caducan pronto">
          Quedan {status.keyset_days_left} días. El equipo de ASTERION debe volver a firmar la lista de claves.
        </Banner>
      )}
    </>
  )
}

function Loading() {
  return (
    <div className="grid">
      <Skeleton h={200} />
      <Skeleton h={200} />
      <Skeleton h={200} />
    </div>
  )
}

function UpdateRow({ app }: { app: AppView }) {
  const { select } = useHub()
  return (
    <div className="row row--update" onClick={() => select(app.id)} tabIndex={0} onKeyDown={(e) => e.key === 'Enter' && select(app.id)}>
      <AppAvatar name={app.name} size={40} />
      <div className="row__main">
        <h3>{app.name}</h3>
        <p className="ver-arrow">
          <span>{app.installed?.version}</span>
          <ArrowRight size={14} aria-hidden />
          <strong>{app.latest?.version}</strong>
        </p>
        {app.latest && app.latest.notes.length > 0 && <p className="row__notes">{app.latest.notes.slice(0, 2).join(' · ')}</p>}
      </div>
      <div className="row__actions">
        <PrimaryButton app={app} small />
      </div>
    </div>
  )
}

function runAll(apps: AppView[], run: (id: string, a: 'update') => Promise<void>) {
  apps.filter((a) => a.actions.includes('update') && !a.installed?.running).forEach((a) => void run(a.id, 'update'))
}

// ---- Inicio ----------------------------------------------------------------------------------------------

export function Home() {
  const { apps, appsLoaded, status, tasks, history, run, go } = useHub()
  const updates = updatesOf(apps)
  const installed = installedOf(apps)
  const active = activeTasks(tasks)
  const recent = history.slice(0, 3)

  const title = !status?.loaded ? 'Bienvenido a ASTERION Hub' : updates.length > 0 ? (updates.length === 1 ? 'Tienes 1 actualización disponible' : `Tienes ${updates.length} actualizaciones disponibles`) : installed.length > 0 ? 'Todo está al día' : 'Instala tu primera aplicación'
  const lead = !status?.loaded ? 'Descubre, instala y mantén al día las aplicaciones de la familia ASTERION desde un solo lugar.' : updates.length > 0 ? 'Actualiza ahora, o cuando te venga bien. Tus aplicaciones siguen funcionando mientras tanto.' : installed.length > 0 ? 'Tus aplicaciones ASTERION están en su última versión.' : 'Elige una aplicación del catálogo. Se descarga, se verifica y se instala sin salir de aquí.'

  return (
    <>
      <PageHeader
        title={title}
        lead={lead}
        actions={
          updates.length > 1 ? (
            <Button variant="primary" icon={<Download size={16} aria-hidden />} onClick={() => runAll(updates, run)}>
              Actualizar todo
            </Button>
          ) : undefined
        }
      />
      <StatusBanners />

      <div className="stats">
        <button className="stat" onClick={() => go('apps')}>
          <strong>{apps.length}</strong>
          <span>Aplicaciones en el catálogo</span>
        </button>
        <button className="stat" onClick={() => go('installed')}>
          <strong>{installed.length}</strong>
          <span>Instaladas</span>
        </button>
        <button className={`stat ${updates.length ? 'stat--marca' : ''}`} onClick={() => go('updates')}>
          <strong>{updates.length}</strong>
          <span>Actualizaciones</span>
        </button>
        <button className="stat" onClick={() => go('tasks')}>
          <strong>{active.length}</strong>
          <span>Tareas activas</span>
        </button>
      </div>

      {active.length > 0 && (
        <section>
          <h2 className="section-title">En curso</h2>
          <div className="stack">
            {active.map((t) => (
              <TaskCard key={t.id} task={t} />
            ))}
          </div>
        </section>
      )}

      {updates.length > 0 && (
        <section>
          <h2 className="section-title">Actualizaciones</h2>
          <div className="stack">
            {updates.map((a) => (
              <UpdateRow key={a.id} app={a} />
            ))}
          </div>
        </section>
      )}

      <section>
        <h2 className="section-title">Aplicaciones</h2>
        {!appsLoaded ? (
          <Loading />
        ) : apps.length === 0 ? (
          <Empty icon={<PackageSearch size={28} />} title={status?.loaded ? 'El catálogo está vacío' : 'No hay catálogo todavía'}>
            {status?.loaded ? 'Aún no se ha publicado ninguna aplicación.' : 'Cuando haya conexión, aquí aparecerán las aplicaciones disponibles.'}
          </Empty>
        ) : (
          <div className="grid">
            {apps.map((a) => (
              <AppCard key={a.id} app={a} />
            ))}
          </div>
        )}
      </section>

      {recent.length > 0 && (
        <section>
          <div className="section-head">
            <h2 className="section-title">Actividad reciente</h2>
            <button className="linklike" onClick={() => go('history')}>
              Ver historial <ArrowRight size={13} aria-hidden />
            </button>
          </div>
          <ul className="plain">
            {recent.map((h) => (
              <li key={h.id} className="activity">
                {h.outcome === 'success' ? <CheckCircle2 size={16} className="ok" aria-hidden /> : <CircleAlert size={16} className="warn" aria-hidden />}
                <span>
                  {actionPast[h.action] ?? h.action} de <strong>{h.app_name}</strong> · {outcomeLabel[h.outcome].toLowerCase()}
                </span>
                <span className="muted">{ago(h.at)}</span>
              </li>
            ))}
          </ul>
        </section>
      )}
    </>
  )
}

// ---- Todas las aplicaciones -----------------------------------------------------------------------------

export function Apps() {
  const { apps, appsLoaded, status } = useHub()
  const [q, setQ] = useState('')
  const list = useMemo(() => {
    const s = q.trim().toLowerCase()
    return s ? apps.filter((a) => [a.name, a.summary, a.publisher].some((x) => x.toLowerCase().includes(s))) : apps
  }, [apps, q])
  return (
    <>
      <PageHeader title="Todas las aplicaciones" lead="Todo lo que ASTERION publica para tu sistema." />
      <StatusBanners />
      <label className="search">
        <Search size={17} aria-hidden />
        <input type="search" placeholder="Buscar aplicaciones" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Buscar aplicaciones" />
      </label>
      {!appsLoaded ? (
        <Loading />
      ) : list.length === 0 ? (
        <Empty icon={<PackageSearch size={28} />} title={q ? 'Sin resultados' : status?.loaded ? 'El catálogo está vacío' : 'No hay catálogo todavía'}>
          {q ? `Ninguna aplicación coincide con “${q}”.` : 'Cuando haya conexión, aquí aparecerán las aplicaciones disponibles.'}
        </Empty>
      ) : (
        <div className="grid">
          {list.map((a) => (
            <AppCard key={a.id} app={a} />
          ))}
        </div>
      )}
    </>
  )
}

// ---- Instaladas -----------------------------------------------------------------------------------------

export function Installed() {
  const { apps, appsLoaded, select } = useHub()
  const list = installedOf(apps)
  return (
    <>
      <PageHeader title="Instaladas" lead="Lo que hay en este equipo, tal como lo detecta el sistema." />
      <StatusBanners />
      {!appsLoaded ? (
        <Loading />
      ) : list.length === 0 ? (
        <Empty icon={<PackageSearch size={28} />} title="Todavía no hay aplicaciones instaladas">
          Instala una desde «Todas las aplicaciones».
        </Empty>
      ) : (
        <div className="stack">
          {list.map((a) => (
            <div key={a.id} className="row row--installed" onClick={() => select(a.id)} tabIndex={0} onKeyDown={(e) => e.key === 'Enter' && select(a.id)}>
              <AppAvatar name={a.name} size={40} />
              <div className="row__main">
                <h3>{a.name}</h3>
                <p>
                  Versión {a.installed?.version}
                  {a.installed?.scope === 'machine' ? ' · todos los usuarios' : ' · tu usuario'}
                  {a.installed?.running ? ' · en ejecución' : ''}
                </p>
                {a.installed?.location && <p className="path muted selectable">{a.installed.location}</p>}
              </div>
              <div className="row__meta">
                <StateChip app={a} />
              </div>
              <div className="row__actions">
                <PrimaryButton app={a} small />
                <SecondaryButtons app={a} small />
              </div>
            </div>
          ))}
        </div>
      )}
    </>
  )
}

// ---- Actualizaciones ------------------------------------------------------------------------------------

export function Updates() {
  const { apps, appsLoaded, run, refresh, refreshing, status } = useHub()
  const list = updatesOf(apps)
  return (
    <>
      <PageHeader
        title="Actualizaciones"
        lead={status?.loaded ? `Última comprobación: ${ago(status.fetched_at)}${status.origin === 'cache' ? ' (datos guardados)' : ''}.` : 'Aún no se ha consultado el catálogo.'}
        actions={
          <>
            <Button busy={refreshing} onClick={() => void refresh()} icon={<RefreshCw size={16} aria-hidden />}>
              Buscar ahora
            </Button>
            {list.length > 1 && (
              <Button variant="primary" onClick={() => runAll(list, run)} icon={<Download size={16} aria-hidden />}>
                Actualizar todo
              </Button>
            )}
          </>
        }
      />
      <StatusBanners />
      {!appsLoaded ? (
        <Loading />
      ) : list.length === 0 ? (
        <Empty icon={<CheckCircle2 size={28} />} title="Todo está al día">
          No hay versiones nuevas para las aplicaciones que tienes instaladas.
        </Empty>
      ) : (
        <div className="stack">
          {list.map((a) => (
            <UpdateRow key={a.id} app={a} />
          ))}
        </div>
      )}
    </>
  )
}

// ---- Descargas y tareas ---------------------------------------------------------------------------------

export function Tasks() {
  const { tasks } = useHub()
  const active = activeTasks(tasks)
  const finished = tasks.filter((t) => !active.includes(t)).reverse()
  return (
    <>
      <PageHeader
        title="Descargas y tareas"
        lead="Progreso real de cada operación."
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
          Cuando instales o actualices algo, verás aquí la descarga, la verificación y la instalación.
        </Empty>
      ) : (
        <>
          {active.length > 0 && (
            <section>
              <h2 className="section-title">En curso</h2>
              <div className="stack">
                {active.map((t) => (
                  <TaskCard key={t.id} task={t} />
                ))}
              </div>
            </section>
          )}
          {finished.length > 0 && (
            <section>
              <h2 className="section-title">Terminadas</h2>
              <div className="stack">
                {finished.map((t) => (
                  <TaskCard key={t.id} task={t} />
                ))}
              </div>
            </section>
          )}
        </>
      )}
    </>
  )
}

// ---- Historial ------------------------------------------------------------------------------------------

export function History() {
  const { history, reloadHistory } = useHub()
  const [open, setOpen] = useState<string | null>(null)
  return (
    <>
      <PageHeader
        title="Historial"
        lead="Qué se hizo, cuándo y cómo terminó."
        actions={
          history.length > 0 ? (
            <Button variant="ghost" icon={<Trash2 size={15} aria-hidden />} onClick={() => void api.clearHistory().then(reloadHistory)}>
              Borrar historial
            </Button>
          ) : undefined
        }
      />
      {history.length === 0 ? (
        <Empty icon={<HistoryIcon size={28} />} title="Aún no hay historial">
          Las instalaciones, actualizaciones, reparaciones y desinstalaciones quedarán registradas aquí.
        </Empty>
      ) : (
        <div className="table" role="table" aria-label="Historial de operaciones">
          <div className="table__head" role="row">
            <span>Fecha</span>
            <span>Operación</span>
            <span>Versión</span>
            <span>Resultado</span>
          </div>
          {history.map((h) => {
            const err = h.error_code ? describeError({ code: h.error_code, detail: h.message ?? '', retryable: false }) : null
            return (
              <div key={h.id} className="table__row" role="row">
                <button className="table__line" onClick={() => setOpen(open === h.id ? null : h.id)} aria-expanded={open === h.id}>
                  <span>{dateTime(h.at)}</span>
                  <span>
                    {actionPast[h.action] ?? h.action} · <strong>{h.app_name}</strong>
                  </span>
                  <span className="ver-arrow">{h.from_version && h.to_version && h.from_version !== h.to_version ? `${h.from_version} → ${h.to_version}` : (h.to_version ?? h.from_version ?? '—')}</span>
                  <span>
                    <Chip tone={h.outcome === 'success' ? 'ok' : h.outcome === 'failed' ? 'fdr' : 'rev'}>{outcomeLabel[h.outcome]}</Chip>
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
      )}
    </>
  )
}

// ---- Ajustes --------------------------------------------------------------------------------------------

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="field">
      <div>
        <strong>{label}</strong>
        {hint && <p>{hint}</p>}
      </div>
      <div className="field__control">{children}</div>
    </div>
  )
}

export function SettingsView() {
  const { settings, saveSettings, boot, status } = useHub()
  if (!settings || !boot) return null
  const set = (patch: Partial<Settings>) => void saveSettings({ ...settings, ...patch })
  const checks: { v: AutoCheck; l: string }[] = [
    { v: 'on_start', l: 'Al abrir el Hub' },
    { v: 'every_six_hours', l: 'Cada 6 horas (con el Hub abierto)' },
    { v: 'daily', l: 'Una vez al día (con el Hub abierto)' },
    { v: 'never', l: 'Nunca (solo manualmente)' },
  ]
  return (
    <>
      <PageHeader title="Ajustes" />
      <section className="settings">
        <Field label="Canal" hint="Beta recibe las versiones antes, con menos pruebas. Estable es lo recomendado.">
          <select value={settings.channel} onChange={(e) => set({ channel: e.target.value as Settings['channel'] })} aria-label="Canal">
            <option value="stable">Estable</option>
            <option value="beta">Beta</option>
          </select>
        </Field>
        <Field label="Buscar actualizaciones" hint="El Hub solo comprueba mientras está abierto. Las aplicaciones no dependen de él.">
          <select value={settings.auto_check} onChange={(e) => set({ auto_check: e.target.value as AutoCheck })} aria-label="Buscar actualizaciones">
            {checks.map((c) => (
              <option key={c.v} value={c.v}>
                {c.l}
              </option>
            ))}
          </select>
        </Field>
        {boot.host.os === 'windows' && (
          <Field label="Instalar para" hint="«Solo yo» no pide permisos de administrador. «Todos los usuarios» muestra el aviso de Windows (UAC).">
            <select value={settings.install_scope} onChange={(e) => set({ install_scope: e.target.value as Settings['install_scope'] })} aria-label="Instalar para">
              <option value="user">Solo yo (recomendado)</option>
              <option value="machine">Todos los usuarios (requiere administrador)</option>
            </select>
          </Field>
        )}
        <Field label="Apariencia">
          <select value={settings.theme} onChange={(e) => set({ theme: e.target.value as Settings['theme'] })} aria-label="Apariencia">
            <option value="system">Seguir al sistema</option>
            <option value="light">Claro</option>
            <option value="dark">Oscuro</option>
          </select>
        </Field>
        <Field label="Conservar instaladores" hint="Guarda los archivos descargados y verificados para reparar sin volver a descargar. Ocupan espacio.">
          <label className="switch">
            <input type="checkbox" checked={settings.keep_installers} onChange={(e) => set({ keep_installers: e.target.checked })} />
            <span>{settings.keep_installers ? 'Sí' : 'No'}</span>
          </label>
        </Field>
      </section>

      <h2 className="section-title">Acerca de</h2>
      <dl className="facts facts--wide">
        <dt>ASTERION Hub</dt>
        <dd>{boot.host.hub_version}</dd>
        <dt>Sistema</dt>
        <dd>
          {boot.host.os === 'macos' ? 'macOS' : boot.host.os === 'windows' ? 'Windows' : 'Desconocido'} {boot.host.os_version} · {boot.host.arch}
        </dd>
        <dt>Catálogo</dt>
        <dd>
          {status?.loaded ? `Canal ${status.channel}, secuencia ${status.sequence} · ${status.origin === 'cache' ? 'datos guardados' : 'al día'}` : 'Sin datos'}
        </dd>
        {status?.keyset_days_left != null && (
          <>
            <dt>Claves de publicación</dt>
            <dd>Válidas {status.keyset_days_left} días más</dd>
          </>
        )}
        <dt>Datos del Hub</dt>
        <dd className="selectable path">{boot.host.data_dir}</dd>
      </dl>
      <div className="row-actions">
        <Button icon={<FolderOpen size={15} aria-hidden />} onClick={() => void api.openLogsFolder()}>
          Abrir carpeta de registros
        </Button>
      </div>
      <p className="note">
        La autenticidad del catálogo se comprueba con una firma Ed25519 de ASTERION y cada archivo con su huella SHA-256. Eso no es una firma de código de Windows o Apple: los instaladores aún no la tienen y el sistema puede avisar de ello al ejecutarlos.
      </p>
    </>
  )
}
