import type { Account, Offers, Plan, PlanOffer } from '@/lib/contract'
import { formatNumber } from '@/lib/format'

/**
 * Pelli Instant is free and open source; official builds sort with a Pelli
 * account, and the Pelli plan sets how many photos a month (free 200, Lite
 * 1,000, Pro and Ultra unlimited, unlimited during a trial). Prices, trial
 * length and links come from Pelli (`getOffers`); the constants below are the
 * fallbacks when Pelli can't be reached.
 */

export const GITHUB_URL = 'https://github.com/Synapsr/Pelli-Instant'
export const PELLI_URL = 'https://pelli.io'

const UTM = 'utm_source=pelli-instant'
export const PELLI_LINKS = {
  register: `https://app.pelli.io/register?${UTM}`,
  billing: `https://app.pelli.io/dashboard/billing?${UTM}`,
  dashboard: `https://app.pelli.io/dashboard?${UTM}`,
}

const DAY = 86_400_000
const ORDER: Plan[] = ['free', 'lite', 'pro', 'ultra']

export function isUnlimited(account: Account): boolean {
  return account.entitlements.monthlyQuota === null
}

/** Photos left this month, null when unlimited. */
export function photosLeft(account: Account): number | null {
  const { monthlyQuota, usedThisMonth } = account.entitlements
  return monthlyQuota === null ? null : Math.max(0, monthlyQuota - usedThisMonth)
}

/** Whole days left in the running Pelli trial, null without trial. */
export function trialDaysLeft(account: Account, now = Date.now()): number | null {
  if (account.trialEndsAt === null) return null
  return Math.max(0, Math.ceil((account.trialEndsAt - now) / DAY))
}

/** "15 €", "9,50 €"… in the interface language. */
export function formatPrice(amount: number, currency: string): string {
  return formatNumber(amount, undefined, { style: 'currency', currency, maximumFractionDigits: Number.isInteger(amount) ? 0 : 2 })
}

/** Plans worth showing to this account: the ones above its own. */
export function upgradesFor(offers: Offers, account: Account | null): PlanOffer[] {
  const from = account && account.trialEndsAt === null ? ORDER.indexOf(account.plan) : 0
  return offers.plans.filter((p) => ORDER.indexOf(p.slug) > from)
}

/** Billing page: Pelli's link when known, else the fallback. */
export function billingUrl(offers: Offers | null): string {
  return offers?.links.billing ?? PELLI_LINKS.billing
}
