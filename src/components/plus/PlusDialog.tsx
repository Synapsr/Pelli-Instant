import { Button, Modal, ModalBody, ModalContent, Spinner } from '@heroui/react'
import { AnimatePresence, motion, useReducedMotion } from 'framer-motion'
import { AlertCircle, ArrowLeft, ArrowRight, BellOff, Check, Copy, ExternalLink, KeyRound, Layers, Lock, ScanFace, Search, Sparkles, X, type LucideIcon } from 'lucide-react'
import { useEffect, useRef, useState, type ReactNode } from 'react'
import { PHOTOS } from '@/components/onboarding/photos'
import { LiquidOrb } from '@/components/ui/LiquidOrb'
import { PelliLogo } from '@/components/ui/Logo'
import { useElementSize } from '@/hooks/use-element-size'
import { api } from '@/lib/api'
import { cn } from '@/lib/cn'
import { PELLI_PRICING_URL } from '@/lib/links'
import { useLanguage, useT, type TKey } from '@/lib/i18n'
import { notify } from '@/lib/notify'
import { formatKeyInput, maskKey, normalizeKey, plusPriceLabel, type PlusView } from '@/lib/plus'
import { useAppStore } from '@/store/app-store'
import { usePackStore } from '@/store/pack-store'
import { usePlusStore } from '@/store/plus-store'
import { useWorkspaceStore } from '@/store/workspace-store'

const EASE = [0.22, 1, 0.36, 1] as const
const POLL_MS = 3000

/** What Plus brings, in the photographer's words (the real numbers, no model names). */
const BENEFITS: { key: 'maximum' | 'duplicates' | 'search' | 'noPromo'; icon: LucideIcon }[] = [
  { key: 'maximum', icon: ScanFace },
  { key: 'duplicates', icon: Layers },
  { key: 'search', icon: Search },
  { key: 'noPromo', icon: BellOff },
]

/** The searches typed, one after the other, on the photo of the showcase. */
const QUERIES = ['plus.showcase.q1', 'plus.showcase.q2', 'plus.showcase.q3'] as const

/**
 * Pelli Instant Plus: the offer (25 € or $25 once), a licence key, the wait
 * while the browser checkout is open, the success, and the errors in plain
 * words. A photo on the left shows what Plus is for; the views change on the
 * right. Signing in with a Pelli Pro or Ultra account is the second way,
 * always after the offer. One dialog for every entry point (Maximum, AI
 * search, settings, the Plus pill).
 */
export function PlusDialog() {
  const open = usePlusStore((s) => s.dialogOpen)
  const close = usePlusStore((s) => s.closeDialog)
  return (
    <Modal
      isOpen={open}
      onClose={close}
      size="3xl"
      radius="lg"
      backdrop="blur"
      hideCloseButton
      scrollBehavior="inside"
      classNames={{ base: 'glass-strong max-h-[calc(100vh-40px)] max-w-[780px] overflow-hidden rounded-[28px] text-foreground', backdrop: 'bg-black/60' }}
    >
      <ModalContent>
        <ModalBody className="relative overflow-hidden p-0">
          <PlusContent />
        </ModalBody>
      </ModalContent>
    </Modal>
  )
}

