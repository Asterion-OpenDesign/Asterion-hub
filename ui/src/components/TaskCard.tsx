import { useState } from 'react'
import { AlertTriangle, CheckCircle2, ChevronDown, RotateCcw, X, XCircle } from 'lucide-react'
import { actionProgressive, describeError, phaseLabel } from '../copy'
import { bytes, eta, percent, speed } from '../format'
import { useHub } from '../store'
import type { TaskView } from '../types'
import { AppAvatar } from './apps'
import { Button, Progress } from './ui'

export function TaskCard({ task }: { task: TaskView }) {
  const { cancel, retry, dismiss } = useHub()
  const [details, setDetails] = useState(false)
  const final = task.phase === 'done' || task.phase === 'failed' || task.phase === 'cancelled'
  const downloading = task.phase === 'downloading' && task.bytes_total > 0
  const pct = percent(task.bytes_done, task.bytes_total)
  const err = task.error ? describeError(task.error) : null

  return (
    <article className={`task task--${task.phase}`}>
      <AppAvatar name={task.app_name} size={40} />
      <div className="task__main">
        <div className="task__head">
          <h3>
            {actionProgressive[task.action]} {task.app_name}
            {task.version && <span className="task__ver"> {task.version}</span>}
          </h3>
          <span className="task__phase">
            {task.phase === 'done' && <CheckCircle2 size={15} aria-hidden />}
            {task.phase === 'failed' && <XCircle size={15} aria-hidden />}
            {task.phase === 'cancelled' && <AlertTriangle size={15} aria-hidden />}
            {phaseLabel[task.phase]}
          </span>
        </div>

        {!final && <Progress label={phaseLabel[task.phase]} value={pct} indeterminate={!downloading} />}
        {downloading && (
          <p className="task__stats">
            <span>
              {bytes(task.bytes_done)} de {bytes(task.bytes_total)} · {pct} %
            </span>
            <span>{[speed(task.bytes_per_sec), eta(task.bytes_done, task.bytes_total, task.bytes_per_sec) && `quedan ${eta(task.bytes_done, task.bytes_total, task.bytes_per_sec)}`].filter(Boolean).join(' · ')}</span>
          </p>
        )}
        {task.phase === 'installing' && <p className="task__stats">El instalador del sistema está trabajando. No se puede cancelar a mitad: Windows revierte los cambios si algo falla.</p>}
        {task.phase === 'done' && task.reboot_required && <p className="task__stats">Hace falta reiniciar el equipo para terminar.</p>}

        {err && (
          <div className="task__error">
            <strong>{err.title}</strong>
            <p>{err.hint}</p>
            <button className="linklike" onClick={() => setDetails(!details)} aria-expanded={details}>
              Detalles técnicos <ChevronDown size={14} className={details ? 'flip' : ''} aria-hidden />
            </button>
            {details && <pre className="selectable">{task.error?.detail}</pre>}
          </div>
        )}
      </div>
      <div className="task__actions">
        {!final && task.cancellable && (
          <Button small variant="secondary" onClick={() => void cancel(task.id)} icon={<X size={15} aria-hidden />}>
            Cancelar
          </Button>
        )}
        {(task.phase === 'failed' || task.phase === 'cancelled') && (
          <Button small variant="primary" onClick={() => void retry(task)} icon={<RotateCcw size={15} aria-hidden />}>
            Reintentar
          </Button>
        )}
        {final && (
          <Button small variant="ghost" onClick={() => void dismiss(task.id)}>
            Descartar
          </Button>
        )}
      </div>
    </article>
  )
}
