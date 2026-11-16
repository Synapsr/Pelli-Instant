import { Button } from '@heroui/react'
import { CircleCheck, Clock, LogIn, LogOut, Sparkles } from 'lucide-react'
import { useEffect, useState } from 'react'
import { ConfirmDialog } from '@/components/ui/ConfirmDialog'
import { SettingRow as Row } from '@/components/ui/SettingRow'
import type { Account } from '@/lib/contract'
import { useLanguage, useT } from '@/lib/i18n'
import { plusPriceLabel, shortKey } from '@/lib/plus'
import { useAppStore } from '@/store/app-store'
import { usePlusStore } from '@/store/plus-store'
import { KeyBox } from './PlusDialog'

export function initials(name: string): string {
  return name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((p) => p[0]?.toUpperCase())
    .join('')
}

export function Avatar({ account, size = 28 }: { account: Account; size?: number }) {
  if (account.avatarUrl) return <img src={account.avatarUrl} alt="" className="shrink-0 rounded-full object-cover" style={{ width: size, height: size }} />
  return (
    <span
      aria-hidden
      className="inline-flex shrink-0 items-center justify-center rounded-full bg-[#5840f0] font-semibold text-white shadow-[inset_0_1px_0_rgba(255,255,255,0.18)]"
      style={{ width: size, height: size, fontSize: size * 0.38 }}
    >
      {initials(account.name)}
    </span>
  )
}

/**
 * Settings → Pelli Instant Plus: where Plus stands, the licence (masked key,
 * release), the way to buy or enter a key, and the optional Pelli account
 * (Pro and Ultra include Plus).
 */
