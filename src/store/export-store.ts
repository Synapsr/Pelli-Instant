import { create } from 'zustand'
import type { ExportOptions, ExportReport, UploadProgress, UploadReport, UploadRequest } from '@/lib/contract'
import { api, toCullError } from '@/lib/api'
import { isKnownUploadError } from '@/lib/gallery'
import { t } from '@/lib/i18n'
import { notify } from '@/lib/notify'
import { playSound } from '@/lib/sound'
import { isSuccessfulRun } from '@/lib/support'
import { useAppStore } from './app-store'
import { usePrefsStore } from './prefs-store'

export type ExportPhase = 'configure' | 'exporting' | 'uploading' | 'done'

/** What the report needs from the export that ran, whatever project is open when it ends. */
export interface ExportContext {
  /** the folder "Reveal in Finder" opens */
  revealPath: string
  /** name of the rejects folder */
  rejectsFolder: string
  /** XMP sidecars were asked for (the Lightroom hint) */
  xmp: boolean
}

interface ExportState {
  projectId: string | null
  phase: ExportPhase
  context: ExportContext | null
  report: ExportReport | null
  /** the run also sends the photos to a Pelli gallery */
  withUpload: boolean
  upload: UploadProgress | null
  uploadReport: UploadReport | null
  error: string | null
  /** machine code of `error` (`unauthorized`, `storage_full`…) when known */
  errorCode: string | null
  /** the dialog was closed while the photos are sent: the title bar shows the upload */
  background: boolean
  /** the sign-in started from the gallery card failed (`offline`, `expired`…) */
  signInError: string | null
  run: (projectId: string, options: ExportOptions, upload: UploadRequest | null, context: ExportContext) => Promise<void>
  cancelUpload: () => Promise<void>
  /** the dialog closes, the upload goes on */
  hide: () => void
  /** the dialog is shown again */
  show: () => void
  reset: () => void
  /** the Pelli sign-in in the browser, from the gallery card */
  signIn: () => Promise<void>
  /** one check of that sign-in (the card asks every 3 s while it waits) */
  pollSignIn: () => Promise<void>
  cancelSignIn: () => Promise<void>
  onUploadProgress: (progress: UploadProgress) => void
  onUploadDone: (report: UploadReport & { projectId: string }) => void
  onUploadError: (event: { projectId: string; code: string; message: string }) => void
}

const IDLE = {
  projectId: null,
  phase: 'configure' as const,
  context: null,
  report: null,
  withUpload: false,
  upload: null,
  uploadReport: null,
  error: null,
  errorCode: null,
  background: false,
}

const anyLocalOutput = (o: ExportOptions) => o.xmp.enabled || o.copySelected.enabled || o.moveRejected.enabled || o.fileList

/** An export or upload failure in the photographer's words. */
export function uploadErrorText(code: string | null, message: string | null): string {
  if (code === 'diskFull') return `${t('errors.diskFullTitle')}. ${t('errors.diskFull')}`
  if (isKnownUploadError(code)) return t(`export.uploadErrors.${code}`)
  return t('export.uploadError', { message: message || code || '' })
}

/**
 * The run ends with its report: a successful one counts toward the support
 * card first, so the report that opens already knows the new count.
 */
function finishRun(patch: Partial<ExportState>) {
  if (isSuccessfulRun({ ...useExportStore.getState(), ...patch })) usePrefsStore.getState().countExport()
  useExportStore.setState(patch)
}

/** Back to the dialog, to see how the upload went. */
function showReport() {
  useExportStore.getState().show()
  useAppStore.getState().openDialog('export')
}

export const useExportStore = create<ExportState>((set, get) => ({
  ...IDLE,
  signInError: null,

  run: async (projectId, options, upload, context) => {
    set({ ...IDLE, projectId, context, withUpload: upload !== null, phase: 'exporting' })
    try {
      const report = anyLocalOutput(options)
        ? await api().exportProject(projectId, options)
        : { xmpWritten: 0, copied: 0, moved: 0, fileListPath: null, errors: [] }
      if (get().projectId !== projectId) return
      set({ report })
      if (upload) {
        set({ phase: 'uploading', upload: null })
        await api().uploadToPelli(projectId, upload)
      } else {
        finishRun({ phase: 'done' })
        playSound('exportDone')
      }
    } catch (error) {
      const e = toCullError(error)
      set({ phase: 'done', error: e.message, errorCode: e.code })
    }
  },

  cancelUpload: async () => {
    const { projectId } = get()
    if (!projectId) return
    await api().cancelUpload(projectId)
    set({ phase: 'done', error: null, errorCode: null, background: false })
  },

  hide: () => {
    if (get().phase === 'uploading') set({ background: true })
  },

  show: () => set({ background: false }),

  reset: () => set({ ...IDLE }),

  signIn: async () => {
    set({ signInError: null })
    try {
      useAppStore.getState().setAuth(await api().authStart())
    } catch (error) {
      set({ signInError: toCullError(error).code })
    }
  },

  pollSignIn: async () => {
    const auth = useAppStore.getState().auth
    if (auth.status !== 'pending') return
    if (Date.now() > auth.expiresAt) {
      useAppStore.getState().setAuth(await api().authLogout())
      set({ signInError: 'expired' })
      return
    }
    try {
      const next = await api().authPoll()
      useAppStore.getState().setAuth(next)
      // Refused in the browser, or the page left open too long.
      if (next.status === 'signedOut') set({ signInError: 'expired' })
    } catch {
      /* offline for a moment: the next check tries again */
    }
  },

  cancelSignIn: async () => {
    set({ signInError: null })
    try {
      useAppStore.getState().setAuth(await api().authLogout())
    } catch {
      /* already signed out */
    }
  },

  onUploadProgress: (progress) => {
    if (progress.projectId !== get().projectId || get().phase !== 'uploading') return
    set({ upload: progress })
  },

  onUploadDone: (report) => {
    if (report.projectId !== get().projectId || get().phase !== 'uploading') return
    finishRun({ phase: 'done', uploadReport: report })
    playSound('exportDone')
    if (!get().background) return
    const skippedOnly = report.uploaded === 0 && (report.skipped ?? 0) > 0
    notify({
      title: skippedOnly ? t('export.reportUploadSkipped', { count: report.skipped ?? 0 }) : t('export.reportUploaded', { count: report.uploaded }),
      description: report.failed > 0 ? t('export.reportUploadFailed', { count: report.failed }) : undefined,
      tone: report.failed > 0 ? 'warning' : 'success',
      action: { label: t('export.openGallery'), run: () => void api().openExternal(report.galleryUrl) },
      timeout: 12_000,
    })
  },

  onUploadError: (event) => {
    if (event.projectId !== get().projectId || event.code === 'cancelled' || get().phase !== 'uploading') return
    set({ phase: 'done', error: event.message || event.code, errorCode: event.code })
    // The plan changed in Pelli since the account was read (a trial that
    // ended): the gallery card says so from now on.
    if (event.code === 'subscription_required' || event.code === 'gallery_limit_reached') void useAppStore.getState().refreshAccount()
    if (!get().background) return
    notify({
      title: t('export.uploadStopped'),
      description: uploadErrorText(event.code, event.message),
      tone: 'danger',
      action: { label: t('export.showUpload'), run: showReport },
      timeout: 12_000,
    })
  },
}))
