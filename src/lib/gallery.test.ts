import { describe, expect, it } from 'vitest'
import type { Account, AuthState } from '@/lib/contract'
import { galleryAccess, isKnownUploadError, needsPlan, smallPreviews } from './gallery'

const account = (sendToGallery?: boolean): Account => ({
  name: 'Camille',
  email: 'camille@example.com',
  slug: 'camille',
  avatarUrl: null,
  plan: 'pro',
  planLabel: 'Pro',
  trialEndsAt: null,
  lifetime: false,
  plus: true,
  entitlements: sendToGallery === undefined ? undefined : { analyze: true, monthlyQuota: null, usedThisMonth: 0, resetsAt: 0, sendToGallery },
})
const signedIn = (sendToGallery?: boolean): AuthState => ({ status: 'signedIn', account: account(sendToGallery) })

describe('gallery access', () => {
  it('asks to sign in without an account, and waits for the browser while signing in', () => {
    expect(galleryAccess({ status: 'signedOut' }, true)).toBe('signedOut')
    expect(galleryAccess({ status: 'pending', authUrl: 'https://app.pelli.io/cull/authorize', expiresAt: 1 }, true)).toBe('pending')
  })

  it('is ready for an account that may use its galleries, trials included', () => {
    expect(galleryAccess(signedIn(true), true)).toBe('ready')
    // An account cached by an older version has no entitlements: Pelli decides at send time.
    expect(galleryAccess(signedIn(), true)).toBe('ready')
  })

  it('names the paywall (trial over, no subscription) before any click, even offline', () => {
    expect(galleryAccess(signedIn(false), true)).toBe('paywall')
    expect(galleryAccess(signedIn(false), false)).toBe('paywall')
  })

  it('sends nothing offline', () => {
    expect(galleryAccess(signedIn(true), false)).toBe('offline')
    expect(galleryAccess({ status: 'signedOut' }, false)).toBe('offline')
  })
})

describe('RAW previews', () => {
  it('warns when RAW files alone carry small previews', () => {
    expect(smallPreviews({ photos: 40, rawOnly: 40, previewLongEdge: 1616 })).toEqual({ count: 40, size: 1616 })
    expect(smallPreviews({ photos: 40, rawOnly: 3, previewLongEdge: 680 })).toEqual({ count: 3, size: 680 })
  })

  it('says nothing for full-size previews, JPEG twins or before the check', () => {
    expect(smallPreviews({ photos: 40, rawOnly: 40, previewLongEdge: 6000 })).toBeNull()
    expect(smallPreviews({ photos: 40, rawOnly: 40, previewLongEdge: 2000 })).toBeNull()
    expect(smallPreviews({ photos: 40, rawOnly: 0, previewLongEdge: null })).toBeNull()
    expect(smallPreviews({ photos: 40, rawOnly: 2, previewLongEdge: null })).toBeNull()
    expect(smallPreviews(null)).toBeNull()
  })
})

describe('upload errors', () => {
  it('explains the known ones and offers the plans when a plan solves them', () => {
    expect(isKnownUploadError('subscription_required')).toBe(true)
    expect(isKnownUploadError('raw_conversion_failed')).toBe(false)
    expect(isKnownUploadError(null)).toBe(false)
    expect(needsPlan('subscription_required')).toBe(true)
    expect(needsPlan('gallery_limit_reached')).toBe(true)
    expect(needsPlan('storage_full')).toBe(true)
    expect(needsPlan('offline')).toBe(false)
  })
})
