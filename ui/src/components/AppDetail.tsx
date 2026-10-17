import { ExternalLink, ShieldCheck, X } from 'lucide-react'
import { api } from '../api'
import { describeError } from '../copy'
import { bytes, day } from '../format'
import { useHub } from '../store'
import { AppAvatar, PrimaryButton, SecondaryButtons, StateChip } from './apps'
import { TaskCard } from './TaskCard'
import { Chip } from './ui'

function osRequirement(os: string, min: string): string {
  const m = /^10\.0\.(\d+)$/.exec(min)
  if (os === 'Windows' && m) return Number(m[1]) >= 22000 ? `Windows 11 (compilación ${m[1]})` : `Windows 10 (compilación ${m[1]})`
  return `${os} ${min}`
}

export function AppDetail() {
  const { apps, selected, select, tasks, boot } = useHub()
  const app = apps.find((a) => a.id === selected)
  if (!app) return null
  const task = tasks.find((t) => t.id === app.active_task) ?? tasks.filter((t) => t.app_id === app.id).at(-1)
  const showTask = task && (app.active_task || task.phase === 'failed' || task.phase === 'cancelled')
  const latest = app.latest
  const os = boot?.host.os === 'macos' ? 'macOS' : 'Windows'

  return (
    <div className="scrim scrim--drawer" onMouseDown={(e) => e.target === e.currentTarget && select(null)}>
      <aside className="drawer" role="dialog" aria-modal="true" aria-label={app.name}>
        <button className="icon-btn drawer__close" onClick={() => select(null)} aria-label="Cerrar">
          <X size={18} />
        </button>

        <div className="drawer__hero">
          <AppAvatar name={app.name} size={64} />
          <div>
            <h2>{app.name}</h2>
            <p className="muted">{app.publisher || 'Publicador desconocido'}</p>
            <div className="drawer__chips">
              <StateChip app={app} />
              {app.installed?.running && <Chip tone="rev">En ejecución</Chip>}
              {app.orphaned && <Chip tone="rev">Ya no está en el catálogo</Chip>}
            </div>
          </div>
        </div>

        <div className="drawer__actions">
          <PrimaryButton app={app} />
          <SecondaryButtons app={app} />
        </div>
        {app.installed?.running && app.actions.length > 0 && <p className="muted small">Ciérrala para actualizar, reparar o desinstalar: el Hub no reemplaza archivos que se están usando.</p>}

        {showTask && task && <TaskCard task={task} />}

        {app.description && <p className="drawer__desc">{app.description}</p>}

        <section>
          <h3 className="section-title">Detalles</h3>
          <dl className="facts">
            {app.installed && (
              <>
                <dt>Versión instalada</dt>
                <dd>{app.installed.version}</dd>
                <dt>Instalada para</dt>
                <dd>{app.installed.scope === 'user' ? 'Solo tu usuario' : 'Todos los usuarios'}</dd>
                {app.installed.location && (
                  <>
                    <dt>Ubicación</dt>
                    <dd className="selectable path">{app.installed.location}</dd>
                  </>
                )}
                <dt>Origen</dt>
                <dd>{app.installed.installed_by_hub ? 'Instalada con ASTERION Hub' : 'Detectada en este equipo'}</dd>
              </>
            )}
            {latest && (
              <>
                <dt>Última versión para {os}</dt>
                <dd>
                  {latest.version} · {day(latest.released_at)}
                </dd>
                {latest.size != null && (
                  <>
                    <dt>Descarga</dt>
                    <dd>{bytes(latest.size)}</dd>
                  </>
                )}
                {latest.installed_size_mb != null && (
                  <>
                    <dt>Espacio en disco</dt>
                    <dd>≈ {latest.installed_size_mb} MB</dd>
                  </>
                )}
                {latest.os_min && (
                  <>
                    <dt>Requisitos</dt>
                    <dd>{osRequirement(os, latest.os_min)} o posterior</dd>
                  </>
                )}
                {latest.requires.length > 0 && (
                  <>
                    <dt>Necesita</dt>
                    <dd>{latest.requires.join(', ')}</dd>
                  </>
                )}
              </>
            )}
            {app.license && (
              <>
                <dt>Licencia</dt>
                <dd>{app.license}</dd>
              </>
            )}
          </dl>
          {app.unavailable && (
            <p className="note note--rev">
              <strong>{describeError(app.unavailable).title}.</strong> {describeError(app.unavailable).hint}
            </p>
          )}
          {app.newer_blocked?.blocked && (
            <p className="note note--rev">
              <strong>
                La versión {app.newer_blocked.version} no se puede instalar aquí: {describeError(app.newer_blocked.blocked).title.toLowerCase()}.
              </strong>{' '}
              {describeError(app.newer_blocked.blocked).hint}
            </p>
          )}
          <p className="note">
            <ShieldCheck size={15} aria-hidden /> Cada descarga se verifica con su huella SHA-256 y con el catálogo firmado antes de ejecutarse. Los instaladores de ASTERION aún no están firmados con certificados de Windows o Apple, así que el sistema puede mostrar su propio aviso.
          </p>
          {app.homepage && (
            <button className="linklike" onClick={() => void api.openLink(app.homepage!)}>
              Sitio web <ExternalLink size={13} aria-hidden />
            </button>
          )}
        </section>

        {app.releases.length > 0 && (
          <section>
            <h3 className="section-title">Novedades</h3>
            <ol className="releases">
              {app.releases.map((r) => (
                <li key={r.version}>
                  <div className="releases__head">
                    <strong>{r.version}</strong>
                    <span className="muted">{day(r.released_at)}</span>
                    {app.installed?.version === r.version && <Chip tone="ok">Instalada</Chip>}
                    {!r.installable && <Chip>No disponible aquí</Chip>}
                  </div>
                  {r.notes.length > 0 ? (
                    <ul>
                      {r.notes.map((n, i) => (
                        <li key={i}>{n}</li>
                      ))}
                    </ul>
                  ) : (
                    <p className="muted small">Sin notas publicadas.</p>
                  )}
                </li>
              ))}
            </ol>
          </section>
        )}
      </aside>
    </div>
  )
}
