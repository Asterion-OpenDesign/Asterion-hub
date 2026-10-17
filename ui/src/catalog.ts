import type { AppView } from './types.ts'

export type SortKey = 'name' | 'recent' | 'status' | 'category'
export type StatusKey = 'all' | 'available' | 'installed' | 'updates'

export interface Query {
  q: string
  status: StatusKey
  category: string | null
  showUnavailable: boolean
}

export const NO_CATEGORY = 'Otras'

const fold = (s: string) => s.normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLowerCase()

export function categoryOf(a: AppView): string {
  return a.category?.trim() || NO_CATEGORY
}

/** Every word typed must appear in the text (accents and case ignored). */
export function matchText(text: string, q: string): boolean {
  const words = fold(q).split(/\s+/).filter(Boolean)
  if (words.length === 0) return true
  const hay = fold(text)
  return words.every((w) => hay.includes(w))
}

/** An app matches when every word is in its name, summary, publisher, category or a tag. */
export function matches(a: AppView, q: string): boolean {
  return matchText([a.name, a.summary, a.publisher, categoryOf(a), ...a.tags].join(' '), q)
}

const statusRank = (a: AppView) => ({ update_available: 0, installed: 1, not_installed: 2, unavailable: 3 })[a.state]

const byName = (a: AppView, b: AppView) => a.name.localeCompare(b.name, 'es', { sensitivity: 'base' })

export function sortApps(apps: AppView[], key: SortKey): AppView[] {
  const list = [...apps]
  switch (key) {
    case 'recent':
      const when = (a: AppView) => Date.parse(a.latest?.released_at ?? '') || 0
      return list.sort((a, b) => when(b) - when(a) || byName(a, b))
    case 'status':
      return list.sort((a, b) => statusRank(a) - statusRank(b) || byName(a, b))
    case 'category':
      return list.sort((a, b) => categoryOf(a).localeCompare(categoryOf(b), 'es') || byName(a, b))
    default:
      return list.sort(byName)
  }
}

export function filterApps(apps: AppView[], f: Query): AppView[] {
  return apps.filter((a) => {
    if (!f.showUnavailable && a.state === 'unavailable') return false
    if (f.category && categoryOf(a) !== f.category) return false
    if (f.status === 'available' && a.state !== 'not_installed') return false
    if (f.status === 'installed' && !a.installed) return false
    if (f.status === 'updates' && a.state !== 'update_available') return false
    return matches(a, f.q)
  })
}

export interface CategoryCount {
  name: string
  count: number
}

/** Categories of the apps the user could actually see (unavailable ones only count when they are shown). */
export function categoriesOf(apps: AppView[], showUnavailable: boolean): CategoryCount[] {
  const m = new Map<string, number>()
  for (const a of apps) {
    if (!showUnavailable && a.state === 'unavailable') continue
    m.set(categoryOf(a), (m.get(categoryOf(a)) ?? 0) + 1)
  }
  return [...m.entries()].map(([name, count]) => ({ name, count })).sort((a, b) => (a.name === NO_CATEGORY ? 1 : b.name === NO_CATEGORY ? -1 : a.name.localeCompare(b.name, 'es')))
}

export function groupByCategory(apps: AppView[]): { name: string; apps: AppView[] }[] {
  const groups: { name: string; apps: AppView[] }[] = []
  for (const a of apps) {
    const name = categoryOf(a)
    const g = groups.find((x) => x.name === name)
    if (g) g.apps.push(a)
    else groups.push({ name, apps: [a] })
  }
  return groups
}

export interface StatusCounts {
  all: number
  available: number
  installed: number
  updates: number
}

export function statusCounts(apps: AppView[], showUnavailable: boolean): StatusCounts {
  const visible = apps.filter((a) => showUnavailable || a.state !== 'unavailable')
  return {
    all: visible.length,
    available: visible.filter((a) => a.state === 'not_installed').length,
    installed: visible.filter((a) => a.installed).length,
    updates: visible.filter((a) => a.state === 'update_available').length,
  }
}
