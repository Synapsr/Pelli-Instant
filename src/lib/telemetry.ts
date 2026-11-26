import type { AppInfo, Language, TelemetryPreview } from '@/lib/contract'
import type { TKey } from '@/lib/i18n'

/**
 * Anonymous telemetry, opt-out (contract §telemetry): a random install id,
 * never linked to the device, the licence or an account; never a photo, a
 * file name or personal data. Helpers for "What is sent?".
 */

/** Events the engine sends, in the order the interface lists them. */
export const TELEMETRY_EVENTS = ['onboarding_completed', 'app_opened', 'analysis_completed', 'export_completed', 'plus_activated', 'telemetry_disabled'] as const
export type TelemetryEvent = (typeof TELEMETRY_EVENTS)[number]

/** The sentence describing an event, null for one the interface doesn't know (shown by its name). */
export function eventLabelKey(event: string): TKey | null {
  return (TELEMETRY_EVENTS as readonly string[]).includes(event) ? (`settings.privacy.events.${event}` as TKey) : null
}

/** The events to list: the engine's, known ones first in their usual order. */
export function previewEvents(preview: TelemetryPreview | null): string[] {
  const events = preview?.events?.length ? preview.events : [...TELEMETRY_EVENTS]
  const known = TELEMETRY_EVENTS.filter((e) => events.includes(e))
  return [...known, ...events.filter((e) => !(TELEMETRY_EVENTS as readonly string[]).includes(e))]
}

const PLATFORMS: Record<AppInfo['platform'], string> = { macos: 'mac', windows: 'windows', linux: 'linux', web: 'mac' }

/**
 * An example payload, as the engine sends it: the engine's own when it gives
 * one, else built here with this computer's values (the numbers are the
 * beta tester's 854-photo shoot).
 */
export function telemetryExample(preview: TelemetryPreview | null, info: AppInfo | null, language: Language): string {
  if (preview?.example) return preview.example
  const arch = info?.arch === 'aarch64' || info?.arch === 'arm64' ? 'arm64' : 'x64'
  return JSON.stringify(
    {
      installId: preview?.installId ?? '00000000-0000-4000-8000-000000000000',
      event: 'analysis_completed',
      appVersion: info?.version ?? '1.1.1-rc.4',
      platform: info ? PLATFORMS[info.platform] : 'mac',
      arch,
      locale: language,
      props: { photos: 854, raw: 854, jpeg: 0, seconds: 10.2, precision: 'standard', selected: 512, toReview: 201, rejected: 141, groups: 96 },
    },
    null,
    2,
  )
}
