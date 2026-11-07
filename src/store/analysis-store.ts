import { create } from 'zustand'
import type { AnalysisSummary, CullSettings, DecisionCounts, FinalDecision, PhotoView, Progress, ProjectSummary, QuotaDetails } from '@/lib/contract'
import { api, toCullError } from '@/lib/api'
import { t } from '@/lib/i18n'
import { notify } from '@/lib/notify'
import { playSound } from '@/lib/sound'

/** A photo that just went through the analysis (the analysis screen shows the last ones). */
export interface RecentPhoto {
  id: string
  thumbUrl: string
  decision: FinalDecision
}

const RECENT_MAX = 7

export interface AnalysisState {
  projectId: string | null
  /** project being analysed (name and size for the analysis screen) */
  project: ProjectSummary | null
  recent: RecentPhoto[]
  /** decisions of the photos analysed by this run only (a rerun starts from zero) */
  counts: DecisionCounts
  running: boolean
  settings: CullSettings | null
  progress: Progress | null
  startedAt: number | null
  lastSummary: AnalysisSummary | null
  error: { code: string; message: string; details?: QuotaDetails } | null
  prepare: (project: ProjectSummary) => void
  start: (projectId: string, settings: CullSettings) => Promise<void>
  onPhotos: (projectId: string, photos: PhotoView[]) => void
  cancel: () => Promise<void>
  onProgress: (progress: Progress & { projectId: string }) => void
  onDone: (summary: AnalysisSummary) => void
  onError: (event: { projectId: string; code: string; message: string; details?: QuotaDetails }) => void
  dismissError: () => void
}

/** Errors the analysis screen explains itself (plans, offers), without a toast. */
const QUOTA_CODES = new Set(['quotaExceeded', 'quotaOffline'])

const ZERO: DecisionCounts = { selected: 0, toReview: 0, rejected: 0 }
/** Decision of each photo of the current run, so a photo sent twice counts once. */
let decided = new Map<string, FinalDecision>()

export const useAnalysisStore = create<AnalysisState>((set, get) => ({
  projectId: null,
  project: null,
  recent: [],
  counts: ZERO,
  running: false,
  settings: null,
  progress: null,
  startedAt: null,
  lastSummary: null,
  error: null,

  prepare: (project) => {
    decided = new Map()
    set({ project, projectId: project.id, recent: [], counts: ZERO, lastSummary: null, error: null })
  },

  onPhotos: (projectId, photos) => {
    const s = get()
    if (!s.running || projectId !== s.projectId) return
    const analysed = photos.filter((p) => p.status === 'analyzed')
    if (analysed.length) {
      const counts = { ...s.counts }
      for (const p of analysed) {
        const before = decided.get(p.id)
        if (before) counts[before] -= 1
        counts[p.decision] += 1
        decided.set(p.id, p.decision)
      }
      set({ counts })
    }
    const fresh = analysed.filter((p): p is PhotoView & { thumbUrl: string } => p.thumbUrl !== null)
    if (!fresh.length) return
    const ids = new Set(fresh.map((p) => p.id))
    const recent = [...s.recent.filter((r) => !ids.has(r.id)), ...fresh.map((p) => ({ id: p.id, thumbUrl: p.thumbUrl, decision: p.decision }))]
    set({ recent: recent.slice(-RECENT_MAX) })
  },

  start: async (projectId, settings) => {
    decided = new Map()
    set({
      projectId,
      counts: ZERO,
      lastSummary: null,
      running: true,
      settings,
      startedAt: Date.now(),
      error: null,
      progress: { stage: 'scanning', done: 0, total: 0, current: null, photosPerSecond: 0, etaMs: null },
    })
    try {
      await api().startAnalysis(projectId, settings)
    } catch (error) {
      const e = toCullError(error)
      get().onError({ projectId, code: e.code, message: e.message, details: e.details as QuotaDetails | undefined })
    }
  },

  cancel: async () => {
    const { projectId, running } = get()
    if (!projectId || !running) return
    set({ running: false, progress: null, lastSummary: null })
    try {
      await api().cancelAnalysis(projectId)
    } finally {
      notify({ title: t('analysis.cancelled') })
    }
  },

  onProgress: (progress) => {
    const s = get()
    if (!s.running || progress.projectId !== s.projectId) return
    const { projectId: _ignored, ...rest } = progress
    void _ignored
    set({ progress: rest })
  },

  onDone: (summary) => {
    const s = get()
    if (summary.projectId !== s.projectId) return
    // A cancelled run also ends with a summary: only a real end celebrates.
    if (!s.running) return
    set({ running: false, lastSummary: summary, progress: null })
    playSound('analysisDone')
  },

  onError: (event) => {
    const s = get()
    if (event.projectId !== s.projectId || event.code === 'cancelled') return
    set({ running: false, progress: null, error: { code: event.code, message: event.message, details: event.details } })
    if (!QUOTA_CODES.has(event.code)) notify({ title: t('analysis.errorTitle'), description: event.message, tone: 'danger' })
  },

  dismissError: () => set({ error: null }),
}))
