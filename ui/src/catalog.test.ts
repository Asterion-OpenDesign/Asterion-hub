import assert from 'node:assert/strict'
import { test } from 'node:test'
import { categoriesOf, filterApps, groupByCategory, matches, NO_CATEGORY, sortApps, statusCounts } from './catalog.ts'
import type { AppView } from './types.ts'

// Test-only fixtures: these never reach the product, which only ever shows what the engine reports.
function app(id: string, over: Partial<AppView> = {}): AppView {
  return {
    id, name: id, summary: '', description: '', publisher: 'ASTERION', homepage: null, license: null, category: null, tags: [],
    state: 'not_installed', installed: null, latest: null, newer_blocked: null, unavailable: null, releases: [], actions: [], active_task: null, orphaned: false,
    ...over,
  }
}
const rel = (released_at: string) => ({ version: '1.0.0', released_at, notes: [], installable: true, blocked: null, size: 1, kind: 'msi', os_min: null, installed_size_mb: null, requires: [] })
const inst = { version: '1.0.0', scope: 'user' as const, location: null, installed_by_hub: true, running: false, can_open: true }

const apps = [
  app('Zeta CAD', { category: 'Diseño', tags: ['dibujo'], latest: rel('2026-01-01T00:00:00Z') }),
  app('Álbum BIM', { category: 'BIM', state: 'installed', installed: inst, summary: 'Modelos paramétricos', latest: rel('2026-03-01T00:00:00Z') }),
  app('Mac Only', { category: 'BIM', state: 'unavailable' }),
  app('Útil', { state: 'update_available', installed: inst, latest: rel('2026-02-01T00:00:00Z') }),
]

test('search ignores accents and case and needs every word', () => {
  assert.ok(matches(apps[1], 'album'))
  assert.ok(matches(apps[1], 'ÁLBUM param'))
  assert.ok(!matches(apps[1], 'album cad'))
  assert.ok(matches(apps[0], 'dibujo'))
  assert.ok(matches(apps[0], 'diseno'))
  assert.ok(matches(apps[3], ''))
})

test('unavailable apps are hidden unless asked for, and counts follow', () => {
  const q = { q: '', status: 'all' as const, category: null, showUnavailable: false }
  assert.deepEqual(filterApps(apps, q).map((a) => a.id), ['Zeta CAD', 'Álbum BIM', 'Útil'])
  assert.equal(filterApps(apps, { ...q, showUnavailable: true }).length, 4)
  assert.deepEqual(statusCounts(apps, false), { all: 3, available: 1, installed: 2, updates: 1 })
  assert.equal(statusCounts(apps, true).all, 4)
})

test('status and category filters combine with the text', () => {
  const base = { q: '', status: 'all' as const, category: null, showUnavailable: true }
  assert.deepEqual(filterApps(apps, { ...base, status: 'installed' }).map((a) => a.id), ['Álbum BIM', 'Útil'])
  assert.deepEqual(filterApps(apps, { ...base, status: 'updates' }).map((a) => a.id), ['Útil'])
  assert.deepEqual(filterApps(apps, { ...base, category: 'BIM' }).map((a) => a.id), ['Álbum BIM', 'Mac Only'])
  assert.deepEqual(filterApps(apps, { ...base, category: NO_CATEGORY }).map((a) => a.id), ['Útil'])
  assert.deepEqual(filterApps(apps, { ...base, category: 'BIM', q: 'mac' }).map((a) => a.id), ['Mac Only'])
})

test('sorting: name (Spanish collation), recent, status, category', () => {
  assert.deepEqual(sortApps(apps, 'name').map((a) => a.id), ['Álbum BIM', 'Mac Only', 'Útil', 'Zeta CAD'])
  assert.deepEqual(sortApps(apps, 'recent').map((a) => a.id).slice(0, 3), ['Álbum BIM', 'Útil', 'Zeta CAD'])
  assert.deepEqual(sortApps(apps, 'status').map((a) => a.id), ['Útil', 'Álbum BIM', 'Zeta CAD', 'Mac Only'])
  assert.deepEqual(sortApps(apps, 'category').map((a) => a.id), ['Álbum BIM', 'Mac Only', 'Zeta CAD', 'Útil'])
})

test('categories are counted over what is visible, "Otras" last, and grouping keeps order', () => {
  assert.deepEqual(categoriesOf(apps, false), [{ name: 'BIM', count: 1 }, { name: 'Diseño', count: 1 }, { name: NO_CATEGORY, count: 1 }])
  assert.deepEqual(categoriesOf(apps, true)[0], { name: 'BIM', count: 2 })
  assert.deepEqual(groupByCategory(sortApps(apps, 'category')).map((g) => [g.name, g.apps.length]), [['BIM', 2], ['Diseño', 1], [NO_CATEGORY, 1]])
})

test('a large catalog filters instantly', () => {
  const many = Array.from({ length: 5000 }, (_, i) => app(`App ${i}`, { category: `Cat ${i % 40}`, tags: ['x'] }))
  const t0 = performance.now()
  const out = filterApps(many, { q: 'app 49', status: 'all', category: null, showUnavailable: false })
  assert.ok(out.length > 0)
  assert.ok(performance.now() - t0 < 500)
})
