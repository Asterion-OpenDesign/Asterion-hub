import { useEffect, useRef, useState, type ButtonHTMLAttributes, type ReactNode } from 'react'
import { Loader2, X } from 'lucide-react'

type Variant = 'primary' | 'accent' | 'inverse' | 'secondary' | 'ghost' | 'danger'

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

export function Pill({ tone = 'neutral', dot = true, children }: { tone?: Tone; dot?: boolean; children: ReactNode }) {
  return (
    <span className={`pill pill--${tone}`}>
      {dot && <span className="dot" aria-hidden />}
      {children}
    </span>
  )
}

export function Chip({ children }: { children: ReactNode }) {
  return <span className="chip">{children}</span>
}

export function Rotulo({ children, marca }: { children: ReactNode; marca?: boolean }) {
  return <div className={`rotulo ${marca ? 'rotulo--marca' : ''}`}>{children}</div>
}

export function Banner({ tone, icon, title, children, action }: { tone: 'rev' | 'fdr' | 'marca'; icon: ReactNode; title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className={`banner banner--${tone}`} role={tone === 'fdr' ? 'alert' : 'status'}>
      <span aria-hidden>{icon}</span>
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

export function Head({ eyebrow, title, lead, actions, display }: { eyebrow?: ReactNode; title: ReactNode; lead?: ReactNode; actions?: ReactNode; display?: boolean }) {
  return (
    <header className="head">
      <div className="head__text">
        {eyebrow && <Rotulo>{eyebrow}</Rotulo>}
        <h1 className={display ? 'display' : 'title'}>{title}</h1>
        {lead && <p className="lede">{lead}</p>}
      </div>
      {actions && <div className="head__actions">{actions}</div>}
    </header>
  )
}

export function Section({ title, count, action, children }: { title: string; count?: number; action?: ReactNode; children: ReactNode }) {
  return (
    <section>
      <div className="section-head">
        <h2 className="section-title">
          {title}
          {count != null && <span className="n">{count}</span>}
        </h2>
        {action}
      </div>
      {children}
    </section>
  )
}

export function Segmented<T extends string>({ value, onChange, options, label }: { value: T; onChange: (v: T) => void; options: { value: T; label: string; count?: number }[]; label: string }) {
  return (
    <div className="seg" role="group" aria-label={label}>
      {options.map((o) => (
        <button key={o.value} aria-pressed={value === o.value} onClick={() => onChange(o.value)}>
          {o.label}
          {o.count != null && <span className="n">{o.count}</span>}
        </button>
      ))}
    </div>
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
      <div className="modal floating" role="dialog" aria-modal="true" aria-label={title} ref={ref}>
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

/** A small popover menu opened by a button; closes on outside click and Escape. */
export function Menu({ trigger, children }: { trigger: (props: { onClick: () => void; 'aria-expanded': boolean; 'aria-haspopup': 'menu' }) => ReactNode; children: (close: () => void) => ReactNode }) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!open) return
    const down = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && setOpen(false)
    const key = (e: KeyboardEvent) => e.key === 'Escape' && setOpen(false)
    window.addEventListener('mousedown', down)
    window.addEventListener('keydown', key)
    return () => {
      window.removeEventListener('mousedown', down)
      window.removeEventListener('keydown', key)
    }
  }, [open])
  return (
    <div className="pos" ref={ref} onClick={(e) => e.stopPropagation()}>
      {trigger({ onClick: () => setOpen(!open), 'aria-expanded': open, 'aria-haspopup': 'menu' })}
      {open && (
        <div className="menu floating" role="menu">
          {children(() => setOpen(false))}
        </div>
      )}
    </div>
  )
}
