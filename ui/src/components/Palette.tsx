import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { ArrowRight, CornerDownLeft, Download, RefreshCw, Search, SunMoon } from 'lucide-react'
import { matches, matchText, sortApps } from '../catalog'
import { nav } from '../copy'
import { useHub, updatesOf } from '../store'
import { NameTile, primaryFor, useDo } from './apps'

interface Item {
  id: string
  section: 'Aplicaciones' | 'Acciones' | 'Ir a'
  label: string
  hint?: string
  icon?: ReactNode
  run: () => void
  alt?: () => void
}

/** Ctrl+K: every app and every action reachable from the keyboard, which is what makes a long catalog bearable. */
export function Palette() {
  const open = useHub((s) => s.palette)
  if (!open) return null
  return <PaletteBody />
}

function PaletteBody() {
  const { apps, setPalette, openApp, go, refresh, updateAll, settings, saveSettings } = useHub()
  const doIt = useDo()
  const [q, setQ] = useState('')
  const [sel, setSel] = useState(0)
  const listRef = useRef<HTMLDivElement>(null)
  const close = () => setPalette(false)

  const items = useMemo<Item[]>(() => {
    const out: Item[] = []
    const found = sortApps(apps.filter((a) => a.state !== 'unavailable' && matches(a, q)), 'name').slice(0, q ? 8 : 5)
    for (const a of found) {
      const p = primaryFor(a)
      out.push({
        id: `app:${a.id}`,
        section: 'Aplicaciones',
        label: a.name,
        hint: p && !p.disabled ? p.label : a.state === 'installed' ? 'Instalada' : undefined,
        icon: <NameTile name={a.name} size={28} />,
        run: () => openApp(a.id),
        alt: p && !p.disabled ? () => (close(), doIt(a, p.kind)) : undefined,
      })
    }
    const actions: Item[] = [
      { id: 'act:refresh', section: 'Acciones', label: 'Buscar actualizaciones', icon: <RefreshCw size={16} />, run: () => (close(), void refresh()) },
      ...(updatesOf(apps).length > 0 ? [{ id: 'act:all', section: 'Acciones' as const, label: `Actualizar todo (${updatesOf(apps).length})`, icon: <Download size={16} />, run: () => (close(), void updateAll()) }] : []),
      {
        id: 'act:theme',
        section: 'Acciones',
        label: 'Cambiar entre tema claro y oscuro',
        icon: <SunMoon size={16} />,
        run: () => {
          if (!settings) return
          const dark = document.documentElement.dataset.theme === 'dark'
          void saveSettings({ ...settings, theme: dark ? 'light' : 'dark' })
          close()
        },
      },
    ]
    const pages: Item[] = nav.map((n) => ({ id: `go:${n.id}`, section: 'Ir a', label: n.label, icon: <ArrowRight size={16} />, run: () => (close(), go(n.id)) }))
    const keep = (i: Item) => matchText(i.label, q)
    return [...out, ...actions.filter(keep), ...pages.filter(keep)]
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [apps, q, settings])

  useEffect(() => setSel(0), [q])
  useEffect(() => {
    listRef.current?.querySelector('[aria-selected="true"]')?.scrollIntoView({ block: 'nearest' })
  }, [sel])

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') (e.preventDefault(), setSel((s) => Math.min(items.length - 1, s + 1)))
    else if (e.key === 'ArrowUp') (e.preventDefault(), setSel((s) => Math.max(0, s - 1)))
    else if (e.key === 'Enter') {
      e.preventDefault()
      const it = items[sel]
      if (!it) return
      if ((e.ctrlKey || e.metaKey) && it.alt) it.alt()
      else it.run()
    }
  }

  let last = ''
  return (
    <div className="scrim scrim--top" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className="palette floating" role="dialog" aria-modal="true" aria-label="Buscar">
        <div className="palette__input">
          <Search size={18} aria-hidden />
          <input autoFocus value={q} onChange={(e) => setQ(e.target.value)} onKeyDown={onKey} placeholder="Busca una aplicación, una acción o una sección" aria-label="Buscar" />
          <kbd>Esc</kbd>
        </div>
        <div className="palette__list" ref={listRef} role="listbox">
          {items.length === 0 && <p className="palette__sec">Sin resultados para «{q}»</p>}
          {items.map((it, i) => {
            const head = it.section !== last ? ((last = it.section), <div className="palette__sec">{it.section}</div>) : null
            return (
              <div key={it.id}>
                {head}
                <button className="palette__item" role="option" aria-selected={sel === i} onMouseMove={() => setSel(i)} onClick={() => it.run()}>
                  {it.icon}
                  <span className="grow">{it.label}</span>
                  {it.hint && <small>{it.hint}</small>}
                </button>
              </div>
            )
          })}
        </div>
        <div className="palette__foot">
          <span>
            <kbd>↑</kbd> <kbd>↓</kbd> moverse
          </span>
          <span>
            <CornerDownLeft size={12} style={{ display: 'inline' }} aria-hidden /> abrir
          </span>
          <span>
            <kbd>Ctrl</kbd> + <CornerDownLeft size={12} style={{ display: 'inline' }} aria-hidden /> instalar, actualizar o abrir la aplicación
          </span>
        </div>
      </div>
    </div>
  )
}
