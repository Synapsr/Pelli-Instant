import { create } from 'zustand'
import type { AppInfo, AppSettings, AuthState, CullSettings, ProjectSummary } from '@/lib/contract'
import { api, initApi, toCullError } from '@/lib/api'
import { detectLanguage, isLanguage, t, useI18nStore } from '@/lib/i18n'
import { notify } from '@/lib/notify'
import { useAnalysisStore } from './analysis-store'
import { usePlusStore } from './plus-store'
import { usePrefsStore } from './prefs-store'
import { useReviewStore } from './review-store'
import { useWorkspaceStore } from './workspace-store'

export type Screen = 'onboarding' | 'home' | 'analysis' | 'workspace'
export type Dialog = 'settings' | 'export' | 'shortcuts' | 'preAnalysis'
export type SettingsTab = 'plus' | 'general' | 'analysis' | 'privacy' | 'cache' | 'about'

/** Settings the interface changes itself (the engine keeps the cache's own). */
export type SettingsPatch = Partial<Pick<AppSettings, 'theme' | 'language' | 'acceleration' | 'background' | 'precision' | 'cull' | 'telemetry' | 'onboarded'>>

export interface OpenError {
  code: string
  message: string
  path: string
}

export interface AppState {
  phase: 'boot' | 'ready' | 'error'
  bootError: string | null
  screen: Screen
  appInfo: AppInfo | null
  settings: AppSettings | null
  /** optional Pelli account: only a second way to get Plus */
  auth: AuthState
  online: boolean
  projects: ProjectSummary[]
  projectsLoaded: boolean
  opening: string | null
  openError: OpenError | null
  loadingProject: boolean
  dialog: Dialog | null
  settingsTab: SettingsTab
  /** project waiting for its first analysis: the pre-analysis dialog shows its criteria */
  pendingAnalysis: ProjectSummary | null

  boot: () => Promise<void>
  setAuth: (auth: AuthState) => void
  setOnline: (online: boolean) => void
  updateSettings: (patch: SettingsPatch) => Promise<void>
  clearCache: () => Promise<void>
  refreshProjects: () => Promise<void>
  refreshAccount: () => Promise<void>
  /** show the welcome tour again (from the settings) */
  startOnboarding: () => void
  /** end of the welcome: never shown again on its own; `telemetry` is the choice of its last step */
  finishOnboarding: (telemetry: boolean) => void
  /** the home screen plays its arrival (orb from afar, sound): on launch and after the tour */
  homeIntro: boolean
  consumeHomeIntro: () => void
  pickAndOpen: () => Promise<void>
  openFolder: (path: string) => Promise<void>
  openProject: (project: ProjectSummary) => Promise<void>
  removeProject: (projectId: string) => Promise<void>
  /** analyse `project` with its criteria (or `settings`), on the analysis screen */
  analyze: (project: ProjectSummary, settings?: CullSettings) => Promise<void>
  /** start the pending first analysis with the criteria chosen for it, optionally kept as the defaults */
  confirmAnalysis: (settings: CullSettings, remember: boolean) => Promise<void>
  dismissAnalysis: () => void
  /** leave the analysis results for the review, on the most useful tab */
  reviewResults: () => void
  dismissOpenError: () => void
  goHome: () => void
  openDialog: (dialog: Dialog, tab?: SettingsTab) => void
  closeDialog: () => void
  signOut: () => Promise<void>
}

/** Review starts where the photographer's eye is needed: "to review" first. */
function openBestTab(): void {
  const review = useReviewStore.getState()
  const counts = review.tabCounts
  review.setTab(counts.toReview > 0 ? 'toReview' : counts.selected > 0 ? 'selected' : 'rejected')
}

/** Saves the computer's language as the app's (first launch). */
async function firstLanguage(engine: Awaited<ReturnType<typeof initApi>>): Promise<AppSettings> {
  const language = detectLanguage()
  const saved = await engine.setSettings({ language })
  return { ...saved, language }
}