function PlusContent() {
  const view = usePlusStore((s) => s.view)
  const dialogOpen = usePlusStore((s) => s.dialogOpen)
  const poll = usePlusStore((s) => s.poll)
  const pollSignIn = usePlusStore((s) => s.pollSignIn)
  const close = usePlusStore((s) => s.closeDialog)
  const t = useT()
  const inner = useRef<HTMLDivElement>(null)
  const { height } = useElementSize(inner)

  // While the checkout (or the Pelli consent) is open in the browser.
  useEffect(() => {
    if (!dialogOpen || (view !== 'waiting' && view !== 'account')) return
    const run = view === 'waiting' ? poll : pollSignIn
    const timer = window.setInterval(() => void run(), POLL_MS)
    return () => window.clearInterval(timer)
  }, [dialogOpen, view, poll, pollSignIn])

  return (
    <div className="flex min-h-[540px]">
      <Showcase />
      <div className="relative min-w-0 flex-1">
      {/* Soft violet light behind the top of the right side. */}
      <span aria-hidden className="pointer-events-none absolute -top-40 left-1/2 h-72 w-[480px] -translate-x-1/2 rounded-full bg-[radial-gradient(closest-side,rgba(88,64,240,0.32),transparent)]" />
      <button
        type="button"
        onClick={close}
        aria-label={t('plus.close')}
        className="focus-ring absolute right-3 top-3 z-20 flex size-10 items-center justify-center rounded-full text-default-500 transition-[background-color,color,scale] duration-150 hover:bg-white/[0.06] hover:text-foreground active:scale-[0.96]"
      >
        <X className="size-[18px]" />
      </button>
      <motion.div animate={{ height: height || 'auto' }} transition={{ type: 'spring', duration: 0.35, bounce: 0 }} className="relative">
        <div ref={inner}>
          <AnimatePresence mode="wait" initial={false}>
            <motion.div
              key={view}
              initial={{ opacity: 0, y: 8, filter: 'blur(4px)' }}
              animate={{ opacity: 1, y: 0, filter: 'blur(0px)', transition: { duration: 0.28, ease: EASE } }}
              exit={{ opacity: 0, y: -6, filter: 'blur(4px)', transition: { duration: 0.12, ease: 'easeIn' } }}
            >
              <ViewContent view={view} />
            </motion.div>
          </AnimatePresence>
        </div>
      </motion.div>
      </div>
    </div>
  )
}

/**
 * The left of the dialog: a real photo, an AI search typing on it, and the
 * promise. Decorative but for the headline; still when motion is reduced.
 */
function Showcase() {
  const t = useT()
  const reduce = useReducedMotion()
  const [index, setIndex] = useState(0)
  const [typed, setTyped] = useState(0)
  const query = t(QUERIES[index])
  const shown = reduce ? query : query.slice(0, typed)
  useEffect(() => {
    if (reduce) return
    const done = typed >= query.length
    const timer = window.setTimeout(
      () => {
        if (done) {
          setIndex((i) => (i + 1) % QUERIES.length)
          setTyped(0)
        } else setTyped((n) => n + 1)
      },
      done ? 1900 : 60,
    )
    return () => window.clearTimeout(timer)
  }, [typed, query, reduce])

  return (
    <aside className="relative w-[300px] shrink-0 overflow-hidden">
      <img src={PHOTOS.goldenField} alt="" aria-hidden className="absolute inset-0 size-full origin-[50%_80%] scale-[1.32] object-cover" />
      <div aria-hidden className="absolute inset-0 bg-[linear-gradient(to_top,rgba(10,8,20,0.96)_8%,rgba(10,8,20,0.55)_42%,rgba(10,8,20,0.05)_70%)]" />
      <div aria-hidden className="absolute inset-x-0 top-0 h-44 bg-gradient-to-b from-[rgba(10,8,20,0.6)] via-[rgba(10,8,20,0.2)] to-transparent" />
      <div aria-hidden className="absolute inset-x-5 top-5 flex h-10 items-center gap-2.5 rounded-full border border-white/15 bg-black/40 px-4 text-[13px] text-white shadow-[0_8px_30px_-8px_rgba(0,0,0,0.6)] backdrop-blur-md">
        <Sparkles className="size-3.5 shrink-0 text-[#c9c1ff]" strokeWidth={2} />
        <span className="min-w-0 truncate">
          {shown}
          <span className="ml-px inline-block h-[14px] w-px translate-y-[2px] animate-pulse bg-white/80" />
        </span>
      </div>
      <div className="absolute inset-x-6 bottom-7">
        <PlusMark className="size-10 rounded-xl" />
        <p className="mt-4 text-[11px] font-semibold uppercase tracking-[0.16em] text-[#c9c1ff]">{t('plus.name')}</p>
        <h2 className="mt-1.5 text-balance text-[25px] font-semibold leading-[1.12] tracking-[-0.02em] text-white">{t('plus.headline')}</h2>
        <p className="mt-2.5 text-pretty text-[13px] leading-relaxed text-white/70">{t('plus.lead')}</p>
      </div>
    </aside>
  )
}

