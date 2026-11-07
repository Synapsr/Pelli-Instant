import type { ExifSummary, FaceView, FileKind } from '@/lib/contract'
import { composeMoment, frameFromMoment, type MomentPlan } from '@/lib/procedural/compose'
import { clamp, createRng, type Rng } from '@/lib/procedural/random'
import { faceBox, type SceneKind, type Shot } from '@/lib/procedural/types'

/** Hidden measures of a mock photo: what a real analysis would measure. */
export interface Measures {
  /** whole-frame focus, 0 … 1 */
  focus: number
  motion: number
  /** focus on the main face, null without faces */
  subjectFocus: number | null
  /** -1 under … 1 over */
  exposureBias: number
  /** eyes openness of the main face (min over faces for group shots) */
  eyes: number | null
  aesthetic: number
}

export interface MockRecord {
  id: string
  index: number
  fileName: string
  relPath: string
  kind: FileKind
  hasRaw: boolean
  hasJpeg: boolean
  sizeBytes: number
  capturedAt: number
  width: number
  height: number
  exif: ExifSummary
  sceneKind: SceneKind
  momentId: number
  setupId: number
  /** visual similarity with the previous frame of the same moment */
  momentSim: number
  exactOf: string | null
  shot: Shot
  faces: FaceView[]
  m: Measures
  tags: string[]
}

export interface ShootSpec {
  seed: number
  count: number
  /** day of the shoot, local midnight in ms */
  day: number
  scenes: SceneKind[]
  /** RAW + JPEG twins for the second body */
  twins: boolean
}

interface Body {
  model: string
  prefix: string
  folder: string
  width: number
  height: number
  rawMb: [number, number]
  lenses: { name: string; focal: number[]; apertures: number[] }[]
}

const BODIES: Body[] = [
  {
    model: 'Canon EOS R5',
    prefix: 'IMG_',
    folder: 'R5',
    width: 8192,
    height: 5464,
    rawMb: [38, 52],
    lenses: [
      { name: 'RF 28-70mm F2 L USM', focal: [28, 35, 50, 70], apertures: [2, 2.2, 2.8] },
      { name: 'RF 85mm F1.2 L USM', focal: [85], apertures: [1.2, 1.4, 1.8, 2] },
    ],
  },
  {
    model: 'Canon EOS R6 Mark II',
    prefix: '_MG_',
    folder: 'R6',
    width: 6000,
    height: 4000,
    rawMb: [24, 32],
    lenses: [
      { name: 'RF 35mm F1.4 L VCM', focal: [35], apertures: [1.4, 1.8, 2.2] },
      { name: 'RF 70-200mm F2.8 L IS USM', focal: [70, 105, 135, 200], apertures: [2.8, 3.2] },
    ],
  },
]

/** Time window of each kind of moment (hours of the day). */
const SCHEDULE: Record<SceneKind, { start: number; end: number; weight: number }> = {
  prep: { start: 10.1, end: 11.35, weight: 0.14 },
  ceremony: { start: 14, end: 14.85, weight: 0.16 },
  groups: { start: 15.2, end: 15.7, weight: 0.08 },
  couple: { start: 16.15, end: 16.8, weight: 0.14 },
  cocktail: { start: 17.25, end: 18.7, weight: 0.18 },
  dinner: { start: 19.5, end: 21.4, weight: 0.14 },
  party: { start: 22, end: 24.4, weight: 0.16 },
}

const ISO: Record<SceneKind, number[]> = {
  prep: [400, 640, 800, 1250, 1600],
  ceremony: [100, 125, 200, 320, 400],
  groups: [100, 125, 160, 200],
  couple: [100, 125, 200, 250, 400],
  cocktail: [200, 320, 400, 640, 800],
  dinner: [1600, 2500, 3200, 5000, 6400],
  party: [3200, 5000, 6400, 10000, 12800],
}
const SHUTTER: Record<SceneKind, string[]> = {
  prep: ['1/160', '1/200', '1/250'],
  ceremony: ['1/500', '1/640', '1/800', '1/1000'],
  groups: ['1/250', '1/320', '1/400'],
  couple: ['1/800', '1/1000', '1/1250', '1/1600'],
  cocktail: ['1/320', '1/400', '1/500'],
  dinner: ['1/100', '1/125', '1/160'],
  party: ['1/40', '1/60', '1/80', '1/100'],
}