export const useAppStore = create<AppState>((set, get) => ({
  phase: 'boot',
  bootError: null,
  screen: 'home',
  appInfo: null,
  settings: null,
  auth: { status: 'signedOut' },
  online: typeof navigator === 'undefined' ? true : navigator.onLine,
  projects: [],
  projectsLoaded: false,
  opening: null,
  openError: null,
  loadingProject: false,
  dialog: null,
  settingsTab: 'general',
  pendingAnalysis: null,

  boot: async () => {
    try {
      const engine = await initApi()
      const [appInfo, stored, auth] = await Promise.all([engine.appInfo(), engine.getSettings(), engine.authStatus()])
      // First launch: no language chosen yet, the app speaks the computer's.
      const settings = isLanguage(stored.language) ? stored : await firstLanguage(engine)
      useI18nStore.getState().setLanguage(settings.language)
      // No account, no gate: the welcome the first time, then the drop zone.
      const screen: Screen = settings.onboarded === false ? 'onboarding' : 'home'
      await usePlusStore.getState().load()
      set({ appInfo, settings, auth, phase: 'ready', screen, homeIntro: screen === 'home' })
      void get().refreshProjects()
      // A cached Pelli account works offline; online, check that its token is
      // still valid and refresh its plan (whether it includes Plus).
      if (auth.status === 'signedIn') void get().refreshAccount()
    } catch (error) {
      set({ phase: 'error', bootError: toCullError(error).message })
    }
  },

  setAuth: (auth) => set({ auth }),

  setOnline: (online) => set({ online }),

  updateSettings: async (patch) => {
    const prev = get().settings
    if (prev) set({ settings: { ...prev, ...patch } })
    if (patch.language) useI18nStore.getState().setLanguage(patch.language)
    try {
      const settings = await api().setSettings(patch)
      set({ settings })
      if (patch.language) void usePlusStore.getState().loadOffer()
    } catch (error) {
      if (prev) set({ settings: prev })
      if (prev && patch.language) useI18nStore.getState().setLanguage(prev.language)
      notify({ title: t('errors.generic'), description: toCullError(error).message, tone: 'danger' })
    }
  },

  clearCache: async () => {
    const settings = await api().clearCache()
    set({ settings })
    notify({ title: t('settings.cache.cleared'), tone: 'success' })
  },

  refreshProjects: async () => {
    try {
      const projects = await api().listProjects()
      set({ projects, projectsLoaded: true })
    } catch (error) {
      set({ projectsLoaded: true })
      notify({ title: t('errors.generic'), description: toCullError(error).message, tone: 'danger' })
    }
  },

  refreshAccount: async () => {
    try {
      get().setAuth(await api().refreshAccount())
    } catch {
      /* keep the last known account */
    }
  },

  startOnboarding: () => set({ screen: 'onboarding', dialog: null }),

  homeIntro: false,
  consumeHomeIntro: () => set({ homeIntro: false }),

  finishOnboarding: (telemetry) => {
    const settings = get().settings
    // One change: the engine counts the install only when telemetry stays on.
    if (settings && (!settings.onboarded || settings.telemetry !== telemetry)) void get().updateSettings({ onboarded: true, telemetry })
    set({ screen: 'home', homeIntro: true })
    void get().refreshProjects()
  },

  pickAndOpen: async () => {
    const path = await api().pickFolder()
    if (path) await get().openFolder(path)
  },

  openFolder: async (path) => {
    if (get().opening) return
    set({ opening: path, openError: null })
    try {
      const project = await api().openFolder(path, usePrefsStore.getState().includeSubfolders)
      set({ opening: null })
      if (project.photoCount === 0) {
        set({ openError: { code: 'noPhotos', message: '', path } })
        return
      }
      void get().refreshProjects()
      await get().openProject(project)
    } catch (error) {
      const e = toCullError(error)
      set({ opening: null, openError: { code: e.code, message: e.message, path } })
    }
  },

  openProject: async (project) => {
    const analysis = useAnalysisStore.getState()
    // Never analysed (or interrupted): its criteria first, then the analysis.
    if (!project.analyzedAt && !(analysis.running && analysis.projectId === project.id)) {
      set({ pendingAnalysis: project, dialog: 'preAnalysis' })
      return
    }
    if (analysis.running && analysis.projectId === project.id) {
      set({ screen: 'analysis' })
      return
    }
    useWorkspaceStore.getState().reset()
    set({ screen: 'workspace', loadingProject: true })
    try {
      const data = await api().getProject(project.id)
      useReviewStore.getState().load(data)
      openBestTab()
    } catch (error) {
      notify({ title: t('errors.generic'), description: toCullError(error).message, tone: 'danger' })
      set({ screen: 'home' })
    } finally {
      set({ loadingProject: false })
    }
  },

  removeProject: async (projectId) => {
    set({ projects: get().projects.filter((p) => p.id !== projectId) })
    try {
      await api().removeProject(projectId)
    } finally {
      void get().refreshProjects()
    }
  },

  analyze: async (project, settings = project.settings) => {
    useWorkspaceStore.getState().reset()
    useAnalysisStore.getState().prepare({ ...project, settings })
    set({ screen: 'analysis', dialog: null, pendingAnalysis: null })
    try {
      const data = await api().getProject(project.id)
      useReviewStore.getState().load(data)
    } catch {
      /* the analysis streams the photos anyway */
    }
    await useAnalysisStore.getState().start(project.id, settings)
  },

  confirmAnalysis: async (settings, remember) => {
    const project = get().pendingAnalysis
    if (!project) return
    if (remember) void get().updateSettings({ cull: settings })
    await get().analyze(project, settings)
  },

  dismissAnalysis: () => set({ dialog: null, pendingAnalysis: null }),

  reviewResults: () => {
    openBestTab()
    useWorkspaceStore.getState().reset()
    set({ screen: 'workspace' })
  },
  dismissOpenError: () => set({ openError: null }),

  goHome: () => {
    useWorkspaceStore.getState().reset()
    set({ screen: 'home', dialog: null, pendingAnalysis: null })
    void get().refreshProjects()
  },

  openDialog: (dialog, tab) => set({ dialog, settingsTab: tab ?? get().settingsTab }),
  closeDialog: () => set({ dialog: null }),

  signOut: async () => {
    get().setAuth(await api().authLogout())
  },
}))
