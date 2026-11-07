import type { Assessment, FinalDecision, PhotoView, Reason, SimilarGroup, Verdict } from '@/lib/contract'
import { EMPTY_MARK, finalDecision } from '@/lib/review/decision'
import type { ReviewData } from '@/lib/review/view'

/** Small hand-made photo for unit tests. */
export function photo(
  id: string,
  options: {
    verdict?: Verdict | null
    reasons?: Reason[]
    score?: number
    groupId?: number | null
    sceneId?: number | null
    camera?: string
    capturedAt?: number
    pending?: boolean
  } = {},
): PhotoView {
  const verdict = options.pending ? null : (options.verdict ?? 'keep')
  const assessment: Assessment | null = verdict
    ? {
        score: options.score ?? 50,
        verdict,
        reasons: options.reasons ?? [],
        groupId: options.groupId ?? null,
        groupRank: 0,
        breakdown: { focus: 0.8, exposure: 0.8, eyes: null, aesthetic: 0.5, uniqueness: 1 },
      }
    : null
  return {
    id,
    relPath: `R5/${id}.CR3`,
    fileName: `${id}.CR3`,
    kind: 'raw',
    hasRaw: true,
    hasJpeg: false,
    sizeBytes: 30_000_000,
    capturedAt: options.capturedAt ?? Number(id.replace(/\D/g, '')) * 1000,
    width: 6000,
    height: 4000,
    exif: { camera: options.camera ?? 'Canon EOS R5', lens: null, iso: 400, shutter: '1/250', aperture: 2, focalLength: 50 },
    thumbUrl: null,
    status: options.pending ? 'pending' : 'analyzed',
    error: null,
    assessment,
    faces: [],
    mark: EMPTY_MARK,
    decision: finalDecision(verdict, EMPTY_MARK) as FinalDecision,
    sceneId: options.sceneId ?? null,
  }
}

export function data(photos: PhotoView[], groups: SimilarGroup[] = []): ReviewData {
  const map: Record<string, PhotoView> = {}
  photos.forEach((p) => (map[p.id] = p))
  const g: Record<number, SimilarGroup> = {}
  groups.forEach((x) => (g[x.id] = x))
  return { photos: map, dateOrder: [...photos].sort((a, b) => (a.capturedAt ?? 0) - (b.capturedAt ?? 0)).map((p) => p.id), groups: g }
}