function ViewContent({ view }: { view: PlusView }) {
  switch (view) {
    case 'offer':
      return <OfferView />
    case 'key':
      return <KeyView />
    case 'waiting':
      return <WaitingView />
    case 'account':
      return <AccountView />
    case 'success':
      return <SuccessView />
    case 'error':
      return <ErrorView />
  }
}

/** The mark of Plus: a violet tile with a spark. */
function PlusMark({ className }: { className?: string }) {
  return (
    <span className={cn('brand-fill flex size-12 items-center justify-center rounded-2xl shadow-[0_12px_32px_-10px_rgba(88,64,240,0.9)]', className)} aria-hidden>
      <Sparkles className="size-[22px]" strokeWidth={1.9} />
    </span>
  )
}

function Header({ eyebrow, title, body, icon }: { eyebrow?: string; title: string; body?: ReactNode; icon?: ReactNode }) {
  return (
    <div className="flex flex-col items-center px-8 text-center">
      {icon}
      {eyebrow && <p className="mt-4 text-[11px] font-semibold uppercase tracking-[0.14em] text-[#a599f5]">{eyebrow}</p>}
      <h2 className="mt-2 text-balance text-[24px] font-semibold leading-tight tracking-[-0.02em] text-foreground">{title}</h2>
      {body && <p className="mt-2.5 max-w-[400px] text-pretty text-[14px] leading-relaxed text-default-500">{body}</p>}
    </div>
  )
}

function usePrice(): string {
  const offer = usePlusStore((s) => s.offer)
  const language = useLanguage()
  return plusPriceLabel(offer, language)
}

function OfferView() {
  const t = useT()
  const price = usePrice()
  const busy = usePlusStore((s) => s.busy)
  const checkout = usePlusStore((s) => s.checkout)
  const setView = usePlusStore((s) => s.setView)
  const signIn = usePlusStore((s) => s.signIn)
  const reason = usePlusStore((s) => s.reason)
  // Opened from the search field: what it asked for comes first.
  const benefits = reason === 'search' ? [...BENEFITS].sort((a, b) => Number(b.key === 'search') - Number(a.key === 'search')) : BENEFITS
  return (
    <div className="px-7 pb-6 pt-7">
      <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-default-400">{t('plus.includes')}</p>
      <ul className="mt-3 flex flex-col gap-1">
        {benefits.map(({ key, icon: Icon }, i) => (
          <motion.li
            key={key}
            initial={{ opacity: 0, x: 8 }}
            animate={{ opacity: 1, x: 0 }}
            transition={{ duration: 0.4, ease: EASE, delay: 0.06 + i * 0.05 }}
            className="flex items-start gap-3 py-1.5"
          >
            <span
              className="mt-0.5 flex size-9 shrink-0 items-center justify-center rounded-xl border border-[#a599f5]/20 bg-gradient-to-b from-[#5840f0]/30 to-[#5840f0]/10 text-[#d0c9ff] shadow-[inset_0_1px_0_rgba(255,255,255,0.08)]"
              aria-hidden
            >
              <Icon className="size-[17px]" strokeWidth={1.8} />
            </span>
            <span className="min-w-0">
              <span className="block text-[14px] font-semibold tracking-[-0.01em] text-foreground">{t(`plus.benefits.${key}.title`)}</span>
              <span className="mt-0.5 block text-pretty text-[12.5px] leading-snug text-default-500">{t(`plus.benefits.${key}.body`)}</span>
            </span>
          </motion.li>
        ))}
      </ul>

      <div className="relative mt-5 overflow-hidden rounded-[22px] border border-[#a599f5]/25 bg-gradient-to-b from-[#5840f0]/[0.16] to-[#5840f0]/[0.04] p-4 shadow-[inset_0_1px_0_rgba(255,255,255,0.06)]">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <p className="tabular text-[34px] font-semibold leading-none tracking-[-0.03em] text-foreground">{price}</p>
            <p className="mt-2 text-pretty text-[12px] leading-snug text-default-500">{t('plus.priceNote')}</p>
          </div>
          <button
            type="button"
            onClick={() => setView('key')}
            className="focus-ring -mr-1 -mt-1 shrink-0 rounded-full px-3 py-1.5 text-[12px] font-medium text-[#b8adf8] transition-[color,background-color] duration-150 hover:bg-white/[0.05] hover:text-[#d4ceff]"
          >
            {t('plus.haveKey')}
          </button>
        </div>
        <Button radius="full" className="btn-brand mt-4 h-12 w-full text-[15px] font-semibold" isLoading={busy} onPress={() => void checkout()} endContent={busy ? undefined : <ArrowRight className="size-4" />}>
          {t('plus.cta')}
        </Button>
        <p className="mt-2.5 flex items-center justify-center gap-1.5 text-center text-[11px] text-default-400">
          <Lock className="size-3 shrink-0" aria-hidden />
          {t('plus.secure')}
        </p>
      </div>

      <button
        type="button"
        onClick={() => void signIn()}
        className="focus-ring group mt-3 flex w-full items-center gap-3 rounded-[18px] border border-white/[0.09] bg-white/[0.035] px-3.5 py-3 text-left transition-[background-color,border-color] duration-150 hover:border-white/[0.18] hover:bg-white/[0.06]"
      >
        <span className="flex h-9 w-16 shrink-0 items-center justify-center rounded-xl bg-white text-black" aria-hidden>
          <PelliLogo className="h-[11px]" title="" />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-[13px] font-semibold text-foreground">{t('plus.accountTitle')}</span>
          <span className="block text-pretty text-[12px] leading-snug text-default-500">{t('plus.accountBody')}</span>
        </span>
        <span className="flex shrink-0 items-center gap-1 rounded-full bg-white/[0.08] px-3 py-1.5 text-[12px] font-medium text-foreground transition-colors duration-150 group-hover:bg-white/[0.14]">
          {t('plus.signIn')}
          <ArrowRight className="size-3.5 transition-transform duration-150 group-hover:translate-x-0.5" aria-hidden />
        </span>
      </button>
      <p className="mt-3 text-pretty text-center text-[11px] text-default-400">{t('plus.soon')}</p>
    </div>
  )
}

