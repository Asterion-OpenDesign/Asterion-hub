import { ExternalLink, MinusCircle, ShieldCheck } from 'lucide-react'
import { api } from '../api'
import { categoryOf } from '../catalog'
import { MoreMenu, PrimaryButton, StatePill, Tile, Ver } from '../components/apps'
import { TaskCard } from '../components/TaskCard'
import { Button, Chip, Empty, Pill, Rotulo } from '../components/ui'
import { describeError } from '../copy'
import { bytes, day } from '../format'
import { useHub } from '../store'
import type { AppTab } from '../types'
import { osRequirement } from './common'

export function AppPage() {
  const { apps, route, setTab, tasks, boot, back } = useHub()
  const app = apps.find((a) => a.id === route.appId)
  const tab: AppTab = route.tab ?? 'resumen'
  if (!app) {
    return (
      <Empty icon={<MinusCircle size={28} />} title="Esta aplicación ya no está en el catálogo" action={<Button onClick={back}>Volver</Button>}>
        Puede que se haya retirado. Si la tenías instalada, sigue en «Instaladas».
      </Empty>
    )
  }
  const task = tasks.find((t) => t.id === app.active_task) ?? tasks.filter((t) => t.app_id === app.id).at(-1)
  const showTask = !!task && (!!app.active_task || task.phase === 'failed' || task.phase === 'cancelled')
  const latest = app.latest
  const os = boot?.host.os === 'macos' ? 'macOS' : 'Windows'
  const paragraphs = app.description.split(/\n{2,}/).filter(Boolean)

  return (
    <>
      <div className="hero">
        <Tile app={app} size={76} />
        <div className="hero__text">
          <Rotulo marca={app.state === 'update_available'}>{app.category ? categoryOf(app) : 'Aplicación'}</Rotulo>
          <h1 className="title" style={{ fontSize: 40, letterSpacing: '-0.034em' }}>
            {app.name}
          </h1>
          <div className="hero__meta">
            <StatePill app={app} />
            {app.installed?.running && <Pill tone="ok">En ejecución</Pill>}
            {app.orphaned && <Pill tone="rev">Ya no está en el catálogo</Pill>}
            <span className="muted small">
              {app.publisher}
              {app.license ? ` · ${app.license}` : ''}
            </span>
          </div>
        </div>
        <div className="hero__actions">
          <PrimaryButton app={app} variant="primary" />
          <MoreMenu app={app} small={false} />
        </div>
      </div>
      {app.installed?.running && app.actions.length > 0 && <p className="note">Ciérrala para actualizar, reparar o desinstalar: el Hub no reemplaza archivos que se están usando.</p>}

      {showTask && task && <TaskCard task={task} />}

      <div className="tabs" role="tablist">
        {(
          [
            ['resumen', 'Resumen'],
            ['novedades', `Novedades${app.releases.length ? ` · ${app.releases.length}` : ''}`],
            ['requisitos', 'Requisitos y seguridad'],
          ] as [AppTab, string][]
        ).map(([id, label]) => (
          <button key={id} role="tab" aria-selected={tab === id} onClick={() => setTab(id)}>
            {label}
          </button>
        ))}
      </div>

      <div className="page-cols">
        <div style={{ display: 'flex', flexDirection: 'column', gap: 26, minWidth: 0 }}>
          {tab === 'resumen' && (
            <>
              <div className="prose">
                {paragraphs.length ? paragraphs.map((p, i) => <p key={i}>{p}</p>) : <p>{app.summary || 'Sin descripción publicada.'}</p>}
              </div>
              {app.tags.length > 0 && (
                <div className="chips">
                  {app.tags.map((t) => (
                    <Chip key={t}>{t}</Chip>
                  ))}
                </div>
              )}
              {latest && latest.notes.length > 0 && (
                <div>
                  <Rotulo>Novedades de la {latest.version}</Rotulo>
                  <ul className="notes">
                    {latest.notes.map((n, i) => (
                      <li key={i}>{n}</li>
                    ))}
                  </ul>
                </div>
              )}
              {app.unavailable && (
                <p className="note note--rev">
                  <strong>{describeError(app.unavailable).title}.</strong> {describeError(app.unavailable).hint}
                </p>
              )}
              {app.newer_blocked?.blocked && (
                <p className="note note--rev">
                  <strong>La versión {app.newer_blocked.version} no se puede instalar aquí: {describeError(app.newer_blocked.blocked).title.toLowerCase()}.</strong> {describeError(app.newer_blocked.blocked).hint}
                </p>
              )}
            </>
          )}

          {tab === 'novedades' &&
            (app.releases.length === 0 ? (
              <Empty icon={<MinusCircle size={28} />} title="Sin versiones publicadas" />
            ) : (
              <div className="releases">
                {app.releases.map((r) => (
                  <div key={r.version} className="release">
                    <div className="release__v">
                      <strong className="maq">{r.version}</strong>
                      <span className="muted small">{day(r.released_at)}</span>
                      {app.installed?.version === r.version && <Pill tone="ok">Instalada</Pill>}
                      {!r.installable && <Pill dot={false}>No disponible aquí</Pill>}
                    </div>
                    {r.notes.length > 0 ? (
                      <ul>
                        {r.notes.map((n, i) => (
                          <li key={i}>{n}</li>
                        ))}
                      </ul>
                    ) : (
                      <p className="muted">Sin notas publicadas.</p>
                    )}
                  </div>
                ))}
              </div>
            ))}

          {tab === 'requisitos' && (
            <>
              <dl className="facts">
                <dt>Sistema</dt>
                <dd>{latest?.os_min ? `${osRequirement(os, latest.os_min)} o posterior` : os}</dd>
                <dt>Descarga</dt>
                <dd>{latest?.size != null ? bytes(latest.size) : '—'}</dd>
                <dt>Espacio en disco</dt>
                <dd>{latest?.installed_size_mb != null ? `≈ ${latest.installed_size_mb} MB` : 'No indicado'}</dd>
                <dt>Necesita</dt>
                <dd>{latest?.requires.length ? latest.requires.join(', ') : 'Nada más'}</dd>
              </dl>
              <div className="prose">
                <p className="note">
                  <ShieldCheck size={16} aria-hidden />
                  <span>
                    Cada descarga se verifica con su huella SHA-256 y con el catálogo firmado antes de ejecutarse. Al desinstalar, el Hub no borra tus proyectos, documentos ni ajustes.
                  </span>
                </p>
                <p className="note">
                  <ShieldCheck size={16} aria-hidden />
                  <span>
                    Los instaladores de ASTERION aún no están firmados con certificados de Windows o Apple, así que el sistema puede mostrar su propio aviso al ejecutarlos. El Hub nunca desactiva esas protecciones.
                  </span>
                </p>
              </div>
            </>
          )}
        </div>

        <aside className="aside" aria-label="Ficha">
          <Rotulo>Ficha</Rotulo>
          <dl className="facts">
            {app.installed && (
              <>
                <dt>Instalada</dt>
                <dd>
                  <Ver from={app.installed.version} to={app.installed.version} /> · {app.installed.scope === 'user' ? 'tu usuario' : 'todos los usuarios'}
                </dd>
              </>
            )}
            {latest && (
              <>
                <dt>Última versión</dt>
                <dd>
                  <Ver from={latest.version} to={latest.version} /> · {day(latest.released_at)}
                </dd>
              </>
            )}
            {app.installed?.installed_by_hub != null && app.installed && (
              <>
                <dt>Origen</dt>
                <dd>{app.installed.installed_by_hub ? 'Instalada con ASTERION Hub' : 'Detectada en este equipo'}</dd>
              </>
            )}
            {app.installed?.location && (
              <>
                <dt>Ubicación</dt>
                <dd className="selectable path">{app.installed.location}</dd>
              </>
            )}
            <dt>Publicador</dt>
            <dd>{app.publisher || '—'}</dd>
            {app.license && (
              <>
                <dt>Licencia</dt>
                <dd>{app.license}</dd>
              </>
            )}
          </dl>
          {app.homepage && (
            <button className="linklike" onClick={() => void api.openLink(app.homepage!)}>
              Sitio web <ExternalLink size={13} aria-hidden />
            </button>
          )}
        </aside>
      </div>
    </>
  )
}
