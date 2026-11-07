import { create } from 'zustand'
import type { ExportOptions, ExportReport, UploadProgress, UploadReport, UploadRequest } from '@/lib/contract'
import { api, toCullError } from '@/lib/api'
import { playSound } from '@/lib/sound'

export type ExportPhase = 'configure' | 'exporting' | 'uploading' | 'done'

interface ExportState {
  projectId: string | null
  phase: ExportPhase
  report: ExportReport | null
  upload: UploadProgress | null
  uploadReport: UploadReport | null
  error: string | null
  /** machine code of `error` (`unauthorized`, `storage_full`…) when known */
  errorCode: string | null
  run: (projectId: string, options: ExportOptions, upload: UploadRequest | null) => Promise<void>
  cancelUpload: () => Promise<void>
  reset: () => void
  onUploadProgress: (progress: UploadProgress) => void
  onUploadDone: (report: UploadReport & { projectId: string }) => void
  onUploadError: (event: { projectId: string; code: string; message: string }) => void
}

const IDLE = { projectId: null, phase: 'configure' as const, report: null, upload: null, uploadReport: null, error: null, errorCode: null }

const anyLocalOutput = (o: ExportOptions) => o.xmp.enabled || o.copySelected.enabled || o.moveRejected.enabled || o.fileList

export const useExportStore = create<ExportState>((set, get) => ({
  ...IDLE,

  run: async (projectId, options, upload) => {
    set({ ...IDLE, projectId, phase: 'exporting' })
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
        set({ phase: 'done' })
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
    set({ phase: 'done', error: null, errorCode: null })
  },

  reset: () => set({ ...IDLE }),

  onUploadProgress: (progress) => {
    if (progress.projectId !== get().projectId || get().phase !== 'uploading') return
    set({ upload: progress })
  },
  onUploadDone: (report) => {
    if (report.projectId !== get().projectId) return
    set({ phase: 'done', uploadReport: report })
    playSound('exportDone')
  },
  onUploadError: (event) => {
    if (event.projectId !== get().projectId || event.code === 'cancelled') return
    set({ phase: 'done', error: event.message || event.code, errorCode: event.code })
  },
}))
