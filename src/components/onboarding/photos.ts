import cityNight from '@/assets/onboarding/city-night.webp'
import couple from '@/assets/onboarding/couple.webp'
import dance from '@/assets/onboarding/dance.webp'
import dress from '@/assets/onboarding/dress.webp'
import drink from '@/assets/onboarding/drink.webp'
import eyesClosed from '@/assets/onboarding/eyes-closed.webp'
import groupDark from '@/assets/onboarding/group-dark.webp'
import mountains from '@/assets/onboarding/mountains.webp'
import portraitA from '@/assets/onboarding/portrait-a.webp'
import portraitB from '@/assets/onboarding/portrait-b.webp'
import portraitBlurry from '@/assets/onboarding/portrait-blurry.webp'
import rings from '@/assets/onboarding/rings.webp'
import ringsRoses from '@/assets/onboarding/rings-roses.webp'

/**
 * Real photos shown by the welcome tour (CC BY 2.0 and CC0, credited in
 * src/assets/onboarding/CREDITS.md). The blurry and dark ones are degraded
 * copies made on purpose, to show what Pelli Instant sets aside.
 */
export const PHOTOS = {
  cityNight,
  couple,
  dance,
  dress,
  drink,
  eyesClosed,
  groupDark,
  mountains,
  portraitA,
  portraitB,
  portraitBlurry,
  rings,
  ringsRoses,
}

/** Starts loading every photo of the tour, so no step shows an empty frame. */
export function preloadPhotos(): void {
  for (const src of Object.values(PHOTOS)) {
    const img = new Image()
    img.decoding = 'async'
    img.src = src
  }
}
