import type { Assessment, CullSettings, GroupKind, Reason, Scene, Sensitivity, SimilarGroup, Verdict } from '@/lib/contract'
import { clamp } from '@/lib/procedural/random'
import type { MockRecord } from './shoot'

/**
 * Decision rules of the mock engine. They imitate the real engine: technical
 * defects reject, borderline cases go to review, similar shots keep the best
 * `keepPerGroup` frames, a detection turned off never gives its reason, and
 * artistic blur only rejects a photo with nothing sharp in it (clearly soft
 * ones go to review, the subject's focus is not judged).
 */

interface Thresholds {
  blur: number
  /** frame focus below which a photo is unusable whatever the settings */
  blurObvious: number
  motion: number
  /** motion beyond which a photo is unusable whatever the settings */
  motionObvious: number
  subject: number
  eyes: number
  exposure: number
  aesthetic: number
  keepScore: number
  margin: number
}

const THRESHOLDS: Record<Sensitivity, Thresholds> = {
  strict: { blur: 0.5, blurObvious: 0.2, motion: 0.42, motionObvious: 0.85, subject: 0.48, eyes: 0.42, exposure: 0.42, aesthetic: 0.42, keepScore: 78, margin: 0.08 },
  balanced: { blur: 0.36, blurObvious: 0.12, motion: 0.58, motionObvious: 0.9, subject: 0.36, eyes: 0.34, exposure: 0.56, aesthetic: 0.34, keepScore: 72, margin: 0.06 },
  lenient: { blur: 0.22, blurObvious: 0.05, motion: 0.76, motionObvious: 0.97, subject: 0.24, eyes: 0.2, exposure: 0.74, aesthetic: 0.24, keepScore: 62, margin: 0.05 },
}

const REASON_ORDER: Reason[] = [
  'blurry',
  'motionBlur',
  'subjectOutOfFocus',
  'eyesClosed',
  'underexposed',
  'overexposed',
  'duplicate',
  'exactDuplicate',
  'lowAesthetic',
]

export interface GroupInfo {
  groupId: number
  rank: number
  size: number
  kind: GroupKind
  /** number of better-ranked members that are kept */
  keptBefore: number
  /** score of the last kept member (close calls go to review) */
  lastKeptScore: number | null
  isExactCopy: boolean
}

interface Technical {
  score: number
  reasons: Reason[]
  /** a reason that rejects on its own */
  blocking: boolean
  /** borderline: let the photographer decide */
  borderline: boolean
  breakdown: Assessment['breakdown']
}

