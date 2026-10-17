import { ArrowRight, CheckCircle2, CircleAlert, Download, PackageSearch } from 'lucide-react'
import { sortApps } from '../catalog'
import { CatalogRow, LaunchCard, UpdateSlab } from '../components/apps'
import { TaskCard } from '../components/TaskCard'
import { Button, Empty, Head, Section } from '../components/ui'
import { actionPast, outcomeLabel } from '../copy'
import { ago } from '../format'
import { activeTasks, installedOf, updatesOf, useHub } from '../store'
import { Loading, StatusBanners } from './common'

export function Home() {
  const { apps, appsLoaded, status, tasks, history, go, updateAll } = useHub()
  const updates = updatesOf(apps)
  const installed = installedOf(apps)
  const active = activeTasks(tasks)
  const discover = sortApps(apps.filter((a) => a.state === 'not_installed'), 'recent').slice(0, 4)
  const available = apps.filter((a) => a.state === 'not_installed').length
  const recent = history.slice(0, 4)

  const title = !status?.loaded
    ? 'Tus aplicaciones, en un solo lugar.'
    : updates.length > 0
      ? updates.length === 1
        ? 'Hay 1 actualización lista.'
        : `Hay ${updates.length} actualizaciones listas.`
      : installed.length > 0
        ? 'Todo está al día.'
        : 'Instala tu primera aplicación.'
  const lead = !status?.loaded
    ? 'Descubre, instala y mantén al día el software de ASTERION sin salir de aquí.'
    : updates.length > 0
      ? 'Actualiza ahora o cuando te venga bien: tus aplicaciones siguen funcionando mientras tanto.'
      : installed.length > 0
        ? `${installed.length === 1 ? 'Tu aplicación está' : `Tus ${installed.length} aplicaciones están`} en la última versión disponible para este equipo.`
        : 'Elige una del catálogo: se descarga, se verifica y se instala sin salir de aquí.'

  return (
    <>
      <Head
        display
        eyebrow="ASTERION Hub"
        title={title}
        lead={lead}
        actions={
          updates.length > 1 ? (
            <Button variant="primary" icon={<Download size={16} aria-hidden />} onClick={() => void updateAll()}>
              Actualizar todo
            </Button>
          ) : undefined
        }
      />
      <StatusBanners />

      {active.length > 0 && (
        <Section title="En curso" count={active.length}>
          <div className="stack">
            {active.map((t) => (
              <TaskCard key={t.id} task={t} />
            ))}
          </div>
        </Section>
      )}

      {updates.length > 0 && (
        <Section title="Actualizaciones" count={updates.length}>
          <div className="stack">
            {updates.map((a) => (
              <UpdateSlab key={a.id} app={a} />
            ))}
          </div>
        </Section>
      )}

      <Section
        title="Tus aplicaciones"
        count={installed.length}
        action={
          installed.length > 0 && (
            <button className="linklike" onClick={() => go('installed')}>
              Ver todas <ArrowRight size={13} aria-hidden />
            </button>
          )
        }
      >
        {!appsLoaded ? (
          <Loading rows={3} h={150} />
        ) : installed.length === 0 ? (
          <Empty icon={<PackageSearch size={28} />} title="Aún no hay nada instalado" action={available > 0 ? <Button variant="primary" onClick={() => go('apps')}>Explorar el catálogo</Button> : undefined}>
            {status?.loaded ? 'Las aplicaciones que instales aparecerán aquí para abrirlas de un clic.' : 'Cuando haya conexión podrás ver el catálogo.'}
          </Empty>
        ) : (
          <div className="grid grid--tiles">
            {sortApps(installed, 'status').map((a) => (
              <LaunchCard key={a.id} app={a} />
            ))}
          </div>
        )}
      </Section>

      {discover.length > 0 && (
        <Section
          title="Para descubrir"
          action={
            <button className="linklike" onClick={() => go('apps')}>
              Ver el catálogo ({apps.filter((a) => a.state !== 'unavailable').length}) <ArrowRight size={13} aria-hidden />
            </button>
          }
        >
          <div className="lamina">
            {discover.map((a) => (
              <CatalogRow key={a.id} app={a} />
            ))}
          </div>
        </Section>
      )}

      {recent.length > 0 && (
        <Section
          title="Actividad reciente"
          action={
            <button className="linklike" onClick={() => go('history')}>
              Historial <ArrowRight size={13} aria-hidden />
            </button>
          }
        >
          <div className="lamina">
            {recent.map((h) => (
              <div key={h.id} className="activity">
                {h.outcome === 'success' ? <CheckCircle2 size={16} className="ok" aria-hidden /> : <CircleAlert size={16} className="warn" aria-hidden />}
                <span>
                  {actionPast[h.action] ?? h.action} de <strong>{h.app_name}</strong> · {outcomeLabel[h.outcome].toLowerCase()}
                </span>
                <span className="muted">{ago(h.at)}</span>
              </div>
            ))}
          </div>
        </Section>
      )}
    </>
  )
}
