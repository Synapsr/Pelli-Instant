import beachSunset from '@/assets/onboarding/beach-sunset.webp'
import dish from '@/assets/onboarding/dish.webp'
import eyesClosed from '@/assets/onboarding/eyes-closed.webp'
import familyBlurry from '@/assets/onboarding/family-blurry.webp'
import familyFloor from '@/assets/onboarding/family-floor.webp'
import familyKiss from '@/assets/onboarding/family-kiss.webp'
import forest from '@/assets/onboarding/forest.webp'
import goldenField from '@/assets/onboarding/golden-field.webp'
import horse from '@/assets/onboarding/horse.webp'
import pregnancy from '@/assets/onboarding/pregnancy.webp'
import pumpkinA from '@/assets/onboarding/pumpkin-a.webp'
import pumpkinB from '@/assets/onboarding/pumpkin-b.webp'
import pumpkinBlurry from '@/assets/onboarding/pumpkin-blurry.webp'
import road from '@/assets/onboarding/road.webp'
import table from '@/assets/onboarding/table.webp'
import tooDark from '@/assets/onboarding/too-dark.webp'
import wedding from '@/assets/onboarding/wedding.webp'

/**
 * Real photos shown by the welcome tour (Pexels licence, credited in
 * src/assets/onboarding/CREDITS.md). `familyBlurry`, `tooDark` and
 * `pumpkinBlurry` are degraded copies made on purpose, to show what Pelli
 * Instant sets aside; the two pumpkins are a real near-duplicate pair.
 */
export const PHOTOS = {
  beachSunset,
  dish,
  eyesClosed,
  familyBlurry,
  familyFloor,
  familyKiss,
  forest,
  goldenField,
  horse,
  pregnancy,
  pumpkinA,
  pumpkinB,
  pumpkinBlurry,
  road,
  table,
  tooDark,
  wedding,
}

/** Starts loading every photo of the tour, so no step shows an empty frame. */
export function preloadPhotos(): void {
  for (const src of Object.values(PHOTOS)) {
    const img = new Image()
    img.decoding = 'async'
    img.src = src
  }
}
