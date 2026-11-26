/**
 * The free version's one-time "support us" card: after a few successful
 * exports, it asks to follow Pelli on Instagram, and in second place to star
 * the project on GitHub. Shown at most once, at the end of a successful
 * export, never as a pop-up; hidden with Plus, like the other Pelli cards.
 */

/** Successful exports before the card shows: by then the app has proved useful. */
export const SUPPORT_AFTER_EXPORTS = 3

/** What the export store knows about the run that just ended. */
export interface RunOutcome {
  phase: string
  /** the export or the upload stopped on an error */
  error: string | null
  report: { errors: readonly unknown[] } | null
  /** the run also sent the photos to a Pelli gallery */
  withUpload: boolean
  /** the upload's report: absent when it was cancelled or failed */
  uploadReport: { failed: number } | null
}

/**
 * The run ended and everything went well: every file written, and when photos
 * were sent to Pelli, every one of them arrived. A partial export, a failed or
 * a cancelled upload is not a success. The one condition both the export count
 * and the support card go by.
 */
export function isSuccessfulRun({ phase, error, report, withUpload, uploadReport }: RunOutcome): boolean {
  if (phase !== 'done' || error !== null || report === null || report.errors.length > 0) return false
  if (!withUpload) return true
  return uploadReport !== null && uploadReport.failed === 0
}

/** Taken once when a report opens: enough successful exports, and the card was never shown. */
export function isSupportCardDue({ exportsDone, shown }: { exportsDone: number; shown: boolean }): boolean {
  return !shown && exportsDone >= SUPPORT_AFTER_EXPORTS
}

export interface SupportCardInput {
  /** the card's turn had come when this report opened ({@link isSupportCardDue}) */
  due: boolean
  /** the run of this report succeeded ({@link isSuccessfulRun}) */
  succeeded: boolean
  /** whether the engine already said if Plus is active */
  plusLoaded: boolean
  plusActive: boolean
}

/**
 * Whether the report shows the card right now. `due` is decided once per
 * report, so the card doesn't vanish as soon as it is recorded as shown; Plus
 * is checked live, so a Plus activated while the report is open hides it.
 */
export function shouldShowSupportCard({ due, succeeded, plusLoaded, plusActive }: SupportCardInput): boolean {
  return due && succeeded && plusLoaded && !plusActive
}
