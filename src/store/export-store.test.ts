import { beforeEach, describe, expect, it } from 'vitest'
import { setApi } from '@/lib/api'
import { createMockApi } from '@/lib/api/mock'
import { MOCK_SETTINGS } from '@/lib/api/mock/fixtures'
import type { CullApi, CullEvents, ExportOptions, UploadRequest } from '@/lib/contract'
import { setNotifier, type ToastInput } from '@/lib/notify'
import { useAppStore } from './app-store'
import { useExportStore } from './export-store'

const PROJECT = 'prj-lea-thomas'
const NO_LOCAL: ExportOptions = {
  xmp: { enabled: false, rating: { selected: 3, toReview: 0, rejected: -1 }, label: { selected: 'green', toReview: 'yellow', rejected: 'red' }, keepUserMarks: true },
  copySelected: { enabled: false, destination: null, includeRaw: true, includeJpeg: true, keepStructure: true },
  moveRejected: { enabled: false, folderName: '_Rejets Pelli' },
  fileList: false,
}
const GALLERY: UploadRequest = { galleryId: null, newGallery: { title: 'Mariage Léa & Thomas', type: 'selection' }, which: 'selected', folderPerScene: true }
const CONTEXT = { revealPath: '/Users/camille/Photos', rejectsFolder: '_Rejets Pelli', xmp: false }

let toasts: ToastInput[] = []

/** The mock engine, its upload and account events routed to the stores as the app does. */
function setup(params: string): CullApi {
  const api = createMockApi({ thumbnails: false, storage: null, latency: 0, speed: 50, params: new URLSearchParams(`mockNoPopups=1&${params}`) })
  setApi(api)
  api.on('upload-progress', (p) => useExportStore.getState().onUploadProgress(p))
  api.on('upload-done', (r) => useExportStore.getState().onUploadDone(r))
  api.on('upload-error', (e) => useExportStore.getState().onUploadError(e))
  api.on('auth-changed', (auth) => useAppStore.getState().setAuth(auth))
  useExportStore.getState().reset()
  useExportStore.setState({ signInError: null })
  useAppStore.setState({ settings: { ...MOCK_SETTINGS }, auth: { status: 'signedOut' }, dialog: null })
  return api
}

function next<E extends keyof CullEvents>(api: CullApi, event: E): Promise<CullEvents[E]> {
  return new Promise((resolve) => {
    const off = api.on(event, (payload) => {
      off()
      resolve(payload)
    })
  })
}

