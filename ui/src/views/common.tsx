import { CircleAlert, CloudOff, RefreshCw, ShieldAlert, WifiOff } from 'lucide-react'
import { describeError } from '../copy'
import { ago } from '../format'
import { useHub } from '../store'
import { Banner, Button, Skeleton } from '../components/ui'

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

export function Loading({ rows = 3, h = 190 }: { rows?: number; h?: number }) {
  return (
    <div className="grid">
      {Array.from({ length: rows }, (_, i) => (
        <Skeleton key={i} h={h} />
      ))}
    </div>
  )
}

export function osRequirement(os: string, min: string): string {
  const m = /^10\.0\.(\d+)$/.exec(min)
  if (os === 'Windows' && m) return Number(m[1]) >= 22000 ? `Windows 11 (compilación ${m[1]})` : `Windows 10 (compilación ${m[1]})`
  return `${os} ${min}`
}