const TAGS: Record<SceneKind, string[]> = {
  prep: ['préparatifs', 'fenêtre', 'lumière naturelle'],
  ceremony: ['cérémonie', 'arche', 'fleurs', 'vœux', 'extérieur'],
  groups: ['photo de groupe', 'famille', 'invités', 'château'],
  couple: ['séance couple', 'portrait', 'coucher de soleil', 'golden hour', 'champ'],
  cocktail: ['cocktail', "vin d'honneur", 'jardin', 'invités'],
  dinner: ['dîner', 'réception', 'guirlandes', 'table'],
  party: ['soirée', 'danse', 'piste de danse', 'fête', 'lumières'],
}

const DETAIL_TAGS: Record<string, string[]> = {
  rings: ['alliances', 'bagues', 'détail', 'bijoux'],
  bouquet: ['bouquet', 'fleurs', 'détail'],
  glasses: ['champagne', 'verres', 'toast', 'détail'],
  cake: ['gâteau', 'pièce montée', 'dessert', 'détail'],
  table: ['table', 'décoration', 'détail', 'bougies'],
}

function frameTags(sceneKind: SceneKind, shot: Shot): string[] {
  const tags = new Set(TAGS[sceneKind])
  if (shot.detail) DETAIL_TAGS[shot.detail].forEach((t) => tags.add(t))
  const roles = new Set(shot.people.map((p) => p.role))
  if (roles.has('bride')) ['mariée', 'robe de mariée'].forEach((t) => tags.add(t))
  if (roles.has('groom')) ['marié', 'costume'].forEach((t) => tags.add(t))
  if (roles.has('bride') && roles.has('groom')) ['couple', 'mariés'].forEach((t) => tags.add(t))
  if (roles.has('bride') && (sceneKind === 'ceremony' || sceneKind === 'couple' || sceneKind === 'groups')) tags.add('bouquet')
  if (roles.has('child')) tags.add('enfant')
  if (roles.has('officiant')) tags.add('officiant')
  if (shot.people.length >= 5) tags.add('groupe')
  if (shot.people.length === 1) tags.add('portrait')
  if (shot.people.some((p) => p.smile > 0.75)) ['sourire', 'rires', 'émotion'].forEach((t) => tags.add(t))
  if (shot.people.some((p) => p.veil)) tags.add('voile')
  return [...tags]
}

function framesInMoment(rng: Rng, kind: SceneKind): number {
  const r = rng.next()
  const bursty = kind === 'party' || kind === 'ceremony' || kind === 'couple'
  if (r < (bursty ? 0.26 : 0.38)) return 1
  if (r < (bursty ? 0.6 : 0.74)) return rng.int(2, 3)
  return rng.int(4, bursty ? 9 : 6)
}

function measureFrame(rng: Rng, kind: SceneKind, plan: MomentPlan, momentBias: number, momentAesthetic: number): Measures {
  const party = kind === 'party'
  let focus = clamp(rng.range(0.74, 0.96) - (party ? 0.12 : kind === 'dinner' ? 0.06 : 0))
  if (rng.chance(party ? 0.1 : 0.05)) focus = rng.range(0.06, 0.32)
  const motion = rng.chance(party ? 0.14 : kind === 'dinner' ? 0.05 : 0.018) ? rng.range(0.55, 0.95) : rng.range(0, 0.18)
  const hasFaces = plan.people.length > 0
  let subjectFocus: number | null = hasFaces ? clamp(focus + rng.range(-0.06, 0.04)) : null
  if (hasFaces && focus > 0.5 && rng.chance(0.045)) subjectFocus = rng.range(0.1, 0.32)
  let exposureBias = momentBias + rng.range(-0.04, 0.04)
  if (rng.chance(0.035)) {
    const under = party || kind === 'dinner' ? rng.chance(0.75) : kind === 'couple' ? rng.chance(0.2) : rng.chance(0.5)
    exposureBias = (under ? -1 : 1) * rng.range(0.6, 0.92)
  }
  const aesthetic = clamp(momentAesthetic + rng.range(-0.07, 0.07) + (plan.detail ? 0.06 : 0))
  return { focus, motion, subjectFocus, exposureBias, eyes: null, aesthetic }
}