function BackButton({ onPress }: { onPress: () => void }) {
  const t = useT()
  return (
    <button
      type="button"
      onClick={onPress}
      aria-label={t('plus.back')}
      className="focus-ring absolute left-3 top-3 z-20 flex size-10 items-center justify-center rounded-full text-default-500 transition-[background-color,color,scale] duration-150 hover:bg-white/[0.06] hover:text-foreground active:scale-[0.96]"
    >
      <ArrowLeft className="size-[18px]" />
    </button>
  )
}

function KeyView() {
  const t = useT()
  const busy = usePlusStore((s) => s.busy)
  const error = usePlusStore((s) => s.error)
  const activate = usePlusStore((s) => s.activate)
  const setView = usePlusStore((s) => s.setView)
  const [value, setValue] = useState('')
  const complete = normalizeKey(value) !== null
  const submit = () => {
    if (complete && !busy) void activate(value)
  }
  return (
    <div className="pb-7 pt-9">
      <BackButton onPress={() => setView('offer')} />
      <Header
        icon={
          <span className="glass flex size-12 items-center justify-center rounded-2xl text-[#b8adf8]" aria-hidden>
            <KeyRound className="size-[22px]" strokeWidth={1.9} />
          </span>
        }
        title={t('plus.key.title')}
        body={t('plus.key.body')}
      />
      <form
        className="mt-6 px-8"
        onSubmit={(e) => {
          e.preventDefault()
          submit()
        }}
      >
        <label htmlFor="plus-key" className="mb-2 block text-[12px] font-medium text-default-500">
          {t('plus.key.label')}
        </label>
        <input
          id="plus-key"
          autoFocus
          spellCheck={false}
          autoComplete="off"
          autoCapitalize="characters"
          value={value}
          onChange={(e) => {
            setValue(formatKeyInput(e.target.value))
            if (usePlusStore.getState().error) usePlusStore.setState({ error: null })
          }}
          placeholder="XXXX-XXXX-XXXX-XXXX"
          aria-invalid={error !== null}
          aria-describedby={error ? 'plus-key-error' : undefined}
          className={cn(
            'tabular h-14 w-full rounded-2xl border bg-white/[0.04] px-4 text-center font-mono text-[20px] tracking-[0.12em] text-foreground outline-none transition-[border-color,background-color] duration-150 placeholder:text-default-300',
            error ? 'border-danger/60' : 'border-white/[0.1] focus:border-[#a599f5]/60 focus:bg-[#5840f0]/[0.08]',
          )}
        />
        <div className="min-h-10 pt-2" aria-live="polite">
          {error && (
            <p id="plus-key-error" className="text-pretty text-[13px] leading-snug text-danger">
              {t(`plus.errors.${error}` as TKey, { plan: '' })}
            </p>
          )}
        </div>
        <Button type="submit" radius="full" className="btn-brand mt-1 h-12 w-full text-[15px] font-semibold" isDisabled={!complete} isLoading={busy}>
          {t('plus.key.activate')}
        </Button>
      </form>
    </div>
  )
}

