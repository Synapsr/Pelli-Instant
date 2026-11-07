import { describe, expect, it } from 'vitest'
import { COMMANDS, DEFAULT_SETTINGS } from '@/lib/contract'
import { CullError, toCullError } from './errors'
import { createHttpApi } from './http'
import { createInvokeApi } from './invoke-api'

describe('command transport', () => {
  it('sends the contract command names with camelCase argument objects', async () => {
    const calls: [string, Record<string, unknown>][] = []
    const api = createInvokeApi(
      async (command, args) => {
        calls.push([command, args])
        return null
      },
      () => () => {},
    )
    await api.openFolder('/photos', true)
    await api.setMarks('p1', ['a', 'b'], { flag: 'pick' })
    await api.startAnalysis('p1', DEFAULT_SETTINGS)
    await api.setSettings({ theme: 'light' })
    await api.largePreview('p1', 'a')
    await api.appInfo()
    await api.installUpdate()
    expect(calls).toEqual([
      [COMMANDS.openFolder, { path: '/photos', recursive: true }],
      [COMMANDS.setMarks, { projectId: 'p1', photoIds: ['a', 'b'], patch: { flag: 'pick' } }],
      [COMMANDS.startAnalysis, { projectId: 'p1', settings: DEFAULT_SETTINGS }],
      [COMMANDS.setSettings, { patch: { theme: 'light' } }],
      [COMMANDS.largePreview, { projectId: 'p1', photoId: 'a' }],
      [COMMANDS.appInfo, {}],
      [COMMANDS.installUpdate, {}],
    ])
  })

  it('has no app to update over the dev bridge', async () => {
    const api = createHttpApi('http://127.0.0.1:1')
    expect(await api.updateStatus()).toEqual({ status: 'disabled' })
    expect(await api.checkForUpdate()).toEqual({ status: 'disabled' })
    await expect(api.installUpdate()).resolves.toBeNull()
  })

  it('normalises transport errors to CullError', async () => {
    const api = createInvokeApi(
      () => Promise.reject({ code: 'permissionDenied', message: 'Operation not permitted' }),
      () => () => {},
    )
    await expect(api.openFolder('/x', false)).rejects.toMatchObject({ code: 'permissionDenied', message: 'Operation not permitted' })
    expect(toCullError({ error: { code: 'notFound', message: 'gone' } })).toMatchObject({ code: 'notFound' })
    expect(toCullError(new TypeError('Failed to fetch')).code).toBe('offline')
    expect(toCullError('boom')).toBeInstanceOf(CullError)
  })
})
