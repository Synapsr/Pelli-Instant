import type { PhotoView, Reason, SimilarGroup } from '@/lib/contract'

export type FilterTab = 'selected' | 'toReview' | 'rejected' | 'all' | 'duplicates'
export type SortKey = 'date' | 'score' | 'name' | 'relevance'

export const TABS: FilterTab[] = ['selected', 'toReview', 'rejected', 'all', 'duplicates']

export const REASONS: Reason[] = [
  'blurry',
  'motionBlur',
  'subjectOutOfFocus',
  'eyesClosed',
  'underexposed',
  'overexposed',
  'duplicate',
  'lowAesthetic',
]

export interface ViewFilters {
  tab: FilterTab
  sort: SortKey
  reason: Reason | null
  sceneId: number | null
  camera: string | null
  stackGroups: boolean
  expanded: Readonly<Record<number, true>>
}

export interface ReviewData {
  photos: Readonly<Record<string, PhotoView>>
  /** photo ids in capture order */
  dateOrder: readonly string[]
  groups: Readonly<Record<number, SimilarGroup>>
}

/** Search hits by photo id (cosine score), null when no search is active. */
export type SearchHits = Readonly<Record<string, number>> | null

export const DEFAULT_FILTERS: ViewFilters = {
  tab: 'all',
  sort: 'date',
  reason: null,
  sceneId: null,
  camera: null,
  stackGroups: true,
  expanded: {},
}

export function groupIdOf(photo: PhotoView): number | null {
  return photo.assessment?.groupId ?? null
}

function hasReason(photo: PhotoView, reason: Reason): boolean {
  const reasons = photo.assessment?.reasons
  if (!reasons) return false
  if (reason === 'duplicate') return reasons.includes('duplicate') || reasons.includes('exactDuplicate')
  return reasons.includes(reason)
}

/** Sidebar + search scope (everything but the decision tab). */
export function inScope(photo: PhotoView, filters: ViewFilters, hits: SearchHits): boolean {
  if (hits && hits[photo.id] === undefined) return false
  if (filters.sceneId !== null && photo.sceneId !== filters.sceneId) return false
  if (filters.camera !== null && (photo.exif.camera ?? '') !== filters.camera) return false
  if (filters.reason !== null && !hasReason(photo, filters.reason)) return false
  return true
}

/** Decision tab; photos still waiting for the analysis only show in "all". */
export function inTab(photo: PhotoView, tab: FilterTab): boolean {
  if (tab === 'all') return true
  if (photo.status !== 'analyzed') return false
  if (tab === 'duplicates') return groupIdOf(photo) !== null
  return photo.decision === tab
}

/** The sort actually applied: relevance without a search falls back to the date. */
export function effectiveSort(sort: SortKey, hits: SearchHits): SortKey {
  return sort === 'relevance' && !hits ? 'date' : sort
}

/** Index of each photo in capture order, cached per `dateOrder` array. */
const dateIndexes = new WeakMap<readonly string[], Map<string, number>>()

export function dateIndex(dateOrder: readonly string[]): Map<string, number> {
  let index = dateIndexes.get(dateOrder)
  if (!index) {
    index = new Map(dateOrder.map((id, i) => [id, i]))
    dateIndexes.set(dateOrder, index)
  }
  return index
}

const byName = (photos: ReviewData['photos']) => (a: string, b: string) =>
  (photos[a]?.fileName ?? '').localeCompare(photos[b]?.fileName ?? '', undefined, { numeric: true })

/**
 * Frames of a burst in the order of the grid's sort: capture order by date,
 * best first by score or relevance (the engine's order), file name by name.
 * Position is not rank: the best is always `group.photoIds[0]`.
 */
export function groupOrder(group: SimilarGroup, sort: SortKey, data: ReviewData): string[] {
  if (sort === 'score' || sort === 'relevance') return group.photoIds
  if (sort === 'name') return [...group.photoIds].sort(byName(data.photos))
  const index = dateIndex(data.dateOrder)
  return [...group.photoIds].sort((a, b) => (index.get(a) ?? Infinity) - (index.get(b) ?? Infinity))
}

/** The best frame of its burst (the engine ranks the members, best first). */
export function isBest(group: SimilarGroup | undefined, id: string): boolean {
  return group !== undefined && group.photoIds.length > 1 && group.photoIds[0] === id
}

/** Place of a frame in its burst, in the grid's order (0-based; -1 when it is in none). */
export function framePosition(group: SimilarGroup | undefined, id: string, sort: SortKey, hits: SearchHits, data: ReviewData): number {
  return group ? groupOrder(group, effectiveSort(sort, hits), data).indexOf(id) : -1
}

/** True when the group shows as one closed stack (stacking on, not unfolded, not the "Doublons" rows). */
export function isStackClosed(groupId: number, filters: ViewFilters): boolean {
  return filters.tab !== 'duplicates' && filters.stackGroups && !filters.expanded[groupId]
}

/**
 * Frames one goes through in a burst (⌥← ⌥→), and what an unfolded burst
 * shows: every member within the scope (search, moment, camera, reason),
 * whatever its decision, in the grid's order. Pending photos only in "all".
 */
export function burstFrames(group: SimilarGroup, data: ReviewData, filters: ViewFilters, hits: SearchHits): string[] {
  return groupOrder(group, effectiveSort(filters.sort, hits), data).filter((id) => {
    const p = data.photos[id]
    return p !== undefined && (p.status === 'analyzed' || filters.tab === 'all') && inScope(p, filters, hits)
  })
}