function WaitingView() {
  const t = useT()
  const url = usePlusStore((s) => s.checkoutUrl)
  const cancel = usePlusStore((s) => s.cancelCheckout)
  const checkout = usePlusStore((s) => s.checkout)
  return (
    <div className="flex flex-col items-center pb-7 pt-10">
      <LiquidOrb size={104} busy />
      <div className="mt-6">
        <Header title={t('plus.waiting.title')} body={t('plus.waiting.body')} />
      </div>
      <div className="mt-5 flex items-center gap-2 text-[13px] text-default-500" role="status" aria-live="polite">
        <Spinner size="sm" color="current" className="text-[#a599f5]" classNames={{ wrapper: 'size-4' }} />
        {t('plus.waiting.checking')}
      </div>
      <div className="mt-6 flex gap-2">
        <Button
          radius="full"
          className="glass glass-hover h-10 px-5 text-[13px] text-foreground"
          startContent={<ExternalLink className="size-3.5" />}
          onPress={() => (url ? void api().openExternal(url) : void checkout())}
        >
          {t('plus.waiting.reopen')}
        </Button>
        <Button radius="full" variant="light" className="h-10 px-5 text-[13px] text-default-500" onPress={() => void cancel()}>
          {t('plus.waiting.cancel')}
        </Button>
      </div>
    </div>
  )
}

function AccountView() {
  const t = useT()
  const auth = useAppStore((s) => s.auth)
  const cancel = usePlusStore((s) => s.cancelSignIn)
  return (
    <div className="flex flex-col items-center pb-7 pt-10">
      <LiquidOrb size={104} busy />
      <div className="mt-6">
        <Header title={t('plus.account.title')} body={t('plus.account.body')} />
      </div>
      <div className="mt-6 flex gap-2">
        <Button
          radius="full"
          className="glass glass-hover h-10 px-5 text-[13px] text-foreground"
          startContent={<ExternalLink className="size-3.5" />}
          onPress={() => auth.status === 'pending' && void api().openExternal(auth.authUrl)}
        >
          {t('plus.account.reopen')}
        </Button>
        <Button radius="full" variant="light" className="h-10 px-5 text-[13px] text-default-500" onPress={() => void cancel()}>
          {t('plus.account.cancel')}
        </Button>
      </div>
    </div>
  )
}

function SuccessView() {
  const t = useT()
  const status = usePlusStore((s) => s.status)
  const reason = usePlusStore((s) => s.reason)
  const close = usePlusStore((s) => s.closeDialog)
  const auth = useAppStore((s) => s.auth)
  const pack = usePackStore((s) => s.pack)
  const choose = usePackStore((s) => s.choose)
  const viaAccount = status.source === 'account' && auth.status === 'signedIn'
  const wantsMaximum = reason === 'precision' || reason === 'onboarding'
  const packReady = pack?.state.status === 'ready'

  const next = (() => {
    if (wantsMaximum && pack && !packReady)
      return {
        label: t('plus.success.downloadMaximum'),
        run: () => {
          close()
          void choose('maximum')
        },
      }
    if (reason === 'search' && useAppStore.getState().screen === 'workspace')
      return {
        label: t('plus.success.trySearch'),
        run: () => {
          close()
          useWorkspaceStore.getState().requestSearchFocus()
        },
      }
    return { label: t('plus.success.done'), run: close }
  })()

  return (
    <div className="flex flex-col items-center pb-7 pt-10">
      <div className="relative">
        <LiquidOrb size={112}>
          <motion.span
            initial={{ opacity: 0, scale: 0.25, filter: 'blur(4px)' }}
            animate={{ opacity: 1, scale: 1, filter: 'blur(0px)' }}
            transition={{ type: 'spring', duration: 0.45, bounce: 0, delay: 0.15 }}
            className="text-white"
          >
            <Check className="size-10" strokeWidth={2.2} />
          </motion.span>
        </LiquidOrb>
      </div>
      <div className="mt-6">
        <Header
          title={t('plus.success.title')}
          body={viaAccount && auth.status === 'signedIn' ? t('plus.success.bodyAccount', { plan: auth.account.planLabel }) : t('plus.success.body')}
        />
      </div>
      {status.key && <KeyBox value={status.key} className="mt-6 w-[calc(100%-64px)]" hint={t('plus.success.keyHint')} label={t('plus.success.keyLabel')} />}
      <Button radius="full" className="btn-brand mt-6 h-12 min-w-[240px] px-8 text-[15px] font-semibold" onPress={next.run}>
        {next.label}
      </Button>
    </div>
  )
}