export function technical(record: MockRecord, settings: CullSettings, uniqueness: number): Technical {
  const t = THRESHOLDS[settings.sensitivity]
  const { m } = record
  const reasons: Reason[] = []
  let blocking = false
  let borderline = false

  const flag = (reason: Reason, value: number, threshold: number, below: boolean, rejects = true) => {
    const hit = below ? value < threshold : value > threshold
    const near = below ? value < threshold + t.margin : value > threshold - t.margin
    if (hit) {
      reasons.push(reason)
      if (rejects) blocking = true
      else borderline = true
    } else if (near) {
      reasons.push(reason)
      borderline = true
    }
  }

  // Each softness is attributed to one cause, then kept only if its detection is on.
  const wanted = (reason: Reason) => (reason === 'blurry' ? settings.detectBlur : reason === 'motionBlur' ? settings.detectMotionBlur : settings.detectSubjectFocus)
  const motionFactor = 1 - Math.max(0, m.motion - 0.2) * 0.8
  const sharpness = settings.detectBlur || settings.detectMotionBlur || settings.detectSubjectFocus
  const blurry = m.focus < t.blur + t.margin
  if (sharpness && settings.artisticBlur) {
    // Artistic blur: rejected only when nothing is sharp, to review when
    // clearly soft, mild softness and the subject's focus not judged.
    const add = (reason: Reason, obvious: boolean) => {
      reasons.push(reason)
      if (obvious) blocking = true
      else borderline = true
    }
    if (blurry) {
      if (wanted('blurry') && m.focus < t.blur) add('blurry', m.focus < t.blurObvious)
    } else if (wanted('motionBlur') && m.motion > t.motion) {
      add('motionBlur', m.motion > t.motionObvious)
    }
  } else if (sharpness) {
    if (blurry) {
      if (wanted('blurry')) flag('blurry', m.focus, t.blur, true)
    } else if (wanted('motionBlur')) {
      flag('motionBlur', m.motion, t.motion, false)
    }
    if (m.subjectFocus !== null && m.focus >= t.blur && wanted('subjectOutOfFocus')) flag('subjectOutOfFocus', m.subjectFocus, t.subject, true)
  }
  // Like the engine: closed eyes on a portrait are only flagged on Strict
  // (a laugh, a kiss, a look down close them on purpose).
  if (settings.detectEyesClosed && m.eyes !== null && settings.sensitivity === 'strict') {
    flag('eyesClosed', m.eyes, t.eyes, true, true)
  }
  const bias = m.exposureBias
  // A visibly dark (bright) photo is judged by the under (over) detection;
  // a well exposed one by either.
  const exposureOn =
    Math.abs(bias) < 0.15 ? settings.detectUnderexposed || settings.detectOverexposed : bias < 0 ? settings.detectUnderexposed : settings.detectOverexposed
  if (exposureOn) {
    const severe = Math.abs(bias) > t.exposure + 0.14
    if (Math.abs(bias) > t.exposure) {
      reasons.push(bias < 0 ? 'underexposed' : 'overexposed')
      if (severe) blocking = true
      else borderline = true
    }
  }
  // The aesthetic only ranks (10% of the score): no `lowAesthetic` any more.

  const focusEff = Math.min(m.focus, m.subjectFocus ?? m.focus) * (settings.detectMotionBlur ? motionFactor : 1)
  const exposureQ = clamp(1 - Math.abs(m.exposureBias) * 1.05)
  const eyesQ = m.eyes === null ? null : clamp(m.eyes * 1.05)
  const weights = eyesQ === null ? { f: 0.55, e: 0.25, y: 0, a: 0.1, u: 0.1 } : { f: 0.5, e: 0.2, y: 0.15, a: 0.1, u: 0.05 }
  // A detection turned off leaves the score, like in the engine.
  const parts: [number, number][] = [
    [sharpness ? weights.f : 0, focusEff],
    [exposureOn ? weights.e : 0, exposureQ],
    [settings.detectEyesClosed ? weights.y : 0, eyesQ ?? 0],
    [weights.a, m.aesthetic],
    [weights.u, uniqueness],
  ]
  const score = (100 * parts.reduce((sum, [w, v]) => sum + w * v, 0)) / parts.reduce((sum, [w]) => sum + w, 0)

  return {
    score: Math.round(clamp(score, 0, 100) * 10) / 10,
    reasons,
    blocking,
    borderline,
    breakdown: {
      focus: clamp(focusEff),
      exposure: exposureQ,
      eyes: eyesQ,
      aesthetic: m.aesthetic,
      uniqueness,
    },
  }
}

function sortReasons(reasons: Reason[]): Reason[] {
  return [...new Set(reasons)].sort((a, b) => REASON_ORDER.indexOf(a) - REASON_ORDER.indexOf(b))
}

/** Verdict of a photo once its group (if any) is known. */
export function assess(record: MockRecord, settings: CullSettings, group: GroupInfo | null): Assessment {
  const uniqueness = group ? clamp(1 - record.momentSim * 0.55 - Math.min(group.size, 6) * 0.03) : 1
  const tech = technical(record, settings, uniqueness)
  const t = THRESHOLDS[settings.sensitivity]
  const reasons = [...tech.reasons]
  let verdict: Verdict

  if (tech.blocking) {
    verdict = 'reject'
  } else if (group && group.isExactCopy) {
    reasons.push('exactDuplicate')
    verdict = 'reject'
  } else if (group && group.size > 1) {
    if (group.keptBefore < settings.keepPerGroup) {
      verdict = tech.borderline ? 'maybe' : 'keep'
    } else {
      reasons.push('duplicate')
      // A close call with the kept frame: the photographer decides. Only a
      // burst (one moment) rejects; a series of poses goes to review.
      const closeCall =
        group.keptBefore === settings.keepPerGroup &&
        group.lastKeptScore !== null &&
        tech.score >= group.lastKeptScore - (settings.sensitivity === 'lenient' ? 6 : settings.sensitivity === 'balanced' ? 2.5 : 0.8)
      verdict = closeCall || group.kind !== 'burst' ? 'maybe' : 'reject'
    }
  } else if (tech.borderline) {
    verdict = 'maybe'
  } else {
    verdict = tech.score >= t.keepScore ? 'keep' : 'maybe'
  }

  return {
    score: tech.score,
    verdict,
    reasons: sortReasons(reasons),
    groupId: group && group.size > 1 ? group.groupId : null,
    groupRank: group ? group.rank : 0,
    breakdown: tech.breakdown,
  }
}

