import { FolderOpen } from 'lucide-react'
import type { ReactNode } from 'react'
import { api } from '../api'
import { Button, Head, Rotulo } from '../components/ui'
import { useHub } from '../store'
import type { AutoCheck, Settings } from '../types'

function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="field">
      <div>
        <strong>{label}</strong>
        {hint && <p>{hint}</p>}
      </div>
      <div>{children}</div>
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
      <Head eyebrow="Preferencias" title="Ajustes" />

      <section style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
        <Rotulo>Catálogo y actualizaciones</Rotulo>
        <div className="settings">
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
        </div>
      </section>

      <section style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
        <Rotulo>Instalación</Rotulo>
        <div className="settings">
          {boot.host.os === 'windows' && (
            <Field label="Instalar para" hint="«Solo yo» no pide permisos de administrador. «Todos los usuarios» muestra el aviso de Windows (UAC).">
              <select value={settings.install_scope} onChange={(e) => set({ install_scope: e.target.value as Settings['install_scope'] })} aria-label="Instalar para">
                <option value="user">Solo yo (recomendado)</option>
                <option value="machine">Todos los usuarios (requiere administrador)</option>
              </select>
            </Field>
          )}
          <Field label="Conservar instaladores" hint="Guarda los archivos descargados y verificados para reparar sin volver a descargar. Ocupan espacio.">
            <label className="switch">
              <input type="checkbox" checked={settings.keep_installers} onChange={(e) => set({ keep_installers: e.target.checked })} />
              <span>{settings.keep_installers ? 'Sí' : 'No'}</span>
            </label>
          </Field>
        </div>
      </section>

      <section style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
        <Rotulo>Apariencia</Rotulo>
        <div className="settings">
          <Field label="Tema">
            <select value={settings.theme} onChange={(e) => set({ theme: e.target.value as Settings['theme'] })} aria-label="Tema">
              <option value="dark">Oscuro</option>
              <option value="light">Claro</option>
              <option value="system">Seguir al sistema</option>
            </select>
          </Field>
        </div>
      </section>

      <section style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
        <Rotulo>Acerca de</Rotulo>
        <dl className="facts facts--box">
          <dt>ASTERION Hub</dt>
          <dd className="maq">{boot.host.hub_version}</dd>
          <dt>Sistema</dt>
          <dd>
            {boot.host.os === 'macos' ? 'macOS' : boot.host.os === 'windows' ? 'Windows' : 'Desconocido'} {boot.host.os_version} · {boot.host.arch}
          </dd>
          <dt>Catálogo</dt>
          <dd>{status?.loaded ? `Canal ${status.channel}, secuencia ${status.sequence} · ${status.origin === 'cache' ? 'datos guardados' : 'al día'}` : 'Sin datos'}</dd>
          {status?.keyset_days_left != null && (
            <>
              <dt>Claves de publicación</dt>
              <dd>Válidas {status.keyset_days_left} días más</dd>
            </>
          )}
          <dt>Datos del Hub</dt>
          <dd className="selectable path">{boot.host.data_dir}</dd>
        </dl>
        <div>
          <Button icon={<FolderOpen size={15} aria-hidden />} onClick={() => void api.openLogsFolder()}>
            Abrir carpeta de registros
          </Button>
        </div>
        <p className="note">
          La autenticidad del catálogo se comprueba con una firma Ed25519 de ASTERION y cada archivo con su huella SHA-256. Eso no es una firma de código de Windows o Apple: los instaladores aún no la tienen y el sistema puede avisar de ello al ejecutarlos.
        </p>
      </section>
    </>
  )
}
