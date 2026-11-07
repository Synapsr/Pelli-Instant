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