/** Similar-shots groups over consecutive frames (time window + similarity). */
export function buildGroups(records: MockRecord[], settings: CullSettings): SimilarGroup[] {
  if (!settings.groupSimilar) {
    if (!settings.detectExactDuplicates) return []
    return records
      .filter((r) => r.exactOf)
      .map((r, i) => ({ id: i + 1, kind: 'exact' as const, photoIds: [r.exactOf as string, r.id] }))
  }
  const chains: MockRecord[][] = []
  let chain: MockRecord[] = []
  for (const r of records) {
    const prev = chain[chain.length - 1]
    if (prev) {
      const dt = (r.capturedAt - prev.capturedAt) / 1000
      const exact = r.exactOf === prev.id || prev.exactOf === r.id
      const sameMoment = r.momentId === prev.momentId
      const sim = exact ? 0.998 : sameMoment ? r.momentSim : r.setupId === prev.setupId ? 0.84 + (r.momentSim % 0.05) : 0.55
      if (exact || (dt <= settings.groupWindowS && sim >= settings.similarityThreshold)) {
        chain.push(r)
        continue
      }
      chains.push(chain)
    }
    chain = [r]
  }
  if (chain.length) chains.push(chain)

  let id = 0
  return chains
    .filter((c) => c.length > 1)
    .map((members) => {
      id += 1
      const scored = members
        .map((r) => ({ r, s: technical(r, settings, 0.5).score - (r.exactOf ? 0.5 : 0) }))
        .sort((a, b) => b.s - a.s)
      // Copies are an exact group only while their detection is on.
      const allExact = settings.detectExactDuplicates && members.length === 2 && members.some((r) => r.exactOf)
      const maxGap = members.reduce((gap, r, i) => (i ? Math.max(gap, r.capturedAt - members[i - 1].capturedAt) : 0), 0)
      const kind: GroupKind = allExact ? 'exact' : maxGap <= 2500 ? 'burst' : 'similar'
      return { id, kind, photoIds: scored.map(({ r }) => r.id) }
    })
}

export function groupInfos(groups: SimilarGroup[], records: Map<string, MockRecord>, settings: CullSettings): Map<string, GroupInfo> {
  const infos = new Map<string, GroupInfo>()
  for (const g of groups) {
    let kept = 0
    let lastKeptScore: number | null = null
    g.photoIds.forEach((pid, rank) => {
      const record = records.get(pid)
      if (!record) return
      const isExactCopy = settings.detectExactDuplicates && Boolean(record.exactOf && g.photoIds.includes(record.exactOf))
      infos.set(pid, { groupId: g.id, rank, size: g.photoIds.length, kind: g.kind, keptBefore: kept, lastKeptScore, isExactCopy })
      const tech = technical(record, settings, 0.5)
      if (!tech.blocking && !isExactCopy && kept < settings.keepPerGroup) {
        kept += 1
        lastKeptScore = tech.score
      }
    })
  }
  return infos
}

export interface SceneLabeler {
  (kind: MockRecord['sceneKind']): string
}

export function buildScenes(records: MockRecord[], settings: CullSettings, label: SceneLabeler): Scene[] {
  const scenes: Scene[] = []
  let current: MockRecord[] = []
  const flush = () => {
    if (!current.length) return
    const counts = new Map<string, number>()
    current.forEach((r) => counts.set(r.sceneKind, (counts.get(r.sceneKind) ?? 0) + 1))
    const kind = [...counts.entries()].sort((a, b) => b[1] - a[1])[0][0] as MockRecord['sceneKind']
    scenes.push({
      id: scenes.length + 1,
      startMs: current[0].capturedAt,
      endMs: current[current.length - 1].capturedAt,
      photoIds: current.map((r) => r.id),
      label: label(kind),
    })
    current = []
  }
  for (const r of records) {
    const prev = current[current.length - 1]
    if (prev && (r.capturedAt - prev.capturedAt) / 1000 > settings.sceneGapS) flush()
    current.push(r)
  }
  flush()
  // Same label twice (e.g. a short gap split the cocktail): number them.
  const seen = new Map<string, number>()
  scenes.forEach((s) => {
    const n = (seen.get(s.label ?? '') ?? 0) + 1
    seen.set(s.label ?? '', n)
    if (n > 1) s.label = `${s.label} ${n}`
  })
  return scenes
}
