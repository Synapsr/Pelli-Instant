import type { DecisionCounts, FinalDecision, PhotoView, UserMark, Verdict } from '@/lib/contract'

/** Mirror of `cull_types::final_decision`: the photographer's flag wins. */
export function finalDecision(verdict: Verdict | null | undefined, mark: UserMark): FinalDecision {
  if (mark.flag === 'pick') return 'selected'
  if (mark.flag === 'reject') return 'rejected'
  if (verdict === 'keep') return 'selected'
  if (verdict === 'reject') return 'rejected'
  return 'toReview'
}

export function applyMarkPatch(photo: PhotoView, patch: Partial<UserMark>): PhotoView {
  const mark: UserMark = { ...photo.mark, ...patch }
  return { ...photo, mark, decision: finalDecision(photo.assessment?.verdict, mark) }
}

export function countDecisions(photos: Iterable<PhotoView>): DecisionCounts {
  const counts: DecisionCounts = { selected: 0, toReview: 0, rejected: 0 }
  for (const p of photos) counts[p.decision] += 1
  return counts
}

export const EMPTY_MARK: UserMark = { flag: 'none', rating: 0, label: 'none' }

export interface MarkChange {
  ids: string[]
  patch: Partial<UserMark>
}

/**
 * "Choose this one" (S, and the compare view's Choose): keeps `winner` and
 * rejects what competes with it. `seen` are the photos shown next to it (a
 * compare page): all of them are rejected. Of `pool` (the rest of its burst,
 * or of the comparison), only the photos selected right now are rejected, by
 * the AI or by the photographer: to-review photos nobody looked at don't move.
 * One list of changes, so one undo puts everything back.
 */
export function chooseChanges(photos: Readonly<Record<string, PhotoView>>, winner: string, { seen = [], pool = [] }: { seen?: readonly string[]; pool?: readonly string[] }): MarkChange[] {
  const rejected = new Set<string>()
  const consider = (id: string, always: boolean) => {
    const p = photos[id]
    if (!p || id === winner || p.mark.flag === 'reject') return
    if (always || p.decision === 'selected') rejected.add(id)
  }
  seen.forEach((id) => consider(id, true))
  pool.forEach((id) => consider(id, false))
  const changes: MarkChange[] = [{ ids: [winner], patch: { flag: 'pick' } }]
  if (rejected.size) changes.push({ ids: [...rejected], patch: { flag: 'reject' } })
  return changes
}
