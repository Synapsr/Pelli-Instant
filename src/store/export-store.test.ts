import { beforeEach, describe, expect, it, vi } from 'vitest'
import { setApi } from '@/lib/api'
import { createMockApi } from '@/lib/api/mock'
import { MOCK_SETTINGS } from '@/lib/api/mock/fixtures'
import type { CullApi, CullEvents, ExportOptions, UploadRequest } from '@/lib/contract'
import { setNotifier, type ToastInput } from '@/lib/notify'
import { NO_PLUS } from '@/lib/plus'
import { isSuccessfulRun, isSupportCardDue, SUPPORT_AFTER_EXPORTS, shouldShowSupportCard } from '@/lib/support'
import { useAppStore } from './app-store'
import { useExportStore } from './export-store'
import { usePlusStore } from './plus-store'
import { usePrefsStore } from './prefs-store'

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

const FILE_LIST: ExportOptions = { ...NO_LOCAL, fileList: true }

/**
 * What the report does with the support card, as `useSupportCard`: its turn
 * taken once when the report opens, the run and Plus checked live, recorded as
 * soon as it shows. Returns whether the card is visible now.
 */
function openReport(): () => boolean {
  const prefs = usePrefsStore.getState()
  const due = isSupportCardDue({ exportsDone: prefs.exportsDone, shown: prefs.supportCardShown })
  const visible = () => {
    const plus = usePlusStore.getState()
    return shouldShowSupportCard({ due, succeeded: isSuccessfulRun(useExportStore.getState()), plusLoaded: plus.loaded, plusActive: plus.status.active })
  }
  if (visible()) usePrefsStore.getState().markSupportCardShown()
  return visible
}

describe('export store: the one-time support card', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    setNotifier(() => {})
    usePrefsStore.setState({ exportsDone: SUPPORT_AFTER_EXPORTS - 1, supportCardShown: false })
    usePlusStore.setState({ status: NO_PLUS, loaded: true })
  })

  it('counts a successful export, with or without a gallery, before its report opens', async () => {
    const api = setup('mockAuth=signedIn&mockPlan=pro')
    await useAppStore.getState().refreshAccount()
    await useExportStore.getState().run(PROJECT, NO_LOCAL, null, CONTEXT)
    expect(isSuccessfulRun(useExportStore.getState())).toBe(true)
    expect(usePrefsStore.getState().exportsDone).toBe(SUPPORT_AFTER_EXPORTS)
    expect(openReport()()).toBe(true)

    const done = next(api, 'upload-done')
    await useExportStore.getState().run(PROJECT, NO_LOCAL, GALLERY, CONTEXT)
    await done
    expect(isSuccessfulRun(useExportStore.getState())).toBe(true)
    expect(usePrefsStore.getState().exportsDone).toBe(SUPPORT_AFTER_EXPORTS + 1)
  })

  it('neither counts nor shows after a partial export', async () => {
    const api = setup('')
    usePrefsStore.setState({ exportsDone: SUPPORT_AFTER_EXPORTS })
    vi.spyOn(api, 'exportProject').mockResolvedValue({ xmpWritten: 0, copied: 0, moved: 0, fileListPath: '/x.txt', errors: [{ photoId: 'p1', message: 'Permission denied' }] })
    await useExportStore.getState().run(PROJECT, FILE_LIST, null, CONTEXT)
    expect(useExportStore.getState()).toMatchObject({ phase: 'done', error: null })
    expect(usePrefsStore.getState().exportsDone).toBe(SUPPORT_AFTER_EXPORTS)
    expect(openReport()()).toBe(false)
    // Not shown, so not spent: the next successful export shows it.
    expect(usePrefsStore.getState().supportCardShown).toBe(false)
  })

  it('neither counts nor shows after a failed upload', async () => {
    const api = setup('mockAuth=signedIn&mockPlan=pro')
    await useAppStore.getState().refreshAccount()
    usePrefsStore.setState({ exportsDone: SUPPORT_AFTER_EXPORTS })
    vi.spyOn(api, 'uploadToPelli').mockResolvedValue()
    await useExportStore.getState().run(PROJECT, NO_LOCAL, GALLERY, CONTEXT)
    useExportStore.getState().onUploadDone({ projectId: PROJECT, galleryId: 'g1', galleryUrl: 'https://app.pelli.io/g1', uploaded: 10, failed: 2 })
    expect(useExportStore.getState()).toMatchObject({ phase: 'done', error: null })
    expect(usePrefsStore.getState().exportsDone).toBe(SUPPORT_AFTER_EXPORTS)
    expect(openReport()()).toBe(false)
  })

  it('neither counts nor shows after an upload stopped by an error', async () => {
    const api = setup('mockAuth=signedIn&mockPlan=pro&mockUploadError=storage_full')
    await useAppStore.getState().refreshAccount()
    usePrefsStore.setState({ exportsDone: SUPPORT_AFTER_EXPORTS })
    const failed = next(api, 'upload-error')
    await useExportStore.getState().run(PROJECT, NO_LOCAL, GALLERY, CONTEXT)
    await failed
    expect(useExportStore.getState().phase).toBe('done')
    expect(usePrefsStore.getState().exportsDone).toBe(SUPPORT_AFTER_EXPORTS)
    expect(openReport()()).toBe(false)
  })

  it('neither counts nor shows after a cancelled upload', async () => {
    setup('mockAuth=signedIn&mockPlan=pro')
    await useAppStore.getState().refreshAccount()
    usePrefsStore.setState({ exportsDone: SUPPORT_AFTER_EXPORTS })
    await useExportStore.getState().run(PROJECT, NO_LOCAL, GALLERY, CONTEXT)
    await useExportStore.getState().cancelUpload()
    expect(useExportStore.getState()).toMatchObject({ phase: 'done', error: null, uploadReport: null })
    expect(usePrefsStore.getState().exportsDone).toBe(SUPPORT_AFTER_EXPORTS)
    expect(openReport()()).toBe(false)
  })

  it('disappears when Plus activates while the report is open', async () => {
    setup('')
    await useExportStore.getState().run(PROJECT, NO_LOCAL, null, CONTEXT)
    const visible = openReport()
    expect(visible()).toBe(true)
    usePlusStore.setState({ status: { ...NO_PLUS, active: true, source: 'license' } })
    expect(visible()).toBe(false)
  })

  it('stays away with Plus, and is not spent while hidden', async () => {
    setup('')
    usePlusStore.setState({ status: { ...NO_PLUS, active: true, source: 'license' } })
    await useExportStore.getState().run(PROJECT, NO_LOCAL, null, CONTEXT)
    expect(openReport()()).toBe(false)
    expect(usePrefsStore.getState().supportCardShown).toBe(false)
  })

  it('shows once: a report closed with Done never brings it back', async () => {
    setup('')
    await useExportStore.getState().run(PROJECT, NO_LOCAL, null, CONTEXT)
    expect(openReport()()).toBe(true)
    expect(usePrefsStore.getState().supportCardShown).toBe(true)
    // Done: the report closes without touching the card.
    useExportStore.getState().reset()
    await useExportStore.getState().run(PROJECT, NO_LOCAL, null, CONTEXT)
    expect(isSuccessfulRun(useExportStore.getState())).toBe(true)
    expect(openReport()()).toBe(false)
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
