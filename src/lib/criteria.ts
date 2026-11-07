import { DEFAULT_SETTINGS, type CullSettings, type Reason } from './contract'

/** A switch of the culling criteria (a boolean of `CullSettings`). */
export type CriterionKey =
  | 'detectBlur'
  | 'detectMotionBlur'
  | 'detectSubjectFocus'
  | 'artisticBlur'
  | 'detectEyesClosed'
  | 'detectUnderexposed'
  | 'detectOverexposed'
  | 'groupSimilar'
  | 'detectExactDuplicates'

export type CriteriaGroupId = 'sharpness' | 'eyes' | 'exposure' | 'similar'

export interface Criterion {
  key: CriterionKey
  /** reason the switch controls (its icon); none for options such as artistic blur */
  reason: Reason | null
}

/** The criteria as the photographer sees them, grouped, in display order. */
export const CRITERIA_GROUPS: { id: CriteriaGroupId; criteria: Criterion[] }[] = [
  {
    id: 'sharpness',
    criteria: [
      { key: 'detectBlur', reason: 'blurry' },
      { key: 'detectMotionBlur', reason: 'motionBlur' },
      { key: 'detectSubjectFocus', reason: 'subjectOutOfFocus' },
      { key: 'artisticBlur', reason: null },
    ],
  },
  { id: 'eyes', criteria: [{ key: 'detectEyesClosed', reason: 'eyesClosed' }] },
  {
    id: 'exposure',
    criteria: [
      { key: 'detectUnderexposed', reason: 'underexposed' },
      { key: 'detectOverexposed', reason: 'overexposed' },
    ],
  },
  {
    id: 'similar',
    criteria: [
      { key: 'groupSimilar', reason: 'duplicate' },
      { key: 'detectExactDuplicates', reason: 'exactDuplicate' },
    ],
  },
]

/** The detections (artistic blur is an option of the sharpness ones). */
export const DETECTIONS: CriterionKey[] = CRITERIA_GROUPS.flatMap((g) => g.criteria.filter((c) => c.reason !== null).map((c) => c.key))

/** At least one of the blur detections is on (artistic blur only matters then). */
export function checksSharpness(s: CullSettings): boolean {
  return s.detectBlur || s.detectMotionBlur || s.detectSubjectFocus
}

/**
 * Fills what an older engine did not send, like the engine reads old
 * projects: each split switch takes the value of the one it came from.
 */
export function normalizeCriteria(value: Partial<CullSettings> & { detectExposure?: boolean }): CullSettings {
  const { detectExposure, ...rest } = value
  const blur = value.detectBlur ?? DEFAULT_SETTINGS.detectBlur
  return {
    ...DEFAULT_SETTINGS,
    detectMotionBlur: blur,
    detectSubjectFocus: blur,
    detectUnderexposed: detectExposure ?? DEFAULT_SETTINGS.detectUnderexposed,
    detectOverexposed: detectExposure ?? DEFAULT_SETTINGS.detectOverexposed,
    ...Object.fromEntries(Object.entries(rest).filter(([, v]) => v !== undefined && v !== null)),
  }
}

/** Same decisions (the switches, sensitivity and kept count; not the tuning values). */
export function sameCriteria(a: CullSettings, b: CullSettings): boolean {
  return (
    a.sensitivity === b.sensitivity &&
    a.keepPerGroup === b.keepPerGroup &&
    CRITERIA_GROUPS.every((g) => g.criteria.every((c) => a[c.key] === b[c.key]))
  )
}

/** Detections turned off, in display order. */
export function disabledDetections(s: CullSettings): CriterionKey[] {
  return DETECTIONS.filter((key) => !s[key])
}
