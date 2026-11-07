import {
  DEFAULT_SETTINGS,
  type AnalysisSummary,
  type AppInfo,
  type AppSettings,
  type Assessment,
  type AuthState,
  type CullApi,
  type CullEvents,
  type CullSettings,
  type ExportOptions,
  type ExportReport,
  type GallerySummary,
  type ModelPack,
  type ModelPackState,
  type PhotoView,
  type Precision,
  type ProjectData,
  type ProjectSummary,
  type Reason,
  type Scene,
  type SimilarGroup,
  type Stage,
  type UpdateStatus,
  type UploadRequest,
  type UserMark,
} from '@/lib/contract'
import { countDecisions, EMPTY_MARK, finalDecision } from '@/lib/review/decision'
import { createRng, hashString } from '@/lib/procedural/random'
import { composeMoment, frameFromMoment } from '@/lib/procedural/compose'
import { normalizeCriteria } from '@/lib/criteria'
import { isLanguage } from '@/lib/i18n'
import { CullError } from '../errors'
import { assess, buildGroups, buildScenes, groupInfos } from './decide'
import {
  daysAgo,
  MOCK_APP_INFO,
  MOCK_GALLERIES,
  MOCK_PROJECTS,
  MOCK_SETTINGS,
  MOCK_UPDATE_VERSION,
  mockAccount,
  mockOffers,
  MOCK_PLANS,
  type MockPlan,
  nextMonthStart,
  NEW_FOLDER,
  SCENE_LABELS,
  shootDay,
  type MockProjectDef,
} from './fixtures'
import { MockImages } from './images'
import { searchRecords } from './search'
import { generateShoot, WEDDING_SCENES, type MockRecord } from './shoot'

export interface MockOptions {
  /** analysis/upload speed multiplier */
  speed?: number
  /** render procedural previews (needs a DOM canvas) */
  thumbnails?: boolean
  /** persistence of auth/settings between reloads */
  storage?: Pick<Storage, 'getItem' | 'setItem'> | null
  /**
   * overrides (`?mockAuth=signedIn&mockPlan=lite&mockUsed=950&mockCount=5000`,
   * `mockTrial=1`: a free account that can still start the Pelli trial,
   * `mockNoAccount=1`: a build without account, `mockUpdate=available`: a newer
   * version is published)
   */
  params?: URLSearchParams
  /** simulated latency of commands, ms */
  latency?: number
}

interface ProjectState {
  def: MockProjectDef
  createdAt: number
  lastOpenedAt: number
  records: MockRecord[] | null
  byId: Map<string, MockRecord>
  marks: Map<string, UserMark>
  assessments: Map<string, Assessment>
  analyzed: Set<string>
  analyzedAt: number | null
  precision: Precision | null
  settings: CullSettings
  groups: SimilarGroup[]
  scenes: Scene[]
  sceneOf: Map<string, number>
  thumbs: Map<string, string>
  cover: string | null
  stopAnalysis: (() => void) | null
  stopUpload: (() => void) | null
  thumbsScheduled: boolean
}

type Handler<E extends keyof CullEvents> = (payload: CullEvents[E]) => void

const STORAGE_AUTH = 'pelli-cull:mock-auth'
const STORAGE_PLAN = 'pelli-cull:mock-plan'
const STORAGE_USED = 'pelli-cull:mock-used'
const STORAGE_SETTINGS = 'pelli-cull:mock-settings'
const STORAGE_PACK = 'pelli-cull:mock-pack'

/** Size of the Maximum pack (the CPU variant of the models). */
const PACK_BYTES = 776_700_000
/** Analysis speed of the Maximum precision relative to Standard. */
const MAXIMUM_SLOWDOWN = 6

function safeStorage(): Pick<Storage, 'getItem' | 'setItem'> | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage
  } catch {
    return null
  }
}

function online(): boolean {
  return typeof navigator === 'undefined' || navigator.onLine !== false
}

/**
 * In-memory fake engine: a believable Pelli Instant for UI development,
 * screenshots and tests. Deterministic shoots, streamed analysis, account,
 * galleries, export and upload.
 */