/** Generates a deterministic wedding shoot of `spec.count` photos. */
export function generateShoot(spec: ShootSpec): MockRecord[] {
  const rng = createRng(spec.seed)
  const totalWeight = spec.scenes.reduce((sum, k) => sum + SCHEDULE[k].weight, 0)
  const records: MockRecord[] = []
  const counters = BODIES.map(() => rng.int(1200, 8400))
  let momentId = 0
  let setupId = 0

  for (const kind of spec.scenes) {
    const target = Math.max(1, Math.round((spec.count * SCHEDULE[kind].weight) / totalWeight))
    const { start, end } = SCHEDULE[kind]
    const span = (end - start) * 3600_000
    let produced = 0
    let setupPlan: MomentPlan | null = null
    const sceneStart = records.length
    while (produced < target) {
      momentId += 1
      if (!setupPlan || rng.chance(0.45)) {
        setupId += 1
        setupPlan = composeMoment(kind, rng)
      }
      // A new moment re-uses the setup (same place, same people) most of the time.
      const plan: MomentPlan = rng.chance(0.6) ? { ...setupPlan, backdrop: setupPlan.backdrop } : composeMoment(kind, rng)
      const frames = Math.min(framesInMoment(rng, kind), target - produced)
      const bodyIndex = kind === 'groups' || plan.people.length > 3 ? 0 : rng.chance(0.55) ? 0 : 1
      const body = BODIES[bodyIndex]
      const lens = rng.pick(body.lenses)
      const focal = rng.pick(lens.focal)
      const aperture = kind === 'groups' ? rng.pick([4, 5, 5.6]) : rng.pick(lens.apertures)
      const momentBias = rng.range(-0.12, 0.12)
      const momentAesthetic = clamp(rng.range(0.3, 0.86) + (kind === 'couple' || kind === 'prep' ? 0.06 : 0) - (kind === 'party' ? 0.06 : 0))
      const momentSim = rng.range(0.9, 0.985)
      const iso = rng.pick(ISO[kind])
      const shutter = rng.pick(SHUTTER[kind])
      const burstGap = rng.chance(0.6) ? rng.range(150, 600) : rng.range(1500, 9000)

      for (let f = 0; f < frames; f++) {
        const m = measureFrame(rng, kind, plan, momentBias, momentAesthetic)
        const shot = frameFromMoment(plan, rng, {
          defocus: m.focus < 0.45 ? 0.15 + ((0.45 - m.focus) / 0.45) * 0.75 : 0,
          subjectDefocus: m.subjectFocus !== null && m.subjectFocus < 0.4 && m.focus > 0.5 ? 0.35 + (0.4 - m.subjectFocus) * 1.5 : 0,
          motion: m.motion > 0.45 ? m.motion : 0,
          exposure: Math.abs(m.exposureBias) > 0.1 ? m.exposureBias : 0,
          blink: kind === 'groups' ? 0.028 : 0.05,
        })
        const faces: FaceView[] = shot.people
          .filter((p) => p.size * 1000 > 18)
          .map((p) => {
            const [right, left] = p.eyes
            const box = faceBox(p, shot.aspect)
            const sizeFactor = clamp(p.size / 0.12, 0.55, 1)
            const sharp = clamp((m.subjectFocus ?? m.focus) * sizeFactor * (1 - m.motion * 0.6) + rng.range(-0.05, 0.05))
            return {
              bbox: { x: clamp(box.x), y: clamp(box.y), w: box.w, h: box.h },
              confidence: clamp(0.82 + p.size + rng.range(0, 0.08), 0, 0.995),
              rightEyeOpen: p.size > 0.04 ? right : null,
              leftEyeOpen: p.size > 0.04 ? left : null,
              eyesClosed: p.size > 0.04 ? right < 0.35 && left < 0.35 : null,
              sharpness: sharp,
            }
          })
          .sort((a, b) => b.bbox.h - a.bbox.h)
        const known = faces.filter((fc) => fc.eyesClosed !== null)
        if (known.length) {
          const opennessOf = (fc: FaceView) => ((fc.rightEyeOpen ?? 1) + (fc.leftEyeOpen ?? 1)) / 2
          m.eyes = kind === 'groups' ? Math.min(...known.map(opennessOf)) : opennessOf(known[0])
        }

        const portrait = shot.aspect < 1
        const index = records.length
        counters[bodyIndex] += 1
        const stem = `${body.prefix}${String(counters[bodyIndex] % 10000).padStart(4, '0')}`
        const twins = spec.twins && bodyIndex === 1
        const prev = records[records.length - 1]
        const scheduled = spec.day + start * 3600_000 + (span * produced) / target + rng.range(0, 25_000)
        const t =
          f === 0
            ? Math.max(scheduled, (prev?.capturedAt ?? 0) + rng.range(4_000, 20_000))
            : (prev?.capturedAt ?? spec.day) + burstGap + rng.range(-80, 80)
        const id = `p${spec.seed.toString(36)}-${index.toString(16).padStart(5, '0')}`
        records.push({
          id,
          index,
          fileName: `${stem}.CR3`,
          relPath: `${body.folder}/${stem}.CR3`,
          kind: 'raw',
          hasRaw: true,
          hasJpeg: twins,
          sizeBytes: Math.round(rng.range(body.rawMb[0], body.rawMb[1]) * 1024 * 1024) + (twins ? 9_800_000 : 0),
          capturedAt: Math.round(t),
          width: portrait ? body.height : body.width,
          height: portrait ? body.width : body.height,
          exif: { camera: body.model, lens: lens.name, iso, shutter, aperture, focalLength: focal },
          sceneKind: kind,
          momentId,
          setupId,
          momentSim: f === 0 ? 0 : clamp(momentSim + rng.range(-0.012, 0.012)),
          exactOf: null,
          shot,
          faces,
          m,
          tags: frameTags(kind, shot),
        })
        produced += 1
      }
    }
    // Keep the scene in chronological order (two bodies interleave).
    const scene = records.splice(sceneStart).sort((a, b) => a.capturedAt - b.capturedAt)
    records.push(...scene)
  }

  // A few exact duplicates (same file exported twice).
  const dupCount = Math.round(spec.count * 0.004)
  for (let i = 0; i < dupCount; i++) {
    const source = records[rng.int(0, records.length - 1)]
    if (source.exactOf) continue
    const stem = source.fileName.replace('.CR3', '')
    records.push({
      ...source,
      id: `${source.id}-copy`,
      fileName: `${stem}-2.JPG`,
      relPath: `${source.relPath.split('/')[0]}/${stem}-2.JPG`,
      kind: 'jpeg',
      hasRaw: false,
      hasJpeg: true,
      sizeBytes: Math.round(source.sizeBytes * 0.28),
      capturedAt: source.capturedAt,
      exactOf: source.id,
      momentSim: 0.998,
      m: { ...source.m, aesthetic: source.m.aesthetic - 0.01 },
    })
  }

  records.sort((a, b) => a.capturedAt - b.capturedAt || a.fileName.localeCompare(b.fileName))
  records.forEach((r, i) => (r.index = i))
  return records
}

export const WEDDING_SCENES: SceneKind[] = ['prep', 'ceremony', 'groups', 'couple', 'cocktail', 'dinner', 'party']
