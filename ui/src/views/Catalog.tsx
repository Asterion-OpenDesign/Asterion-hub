import { useMemo } from 'react'
import { LayoutGrid, List, PackageSearch, Search, X } from 'lucide-react'
import { categoriesOf, filterApps, groupByCategory, sortApps, statusCounts, type SortKey } from '../catalog'
import { AppCard, CatalogRow } from '../components/apps'
import { Button, Empty, Head, Segmented } from '../components/ui'
import { unavailableOf, useHub } from '../store'
import { Loading, StatusBanners } from './common'

const sorts: { value: SortKey; label: string }[] = [
  { value: 'name', label: 'Nombre' },
  { value: 'recent', label: 'Más recientes' },
  { value: 'status', label: 'Estado' },
  { value: 'category', label: 'Categoría' },
]

export function Catalog() {
  const { apps, appsLoaded, status, prefs, setPrefs, query, setQuery, statusFilter, setStatusFilter, category, setCategory, clearFilters } = useHub()

  const list = useMemo(() => sortApps(filterApps(apps, { q: query, status: statusFilter, category, showUnavailable: prefs.showUnavailable }), prefs.sort), [apps, query, statusFilter, category, prefs.showUnavailable, prefs.sort])
  const cats = useMemo(() => categoriesOf(apps, prefs.showUnavailable), [apps, prefs.showUnavailable])
  const counts = useMemo(() => statusCounts(apps, prefs.showUnavailable), [apps, prefs.showUnavailable])
  const hidden = unavailableOf(apps).length
  const filtered = !!query || statusFilter !== 'all' || !!category
  const groups = prefs.sort === 'category' ? groupByCategory(list) : [{ name: '', apps: list }]

  const renderItems = (items: typeof list) =>
    prefs.view === 'grid' ? (
      <div className="grid">
        {items.map((a) => (
          <AppCard key={a.id} app={a} />
        ))}
      </div>
    ) : (
      <div className="lamina">
        {items.map((a) => (
          <CatalogRow key={a.id} app={a} />
        ))}
      </div>
    )

  return (
    <>
      <Head
        eyebrow={`Catálogo${apps.length ? ` · ${apps.length - (prefs.showUnavailable ? 0 : hidden)} aplicaciones` : ''}`}
        title="Todas las aplicaciones"
      />
      <StatusBanners />

      <div className="toolbar">
        <label className="field-search">
          <Search size={17} aria-hidden />
          <input type="search" placeholder="Filtrar por nombre, categoría o etiqueta" value={query} onChange={(e) => setQuery(e.target.value)} aria-label="Filtrar aplicaciones" />
          {query && (
            <button className="icon-btn" style={{ width: 28, height: 28 }} onClick={() => setQuery('')} aria-label="Borrar búsqueda">
              <X size={15} />
            </button>
          )}
        </label>
        <select value={prefs.sort} onChange={(e) => setPrefs({ sort: e.target.value as SortKey })} aria-label="Ordenar por">
          {sorts.map((s) => (
            <option key={s.value} value={s.value}>
              Ordenar: {s.label}
            </option>
          ))}
        </select>
        <div className="seg" role="group" aria-label="Vista">
          <button aria-pressed={prefs.view === 'grid'} onClick={() => setPrefs({ view: 'grid' })} aria-label="Cuadrícula" title="Cuadrícula">
            <LayoutGrid size={16} />
          </button>
          <button aria-pressed={prefs.view === 'list'} onClick={() => setPrefs({ view: 'list' })} aria-label="Lista" title="Lista">
            <List size={16} />
          </button>
        </div>
      </div>

      <div className="toolbar">
        <Segmented
          label="Estado"
          value={statusFilter}
          onChange={setStatusFilter}
          options={[
            { value: 'all', label: 'Todas', count: counts.all },
            { value: 'available', label: 'Disponibles', count: counts.available },
            { value: 'installed', label: 'Instaladas', count: counts.installed },
            { value: 'updates', label: 'Con actualización', count: counts.updates },
          ]}
        />
        {cats.length > 1 && (
          <div className="chips" role="group" aria-label="Categorías">
            {cats.map((c) => (
              <button key={c.name} className="fchip" aria-pressed={category === c.name} onClick={() => setCategory(category === c.name ? null : c.name)}>
                {c.name} <span className="n">{c.count}</span>
              </button>
            ))}
          </div>
        )}
      </div>
      <div className="result-line">
        <span>
          {list.length === counts.all || !filtered ? `${list.length} ${list.length === 1 ? 'aplicación' : 'aplicaciones'}` : `${list.length} de ${counts.all} aplicaciones`}
          {filtered && (
            <>
              {' · '}
              <button className="linklike" onClick={clearFilters}>
                Quitar filtros
              </button>
            </>
          )}
        </span>
        {hidden > 0 && (
          <label className="switch small">
            <input type="checkbox" checked={prefs.showUnavailable} onChange={(e) => setPrefs({ showUnavailable: e.target.checked })} />
            <span>
              Mostrar {hidden} no {hidden === 1 ? 'disponible' : 'disponibles'} para este equipo
            </span>
          </label>
        )}
      </div>

      {!appsLoaded ? (
        <Loading rows={6} h={210} />
      ) : list.length === 0 ? (
        <Empty
          icon={<PackageSearch size={28} />}
          title={filtered ? 'Ninguna aplicación coincide' : status?.loaded ? 'El catálogo está vacío' : 'No hay catálogo todavía'}
          action={filtered ? <Button onClick={clearFilters}>Quitar filtros</Button> : undefined}
        >
          {filtered ? 'Prueba con otras palabras, otra categoría u otro estado.' : status?.loaded ? 'Aún no se ha publicado ninguna aplicación para este sistema.' : 'Cuando haya conexión, aquí aparecerán las aplicaciones disponibles.'}
        </Empty>
      ) : (
        groups.map((g) => (
          <section key={g.name || 'all'}>
            {g.name && (
              <div className="group-head">
                <h2>{g.name}</h2>
                <span>{g.apps.length}</span>
              </div>
            )}
            {renderItems(g.apps)}
          </section>
        ))
      )}
    </>
  )
}
