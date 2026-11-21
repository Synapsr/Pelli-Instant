/**
 * The free version's one-time "support us" card: after a few successful
 * exports, it asks to follow Pelli on Instagram, and in second place to star
 * the project on GitHub. Shown once, at the end of an export, never as a
 * pop-up; hidden with Plus, like the other Pelli cards.
 */

/** Successful exports before the card shows: by then the app has proved useful. */
export const SUPPORT_AFTER_EXPORTS = 3

export interface SupportCardInput {
  /** successful exports on this computer */
  exportsDone: number
  /** the card was closed, or one of its links opened */
  closed: boolean
  /** whether the engine already said if Plus is active */
  plusLoaded: boolean
  plusActive: boolean
}

export function shouldAskSupport({ exportsDone, closed, plusLoaded, plusActive }: SupportCardInput): boolean {
  return plusLoaded && !plusActive && !closed && exportsDone >= SUPPORT_AFTER_EXPORTS
}