/**
 * The photo a closed stack shows: the photographer's own kept photo, else
 * the best selected member, else the best member, among those in the tab.
 */
export function stackCover(group: SimilarGroup, photos: ReviewData['photos'], passes: (id: string) => boolean): string | undefined {
  const members = group.photoIds.filter(passes)
  return members.find((id) => photos[id]?.mark.flag === 'pick') ?? members.find((id) => photos[id]?.decision === 'selected') ?? members[0]
}

export function sortIds(data: ReviewData, sort: SortKey, hits: SearchHits): string[] {
  const ids = [...data.dateOrder]
  const { photos } = data
  if (sort === 'date') return ids
  if (sort === 'relevance' && hits) return ids.sort((a, b) => (hits[b] ?? 0) - (hits[a] ?? 0))
  if (sort === 'name') return ids.sort(byName(photos))
  const score = (id: string) => photos[id].assessment?.score ?? -1
  return ids.sort((a, b) => score(b) - score(a))
}

/**
 * Ids shown in the grid. Groups are stacked behind their cover (`stackCover`)
 * in the tab, placed where the group first appears, unless expanded. An
 * expanded group shows the whole burst (`burstFrames`), whatever each frame's
 * decision, in the grid's order.
 */
export function computeVisible(data: ReviewData, filters: ViewFilters, hits: SearchHits): string[] {
  const sorted = sortIds(data, effectiveSort(filters.sort, hits), hits)
  const passes = (id: string) => {
    const p = data.photos[id]
    return p !== undefined && inTab(p, filters.tab) && inScope(p, filters, hits)
  }
  if (filters.tab === 'duplicates') return computeGroupRows(data, filters, hits).flatMap((row) => row.ids)

  const out: string[] = []
  const emitted = new Set<number>()
  for (const id of sorted) {
    if (!passes(id)) continue
    const gid = groupIdOf(data.photos[id])
    const group = gid === null ? undefined : data.groups[gid]
    if (!filters.stackGroups || !group) {
      out.push(id)
      continue
    }
    if (emitted.has(group.id)) continue
    emitted.add(group.id)
    if (filters.expanded[group.id]) out.push(...burstFrames(group, data, filters, hits))
    else out.push(stackCover(group, data.photos, passes) ?? id)
  }
  return out
}

export interface GroupRow {
  group: SimilarGroup
  /** members in scope, in the grid's order (`groupOrder`) */
  ids: string[]
}

/** Rows of the "Doublons" view, in capture order of their first frame. */
export function computeGroupRows(data: ReviewData, filters: ViewFilters, hits: SearchHits): GroupRow[] {
  const rows: GroupRow[] = []
  const seen = new Set<number>()
  for (const id of data.dateOrder) {
    const p = data.photos[id]
    if (!p) continue
    const gid = groupIdOf(p)
    if (gid === null || seen.has(gid)) continue
    seen.add(gid)
    const group = data.groups[gid]
    if (!group) continue
    const ids = groupOrder(group, effectiveSort(filters.sort, hits), data).filter((pid) => {
      const member = data.photos[pid]
      return member !== undefined && member.status === 'analyzed' && inScope(member, filters, hits)
    })
    if (ids.length > 1) rows.push({ group, ids })
  }
  return rows
}

export type TabCounts = Record<FilterTab, number>

export function computeTabCounts(data: ReviewData, filters: ViewFilters, hits: SearchHits): TabCounts {
  const counts: TabCounts = { selected: 0, toReview: 0, rejected: 0, all: 0, duplicates: 0 }
  const groups = new Set<number>()
  for (const id of data.dateOrder) {
    const p = data.photos[id]
    if (!p || !inScope(p, filters, hits)) continue
    counts.all += 1
    if (p.status !== 'analyzed') continue
    counts[p.decision] += 1
    const gid = groupIdOf(p)
    if (gid !== null) groups.add(gid)
  }
  counts.duplicates = groups.size
  return counts
}

export function computeReasonCounts(photos: Iterable<PhotoView>): Partial<Record<Reason, number>> {
  const counts: Partial<Record<Reason, number>> = {}
  for (const p of photos) {
    const reasons = p.assessment?.reasons
    if (!reasons) continue
    for (const r of reasons) {
      const key = r === 'exactDuplicate' ? 'duplicate' : r
      if (r === 'exactDuplicate' && reasons.includes('duplicate')) continue
      counts[key] = (counts[key] ?? 0) + 1
    }
  }
  return counts
}

export function computeCameraCounts(photos: Iterable<PhotoView>): { camera: string; count: number }[] {
  const counts = new Map<string, number>()
  for (const p of photos) {
    const camera = p.exif.camera ?? ''
    counts.set(camera, (counts.get(camera) ?? 0) + 1)
  }
  return [...counts.entries()].map(([camera, count]) => ({ camera, count })).sort((a, b) => b.count - a.count)
}

export function sameIds(a: readonly string[], b: readonly string[]): boolean {
  if (a === b) return true
  if (a.length !== b.length) return false
  for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return false
  return true
}

export function captureOrder(photos: readonly PhotoView[]): string[] {
  return [...photos]
    .sort((a, b) => (a.capturedAt ?? Number.MAX_SAFE_INTEGER) - (b.capturedAt ?? Number.MAX_SAFE_INTEGER) || a.relPath.localeCompare(b.relPath))
    .map((p) => p.id)
}
