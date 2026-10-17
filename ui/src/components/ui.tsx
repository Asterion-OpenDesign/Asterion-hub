import { useEffect, useRef, type ButtonHTMLAttributes, type ReactNode } from 'react'
import { Loader2, X } from 'lucide-react'

type Variant = 'primary' | 'secondary' | 'ghost' | 'danger'

interface BtnProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant
  small?: boolean
  busy?: boolean
  icon?: ReactNode
}

export function Button({ variant = 'secondary', small, busy, icon, children, className = '', disabled, ...rest }: BtnProps) {
  return (
    <button {...rest} disabled={disabled || busy} className={`btn btn--${variant} ${small ? 'btn--sm' : ''} ${className}`}>
      {busy ? <Loader2 className="spin" size={16} aria-hidden /> : icon}
      {children}
    </button>
  )
}

export type Tone = 'neutral' | 'ok' | 'rev' | 'fdr' | 'marca'

export function Chip({ tone = 'neutral', children }: { tone?: Tone; children: ReactNode }) {
  return <span className={`chip chip--${tone}`}>{children}</span>
}

export function Banner({ tone, icon, title, children, action }: { tone: 'rev' | 'fdr' | 'marca'; icon: ReactNode; title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className={`banner banner--${tone}`} role={tone === 'fdr' ? 'alert' : 'status'}>
      <span className="banner__icon" aria-hidden>
        {icon}
      </span>
      <div className="banner__body">
        <strong>{title}</strong>
        {children && <p>{children}</p>}
      </div>
      {action}
    </div>
  )
}

export function Progress({ value, indeterminate, label }: { value?: number; indeterminate?: boolean; label: string }) {
  return (
    <div className="progress" role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={indeterminate ? undefined : value}>
      <div className={`progress__bar ${indeterminate ? 'progress__bar--indeterminate' : ''}`} style={indeterminate ? undefined : { width: `${value ?? 0}%` }} />
    </div>
  )
}

export function Empty({ icon, title, children, action }: { icon: ReactNode; title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="empty">
      <div className="empty__icon" aria-hidden>
        {icon}
      </div>
      <h3>{title}</h3>
      {children && <p>{children}</p>}
      {action}
    </div>
  )
}

export function Skeleton({ h = 120 }: { h?: number }) {
  return <div className="skeleton" style={{ height: h }} aria-hidden />
}

export function PageHeader({ title, lead, actions }: { title: string; lead?: ReactNode; actions?: ReactNode }) {
  return (
    <header className="page-header">
      <div>
        <h1>{title}</h1>
        {lead && <p className="lead">{lead}</p>}
      </div>
      {actions && <div className="page-header__actions">{actions}</div>}
    </header>
  )
}

export function Modal({ title, onClose, children, footer }: { title: string; onClose: () => void; children: ReactNode; footer: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null
    ref.current?.querySelector<HTMLElement>('[data-autofocus]')?.focus()
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && onClose()
    window.addEventListener('keydown', onKey)
    return () => {
      window.removeEventListener('keydown', onKey)
      prev?.focus?.()
    }
  }, [onClose])
  return (
    <div className="scrim" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal" role="dialog" aria-modal="true" aria-label={title} ref={ref}>
        <div className="modal__head">
          <h2>{title}</h2>
          <button className="icon-btn" onClick={onClose} aria-label="Cerrar">
            <X size={18} />
          </button>
        </div>
        <div className="modal__body">{children}</div>
        <div className="modal__foot">{footer}</div>
      </div>
    </div>
  )
}
