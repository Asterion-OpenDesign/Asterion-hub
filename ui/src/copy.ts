import type { Action, AppView, ErrorView, Phase } from './types'

export const actionLabel: Record<Action | 'open', string> = {
  install: 'Instalar',
  update: 'Actualizar',
  repair: 'Reparar',
  uninstall: 'Desinstalar',
  open: 'Abrir',
}

export const actionProgressive: Record<Action, string> = {
  install: 'Instalando',
  update: 'Actualizando',
  repair: 'Reparando',
  uninstall: 'Desinstalando',
}

export const actionPast: Record<string, string> = {
  install: 'Instalación',
  update: 'Actualización',
  repair: 'Reparación',
  uninstall: 'Desinstalación',
}

export const phaseLabel: Record<Phase, string> = {
  queued: 'En cola',
  preparing: 'Preparando',
  downloading: 'Descargando',
  verifying: 'Verificando integridad',
  waiting_for_installer: 'Esperando al instalador',
  installing: 'Instalando',
  finishing: 'Comprobando el resultado',
  done: 'Completada',
  failed: 'Con errores',
  cancelled: 'Cancelada',
}

export const outcomeLabel: Record<string, string> = {
  success: 'Completada',
  failed: 'Con errores',
  cancelled: 'Cancelada',
  interrupted: 'Interrumpida',
}

export interface ErrorCopy {
  title: string
  hint: string
}

/** User-facing text for every stable error code the engine can send. `detail` stays English, for the details panel. */
export function describeError(e: ErrorView): ErrorCopy {
  switch (e.code) {
    case 'offline':
      return { title: 'Sin conexión', hint: 'No se pudo contactar con el servidor. Tus aplicaciones instaladas siguen funcionando; vuelve a intentarlo cuando haya conexión.' }
    case 'http_status':
      return { title: 'El servidor no respondió como se esperaba', hint: 'Puede ser un fallo temporal. Inténtalo de nuevo en unos minutos.' }
    case 'download_interrupted':
      return { title: 'La descarga se interrumpió', hint: 'Lo descargado se conserva: al reintentar continúa donde se quedó.' }
    case 'url_refused':
      return { title: 'Dirección de descarga rechazada', hint: 'La dirección no cumple la política de seguridad del Hub. No se descargó nada.' }
    case 'bad_signature':
      return { title: 'No se pudo comprobar la autenticidad del catálogo', hint: 'La firma no coincide con la clave de publicación de ASTERION. Se ha descartado el contenido; no se instalará nada.' }
    case 'metadata_expired':
      return { title: 'Información caducada', hint: 'El catálogo guardado ha caducado. Conéctate a internet y actualiza antes de instalar.' }
    case 'metadata_rollback':
      return { title: 'Catálogo más antiguo que el ya conocido', hint: 'El Hub rechaza versiones anteriores del catálogo para protegerte de versiones retiradas.' }
    case 'invalid_metadata':
      return { title: 'Catálogo no válido', hint: 'El catálogo recibido no cumple el formato esperado. Se ha descartado.' }
    case 'unsupported_schema':
      return { title: 'Este Hub es demasiado antiguo para el catálogo', hint: 'Actualiza ASTERION Hub para ver las aplicaciones nuevas.' }
    case 'size_mismatch':
      return { title: 'El archivo descargado no tiene el tamaño esperado', hint: 'Se ha eliminado. Vuelve a intentarlo.' }
    case 'hash_mismatch':
      return { title: 'La integridad del archivo no se pudo verificar', hint: 'El archivo no coincide con su huella SHA-256. Se ha eliminado y no se ejecutó.' }
    case 'unexpected_package':
      return { title: 'El paquete no es el esperado', hint: 'El contenido no coincide con lo que anuncia el catálogo. No se ejecutó.' }
    case 'no_build_for_os':
      return { title: 'No disponible para este sistema operativo', hint: 'Esta aplicación aún no tiene versión para tu sistema.' }
    case 'no_build_for_arch':
      return { title: 'No disponible para esta arquitectura', hint: 'No hay una versión para el procesador de este equipo.' }
    case 'os_too_old':
      return { title: 'Tu sistema operativo es demasiado antiguo', hint: 'La versión disponible necesita una versión más reciente del sistema.' }
    case 'hub_too_old':
      return { title: 'Necesita un ASTERION Hub más reciente', hint: 'Actualiza el Hub para poder instalar esta versión.' }
    case 'no_release':
      return { title: 'Sin versiones publicadas', hint: 'Esta aplicación todavía no tiene ninguna versión disponible.' }
    case 'dependency_missing':
      return { title: 'Falta una dependencia', hint: `Instala primero: ${e.detail.replace(/^missing dependency: /, '')}` }
    case 'disk_space':
      return { title: 'No hay espacio suficiente en disco', hint: 'Libera espacio y vuelve a intentarlo.' }
    case 'app_running':
      return { title: 'La aplicación está abierta', hint: 'Ciérrala para continuar: el Hub no reemplaza archivos que se están usando.' }
    case 'installer_busy':
      return { title: 'Ya hay otra instalación en curso', hint: 'Espera a que termine y reintenta.' }
    case 'install_cancelled':
      return { title: 'Instalación cancelada', hint: 'Se canceló desde el instalador del sistema. Nada ha cambiado.' }
    case 'installer_failed':
      return { title: 'El instalador terminó con un error', hint: 'Windows revirtió los cambios: la versión anterior sigue intacta. Los detalles técnicos están en el registro.' }
    case 'not_installed':
      return { title: 'La aplicación no está instalada', hint: 'No se encontró en este equipo.' }
    case 'newer_installed':
      return { title: 'Ya hay una versión más reciente instalada', hint: 'No se bajará de versión automáticamente.' }
    case 'cancelled':
      return { title: 'Cancelada', hint: 'Lo descargado se conserva por si quieres continuar.' }
    case 'unsupported':
      return { title: 'Operación no disponible', hint: e.detail.replace(/^operation not supported on this system: /, '') }
    case 'unknown_app':
      return { title: 'Aplicación desconocida', hint: 'El catálogo ya no incluye esta aplicación.' }
    case 'interrupted':
      return { title: 'Interrumpida', hint: 'ASTERION Hub se cerró mientras la operación estaba en curso. Comprueba el estado de la aplicación.' }
    default:
      return { title: 'Algo salió mal', hint: 'Los detalles técnicos están en el registro del Hub.' }
  }
}

export function stateLabel(a: AppView): string {
  switch (a.state) {
    case 'installed':
      return 'Instalada'
    case 'update_available':
      return 'Actualización disponible'
    case 'not_installed':
      return 'Disponible'
    case 'unavailable':
      return 'No disponible'
  }
}

export const nav: { id: import('./types').Page; label: string }[] = [
  { id: 'home', label: 'Inicio' },
  { id: 'apps', label: 'Todas las aplicaciones' },
  { id: 'installed', label: 'Instaladas' },
  { id: 'updates', label: 'Actualizaciones' },
  { id: 'tasks', label: 'Descargas y tareas' },
  { id: 'history', label: 'Historial' },
  { id: 'settings', label: 'Ajustes' },
]
