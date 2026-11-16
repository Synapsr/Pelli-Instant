import type { ProjectSummary } from '@/lib/contract'

/** Recent shoots the home screen shows under the drop zone (4 in a narrow window). */
export const RECENT_COUNT = 5
export const RECENT_COUNT_NARROW = 4
/** Above this many projects, the full list gets a search field. */
export const SEARCH_THRESHOLD = 12

/** Last opened first (ties: the most recently created). */
export function sortProjects(projects: readonly ProjectSummary[]): ProjectSummary[] {
  return [...projects].sort((a, b) => b.lastOpenedAt - a.lastOpenedAt || b.createdAt - a.createdAt)
}

/** Lower case without accents, for a forgiving search ("Lea" finds "Léa"). */
export function foldText(text: string): string {
  return text
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .toLowerCase()
}

/** Projects whose name or folder holds every word of the query, in any order. */
export function filterProjects(projects: readonly ProjectSummary[], query: string): ProjectSummary[] {
  const words = foldText(query).split(/\s+/).filter(Boolean)
  if (!words.length) return [...projects]
  return projects.filter((p) => {
    const haystack = foldText(`${p.name} ${p.rootPath.split(/[\\/]/).pop() ?? ''}`)
    return words.every((w) => haystack.includes(w))
  })
}

/** The home screen offers "See all (N)" only when some shoots don't fit under the drop zone. */
export function hasMoreProjects(projects: readonly ProjectSummary[], shown = RECENT_COUNT): boolean {
  return projects.length > shown
}
