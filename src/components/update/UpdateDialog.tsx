import { Button, Modal, ModalBody, ModalContent } from '@heroui/react'
import { motion } from 'framer-motion'
import { ArrowUpRight, Check } from 'lucide-react'
import { useEffect, useState } from 'react'
import { InstantMark } from '@/components/ui/Logo'
import { api } from '@/lib/api'
import { useLanguage, useT } from '@/lib/i18n'
import { releaseHighlights } from '@/lib/release-notes'
import { useAnalysisStore } from '@/store/analysis-store'
import { useAppStore } from '@/store/app-store'
import { usePlusStore } from '@/store/plus-store'
import { usePrefsStore } from '@/store/prefs-store'
import { useUpdateStore } from '@/store/update-store'
import { installLine, RELEASES_URL, updateError, UpdateRing } from './UpdateNotice'

const EASE = [0.22, 1, 0.36, 1] as const

/**
 * A new version, announced once in the middle of the window: what is new
 * (the release notes, in the app language when they have it), install now or
 * later. Later keeps the pill of the title bars; the next version is
 * announced again. It waits for a quiet moment: not during the welcome tour,
 * an analysis or another dialog.
 */
export function UpdateDialog() {
  const status = useUpdateStore((s) => s.status)
  const dismissed = usePrefsStore((s) => s.dismissedUpdate)
  const dismiss = usePrefsStore((s) => s.setDismissedUpdate)
  const screen = useAppStore((s) => s.screen)
  const otherDialog = useAppStore((s) => s.dialog !== null)
  const plusOpen = usePlusStore((s) => s.dialogOpen)
  const analysing = useAnalysisStore((s) => s.running)
  // The version on screen: it stays while it downloads and installs.
  const [shown, setShown] = useState<string | null>(null)

  const offered = status.status === 'available' ? status.version : null
  const quiet = (screen === 'home' || screen === 'workspace') && !otherDialog && !plusOpen && !analysing
  useEffect(() => {
    if (shown === null && offered && offered !== dismissed && quiet) setShown(offered)
  }, [shown, offered, dismissed, quiet])

  const close = () => {
    if (shown) dismiss(shown)
    setShown(null)
  }

  return (
    <Modal
      isOpen={shown !== null}
      onClose={close}
      size="md"
      backdrop="blur"
      hideCloseButton
      classNames={{ base: 'glass-strong overflow-hidden rounded-[28px] text-foreground', backdrop: 'bg-black/55' }}
    >
      <ModalContent>
        <ModalBody className="relative p-0">{shown && <Announcement version={shown} onClose={close} />}</ModalBody>
      </ModalContent>
    </Modal>
  )
}

function Announcement({ version, onClose }: { version: string; onClose: () => void }) {
  const t = useT()
  const language = useLanguage()
  const status = useUpdateStore((s) => s.status)
  const install = useUpdateStore((s) => s.install)
  const analysing = useAnalysisStore((s) => s.running)
  // The notes come with the offer; kept once the install has started.
  const [notes] = useState(() => (status.status === 'available' ? (status.notes ?? null) : null))
  const highlights = releaseHighlights(notes, language)
  const installing = status.status === 'downloading' || status.status === 'restarting'
  const failed = status.status === 'failed' && status.version !== null ? status : null

  return (
    <div className="flex flex-col items-center px-8 pb-7 pt-9 text-center">
      <span aria-hidden className="pointer-events-none absolute -top-36 left-1/2 h-64 w-[460px] -translate-x-1/2 rounded-full bg-[radial-gradient(closest-side,rgba(88,64,240,0.4),transparent)]" />
      <motion.div
        initial={{ opacity: 0, scale: 0.85, filter: 'blur(4px)' }}
        animate={{ opacity: 1, scale: 1, filter: 'blur(0px)' }}
        transition={{ type: 'spring', duration: 0.5, bounce: 0.15 }}
        className="relative rounded-[22px] shadow-[0_18px_48px_-14px_rgba(88,64,240,0.85)]"
      >
        <InstantMark size={72} />
      </motion.div>
      <p className="relative mt-5 text-[11px] font-semibold uppercase tracking-[0.16em] text-[#a599f5]">{t('update.eyebrow')}</p>
      <h2 className="tabular relative mt-1.5 text-balance text-[22px] font-semibold leading-tight tracking-[-0.02em] text-foreground">{t('update.title', { version })}</h2>

      {highlights.length > 0 ? (
        <ul className="relative mt-5 flex w-full flex-col gap-2 rounded-2xl border border-white/[0.07] bg-white/[0.03] p-4 text-left">
          {highlights.map((line, i) => (
            <motion.li
              key={line}
              initial={{ opacity: 0, y: 6 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.35, ease: EASE, delay: 0.12 + i * 0.05 }}
              className="flex items-start gap-2.5 text-[13px] leading-snug text-default-600 dark:text-default-500"
            >
              <span className="mt-px flex size-[18px] shrink-0 items-center justify-center rounded-full bg-[#5840f0]/20 text-[#b8adf8]" aria-hidden>
                <Check className="size-3" strokeWidth={2.6} />
              </span>
              <span className="text-pretty text-foreground/85">{line}</span>
            </motion.li>
          ))}
        </ul>
      ) : (
        <p className="relative mt-3 max-w-[340px] text-pretty text-[14px] leading-relaxed text-default-500">{t('update.fallback')}</p>
      )}
      <button
        type="button"
        onClick={() => void api().openExternal(RELEASES_URL)}
        className="focus-ring relative mt-3 inline-flex h-9 items-center gap-1 rounded-full px-3 text-[13px] font-medium text-[#b8adf8] transition-[color,background-color] duration-150 hover:bg-white/[0.04] hover:text-[#d4ceff]"
      >
        {t('update.whatsNew')}
        <ArrowUpRight className="size-3.5" aria-hidden />
      </button>

      <div className="relative mt-4 w-full" aria-live="polite">
        {installing ? (
          <div className="flex flex-col items-center gap-3">
            <p className="flex items-center gap-2 text-[13px] text-default-500">
              <span className="text-[#a599f5]">
                <UpdateRing progress={status.status === 'downloading' ? status.progress : null} />
              </span>
              {installLine(t, status, analysing)}
            </p>
            <Button radius="full" variant="light" className="h-10 px-5 text-[13px] text-default-500" onPress={onClose}>
              {t('update.hide')}
            </Button>
          </div>
        ) : (
          <>
            {failed && <p className="mb-3 text-pretty text-[13px] leading-snug text-warning">{updateError(t, failed)}</p>}
            <Button radius="full" className="btn-brand h-12 w-full text-[15px] font-semibold" onPress={() => void install()}>
              {failed ? t('update.retry') : t('update.install')}
            </Button>
            <Button radius="full" variant="light" className="mt-1.5 h-10 w-full text-[13px] text-default-500" onPress={onClose}>
              {t('update.later')}
            </Button>
          </>
        )}
      </div>
      <p className="relative mt-2 text-pretty text-[11.5px] leading-snug text-default-400">{t('update.body')}</p>
    </div>
  )
}
