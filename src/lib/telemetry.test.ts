import { describe, expect, it } from 'vitest'
import type { AppInfo } from '@/lib/contract'
import { MOCK_APP_INFO } from '@/lib/api/mock/fixtures'
import { translate } from '@/lib/i18n'
import { eventLabelKey, previewEvents, telemetryExample, TELEMETRY_EVENTS } from './telemetry'

describe('telemetry details', () => {
  it('describes every known event in every language', () => {
    for (const event of TELEMETRY_EVENTS) {
      const key = eventLabelKey(event)
      expect(key).not.toBeNull()
      expect(translate('fr', key!)).not.toBe(key)
      expect(translate('pl', key!)).not.toBe(translate('en', key!))
    }
    expect(eventLabelKey('something_new')).toBeNull()
  })

  it('lists the engine’s events, known ones first, unknown ones kept', () => {
    expect(previewEvents(null)).toEqual([...TELEMETRY_EVENTS])
    expect(previewEvents({ installId: null, events: ['something_new', 'app_opened', 'onboarding_completed'] })).toEqual(['onboarding_completed', 'app_opened', 'something_new'])
  })

  it('shows the engine’s example, or builds one with this computer’s values', () => {
    expect(telemetryExample({ installId: null, events: [], example: '{"event":"app_opened"}' }, null, 'fr')).toBe('{"event":"app_opened"}')
    const info: AppInfo = { ...MOCK_APP_INFO, platform: 'windows', arch: 'x86_64', version: '1.1.1-rc.3' }
    const example = JSON.parse(telemetryExample({ installId: 'abc', events: [] }, info, 'de'))
    expect(example).toMatchObject({ installId: 'abc', event: 'analysis_completed', appVersion: '1.1.1-rc.3', platform: 'windows', arch: 'x64', locale: 'de' })
    expect(Object.keys(example.props)).toEqual(['photos', 'raw', 'jpeg', 'seconds', 'precision', 'selected', 'toReview', 'rejected', 'groups'])
    // Never a file name or a path in what is shown.
    expect(JSON.stringify(example)).not.toMatch(/\/|\\\\|\.(jpg|cr3|nef)/i)
  })
})
