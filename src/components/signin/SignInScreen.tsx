import { Button, Link, Spinner } from '@heroui/react'
import { motion } from 'framer-motion'
import { ArrowRight, ExternalLink } from 'lucide-react'
import { useEffect, useState } from 'react'
import { InstantLogo } from '@/components/ui/Logo'
import { Backdrop } from '@/components/ui/Backdrop'
import { LiquidOrb } from '@/components/ui/LiquidOrb'
import { WindowBar } from '@/components/ui/WindowBar'
import { api, toCullError } from '@/lib/api'
import { formatNumber } from '@/lib/format'
import { PELLI_LINKS } from '@/lib/plans'
import { useT } from '@/lib/i18n'
import { useAppStore } from '@/store/app-store'

const POLL_MS = 3000
const EASE = [0.22, 1, 0.36, 1] as const

/** Staggered entrance: rise, fade and un-blur (see make-interfaces-feel-better). */
const enter = (delay: number) => ({
  initial: { opacity: 0, y: 12, filter: 'blur(4px)' },
  animate: { opacity: 1, y: 0, filter: 'blur(0px)' },
  transition: { duration: 0.6, ease: EASE, delay },
})

/** Sign-in: one promise, one button. */
export function SignInScreen() {
  const t = useT()
  const auth = useAppStore((s) => s.auth)
  const setAuth = useAppStore((s) => s.setAuth)
  const offers = useAppStore((s) => s.offers)
  const [starting, setStarting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const pending = auth.status === 'pending'

  // Poll the engine while the browser consent is open.
  useEffect(() => {
    if (auth.status !== 'pending') return
    const expiresAt = auth.expiresAt
    const timer = setInterval(async () => {
      if (Date.now() > expiresAt) {
        clearInterval(timer)
        setError(t('signIn.expired'))
        setAuth(await api().authLogout())
        return
      }
      try {
        setAuth(await api().authPoll())
      } catch (e) {
        if (toCullError(e).code !== 'offline') setError(t('signIn.failed'))
      }
    }, POLL_MS)
    return () => clearInterval(timer)
  }, [auth, setAuth, t])

  const start = async () => {
    setStarting(true)
    setError(null)
    try {
      setAuth(await api().authStart())
    } catch {
      setError(t('signIn.failed'))
    } finally {
      setStarting(false)
    }
  }

  const cancel = async () => setAuth(await api().authLogout())

  return (
    <main className="relative flex h-full flex-col overflow-hidden text-white animate-fade-in">
      <Backdrop />
      <WindowBar>
        <InstantLogo size="sm" className="text-white" />
      </WindowBar>

      <div className="relative z-10 flex flex-1 flex-col items-center justify-center px-8 pb-16 text-center">
        {/* No CSS blur: a filter would clip the orb's halo to a square while it runs. */}
        <motion.div initial={{ opacity: 0, scale: 0.9 }} animate={{ opacity: 1, scale: 1 }} transition={{ type: 'spring', duration: 0.8, bounce: 0 }}>
          <LiquidOrb size={150} busy={pending} />
        </motion.div>

        <motion.span {...enter(0.1)} className="glass mt-10 inline-flex h-7 items-center rounded-full px-3 text-[12px] font-medium text-white/75">
          {t('signIn.eyebrow')}
        </motion.span>
        <motion.h1
          {...enter(0.15)}
          className="mt-5 text-balance text-[52px] font-semibold leading-[1.04] tracking-[-0.035em]"
        >
          {t('signIn.title')}
          <br />
          <span className="text-brand">{t('signIn.titleAccent')}</span>
        </motion.h1>
        <motion.p
          {...enter(0.25)}
          className="mt-5 max-w-[480px] text-pretty text-base leading-relaxed text-white/55"
        >
          {t('signIn.valueProp')}
        </motion.p>

        <motion.div
          {...enter(0.35)}
          className="mt-10 flex min-h-[96px] flex-col items-center"
        >
          {pending ? (
            <div className="glass flex max-w-[420px] flex-col items-center rounded-3xl px-6 py-5" role="status" aria-live="polite">
              <div className="flex items-center gap-3">
                <Spinner size="sm" color="current" className="text-brand" />
                <p className="text-sm font-semibold text-white">{t('signIn.waitingTitle')}</p>
              </div>
              <p className="mt-2 text-[13px] leading-relaxed text-white/55">{t('signIn.waitingBody')}</p>
              <div className="mt-4 flex gap-2">
                <Button size="sm" radius="full" className="bg-white/10 text-white hover:bg-white/15" startContent={<ExternalLink className="size-3.5" />} onPress={() => auth.status === 'pending' && void api().openExternal(auth.authUrl)}>
                  {t('signIn.reopen')}
                </Button>
                <Button size="sm" radius="full" variant="light" className="text-white/60" onPress={() => void cancel()}>
                  {t('signIn.cancel')}
                </Button>
              </div>
            </div>
          ) : (
            <>
              <Button
                size="lg"
                radius="full"
                className="btn-brand h-14 pl-9 pr-8 text-base font-semibold"
                isLoading={starting}
                onPress={() => void start()}
                endContent={!starting && <ArrowRight className="size-4" />}
              >
                {t('signIn.cta')}
              </Button>
              <p className="mt-5 text-[13px] text-white/40">
                {t('signIn.noAccount')}{' '}
                <Link as="button" size="sm" className="text-[13px] font-medium text-brand" onPress={() => void api().openExternal(offers?.links.register ?? PELLI_LINKS.register)}>
                  {t('signIn.createAccount')}
                </Link>
              </p>
              {offers && <p className="mt-1.5 text-[12px] text-white/35">{t('signIn.trialHint', { days: formatNumber(offers.trialDays) })}</p>}
            </>
          )}
          {error && (
            <p role="alert" className="mt-3 text-[13px] text-danger">
              {error}
            </p>
          )}
        </motion.div>
      </div>
    </main>
  )
}