export function PlusSettings() {
  const t = useT()
  const language = useLanguage()
  const status = usePlusStore((s) => s.status)
  const offer = usePlusStore((s) => s.offer)
  const openDialog = usePlusStore((s) => s.openDialog)
  const release = usePlusStore((s) => s.release)
  const signIn = usePlusStore((s) => s.signIn)
  const auth = useAppStore((s) => s.auth)
  const signOut = useAppStore((s) => s.signOut)
  const [confirmRelease, setConfirmRelease] = useState(false)
  const [confirmSignOut, setConfirmSignOut] = useState(false)
  const loadOffer = usePlusStore((s) => s.loadOffer)
  const price = plusPriceLabel(offer, language)
  // Pelli's price for this country (the locale's guess until it answers, or offline).
  useEffect(() => {
    if (!status.active && !offer) void loadOffer()
  }, [status.active, offer, loadOffer])
  const account = auth.status === 'signedIn' ? auth.account : null

  return (
    <div>
      <h3 className="mb-4 text-lg font-semibold tracking-tight text-foreground">{t('settings.nav.plus')}</h3>

      <div className="relative overflow-hidden rounded-2xl border border-[#5840f0]/25 bg-gradient-to-br from-[#5840f0]/[0.16] via-[#5840f0]/[0.05] to-transparent p-5">
        <span aria-hidden className="pointer-events-none absolute -right-16 -top-20 size-56 rounded-full bg-[radial-gradient(closest-side,rgba(124,106,240,0.35),transparent)]" />
        <div className="relative flex items-start gap-4">
          <span className="brand-fill flex size-11 shrink-0 items-center justify-center rounded-xl" aria-hidden>
            <Sparkles className="size-5" strokeWidth={1.9} />
          </span>
          <div className="min-w-0 flex-1">
            {status.active ? (
              <>
                <p className="flex items-center gap-2 text-[15px] font-semibold text-foreground">
                  {t('plus.settings.activeTitle')}
                  <CircleCheck className="size-4 text-success" aria-hidden />
                </p>
                <p className="mt-0.5 text-[13px] text-default-500">
                  {status.source === 'account' && account ? t('plus.settings.viaAccount', { plan: account.planLabel }) : t('plus.settings.viaLicense')}
                </p>
              </>
            ) : (
              <>
                <p className="text-[15px] font-semibold text-foreground">{t('plus.settings.freeTitle')}</p>
                <p className="mt-0.5 max-w-[460px] text-pretty text-[13px] leading-snug text-default-500">{t('plus.settings.freeBody')}</p>
                <div className="mt-4 flex flex-wrap items-center gap-2">
                  <Button radius="full" className="btn-brand h-10 px-5 text-[13px] font-semibold" onPress={() => openDialog('settings')}>
                    <span className="tabular">{t('plus.settings.buy', { price })}</span>
                  </Button>
                  <Button radius="full" variant="flat" className="h-10 px-4 text-[13px]" onPress={() => openDialog('settings', 'key')}>
                    {t('plus.settings.enterKey')}
                  </Button>
                </div>
              </>
            )}
          </div>
        </div>
        {status.pending && !status.active && (
          <div className="relative mt-4 flex items-center gap-3 rounded-xl bg-black/20 px-3.5 py-2.5">
            <Clock className="size-4 shrink-0 text-warning" aria-hidden />
            <p className="min-w-0 flex-1 text-[13px] text-default-600">{t('plus.settings.pending')}</p>
            <Button size="sm" radius="full" variant="flat" className="h-9 px-4" onPress={() => openDialog('settings', 'waiting')}>
              {t('plus.settings.resume')}
            </Button>
          </div>
        )}
      </div>

      {status.key && (
        <>
          <KeyBox value={status.key} label={t('plus.settings.key')} className="mt-3" />
          <Row label={t('plus.settings.release')} hint={t('plus.settings.releaseHint')}>
            <Button size="sm" radius="lg" variant="flat" color="danger" onPress={() => setConfirmRelease(true)}>
              {t('plus.settings.release')}
            </Button>
          </Row>
        </>
      )}

      <div className="mt-6">
        <h4 className="mb-1 text-[11px] font-semibold uppercase tracking-[0.06em] text-default-400">{t('plus.settings.account')}</h4>
        {account ? (
          <div className="flex items-center gap-3.5 border-b border-white/[0.06] py-3.5">
            <Avatar account={account} size={40} />
            <div className="min-w-0 flex-1">
              <p className="flex items-center gap-2 truncate text-[14px] font-semibold text-foreground">
                {account.name}
                <span className="rounded-full bg-white/[0.06] px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-default-500">{account.planLabel}</span>
              </p>
              <p className="truncate text-[12px] text-default-500">{account.email}</p>
            </div>
            <Button size="sm" radius="lg" variant="light" startContent={<LogOut className="size-4" />} onPress={() => setConfirmSignOut(true)}>
              {t('plus.settings.signOut')}
            </Button>
          </div>
        ) : (
          <Row label={t('plus.included')} hint={t('plus.settings.accountHint')}>
            <Button
              size="sm"
              radius="lg"
              variant="flat"
              startContent={<LogIn className="size-4" />}
              isDisabled={auth.status === 'pending'}
              onPress={() => {
                openDialog('settings')
                void signIn()
              }}
            >
              {t('plus.settings.signIn')}
            </Button>
          </Row>
        )}
      </div>

      <p className="mt-5 text-pretty text-[12px] text-default-400">{t('plus.soon')}</p>

      <ConfirmDialog
        open={confirmRelease}
        title={t('plus.settings.releaseTitle')}
        body={t('plus.settings.releaseBody', { key: shortKey(status.key) ?? '' })}
        confirmLabel={t('plus.settings.release')}
        danger
        onConfirm={() => void release()}
        onClose={() => setConfirmRelease(false)}
      />
      <ConfirmDialog
        open={confirmSignOut}
        title={t('plus.settings.signOutTitle')}
        body={status.source === 'account' ? t('plus.settings.signOutBodyPlus') : t('plus.settings.signOutBody')}
        confirmLabel={t('plus.settings.signOut')}
        danger
        onConfirm={() => void signOut()}
        onClose={() => setConfirmSignOut(false)}
      />
    </div>
  )
}