/** A licence key, masked, with a copy button (the full key goes to the clipboard). */
export function KeyBox({ value, label, hint, className }: { value: string; label: string; hint?: string; className?: string }) {
  const t = useT()
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value)
      notify({ title: t('plus.settings.copied'), tone: 'success', timeout: 2000 })
    } catch {
      /* no clipboard: the key stays on the thank-you page */
    }
  }
  return (
    <div className={cn('rounded-2xl border border-white/[0.08] bg-white/[0.03] px-4 py-3', className)}>
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <p className="text-[11px] font-medium text-default-400">{label}</p>
          <p className="tabular mt-0.5 truncate font-mono text-[14px] tracking-[0.08em] text-foreground">{maskKey(value)}</p>
        </div>
        <Button size="sm" radius="full" variant="flat" className="h-10 min-w-0 px-4 text-[12px]" startContent={<Copy className="size-3.5" />} onPress={() => void copy()}>
          {t('plus.settings.copy')}
        </Button>
      </div>
      {hint && <p className="mt-2 text-pretty text-[11px] leading-snug text-default-500">{hint}</p>}
    </div>
  )
}

function ErrorView() {
  const t = useT()
  const error = usePlusStore((s) => s.error) ?? 'internal'
  const setView = usePlusStore((s) => s.setView)
  const auth = useAppStore((s) => s.auth)
  const signOut = useAppStore((s) => s.signOut)
  const plan = auth.status === 'signedIn' ? auth.account.planLabel : ''
  const retryView: PlusView = error === 'invalidKey' || error === 'otherDevice' || error === 'revoked' ? 'key' : 'offer'
  return (
    <div className="flex flex-col items-center pb-7 pt-10">
      <span className="flex size-14 items-center justify-center rounded-2xl bg-gradient-to-br from-warning/20 to-warning/5 text-warning" aria-hidden>
        <AlertCircle className="size-7" strokeWidth={1.8} />
      </span>
      <div className="mt-5">
        <Header title={t(error === 'expired' ? 'plus.errors.expiredTitle' : 'plus.errors.title')} body={t(`plus.errors.${error}`, { plan })} />
      </div>
      <div className="mt-6 flex flex-wrap justify-center gap-2">
        {error === 'notIncluded' ? (
          <>
            <Button radius="full" className="btn-brand h-11 px-6 font-semibold" onPress={() => setView('offer')}>
              {t('plus.back')}
            </Button>
            <Button radius="full" className="glass glass-hover h-11 px-5 text-foreground" endContent={<ExternalLink className="size-3.5" />} onPress={() => void api().openExternal(PELLI_PRICING_URL)}>
              {t('plus.errors.seePlans')}
            </Button>
            <Button radius="full" variant="light" className="h-11 px-5 text-default-500" onPress={() => void signOut().then(() => setView('offer'))}>
              {t('plus.errors.signOut')}
            </Button>
          </>
        ) : (
          <Button radius="full" className="btn-brand h-11 min-w-[180px] px-6 font-semibold" onPress={() => setView(retryView)}>
            {t('plus.errors.retry')}
          </Button>
        )}
      </div>
    </div>
  )
}