describe('export store: sending to a Pelli gallery', () => {
  beforeEach(() => {
    toasts = []
    setNotifier((toast) => toasts.push(toast))
  })

  it('sends with the dialog open, then reports the gallery to open', async () => {
    const api = setup('mockAuth=signedIn&mockPlan=pro')
    await useAppStore.getState().refreshAccount()
    const done = next(api, 'upload-done')
    await useExportStore.getState().run(PROJECT, NO_LOCAL, GALLERY, CONTEXT)
    expect(useExportStore.getState().phase).toBe('uploading')
    const report = await done
    expect(useExportStore.getState()).toMatchObject({ phase: 'done', background: false, error: null })
    expect(useExportStore.getState().uploadReport?.galleryUrl).toBe('https://app.pelli.io/dashboard/gallery/mariage-lea-thomas')
    expect(report.uploaded).toBeGreaterThan(0)
    expect(toasts).toEqual([])
  })

  it('goes on in the background once the dialog closes, and says so when it ends', async () => {
    const api = setup('mockAuth=signedIn&mockPlan=ultra')
    await useAppStore.getState().refreshAccount()
    const done = next(api, 'upload-done')
    await useExportStore.getState().run(PROJECT, NO_LOCAL, GALLERY, CONTEXT)
    useExportStore.getState().hide()
    expect(useExportStore.getState().background).toBe(true)
    const report = await done
    expect(useExportStore.getState()).toMatchObject({ phase: 'done', background: true })
    expect(toasts).toHaveLength(1)
    expect(toasts[0]).toMatchObject({ tone: 'success', title: expect.stringContaining(String(report.uploaded)) })
    expect(toasts[0].action?.label).toBeTruthy()

    // Sending again lands in the same gallery: only what is new goes.
    const again = next(api, 'upload-done')
    await useExportStore.getState().run(PROJECT, NO_LOCAL, GALLERY, CONTEXT)
    const second = await again
    expect(second.skipped).toBe(report.uploaded)
    expect(second.uploaded).toBe(0)
  })

  it('only hides an upload: an export on disk keeps its dialog', () => {
    setup('mockAuth=signedIn')
    useExportStore.setState({ phase: 'exporting' })
    useExportStore.getState().hide()
    expect(useExportStore.getState().background).toBe(false)
  })

  it('names the paywall, refreshes the account and offers to show the report from the toast', async () => {
    const api = setup('mockAuth=signedIn&mockPlan=free')
    await useAppStore.getState().refreshAccount()
    const failed = next(api, 'upload-error')
    await useExportStore.getState().run(PROJECT, NO_LOCAL, GALLERY, CONTEXT)
    useExportStore.getState().hide()
    expect((await failed).code).toBe('subscription_required')
    expect(useExportStore.getState()).toMatchObject({ phase: 'done', errorCode: 'subscription_required' })
    expect(toasts).toHaveLength(1)
    expect(toasts[0].tone).toBe('danger')
    toasts[0].action?.run()
    expect(useAppStore.getState().dialog).toBe('export')
    expect(useExportStore.getState().background).toBe(false)
  })

  it('keeps the photographer’s cancel quiet', async () => {
    setup('mockAuth=signedIn')
    await useAppStore.getState().refreshAccount()
    await useExportStore.getState().run(PROJECT, NO_LOCAL, GALLERY, CONTEXT)
    await useExportStore.getState().cancelUpload()
    expect(useExportStore.getState()).toMatchObject({ phase: 'done', error: null, background: false })
    useExportStore.getState().onUploadError({ projectId: PROJECT, code: 'cancelled', message: 'cancelled' })
    expect(useExportStore.getState().error).toBeNull()
    expect(toasts).toEqual([])
  })

  it('signs in from the gallery card through the browser consent', async () => {
    setup('mockAuth=signedOut&mockPlan=trial')
    await useExportStore.getState().signIn()
    expect(useAppStore.getState().auth.status).toBe('pending')
    await useExportStore.getState().pollSignIn()
    await useExportStore.getState().pollSignIn()
    const auth = useAppStore.getState().auth
    expect(auth.status).toBe('signedIn')
    if (auth.status === 'signedIn') expect(auth.account.entitlements?.sendToGallery).toBe(true)
    expect(useExportStore.getState().signInError).toBeNull()
  })

  it('says why a sign-in could not start, and cancels a waiting one', async () => {
    setup('mockOffline=1')
    await useExportStore.getState().signIn()
    expect(useExportStore.getState().signInError).toBe('offline')
    setup('mockAuth=signedOut')
    await useExportStore.getState().signIn()
    await useExportStore.getState().cancelSignIn()
    expect(useAppStore.getState().auth.status).toBe('signedOut')
  })
})

describe('upload check (mock)', () => {
  it('reports RAW files alone and the size of their embedded previews', async () => {
    const api = setup('mockPreview=1616')
    const check = await api.uploadCheck('prj-ines-karim')
    // RAW files without twins, but a few JPEGs from a phone.
    expect(check.rawOnly).toBeGreaterThan(check.photos * 0.9)
    expect(check.rawOnly).toBeLessThanOrEqual(check.photos)
    expect(check.previewLongEdge).toBe(1616)
  })

  it('has nothing to say about RAW + JPEG pairs', async () => {
    const api = setup('')
    const check = await api.uploadCheck('prj-lea-thomas')
    expect(check.rawOnly).toBeLessThan(check.photos)
    expect(check.previewLongEdge === null || check.previewLongEdge >= 2000).toBe(true)
  })
})
