import { create } from 'zustand'
import type { AppInfo, AppSettings, AuthState, CullSettings, Offers, ProjectSummary } from '@/lib/contract'
import { api, initApi, toCullError } from '@/lib/api'
import { detectLanguage, isLanguage, t, useI18nStore } from '@/lib/i18n'
import { notify } from '@/lib/notify'
import { useAnalysisStore } from './analysis-store'
import { usePrefsStore } from './prefs-store'
import { useReviewStore } from './review-store'
import { useWorkspaceStore } from './workspace-store'

export type Screen = 'signIn' | 'onboarding' | 'home' | 'analysis' | 'workspace'
export type Dialog = 'settings' | 'export' | 'shortcuts' | 'preAnalysis'
export type SettingsTab = 'account' | 'general' | 'analysis' | 'cache' | 'about'

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
  auth: AuthState
  /** Pelli's plans, prices and links (null until loaded, or offline) */
  offers: Offers | null
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
  updateSettings: (patch: Partial<Pick<AppSettings, 'theme' | 'language' | 'acceleration' | 'background' | 'precision' | 'cull'>>) => Promise<void>
  clearCache: () => Promise<void>
  refreshProjects: () => Promise<void>
  refreshAccount: () => Promise<void>
  loadOffers: () => Promise<void>
  /** show the welcome tour again (from the settings) */
  startOnboarding: () => void
  finishOnboarding: () => void
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

/** First screen once signed in: the welcome tour the first time, then home. */
function afterSignIn(): Screen {
  return usePrefsStore.getState().onboarded ? 'home' : 'onboarding'
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
  screen: 'signIn',
  appInfo: null,
  settings: null,
  auth: { status: 'signedOut' },
  offers: null,
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
      // A build without account (PELLI_INSTANT_NO_ACCOUNT=1) never signs in.
      const ready = appInfo.noAccount || auth.status === 'signedIn'
      const screen = ready ? afterSignIn() : 'signIn'
      set({ appInfo, settings, auth, phase: 'ready', screen, homeIntro: screen === 'home' })
      // Without account the app never contacts Pelli (no offers to show).
      if (!appInfo.noAccount) void get().loadOffers()
      if (ready) void get().refreshProjects()
      if (auth.status === 'signedIn' && !appInfo.noAccount) {
        // The cached account works offline; online, check that the 30-day
        // token is still valid (an expired one signs out here, before any
        // export fails on it) and refresh the plan and quota.
        void get().refreshAccount()
      }
    } catch (error) {
      set({ phase: 'error', bootError: toCullError(error).message })
    }
  },

  setAuth: (auth) => {
    const prev = get().auth
    set({ auth })
    if (get().appInfo?.noAccount) return
    if (auth.status === 'signedIn' && prev.status !== 'signedIn') {
      const screen = afterSignIn()
      set({ screen, homeIntro: screen === 'home' })
      void get().refreshProjects()
    }
    if (auth.status === 'signedOut' && prev.status === 'signedIn') {
      useReviewStore.getState().reset()
      set({ screen: 'signIn', dialog: null })
    }
  },

  setOnline: (online) => set({ online }),

  updateSettings: async (patch) => {
    const prev = get().settings
    if (prev) set({ settings: { ...prev, ...patch } })
    if (patch.language) useI18nStore.getState().setLanguage(patch.language)
    try {
      const settings = await api().setSettings(patch)
      set({ settings })
      if (patch.language && !get().appInfo?.noAccount) void get().loadOffers()
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

  loadOffers: async () => {
    const language = get().settings?.language ?? 'en'
    try {
      set({ offers: await api().getOffers(language) })
    } catch {
      /* offline on the first launch: the screens fall back to plain links */
    }
  },

  startOnboarding: () => set({ screen: 'onboarding', dialog: null }),

  homeIntro: false,
  consumeHomeIntro: () => set({ homeIntro: false }),

  finishOnboarding: () => {
    usePrefsStore.getState().setOnboarded(true)
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
