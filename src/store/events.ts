import type { CullApi, PhotoView } from '@/lib/contract'
import { useAnalysisStore } from './analysis-store'
import { useAppStore } from './app-store'
import { useExportStore } from './export-store'
import { usePackStore } from './pack-store'
import { useReviewStore } from './review-store'
import { useUpdateStore } from './update-store'

/**
 * Routes engine events to the stores. Streamed photos are batched (one store
 * update per ~120 ms) so a fast analysis never re-renders the grid per photo.
 */
export function connectEvents(engine: CullApi): () => void {
  let queue: PhotoView[] = []
  let queuedProject: string | null = null
  let timer: ReturnType<typeof setTimeout> | null = null

  const flush = () => {
    timer = null
    const batch = queue
    queue = []
    const review = useReviewStore.getState()
    if (batch.length && review.projectId === queuedProject) review.upsertPhotos(batch)
    if (batch.length && queuedProject) useAnalysisStore.getState().onPhotos(queuedProject, batch)
  }
  const enqueue = (projectId: string, photos: PhotoView[]) => {
    if (queuedProject !== projectId) {
      queue = []
      queuedProject = projectId
    }
    queue.push(...photos)
    timer ??= setTimeout(flush, 120)
  }

  const offs = [
    engine.on('analysis-progress', (p) => useAnalysisStore.getState().onProgress(p)),
    engine.on('analysis-photo', ({ projectId, photo }) => enqueue(projectId, [photo])),
    engine.on('photos-updated', ({ projectId, photos }) => enqueue(projectId, photos)),
    engine.on('analysis-done', (summary) => {
      if (timer) {
        clearTimeout(timer)
        flush()
      }
      useAnalysisStore.getState().onDone(summary)
      void engine.getProject(summary.projectId).then((data) => {
        const review = useReviewStore.getState()
        if (review.projectId === summary.projectId) review.load(data, { keepView: true })
      })
      void useAppStore.getState().refreshProjects()
      void useAppStore.getState().refreshAccount()
    }),
    engine.on('analysis-error', (e) => useAnalysisStore.getState().onError(e)),
    engine.on('upload-progress', (p) => useExportStore.getState().onUploadProgress(p)),
    engine.on('upload-done', (r) => useExportStore.getState().onUploadDone(r)),
    engine.on('upload-error', (e) => useExportStore.getState().onUploadError(e)),
    engine.on('auth-changed', (auth) => useAppStore.getState().setAuth(auth)),
    engine.on('model-pack', (pack) => usePackStore.getState().onPack(pack)),
    engine.on('update-status', (status) => useUpdateStore.getState().onStatus(status)),
  ]
  void usePackStore.getState().load()
  void useUpdateStore.getState().load()

  const setOnline = () => useAppStore.getState().setOnline(navigator.onLine)
  window.addEventListener('online', setOnline)
  window.addEventListener('offline', setOnline)

  return () => {
    offs.forEach((off) => off())
    window.removeEventListener('online', setOnline)
    window.removeEventListener('offline', setOnline)
    if (timer) clearTimeout(timer)
  }
}
