import { describe, expect, it } from 'vitest'
import { DEFAULT_SETTINGS, type CullEvents, type UpdateStatus } from '@/lib/contract'
import { MOCK_UPDATE_VERSION, NEW_FOLDER } from './fixtures'
import { createMockApi } from './index'
import { concepts } from './search'

function engine(params = '') {
  return createMockApi({ thumbnails: false, storage: null, latency: 0, speed: 60, params: new URLSearchParams(params) })
}

/** Resolves once `ready()` holds, checked every 20 ms. */
async function until(ready: () => boolean, timeout = 15_000): Promise<void> {
  const end = Date.now() + timeout
  while (!ready()) {
    if (Date.now() > end) throw new Error('timed out')
    await new Promise((resolve) => setTimeout(resolve, 20))
  }
}

function waitFor<E extends keyof CullEvents>(api: ReturnType<typeof engine>, event: E, timeout = 15_000): Promise<CullEvents[E]> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`timeout waiting for ${event}`)), timeout)
    const off = api.on(event, (payload) => {
      clearTimeout(timer)
      off()
      resolve(payload)
    })
  })
}

describe('mock engine', () => {
  it('lists recent projects with counts and a signed-out first launch', async () => {
    const api = engine()
    expect((await api.authStatus()).status).toBe('signedOut')
    const projects = await api.listProjects()
    expect(projects.length).toBeGreaterThanOrEqual(4)
    const analysed = projects.find((p) => p.analyzedAt !== null)
    expect(analysed).toBeTruthy()
    const { selected, toReview, rejected } = analysed!.counts
    expect(selected + toReview + rejected).toBe(analysed!.photoCount)
    expect(selected).toBeGreaterThan(0)
    expect(rejected).toBeGreaterThan(selected)
  })

  it('signs in through the browser consent after a couple of polls', async () => {
    const api = engine()
    const changes: string[] = []
    api.on('auth-changed', (s) => changes.push(s.status))
    expect((await api.authStart()).status).toBe('pending')
    expect((await api.authPoll()).status).toBe('pending')
    const signed = await api.authPoll()
    expect(signed.status).toBe('signedIn')
    if (signed.status === 'signedIn') expect(signed.account.entitlements.analyze).toBe(true)
    expect(changes).toEqual(['pending', 'signedIn'])
    expect((await api.authLogout()).status).toBe('signedOut')
  })

  it('refuses an analysis bigger than what is left of the monthly quota', async () => {
    const api = engine('mockAuth=signedIn&mockPlan=free&mockUsed=150&mockCount=240')
    const project = await api.openFolder(NEW_FOLDER.rootPath, true)
    const error = waitFor(api, 'analysis-error')
    await api.startAnalysis(project.id, DEFAULT_SETTINGS)
    const event = await error
    expect(event.code).toBe('quotaExceeded')
    expect(event.details).toMatchObject({ monthlyQuota: 200, usedThisMonth: 150, remaining: 50, needed: project.photoCount })
  })

  it('counts new photos on limited plans only', async () => {
    const lite = engine('mockAuth=signedIn&mockPlan=lite&mockUsed=0&mockCount=240')
    const project = await lite.openFolder(NEW_FOLDER.rootPath, true)
    const usage = waitFor(lite, 'auth-changed')
    await lite.startAnalysis(project.id, DEFAULT_SETTINGS)
    const state = await usage
    expect(state.status === 'signedIn' && state.account.entitlements.usedThisMonth).toBe(project.photoCount)

    for (const query of ['mockAuth=signedIn&mockPlan=ultra&mockUsed=0&mockCount=240', 'mockNoAccount=1&mockPlan=free&mockUsed=199&mockCount=240']) {
      const api = engine(query)
      const p = await api.openFolder(NEW_FOLDER.rootPath, true)
      const done = waitFor(api, 'analysis-done')
      await api.startAnalysis(p.id, DEFAULT_SETTINGS)
      expect((await done).analyzed).toBe(p.photoCount)
    }
  })

  it('opens a new folder as a pending project, then streams the analysis', async () => {
    const api = engine('mockCount=240')
    const project = await api.openFolder(NEW_FOLDER.rootPath, true)
    expect(project.analyzedAt).toBeNull()
    expect(project.photoCount).toBeGreaterThanOrEqual(240)
    const before = await api.getProject(project.id)
    expect(before.photos.every((p) => p.status === 'pending' && p.assessment === null)).toBe(true)

    const streamed: string[] = []
    const stages = new Set<string>()
    api.on('analysis-photo', ({ photo }) => streamed.push(photo.id))
    api.on('analysis-progress', (p) => stages.add(p.stage))
    const done = waitFor(api, 'analysis-done')
    await api.startAnalysis(project.id, DEFAULT_SETTINGS)
    const summary = await done

    expect(summary.analyzed).toBe(project.photoCount)
    expect(new Set(streamed).size).toBe(project.photoCount)
    expect([...stages]).toEqual(expect.arrayContaining(['scanning', 'analyzing', 'grouping', 'done']))
    const after = await api.getProject(project.id)
    expect(after.project.analyzedAt).not.toBeNull()
    expect(after.groups.length).toBeGreaterThan(0)
    expect(after.scenes.length).toBeGreaterThan(1)
    expect(after.scenes.every((s) => typeof s.label === 'string')).toBe(true)
    // Groups are ordered best first and their members point back to them.
    const g = after.groups[0]
    const members = g.photoIds.map((id) => after.photos.find((p) => p.id === id))
    expect(members.every((m) => m?.assessment?.groupId === g.id)).toBe(true)
    expect(members[0]!.assessment!.score).toBeGreaterThanOrEqual(members[1]!.assessment!.score)
  })

  it('lets user flags win and recompute keeps them', async () => {
    const api = engine()
    const data = await api.getProject('prj-lea-thomas')
    const rejected = data.photos.find((p) => p.decision === 'rejected' && p.mark.flag === 'none')!
    const [updated] = await api.setMarks('prj-lea-thomas', [rejected.id], { flag: 'pick', rating: 5 })
    expect(updated.decision).toBe('selected')
    expect(updated.mark).toEqual({ flag: 'pick', rating: 5, label: 'none' })
    const strict = await api.recompute('prj-lea-thomas', { ...DEFAULT_SETTINGS, sensitivity: 'strict' })
    expect(strict.photos.find((p) => p.id === rejected.id)?.decision).toBe('selected')
  })

  it('rejects more with a stricter sensitivity and keeps more per group on demand', async () => {
    const api = engine()
    const id = 'prj-lea-thomas'
    const strict = (await api.recompute(id, { ...DEFAULT_SETTINGS, sensitivity: 'strict' })).project.counts
    const balanced = (await api.recompute(id, { ...DEFAULT_SETTINGS, sensitivity: 'balanced' })).project.counts
    const lenient = (await api.recompute(id, { ...DEFAULT_SETTINGS, sensitivity: 'lenient' })).project.counts
    expect(strict.rejected).toBeGreaterThan(balanced.rejected)
    expect(balanced.rejected).toBeGreaterThan(lenient.rejected)
    const keepTwo = (await api.recompute(id, { ...DEFAULT_SETTINGS, keepPerGroup: 2 })).project.counts
    expect(keepTwo.selected).toBeGreaterThan(balanced.selected)
    // Off, bursts stay grouped (stacked) but no frame is set aside as a duplicate.
    const grouped = await api.recompute(id, DEFAULT_SETTINGS)
    const ungrouped = await api.recompute(id, { ...DEFAULT_SETTINGS, groupSimilar: false })
    expect(ungrouped.groups).toEqual(grouped.groups)
    expect(ungrouped.photos.some((p) => p.assessment?.reasons.includes('duplicate'))).toBe(false)
    expect(ungrouped.project.counts.rejected).toBeLessThan(grouped.project.counts.rejected)
  })

  it('honours each criterion and artistic blur like the engine', async () => {
    const api = engine()
    const id = 'prj-lea-thomas'
    const reasons = (d: Awaited<ReturnType<typeof api.recompute>>) => new Set(d.photos.flatMap((p) => p.assessment?.reasons ?? []))
    const all = await api.recompute(id, DEFAULT_SETTINGS)
    expect([...reasons(all)]).toEqual(expect.arrayContaining(['blurry', 'motionBlur', 'underexposed', 'duplicate', 'exactDuplicate']))
    const off = await api.recompute(id, { ...DEFAULT_SETTINGS, detectMotionBlur: false, detectUnderexposed: false, detectExactDuplicates: false })
    const left = reasons(off)
    for (const r of ['motionBlur', 'underexposed', 'exactDuplicate'] as const) expect(left.has(r)).toBe(false)
    expect(left.has('blurry') && left.has('overexposed') && left.has('duplicate')).toBe(true)
    expect(off.groups.some((g) => g.kind === 'exact')).toBe(false)
    expect(off.project.counts.rejected).toBeLessThan(all.project.counts.rejected)
    // Artistic blur never rejects more, and keeps a blurry verdict only when obvious or on the face.
    const artistic = await api.recompute(id, { ...DEFAULT_SETTINGS, artisticBlur: true })
    expect(artistic.project.counts.rejected).toBeLessThanOrEqual(all.project.counts.rejected)
    expect(artistic.project.settings.artisticBlur).toBe(true)
  })

  it('starts new projects from the default criteria', async () => {
    const api = engine()
    const cull = { ...DEFAULT_SETTINGS, artisticBlur: true, detectEyesClosed: false, keepPerGroup: 9 }
    const saved = await api.setSettings({ cull })
    expect(saved.cull.keepPerGroup).toBe(5)
    const project = await api.openFolder('/Users/camille/Photos/2026-10-03 Portraits', true)
    expect(project.settings).toMatchObject({ artisticBlur: true, detectEyesClosed: false })
    await api.setSettings({ cull: DEFAULT_SETTINGS })
    expect((await api.openFolder('/Users/camille/Photos/2026-10-03 Portraits', true)).settings).toEqual(DEFAULT_SETTINGS)
  })

  it('switches the background analysis', async () => {
    const api = engine()
    expect((await api.getSettings()).background).toBe(false)
    expect(await api.setSettings({ background: true })).toMatchObject({ background: true })
    expect(await api.setSettings({ background: false })).toMatchObject({ background: false })
  })

  it('downloads the Maximum pack, pauses, resumes and removes it', async () => {
    const api = engine()
    const states: string[] = []
    api.on('model-pack', (pack) => states.push(pack.state.status))
    expect((await api.modelPack()).state.status).toBe('absent')
    expect((await api.appInfo()).capabilities.maximum).toBe(false)
    await api.downloadModelPack()
    await until(() => states.includes('downloading'))
    const paused = await api.cancelModelPack()
    expect(paused.state).toMatchObject({ status: 'failed', code: 'cancelled' })
    await api.downloadModelPack()
    await until(() => states.at(-1) === 'ready', 20_000)
    expect((await api.appInfo()).capabilities.maximum).toBe(true)
    expect((await api.removeModelPack()).state.status).toBe('absent')
  }, 30_000)

  it('analyses in maximum precision only once the pack is ready', async () => {
    const api = engine()
    await api.setSettings({ precision: 'maximum' })
    const [project] = await api.listProjects()
    const done = waitFor(api, 'analysis-done')
    await api.startAnalysis(project.id, project.settings)
    expect((await done).precision).toBe('standard')

    // Pack ready: the models load (a stage of their own) before the first photo.
    const ready = engine('mockPack=ready')
    await ready.setSettings({ precision: 'maximum' })
    const stages: string[] = []
    ready.on('analysis-progress', (p) => stages.at(-1) !== p.stage && stages.push(p.stage))
    const maxDone = waitFor(ready, 'analysis-done', 25_000)
    await ready.startAnalysis(project.id, project.settings)
    expect((await maxDone).precision).toBe('maximum')
    expect(stages.slice(0, 3)).toEqual(['scanning', 'loading', 'analyzing'])
  }, 30_000)

  it('searches in natural language, in several languages', async () => {
    const api = engine()
    expect(concepts('Mariée et bouquet')).toEqual(['mariee', 'bouquet'])
    expect(concepts('bride')).toEqual(['mariee'])
    expect(concepts('coucher de soleil')).toEqual(['coucher de soleil'])
    const fr = await api.search('prj-lea-thomas', 'mariée bouquet')
    const en = await api.search('prj-lea-thomas', 'bride bouquet')
    expect(fr.length).toBeGreaterThan(10)
    expect(en.map((h) => h.photoId)).toEqual(fr.map((h) => h.photoId))
    expect(fr[0].score).toBeGreaterThanOrEqual(fr[fr.length - 1].score)
    expect(await api.search('prj-lea-thomas', 'zzz')).toEqual([])
  })

  it('reports folder errors with typed codes', async () => {
    const api = engine()
    await expect(api.openFolder('/Volumes/Carte vide', true)).rejects.toMatchObject({ code: 'noPhotos' })
    await expect(api.openFolder('/Volumes/Dossier verrouillé', true)).rejects.toMatchObject({ code: 'permissionDenied' })
  })

  it('exports and uploads with progress events', async () => {
    const api = engine('mockAuth=signedIn')
    const data = await api.getProject('prj-lea-thomas')
    const selected = data.photos.filter((p) => p.decision === 'selected').length
    const report = await api.exportProject('prj-lea-thomas', {
      xmp: { enabled: true, rating: { selected: 3, toReview: 2, rejected: -1 }, label: { selected: 'green', toReview: 'yellow', rejected: 'red' }, keepUserMarks: true },
      copySelected: { enabled: true, destination: '/tmp/out', includeRaw: true, includeJpeg: false, keepStructure: true },
      moveRejected: { enabled: true, folderName: '_Rejets Pelli' },
      fileList: true,
    })
    expect(report.copied).toBe(selected)
    expect(report.moved).toBe(data.photos.filter((p) => p.decision === 'rejected').length)
    expect(report.fileListPath).toContain('Pelli Instant')

    const progress: number[] = []
    api.on('upload-progress', (p) => progress.push(p.done))
    const done = waitFor(api, 'upload-done')
    await api.uploadToPelli('prj-lea-thomas', { galleryId: null, newGallery: { title: 'Mariage Léa & Thomas', type: 'selection' }, which: 'selected', folderPerScene: true })
    const result = await done
    expect(result.uploaded).toBe(selected)
    expect(result.galleryUrl).toContain('mariage-lea-thomas')
    expect(progress[progress.length - 1]).toBe(selected)
  })

  it('installs a published update once the analysis under way is done', async () => {
    const quiet = engine()
    expect(await quiet.updateStatus()).toEqual({ status: 'idle' })
    expect(await quiet.checkForUpdate()).toEqual({ status: 'upToDate' })

    const api = engine('mockUpdate=available')
    expect(await api.checkForUpdate()).toEqual({ status: 'available', version: MOCK_UPDATE_VERSION })
    const seen: string[] = []
    api.on('update-status', (s) => seen.at(-1) !== s.status && seen.push(s.status === 'restarting' ? `restarting:${s.waiting}` : s.status))
    api.on('analysis-done', () => seen.push('analysis-done'))
    const [project] = await api.listProjects()
    await api.startAnalysis(project.id, project.settings)
    await api.installUpdate()
    await until(() => seen.includes('restarting:false'))
    expect(seen.slice(seen.indexOf('downloading'))).toEqual(['downloading', 'restarting:true', 'analysis-done', 'restarting:false'])
  })

  it('keeps a failed download to try again', async () => {
    const api = engine('mockUpdate=offline')
    await api.checkForUpdate()
    let last: UpdateStatus = await api.updateStatus()
    api.on('update-status', (s) => (last = s))
    await api.installUpdate()
    await until(() => last.status === 'failed')
    expect(last).toEqual({ status: 'failed', code: 'offline', message: 'No network', version: MOCK_UPDATE_VERSION })
    await api.installUpdate()
    await until(() => last.status === 'restarting' && !last.waiting)
  })

  it('never updates a copy that has updates turned off', async () => {
    const api = engine('mockUpdate=disabled')
    expect(await api.checkForUpdate()).toEqual({ status: 'disabled' })
    await api.installUpdate()
    expect(await api.updateStatus()).toEqual({ status: 'disabled' })
  })
})
