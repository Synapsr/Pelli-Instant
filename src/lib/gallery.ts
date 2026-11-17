import type { AuthState, UploadCheck } from '@/lib/contract'

/**
 * Where the photographer stands for sending to a Pelli gallery:
 * - `offline`: no network, nothing can be sent;
 * - `signedOut`: no Pelli account on this computer (sign in, or try Pelli);
 * - `pending`: the sign-in page is open in the browser;
 * - `paywall`: signed in, but the account is behind Pelli's paywall (trial
 *   over, no subscription): Pelli refuses galleries with `subscription_required`;
 * - `ready`: a new gallery can be created and filled.
 */
export type GalleryAccess = 'offline' | 'signedOut' | 'pending' | 'paywall' | 'ready'

export function galleryAccess(auth: AuthState, online: boolean): GalleryAccess {
  if (auth.status === 'pending') return 'pending'
  if (auth.status === 'signedOut') return online ? 'signedOut' : 'offline'
  if (auth.account.entitlements?.sendToGallery === false) return 'paywall'
  return online ? 'ready' : 'offline'
}

/**
 * Below this long edge, the preview a camera embedded in its RAW files is
 * worth a word before sending: the gallery shows it as the photo.
 */
export const SMALL_PREVIEW_PX = 2000

/** RAW files alone whose embedded previews are small: what the warning says, or null. */
export function smallPreviews(check: UploadCheck | null): { count: number; size: number } | null {
  if (!check || check.rawOnly === 0 || check.previewLongEdge === null) return null
  return check.previewLongEdge < SMALL_PREVIEW_PX ? { count: check.rawOnly, size: check.previewLongEdge } : null
}

/** Upload failures explained in the photographer's words (others show the engine message). */
export const UPLOAD_ERRORS = ['unauthorized', 'offline', 'storage_full', 'gallery_limit_reached', 'subscription_required', 'gallery_not_found'] as const
export type KnownUploadError = (typeof UPLOAD_ERRORS)[number]

export const isKnownUploadError = (code: string | null): code is KnownUploadError => UPLOAD_ERRORS.includes(code as KnownUploadError)

/** The failures a change of Pelli plan solves: the report offers Pelli's plans. */
export const needsPlan = (code: string | null): boolean => code === 'subscription_required' || code === 'gallery_limit_reached' || code === 'storage_full'
