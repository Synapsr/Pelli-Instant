/**
 * The free version's "support us" card: after every sort that ends well, the
 * result screen asks to follow Pelli on Instagram, and in second place to star
 * the project on GitHub, with a discreet line to Plus, which hides it. An
 * inline card, never a pop-up: closing it (or opening one of its links) hides
 * it for this sort only, and nothing is recorded or sent.
 */

export interface SupportCardInput {
  /** the sort that just ended offers the card, and it was not closed */
  offered: boolean
  /** a Plus status really arrived (not a failed request): until then, no promotion */
  plusKnown: boolean
  plusActive: boolean
}

/**
 * Whether the card shows right now. Plus is checked live: a Plus activated
 * while the card is visible hides it, and nothing shows before a Plus status
 * really arrived: a failed status request shows nothing.
 */
export function isSupportCardVisible({ offered, plusKnown, plusActive }: SupportCardInput): boolean {
  return offered && plusKnown && !plusActive
}
