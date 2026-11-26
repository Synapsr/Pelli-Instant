/**
 * The free version's support dialog: on the first way to the review of a
 * sort that ended well, a light dialog asks to follow Pelli on Instagram, and
 * in second place to star the project on GitHub, with a clear way to Plus,
 * which hides it. Every way out of it goes on to the review; nothing is
 * recorded or sent.
 */

export interface SupportInput {
  /** the sort offers the dialog (or it is open) */
  offered: boolean
  /** a Plus status really arrived (not a failed request): until then, no promotion */
  plusKnown: boolean
  plusActive: boolean
}

/**
 * Whether the dialog may show. Plus is checked live: a Plus activated while
 * it is open hides it, and nothing shows before a Plus status really
 * arrived: a failed status request shows nothing.
 */
export function isSupportVisible({ offered, plusKnown, plusActive }: SupportInput): boolean {
  return offered && plusKnown && !plusActive
}
