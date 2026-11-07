import type { Language } from '@/lib/contract'
import { localeOf, useI18nStore } from '@/lib/i18n'

/**
 * Locale-aware formatting through `Intl`. Capture times come from EXIF, carry
 * no time zone and are read as UTC by the engine: they are formatted in UTC so
 * "14:32" stays the time shown by the camera.
 */

const cache = new Map<string, Intl.NumberFormat | Intl.DateTimeFormat | Intl.RelativeTimeFormat>()

function cached<T extends Intl.NumberFormat | Intl.DateTimeFormat | Intl.RelativeTimeFormat>(key: string, make: () => T): T {
  let f = cache.get(key) as T | undefined
  if (!f) {
    f = make()
    cache.set(key, f)
  }
  return f
}

function lang(language?: Language): Language {
  return language ?? useI18nStore.getState().language
}

export function formatNumber(value: number, language?: Language, options: Intl.NumberFormatOptions = {}): string {
  const l = lang(language)
  // French uses a narrow no-break space many UI fonts render too thin: use a regular one.
  return cached(`n:${l}:${JSON.stringify(options)}`, () => new Intl.NumberFormat(localeOf(l), options))
    .format(value)
    .replace(/\u202f/g, '\u00a0')
}

export function formatPercent(fraction: number, language?: Language): string {
  return formatNumber(fraction, language, { style: 'percent', maximumFractionDigits: 0 })
}

export function formatSignedNumber(value: number, language?: Language): string {
  return formatNumber(value, language, { signDisplay: 'exceptZero' })
}

export function formatBytes(bytes: number, language?: Language): string {
  const units = ['byte', 'kilobyte', 'megabyte', 'gigabyte', 'terabyte'] as const
  let value = bytes
  let unit = 0
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000
    unit += 1
  }
  return formatNumber(value, language, {
    style: 'unit',
    unit: units[unit],
    unitDisplay: 'short',
    maximumFractionDigits: value < 10 && unit > 1 ? 1 : 0,
  })
}

/** "1 min 20 s", "45 s", "2 h 05 min". */
export function formatDuration(ms: number, language?: Language): string {
  const total = Math.max(0, Math.round(ms / 1000))
  const h = Math.floor(total / 3600)
  const m = Math.floor((total % 3600) / 60)
  const s = total % 60
  const unit = (value: number, u: 'hour' | 'minute' | 'second') => formatNumber(value, language, { style: 'unit', unit: u, unitDisplay: 'short' })
  if (h) return `${unit(h, 'hour')} ${unit(m, 'minute')}`
  if (m) return s && m < 10 ? `${unit(m, 'minute')} ${unit(s, 'second')}` : unit(m, 'minute')
  return unit(s, 'second')
}

/** Capture time ("14:32"), in UTC (EXIF has no zone). */
export function formatCaptureTime(ms: number, language?: Language, seconds = false): string {
  const l = lang(language)
  return cached(
    `ct:${l}:${seconds}`,
    () => new Intl.DateTimeFormat(localeOf(l), { hour: '2-digit', minute: '2-digit', second: seconds ? '2-digit' : undefined, timeZone: 'UTC' }),
  ).format(ms)
}

export function formatCaptureDate(ms: number, language?: Language): string {
  const l = lang(language)
  return cached(
    `cd:${l}`,
    () =>
      new Intl.DateTimeFormat(localeOf(l), {
        weekday: 'short',
        day: 'numeric',
        month: 'short',
        year: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
        timeZone: 'UTC',
      }),
  ).format(ms)
}

/** Capture day ("sam. 14 juin"), in UTC like the capture times. */
export function formatCaptureDay(ms: number, language?: Language): string {
  const l = lang(language)
  return cached(`cday:${l}`, () => new Intl.DateTimeFormat(localeOf(l), { weekday: 'short', day: 'numeric', month: 'short', timeZone: 'UTC' })).format(ms)
}

const DAY_MS = 86_400_000

/** True when the capture times fall on more than one calendar day (UTC, like EXIF). */
export function spansSeveralDays(times: readonly (number | null)[]): boolean {
  const days = new Set(times.filter((t): t is number => t !== null).map((t) => Math.floor(t / DAY_MS)))
  return days.size > 1
}

/**
 * "14:32 – 16:10"; with `withDay` (shoots over several days) the day is
 * added: "sam. 14 juin, 14:32 – 16:10", or both days when the range crosses
 * midnight.
 */
export function formatTimeRange(start: number | null, end: number | null, language?: Language, withDay = false): string {
  if (start === null) return ''
  const a = formatCaptureTime(start, language)
  const b = end === null ? a : formatCaptureTime(end, language)
  if (!withDay) return b === a ? a : `${a} – ${b}`
  const dayA = formatCaptureDay(start, language)
  const sameDay = end === null || Math.floor(start / DAY_MS) === Math.floor(end / DAY_MS)
  if (sameDay) return b === a ? `${dayA}, ${a}` : `${dayA}, ${a} – ${b}`
  return `${dayA}, ${a} – ${formatCaptureDay(end, language)}, ${b}`
}

/** Calendar date of a real event (project opened…), local time zone. */
export function formatDay(ms: number, language?: Language): string {
  const l = lang(language)
  const sameYear = new Date(ms).getFullYear() === new Date().getFullYear()
  return cached(`d:${l}:${sameYear}`, () =>
    new Intl.DateTimeFormat(localeOf(l), { day: 'numeric', month: 'short', year: sameYear ? undefined : 'numeric' }),
  ).format(ms)
}

/** "il y a 2 jours", "hier", "à l’instant". */
/** "1 December" (in sentences: no abbreviation to clash with the full stop). */
export function formatLongDay(ms: number, language?: Language): string {
  const l = lang(language)
  const sameYear = new Date(ms).getFullYear() === new Date().getFullYear()
  return cached(`dl:${l}:${sameYear}`, () =>
    new Intl.DateTimeFormat(localeOf(l), { day: 'numeric', month: 'long', year: sameYear ? undefined : 'numeric' }),
  ).format(ms)
}

export function formatRelative(ms: number, language?: Language, now = Date.now()): string {
  const l = lang(language)
  const rtf = cached(`r:${l}`, () => new Intl.RelativeTimeFormat(localeOf(l), { numeric: 'auto' }))
  const diff = (ms - now) / 1000
  const abs = Math.abs(diff)
  if (abs < 60) return rtf.format(0, 'second')
  if (abs < 3600) return rtf.format(Math.round(diff / 60), 'minute')
  if (abs < 86_400) return rtf.format(Math.round(diff / 3600), 'hour')
  if (abs < 86_400 * 7) return rtf.format(Math.round(diff / 86_400), 'day')
  if (abs < 86_400 * 30) return rtf.format(Math.round(diff / (86_400 * 7)), 'week')
  return formatDay(ms, l)
}

/** "f/2.8" */
export function formatAperture(value: number | null, language?: Language): string | null {
  return value === null ? null : `ƒ/${formatNumber(value, language, { maximumFractionDigits: 1 })}`
}

export function formatFocal(value: number | null, language?: Language): string | null {
  return value === null ? null : `${formatNumber(value, language, { maximumFractionDigits: 0 })} mm`
}

/** Middle-ellipsis for long paths: "/Users/camille/…/Mariage Léa & Thomas". */
export function shortenPath(path: string, max = 48): string {
  if (path.length <= max) return path
  const parts = path.split(/([\\/])/)
  const last = parts.slice(-1)[0]
  const head = parts.slice(0, 5).join('')
  const candidate = `${head}…/${last}`
  return candidate.length < path.length ? candidate : `…${path.slice(-max)}`
}