export function createMockApi(options: MockOptions = {}): CullApi {
  const params = options.params ?? (typeof location !== 'undefined' ? new URLSearchParams(location.search) : new URLSearchParams())
  const storage = options.storage === undefined ? safeStorage() : options.storage
  const speed = options.speed ?? (Number(params.get('mockSpeed')) || 1)
  const latency = options.latency ?? 120
  const images = new MockImages(options.thumbnails ?? (params.get('mockThumbs') === '0' ? false : undefined))
  const now = () => Date.now()

  // ---- events ----------------------------------------------------------------
  const handlers = new Map<keyof CullEvents, Set<Handler<keyof CullEvents>>>()
  function emit<E extends keyof CullEvents>(event: E, payload: CullEvents[E]): void {
    handlers.get(event)?.forEach((h) => {
      try {
        ;(h as Handler<E>)(payload)
      } catch (error) {
        console.error(`[mock] ${event} handler failed`, error)
      }
    })
  }
  const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms))
  const respond = async <T>(value: T | (() => T), ms = latency): Promise<T> => {
    if (ms > 0) await wait(ms)
    return typeof value === 'function' ? (value as () => T)() : value
  }

  // ---- persistent bits ----------------------------------------------------------
  const read = (key: string): string | null => {
    try {
      return storage?.getItem(key) ?? null
    } catch {
      return null
    }
  }
  const write = (key: string, value: string) => {
    try {
      storage?.setItem(key, value)
    } catch {
      /* private mode */
    }
  }

  const isPlan = (v: string | null): v is MockPlan => v !== null && (MOCK_PLANS as string[]).includes(v)
  const paramPlan = params.get('mockPlan')
  if (isPlan(paramPlan)) write(STORAGE_PLAN, paramPlan)
  const paramAuth = params.get('mockAuth')
  if (paramAuth === 'signedIn' || paramAuth === 'signedOut') write(STORAGE_AUTH, paramAuth)
  // Photos sorted this month, kept per month like Pelli's counter.
  const month = new Date().toISOString().slice(0, 7)
  const paramUsed = params.get('mockUsed')
  if (paramUsed !== null) write(STORAGE_USED, `${month}:${Number(paramUsed) || 0}`)
  const savedUsed = read(STORAGE_USED)?.split(':')
  let used = paramUsed !== null ? Number(paramUsed) || 0 : savedUsed?.[0] === month ? Number(savedUsed[1]) || 0 : 0

  const trialAvailable = params.get('mockTrial') === '1'
  const storedPlan = read(STORAGE_PLAN)
  let plan: MockPlan = isPlan(paramPlan) ? paramPlan : isPlan(storedPlan) ? storedPlan : 'pro'
  let auth: AuthState =
    (paramAuth ?? read(STORAGE_AUTH)) === 'signedIn' ? { status: 'signedIn', account: mockAccount(plan, used, trialAvailable) } : { status: 'signedOut' }

  /** Counts `photos` on the monthly quota and tells the UI the new usage. */
  function countUsage(photos: number): void {
    used += photos
    write(STORAGE_USED, `${month}:${used}`)
    if (auth.status === 'signedIn') {
      auth = { status: 'signedIn', account: mockAccount(plan, used, trialAvailable) }
      emit('auth-changed', auth)
    }
  }
  let polls = 0

  let settings: AppSettings = { ...MOCK_SETTINGS }
  try {
    const saved = read(STORAGE_SETTINGS)
    if (saved) {
      const stored = JSON.parse(saved) as Partial<AppSettings>
      settings = { ...settings, ...stored, cull: normalizeCriteria(stored.cull ?? settings.cull) }
    }
  } catch {
    /* ignore */
  }
  const paramTheme = params.get('mockTheme')
  if (paramTheme === 'light' || paramTheme === 'dark' || paramTheme === 'system') settings.theme = paramTheme
  const paramLang = params.get('mockLang')
  if (isLanguage(paramLang)) settings.language = paramLang

  const baseInfo: AppInfo =
    params.get('mockModels') === 'missing'
      ? { ...MOCK_APP_INFO, modelsReady: false, capabilities: { faces: true, eyes: false, embeddings: false, aesthetic: false, textSearch: false, maximum: false } }
      : { ...MOCK_APP_INFO, platform: typeof window !== 'undefined' ? 'macos' : 'web' }
  const appInfo: AppInfo = { ...baseInfo, noAccount: params.get('mockNoAccount') === '1' }

  // ---- Maximum pack -----------------------------------------------------------------
  // `?mockPack=ready|paused|failed|absent`, `?mockDisk=low` (not enough space).
  const paramPack = params.get('mockPack')
  const freeBytes = params.get('mockDisk') === 'low' ? 420_000_000 : 182_000_000_000
  const initialPack = (): ModelPackState => {
    const stored = paramPack ?? read(STORAGE_PACK)
    if (stored === 'ready') return { status: 'ready' }
    if (stored === 'paused' || stored?.startsWith('part:'))
      return { status: 'failed', code: 'cancelled', message: 'Paused', receivedBytes: Number(stored.slice(5)) || Math.round(PACK_BYTES * 0.38), totalBytes: PACK_BYTES }
    if (stored === 'failed') return { status: 'failed', code: 'offline', message: 'No network', receivedBytes: Math.round(PACK_BYTES * 0.62), totalBytes: PACK_BYTES }
    return { status: 'absent' }
  }
  let packState: ModelPackState = initialPack()
  let stopPack: (() => void) | null = null
  const pack = (): ModelPack => ({
    id: 'maximum',
    sizeBytes: PACK_BYTES,
    diskBytes: PACK_BYTES,
    freeBytes,
    state: packState,
  })
  const setPack = (state: ModelPackState) => {
    packState = state
    appInfo.capabilities = { ...appInfo.capabilities, maximum: state.status === 'ready' }
    if (state.status === 'ready' || state.status === 'absent') write(STORAGE_PACK, state.status)
    else if (state.status === 'failed') write(STORAGE_PACK, `part:${state.receivedBytes}`)
    emit('model-pack', pack())
  }
  appInfo.capabilities = { ...appInfo.capabilities, maximum: packState.status === 'ready' }
  const receivedOf = (state: ModelPackState) =>
    state.status === 'downloading' || state.status === 'failed' ? state.receivedBytes : 0

  function runPackDownload(): void {
    if (packState.status === 'downloading' || packState.status === 'preparing' || packState.status === 'ready') return
    const start = receivedOf(packState)
    if (freeBytes < PACK_BYTES - start + 200_000_000) throw new CullError('diskFull', 'Not enough free disk space')
    if (!online()) throw new CullError('offline', 'No network')
    let stopped = false
    stopPack = () => {
      stopped = true
    }
    let received = start
    let rate: number | null = null
    let last = now()
    setPack({ status: 'downloading', receivedBytes: received, totalBytes: PACK_BYTES, bytesPerSecond: null })
    const tick = () => {
      if (stopped) return
      if (!online()) {
        stopPack = null
        setPack({ status: 'failed', code: 'offline', message: 'No network', receivedBytes: received, totalBytes: PACK_BYTES })
        return
      }
      const t = now()
      const dt = Math.max(0.001, (t - last) / 1000)
      last = t
      // ~28 MB/s with some jitter, faster with ?mockSpeed.
      const instant = 28_000_000 * speed * (0.75 + Math.random() * 0.5)
      received = Math.min(PACK_BYTES, received + instant * dt)
      rate = rate === null ? instant : rate * 0.8 + instant * 0.2
      if (received < PACK_BYTES) {
        setPack({ status: 'downloading', receivedBytes: Math.round(received), totalBytes: PACK_BYTES, bytesPerSecond: Math.round(rate) })
        setTimeout(tick, 250)
        return
      }
      let progress = 0
      const prepare = () => {
        if (stopped) return
        progress = Math.min(1, progress + 0.07 * speed)
        if (progress < 1) {
          setPack({ status: 'preparing', progress })
          setTimeout(prepare, 200)
        } else {
          stopPack = null
          setPack({ status: 'ready' })
        }
      }
      prepare()
    }
    setTimeout(tick, 250)
  }

  // ---- projects -------------------------------------------------------------------
  const projects = new Map<string, ProjectState>()
  const mockCount = Number(params.get('mockCount')) || null

  function addProject(def: MockProjectDef): ProjectState {
    const state: ProjectState = {
      def,
      createdAt: daysAgo(def.openedDaysAgo + 2),
      lastOpenedAt: daysAgo(def.openedDaysAgo),
      records: null,
      byId: new Map(),
      marks: new Map(),
      assessments: new Map(),
      analyzed: new Set(),
      analyzedAt: def.analyzed ? daysAgo(def.analyzed.daysAgo) : null,
      precision: def.analyzed ? 'standard' : null,
      // Like the engine: a project never analysed starts from the defaults.
      settings: def.analyzed ? { ...DEFAULT_SETTINGS } : { ...settings.cull },
      groups: [],
      scenes: [],
      sceneOf: new Map(),
      thumbs: new Map(),
      cover: null,
      stopAnalysis: null,
      stopUpload: null,
      thumbsScheduled: false,
    }
    projects.set(def.id, state)
    return state
  }
  MOCK_PROJECTS.forEach(addProject)

  function sceneLabel(kind: MockRecord['sceneKind']): string {
    return SCENE_LABELS[settings.language]?.[kind] ?? SCENE_LABELS.en[kind]
  }

  function ensureRecords(ps: ProjectState): MockRecord[] {
    if (ps.records) return ps.records
    const records = generateShoot({
      seed: ps.def.seed,
      count: ps.def.count,
      day: shootDay(ps.def.shotDaysAgo),
      scenes: ps.def.scenes,
      twins: ps.def.twins,
    })
    ps.records = records
    ps.byId = new Map(records.map((r) => [r.id, r]))
    if (ps.def.analyzed) {
      records.forEach((r) => ps.analyzed.add(r.id))
      decideAll(ps)
      seedMarks(ps)
    }
    return records
  }

  /** The photographer already reviewed part of an analysed shoot. */
  function seedMarks(ps: ProjectState): void {
    const rng = createRng(ps.def.seed * 31)
    const records = ps.records ?? []
    const reviewed = Math.floor(records.length * 0.35)
    for (let i = 0; i < reviewed; i++) {
      const r = records[i]
      const a = ps.assessments.get(r.id)
      if (!a) continue
      if (a.verdict === 'keep' && rng.chance(0.3)) ps.marks.set(r.id, { flag: 'pick', rating: rng.pick([3, 4, 4, 5]), label: 'none' })
      else if (a.verdict === 'maybe' && rng.chance(0.35)) ps.marks.set(r.id, { flag: rng.chance(0.55) ? 'pick' : 'reject', rating: 0, label: 'none' })
      else if (a.verdict === 'keep' && rng.chance(0.06)) ps.marks.set(r.id, { flag: 'none', rating: 0, label: rng.pick(['green', 'blue', 'yellow'] as const) })
    }
  }

  function decideAll(ps: ProjectState): void {
    const records = (ps.records ?? []).filter((r) => ps.analyzed.has(r.id))
    ps.groups = buildGroups(records, ps.settings)
    const infos = groupInfos(ps.groups, ps.byId, ps.settings)
    records.forEach((r) => ps.assessments.set(r.id, assess(r, ps.settings, infos.get(r.id) ?? null)))
    ps.scenes = buildScenes(records, ps.settings, sceneLabel)
    ps.sceneOf = new Map()
    ps.scenes.forEach((s) => s.photoIds.forEach((id) => ps.sceneOf.set(id, s.id)))
  }

  function view(ps: ProjectState, r: MockRecord): PhotoView {
    const analyzed = ps.analyzed.has(r.id)
    const assessment = analyzed ? (ps.assessments.get(r.id) ?? null) : null
    const mark = ps.marks.get(r.id) ?? EMPTY_MARK
    return {
      id: r.id,
      relPath: r.relPath,
      fileName: r.fileName,
      kind: r.kind,
      hasRaw: r.hasRaw,
      hasJpeg: r.hasJpeg,
      sizeBytes: r.sizeBytes,
      capturedAt: r.capturedAt,
      width: r.width,
      height: r.height,
      exif: r.exif,
      thumbUrl: ps.thumbs.get(r.id) ?? null,
      status: analyzed ? 'analyzed' : 'pending',
      error: null,
      assessment,
      faces: analyzed ? r.faces : [],
      mark,
      decision: finalDecision(assessment?.verdict, mark),
      sceneId: ps.sceneOf.get(r.id) ?? null,
    }
  }

  function thumbKey(ps: ProjectState, r: MockRecord): string {
    return `${ps.def.seed}:${r.shot.seed}`
  }

  async function coverOf(ps: ProjectState): Promise<string | null> {
    if (ps.cover) return ps.cover
    const records = ensureRecords(ps)
    const kinds = ['couple', 'ceremony', 'prep', 'groups'] as const
    const wanted = kinds.filter((k) => ps.def.scenes.includes(k))
    const kind = wanted[ps.def.seed % Math.max(1, wanted.length)] ?? ps.def.scenes[0]
    const clean = (r: MockRecord) =>
      r.sceneKind === kind && r.shot.people.length > 0 && r.m.focus > 0.75 && r.m.motion < 0.3 && Math.abs(r.m.exposureBias) < 0.15 && (r.m.eyes ?? 1) > 0.6
    const candidates = records.filter(clean)
    const pick = candidates.length ? candidates[Math.floor(candidates.length * 0.4)] : records[Math.floor(records.length / 3)]
    if (!pick) return null
    ps.cover = await images.thumb(thumbKey(ps, pick), pick.shot, true)
    return ps.cover
  }

  function summary(ps: ProjectState): ProjectSummary {
    const records = ensureRecords(ps)
    return {
      id: ps.def.id,
      name: ps.def.name,
      rootPath: ps.def.rootPath,
      photoCount: records.length,
      rawCount: records.filter((r) => r.hasRaw).length,
      createdAt: ps.createdAt,
      lastOpenedAt: ps.lastOpenedAt,
      analyzedAt: ps.analyzedAt,
      coverThumbUrl: ps.cover,
      counts: countDecisions(records.map((r) => view(ps, r))),
      settings: ps.settings,
      precision: ps.precision,
    }
  }

  function data(ps: ProjectState): ProjectData {
    const records = ensureRecords(ps)
    return {
      project: summary(ps),
      photos: records.map((r) => view(ps, r)),
      groups: ps.groups,
      scenes: ps.scenes,
    }
  }

  function project(projectId: string): ProjectState {
    const ps = projects.get(projectId)
    if (!ps) throw new CullError('notFound', `Unknown project ${projectId}`)
    return ps
  }

  /** Background generation of the previews of an analysed shoot. */
  function scheduleThumbs(ps: ProjectState): void {
    if (ps.thumbsScheduled || !images.enabled) return
    ps.thumbsScheduled = true
    // What the stacked grid shows first: singletons and the best frame of each group.
    const lead = new Set(ps.groups.map((g) => g.photoIds[0]))
    const grouped = new Set(ps.groups.flatMap((g) => g.photoIds))
    const records = ensureRecords(ps)
      .filter((r) => ps.analyzed.has(r.id) && !ps.thumbs.has(r.id))
      .map((r, i) => ({ r, key: (lead.has(r.id) || !grouped.has(r.id) ? 0 : 1) * 1e6 + i }))
      .sort((a, b) => a.key - b.key)
      .map(({ r }) => r)
    let batch: PhotoView[] = []
    let timer: ReturnType<typeof setTimeout> | null = null
    const flush = () => {
      timer = null
      if (!batch.length) return
      emit('photos-updated', { projectId: ps.def.id, photos: batch })
      batch = []
    }
    records.forEach((r) => {
      void images.thumb(thumbKey(ps, r), r.shot).then((url) => {
        if (!url) return
        ps.thumbs.set(r.id, url)
        batch.push(view(ps, r))
        timer ??= setTimeout(flush, 140)
      })
    })
  }

  // ---- analysis ---------------------------------------------------------------------
  function runAnalysis(ps: ProjectState, cullSettings: CullSettings): void {
    const records = ensureRecords(ps)
    const projectId = ps.def.id
    const started = now()
    let stopped = false
    ps.stopAnalysis = () => {
      stopped = true
    }
    ps.settings = { ...cullSettings }
    ps.analyzed.clear()
    ps.assessments.clear()
    ps.groups = []
    ps.scenes = []
    ps.sceneOf = new Map()
    ps.analyzedAt = null
    const precision: Precision = settings.precision === 'maximum' && packState.status === 'ready' ? 'maximum' : 'standard'

    const total = records.length
    const rate = (19 * speed) / (precision === 'maximum' ? MAXIMUM_SLOWDOWN : 1)
    let done = 0
    let measuredRate = 0
    let lastTick = now()

    const progress = (stage: Stage, d: number, current: string | null) => {
      const remaining = stage === 'analyzing' ? total - d : 0
      emit('analysis-progress', {
        projectId,
        stage,
        done: d,
        total,
        current,
        photosPerSecond: Math.round(measuredRate * 10) / 10,
        etaMs: stage === 'analyzing' && measuredRate > 0 ? Math.round((remaining / measuredRate) * 1000 + 1500 / speed) : null,
      })
    }

    // Scanning: quick pass over the files.
    let scanned = 0
    const scan = () => {
      if (stopped) return
      scanned = Math.min(total, scanned + Math.ceil(total / 8))
      progress('scanning', scanned, records[scanned - 1]?.relPath ?? null)
      if (scanned < total) setTimeout(scan, 90 / speed)
      else if (precision === 'maximum') {
        // The pack's models load before the first photo, like the engine.
        emit('analysis-progress', { projectId, stage: 'loading', done: 0, total: 0, current: null, photosPerSecond: 0, etaMs: null })
        setTimeout(analyze, 900 / speed)
      } else setTimeout(analyze, 120 / speed)
    }

    let budget = 0
    const inflight: Promise<unknown>[] = []
    const analyze = () => {
      if (stopped) return
      const t = now()
      const dt = (t - lastTick) / 1000
      lastTick = t
      budget += rate * dt * (0.8 + Math.random() * 0.4)
      const n = Math.min(total - done, Math.floor(budget))
      budget -= n
      const batch = records.slice(done, done + n)
      done += n
      const instant = n / Math.max(dt, 0.001)
      measuredRate = measuredRate ? measuredRate * 0.85 + instant * 0.15 : rate
      inflight.push(Promise.all(
        batch.map(async (r) => {
          const url = await images.thumb(thumbKey(ps, r), r.shot, true)
          if (stopped) return
          if (url) ps.thumbs.set(r.id, url)
          ps.analyzed.add(r.id)
          ps.assessments.set(r.id, assess(r, ps.settings, null))
          emit('analysis-photo', { projectId, photo: view(ps, r) })
        }),
      ))
      progress('analyzing', done, records[done - 1]?.fileName ?? null)
      if (done < total) setTimeout(analyze, 110)
      else void Promise.all(inflight).then(() => setTimeout(group, 200 / speed))
    }

    let groupedSteps = 0
    const group = () => {
      if (stopped) return
      groupedSteps += 1
      progress('grouping', Math.min(total, Math.round((total * groupedSteps) / 6)), null)
      if (groupedSteps < 6) {
        setTimeout(group, 180 / speed)
        return
      }
      decideAll(ps)
      ps.analyzedAt = now()
      ps.precision = precision
      ps.stopAnalysis = null
      progress('done', total, null)
      const reasons: Partial<Record<Reason, number>> = {}
      ps.assessments.forEach((a) => a.reasons.forEach((reason) => (reasons[reason] = (reasons[reason] ?? 0) + 1)))
      const result: AnalysisSummary = {
        projectId,
        analyzed: ps.analyzed.size,
        failed: 0,
        durationMs: now() - started,
        counts: summary(ps).counts,
        reasons,
        groups: ps.groups.length,
        precision,
      }
      emit('analysis-done', result)
    }

    setTimeout(scan, 60)
  }

  // ---- upload ---------------------------------------------------------------------------
  function runUpload(ps: ProjectState, request: UploadRequest): void {
    const projectId = ps.def.id
    const records = ensureRecords(ps)
    const wanted = records.filter((r) => {
      const decision = view(ps, r).decision
      return decision === 'selected' || (request.which === 'selectedAndToReview' && decision === 'toReview')
    })
    const gallery = request.galleryId ? MOCK_GALLERIES.find((g) => g.id === request.galleryId) : null
    const galleryId = gallery?.id ?? `gal-${hashString(request.newGallery?.title ?? 'new').toString(36)}`
    const slug = gallery?.slug ?? (request.newGallery?.title ?? 'galerie').toLowerCase().normalize('NFD').replace(/[\u0300-\u036f]/g, '').replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)/g, '')
    let stopped = false
    ps.stopUpload = () => {
      stopped = true
    }
    if (!online()) {
      ps.stopUpload = null
      setTimeout(() => emit('upload-error', { projectId, code: 'offline', message: 'No network' }), 300)
      return
    }
    let done = 0
    let bytes = 0
    const tick = () => {
      if (stopped) return
      const n = Math.min(wanted.length - done, Math.max(1, Math.round(4 * speed)))
      for (let i = 0; i < n; i++) bytes += Math.round(wanted[done + i].sizeBytes * 0.22)
      done += n
      emit('upload-progress', { projectId, galleryId, done, total: wanted.length, bytesSent: bytes, current: wanted[done - 1]?.fileName ?? null })
      if (done < wanted.length) setTimeout(tick, 160)
      else {
        ps.stopUpload = null
        emit('upload-done', {
          projectId,
          galleryId,
          galleryUrl: `https://app.pelli.io/dashboard/gallery/${slug}`,
          uploaded: wanted.length,
          failed: 0,
        })
      }
    }
    setTimeout(tick, 400)
  }

  // ---- app updates ------------------------------------------------------------------------
  // `?mockUpdate=available`: a newer version is published, found by the launch
  // check; `offline`: published, but its first download fails (a retry works);
  // `disabled`: a copy that never updates. Default: nothing new.
  const paramUpdate = params.get('mockUpdate')
  const published = paramUpdate === 'available' || paramUpdate === 'offline' ? MOCK_UPDATE_VERSION : null
  let failDownload = paramUpdate === 'offline'
  let updateStatus: UpdateStatus = paramUpdate === 'disabled' ? { status: 'disabled' } : { status: 'idle' }
  let exports = 0
  const setUpdate = (status: UpdateStatus) => {
    updateStatus = status
    emit('update-status', status)
  }
  /** Like `Engine::busy`: an analysis, an export, an upload or the Maximum pack under way. */
  const busy = () =>
    exports > 0 ||
    packState.status === 'downloading' ||
    packState.status === 'preparing' ||
    [...projects.values()].some((ps) => ps.stopAnalysis !== null || ps.stopUpload !== null)

  /** A check from the window reports its outcome; a background one only news. */
  function checkUpdate(manual: boolean): UpdateStatus {
    const { status } = updateStatus
    if (status === 'disabled' || status === 'downloading' || status === 'restarting') return updateStatus
    if (!online()) {
      if (manual) setUpdate({ status: 'failed', code: 'offline', message: 'No network', version: null })
    } else setUpdate(published ? { status: 'available', version: published } : { status: 'upToDate' })
    return updateStatus
  }
  // The launch check (8 s after launch in the app).
  if (published) setTimeout(() => checkUpdate(false), 2500 / speed)

  function runUpdate(version: string): void {
    let progress = 0
    setUpdate({ status: 'downloading', version, progress })
    const restart = () => {
      if (busy()) {
        if (updateStatus.status !== 'restarting') setUpdate({ status: 'restarting', version, waiting: true })
        setTimeout(restart, 250)
        return
      }
      setUpdate({ status: 'restarting', version, waiting: false })
      // The app restarts on the new version: the page reloads, the update done.
      if (typeof location !== 'undefined') {
        const url = new URL(location.href)
        url.searchParams.delete('mockUpdate')
        setTimeout(() => location.replace(url), 1200)
      }
    }
    const tick = () => {
      // ~32 MB at ~12 MB/s, faster with ?mockSpeed.
      progress = Math.min(1, progress + 0.045 * speed * (0.6 + Math.random() * 0.8))
      if (!online() || (failDownload && progress >= 0.4)) {
        failDownload = false
        setUpdate({ status: 'failed', code: 'offline', message: 'No network', version })
        return
      }
      if (progress < 1) {
        setUpdate({ status: 'downloading', version, progress })
        setTimeout(tick, 120)
      } else restart()
    }
    setTimeout(tick, 120)
  }

  // ---- API --------------------------------------------------------------------------------
  const api: CullApi = {
    appInfo: () => respond(appInfo, 40),
    getSettings: () => respond(() => ({ ...settings }), 30),
    setSettings: (patch) =>
      respond(() => {
        settings = { ...settings, ...patch, ...(patch.cull ? { cull: { ...patch.cull, keepPerGroup: Math.min(5, Math.max(1, patch.cull.keepPerGroup)) } } : {}) }
        write(
          STORAGE_SETTINGS,
          JSON.stringify({
            theme: settings.theme,
            language: settings.language,
            acceleration: settings.acceleration,
            background: settings.background,
            precision: settings.precision,
            cull: settings.cull,
          }),
        )
        return { ...settings }
      }, 30),
    clearCache: () =>
      respond(() => {
        settings = { ...settings, cacheSizeBytes: 0 }
        return { ...settings }
      }, 700),

    authStatus: () => respond(() => auth, 60),
    authStart: () =>
      respond(() => {
        polls = 0
        auth = { status: 'pending', authUrl: 'https://app.pelli.io/cull/connect?code=K7QM-4ZTP', expiresAt: now() + 10 * 60_000 }
        emit('auth-changed', auth)
        return auth
      }, 400),
    authPoll: () =>
      respond(() => {
        if (auth.status !== 'pending') return auth
        polls += 1
        if (polls >= 2) {
          auth = { status: 'signedIn', account: mockAccount(plan, used, trialAvailable) }
          write(STORAGE_AUTH, 'signedIn')
          emit('auth-changed', auth)
        }
        return auth
      }),
    authLogout: () =>
      respond(() => {
        auth = { status: 'signedOut' }
        write(STORAGE_AUTH, 'signedOut')
        emit('auth-changed', auth)
        return auth
      }),
    refreshAccount: () =>
      respond(() => {
        const stored = read(STORAGE_PLAN)
        plan = isPlan(paramPlan) ? paramPlan : isPlan(stored) ? stored : plan
        if (auth.status === 'signedIn') auth = { status: 'signedIn', account: mockAccount(plan, used, trialAvailable) }
        return auth
      }, 300),
    getOffers: (lang) =>
      respond(() => {
        if (!online()) throw new CullError('offline', 'No network')
        return mockOffers(lang)
      }, 250),

    modelPack: () => respond(pack, 40),
    downloadModelPack: () => respond(() => runPackDownload(), 80),
    cancelModelPack: () =>
      respond(() => {
        if (packState.status === 'downloading' || packState.status === 'preparing') {
          stopPack?.()
          stopPack = null
          setPack({ status: 'failed', code: 'cancelled', message: 'Paused', receivedBytes: receivedOf(packState) || PACK_BYTES, totalBytes: PACK_BYTES })
        }
        return pack()
      }, 60),
    removeModelPack: () =>
      respond(() => {
        stopPack?.()
        stopPack = null
        setPack({ status: 'absent' })
        return pack()
      }, 400),

    pickFolder: () => respond(NEW_FOLDER.rootPath, 350),
    listProjects: async () => {
      await wait(latency)
      const list = [...projects.values()].sort((a, b) => b.lastOpenedAt - a.lastOpenedAt)
      await Promise.all(list.map(coverOf))
      return list.map(summary)
    },
    openFolder: async (path, recursive) => {
      await wait(500 / Math.max(1, speed / 2))
      const lower = path.toLowerCase()
      if (lower.includes('verrouill') || lower.includes('protected')) throw new CullError('permissionDenied', `Operation not permitted: ${path}`)
      if (lower.includes('vide') || lower.includes('empty')) throw new CullError('noPhotos', `No supported photo in ${path}`)
      const existing = [...projects.values()].find((p) => p.def.rootPath === path)
      if (existing) {
        existing.lastOpenedAt = now()
        // Never analysed: still a new project, it follows the current defaults.
        if (existing.analyzedAt === null && existing.analyzed.size === 0 && !existing.stopAnalysis) existing.settings = { ...settings.cull }
        return summary(existing)
      }
      const name = path.split(/[\\/]/).filter(Boolean).pop() ?? 'Nouveau dossier'
      const isNew = path === NEW_FOLDER.rootPath
      const count = mockCount ?? (isNew ? NEW_FOLDER.count : 600 + (hashString(path) % 900))
      const def: MockProjectDef = {
        id: `prj-${hashString(path).toString(36)}`,
        name: name.replace(/^\d{4}-\d{2}-\d{2}\s*/, ''),
        rootPath: path,
        count: recursive ? count : Math.round(count * 0.56),
        seed: isNew ? NEW_FOLDER.seed : hashString(path) % 10_000,
        scenes: WEDDING_SCENES,
        twins: true,
        shotDaysAgo: 2,
        openedDaysAgo: 0,
        analyzed: null,
      }
      const ps = addProject(def)
      ps.createdAt = now()
      ps.lastOpenedAt = now()
      void coverOf(ps)
      return summary(ps)
    },
    removeProject: (projectId) =>
      respond(() => {
        projects.get(projectId)?.stopAnalysis?.()
        projects.delete(projectId)
      }),
    getProject: async (projectId) => {
      await wait(latency)
      const ps = project(projectId)
      ps.lastOpenedAt = now()
      const result = data(ps)
      if (ps.analyzedAt) scheduleThumbs(ps)
      return result
    },

    startAnalysis: (projectId, cullSettings) =>
      respond(() => {
        const ps = project(projectId)
        ps.stopAnalysis?.()
        const criteria = normalizeCriteria(cullSettings ?? settings.cull)
        // Like the engine: photos never analysed count on the monthly quota.
        const fresh = ps.analyzedAt !== null || ps.analyzed.size > 0 ? 0 : ensureRecords(ps).length
        const quota = auth.status === 'signedIn' ? auth.account.entitlements.monthlyQuota : null
        if (!appInfo.noAccount && fresh > 0 && quota !== null) {
          const fail = (code: 'quotaExceeded' | 'quotaOffline', details: CullEvents['analysis-error']['details']) => {
            setTimeout(() => emit('analysis-error', { projectId, code, message: code, details }), 900 / speed)
          }
          if (!online()) return fail('quotaOffline', { monthlyQuota: quota, needed: fresh })
          const remaining = Math.max(0, quota - used)
          if (fresh > remaining) {
            return fail('quotaExceeded', { monthlyQuota: quota, usedThisMonth: used, remaining, resetsAt: nextMonthStart(), needed: fresh })
          }
          countUsage(fresh)
        }
        runAnalysis(ps, criteria)
      }, 80),
    cancelAnalysis: (projectId) =>
      respond(() => {
        const ps = project(projectId)
        ps.stopAnalysis?.()
        ps.stopAnalysis = null
      }, 60),
    recompute: (projectId, cullSettings) =>
      respond(() => {
        const ps = project(projectId)
        ps.settings = normalizeCriteria(cullSettings)
        ensureRecords(ps)
        decideAll(ps)
        return data(ps)
      }, 160),

    setMarks: (projectId, photoIds, patch) =>
      respond(() => {
        const ps = project(projectId)
        return photoIds.flatMap((id) => {
          const r = ps.byId.get(id)
          if (!r) return []
          ps.marks.set(id, { ...(ps.marks.get(id) ?? EMPTY_MARK), ...patch })
          return [view(ps, r)]
        })
      }, 40),
    largePreview: async (projectId, photoId) => {
      const ps = project(projectId)
      const r = ps.byId.get(photoId)
      if (!r) throw new CullError('notFound', `Unknown photo ${photoId}`)
      await wait(30)
      return images.largePreview(thumbKey(ps, r), r.shot)
    },
    search: (projectId, query) =>
      respond(() => {
        const ps = project(projectId)
        return searchRecords(ensureRecords(ps).filter((r) => ps.analyzed.has(r.id)), query)
      }, 320),

    exportProject: async (projectId, exportOptions: ExportOptions): Promise<ExportReport> => {
      const ps = project(projectId)
      const views = ensureRecords(ps).map((r) => view(ps, r))
      exports += 1
      await wait(Math.min(2600, 700 + views.length * 1.2) / speed).finally(() => {
        exports -= 1
      })
      const selected = views.filter((v) => v.decision === 'selected')
      const rejected = views.filter((v) => v.decision === 'rejected')
      const perPhoto = (v: PhotoView) =>
        (exportOptions.copySelected.includeRaw && v.hasRaw ? 1 : 0) + (exportOptions.copySelected.includeJpeg && v.hasJpeg ? 1 : 0) || 1
      return {
        xmpWritten: exportOptions.xmp.enabled ? views.filter((v) => v.hasRaw).length : 0,
        copied: exportOptions.copySelected.enabled ? selected.reduce((n, v) => n + perPhoto(v), 0) : 0,
        moved: exportOptions.moveRejected.enabled ? rejected.length : 0,
        fileListPath: exportOptions.fileList ? `${ps.def.rootPath}/Pelli Instant - sélection.txt` : null,
        errors: [],
      }
    },
    listGalleries: async (): Promise<GallerySummary[]> => {
      await wait(300)
      return Promise.all(
        MOCK_GALLERIES.map(async (g) => {
          const rng = createRng(g.seed)
          const shot = frameFromMoment(composeMoment(rng.pick(['couple', 'ceremony', 'cocktail'] as const), rng), rng, {
            defocus: 0,
            subjectDefocus: 0,
            motion: 0,
            exposure: 0,
            blink: 0,
          })
          return {
            id: g.id,
            title: g.title,
            slug: g.slug,
            status: g.status,
            imageCount: g.imageCount,
            coverUrl: g.imageCount ? await images.thumb(`gallery:${g.id}`, shot, true) : null,
            createdAt: daysAgo(g.daysAgo),
          }
        }),
      )
    },
    uploadToPelli: (projectId, request) =>
      respond(() => {
        const ps = project(projectId)
        if (auth.status !== 'signedIn') throw new CullError('unauthorized', 'Sign in to Pelli first')
        runUpload(ps, request)
      }, 200),
    cancelUpload: (projectId) =>
      respond(() => {
        const ps = project(projectId)
        ps.stopUpload?.()
        ps.stopUpload = null
      }, 60),

    revealInFolder: (path) => respond(() => console.info(`[mock] reveal ${path}`), 20),
    openExternal: (url) =>
      respond(() => {
        if (typeof window !== 'undefined' && !params.has('mockNoPopups')) window.open(url, '_blank', 'noopener')
      }, 20),

    updateStatus: () => respond(() => updateStatus, 20),
    checkForUpdate: async () => {
      if (updateStatus.status !== 'disabled' && updateStatus.status !== 'downloading' && updateStatus.status !== 'restarting') setUpdate({ status: 'checking' })
      await wait(900 / speed)
      return checkUpdate(true)
    },
    installUpdate: () =>
      respond(() => {
        const version = updateStatus.status === 'available' || updateStatus.status === 'failed' ? updateStatus.version : null
        if (version) runUpdate(version)
      }, 40),

    on(event, handler) {
      let set = handlers.get(event)
      if (!set) {
        set = new Set()
        handlers.set(event, set)
      }
      const h = handler as Handler<keyof CullEvents>
      set.add(h)
      return () => set.delete(h)
    },
  }
  return api
}
