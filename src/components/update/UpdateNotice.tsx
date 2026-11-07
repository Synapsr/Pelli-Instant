import { Button, Popover, PopoverContent, PopoverTrigger, Tooltip } from '@heroui/react'
import { AnimatePresence, motion } from 'framer-motion'
import { ArrowUpRight, Clock3, RotateCcw, RotateCw } from 'lucide-react'
import { useState, type ReactNode } from 'react'
import { api } from '@/lib/api'
import { cn } from '@/lib/cn'
import type { UpdateStatus } from '@/lib/contract'
import { formatPercent } from '@/lib/format'
import { useT, type Translate } from '@/lib/i18n'
import { GITHUB_URL } from '@/lib/plans'
import { useAnalysisStore } from '@/store/analysis-store'
import { useUpdateStore } from '@/store/update-store'

/** Release notes of the version on offer (always the latest release). */
export const RELEASES_URL = `${GITHUB_URL}/releases/latest`

type Failed = Extract<UpdateStatus, { status: 'failed' }>
type Installing = Extract<UpdateStatus, { status: 'downloading' | 'restarting' }>

/** Why checking or installing failed, in plain words. */
export function updateError(t: Translate, status: Failed): string {
  if (status.code === 'offline') return status.version ? t('update.offline') : t('update.checkOffline')
  if (status.code === 'unsupported' || status.code === 'signature') return t(`update.${status.code}`)
  return t('update.failed', { detail: status.message })
}

/** Where an install stands, once started. */
export function installLine(t: Translate, status: Installing, analysing: boolean): string {
  if (status.status === 'downloading') return t('update.downloading')
  if (!status.waiting) return t('update.restarting')
  return analysing ? t('update.waitingAnalysis') : t('update.waiting')
}

/** Download progress as a ring; spins while the size is unknown. */
export function UpdateRing({ progress }: { progress: number | null }) {
  const r = 6
  const c = 2 * Math.PI * r
  return (
    <svg viewBox="0 0 16 16" className={cn('size-4 shrink-0 -rotate-90', progress === null && 'animate-spin')} aria-hidden>
      <circle cx="8" cy="8" r={r} fill="none" stroke="currentColor" strokeOpacity={0.2} strokeWidth={2} />
      <circle
        cx="8"
        cy="8"
        r={r}
        fill="none"
        stroke="currentColor"
        strokeWidth={2}
        strokeLinecap="round"
        strokeDasharray={c}
        strokeDashoffset={c * (1 - Math.max(0.04, progress ?? 0.25))}
        className="transition-[stroke-dashoffset] duration-200 ease-out"
      />
    </svg>
  )
}

/** The pill: 32 px tall, a 40 px hit area. */
const PILL =
  'focus-ring relative inline-flex h-8 shrink-0 items-center gap-2 rounded-full text-[12px] font-medium after:absolute after:-inset-1 transition-[background-color,color,box-shadow,scale] duration-150 ease-out'
const QUIET = 'bg-white/[0.06] text-foreground/85'
const SWAP = {
  initial: { opacity: 0, scale: 0.9, filter: 'blur(4px)' },
  animate: { opacity: 1, scale: 1, filter: 'blur(0px)' },
  exit: { opacity: 0, scale: 0.9, filter: 'blur(4px)' },
  transition: { type: 'spring', duration: 0.3, bounce: 0 },
} as const

/**
 * Title bar notice of an app update: offered when one is published, then its
 * download, the wait for the work under way and the restart, or a quiet retry
 * when it failed. Nothing otherwise. `compact` keeps only the icon (and the
 * download percent), for crowded bars.
 */
export function UpdateNotice({ compact = false, className }: { compact?: boolean; className?: string }) {
  const t = useT()
  const status = useUpdateStore((s) => s.status)
  const install = useUpdateStore((s) => s.install)
  const analysing = useAnalysisStore((s) => s.running)
  const [open, setOpen] = useState(false)

  const label = (text: string) => <span className={cn('whitespace-nowrap', compact && 'sr-only')}>{text}</span>
  const tip = (text: string, pill: ReactNode, always = false) =>
    compact || always ? (
      <Tooltip content={text} delay={300} closeDelay={0}>
        {pill}
      </Tooltip>
    ) : (
      pill
    )

  let content: ReactNode = null
  switch (status.status) {
    case 'available':
      content = (
        <Popover placement="bottom-end" isOpen={open} onOpenChange={setOpen} offset={10} radius="lg">
          <PopoverTrigger>
            <button
              type="button"
              title={compact ? t('update.available') : undefined}
              className={cn(
                PILL,
                compact ? 'w-8 justify-center' : 'px-3',
                'bg-[#5840f0]/[0.16] text-[#d9d3fd] shadow-[inset_0_0_0_1px_rgba(124,106,240,0.35)] hover:bg-[#5840f0]/[0.26] active:scale-[0.96]',
              )}
            >
              <span aria-hidden className="size-1.5 shrink-0 rounded-full bg-[#a599f5] shadow-[0_0_8px_rgba(165,153,245,0.9)]" />
              {label(t('update.available'))}
            </button>
          </PopoverTrigger>
          <PopoverContent className="w-[300px] items-stretch p-4">
            <p className="tabular text-[13px] font-semibold text-foreground">{t('update.title', { version: status.version })}</p>
            <p className="mt-1 text-pretty text-xs leading-snug text-default-500">{t('update.body')}</p>
            <button
              type="button"
              onClick={() => void api().openExternal(RELEASES_URL)}
              className="focus-ring -mx-1 mt-1 inline-flex h-10 items-center gap-1 self-start rounded-lg px-1 text-xs font-medium text-[#a599f5] transition-[color,scale] duration-150 hover:text-[#c9c1fb] active:scale-[0.96]"
            >
              {t('update.whatsNew')}
              <ArrowUpRight className="size-3.5" />
            </button>
            <Button
              radius="full"
              className="btn-brand mt-1 h-10 text-[13px] font-semibold data-[pressed=true]:scale-[0.96]"
              onPress={() => {
                setOpen(false)
                void install()
              }}
            >
              {t('update.install')}
            </Button>
          </PopoverContent>
        </Popover>
      )
      break
    case 'downloading':
    case 'restarting': {
      const line = installLine(t, status, analysing)
      const icon =
        status.status === 'downloading' ? (
          <UpdateRing progress={status.progress} />
        ) : status.waiting ? (
          <Clock3 className="size-3.5 shrink-0" />
        ) : (
          <RotateCw className="size-3.5 shrink-0 animate-spin" />
        )
      const percent = status.status === 'downloading' && status.progress !== null ? formatPercent(status.progress) : null
      content = tip(
        line,
        <span role="status" aria-live="polite" className={cn(PILL, QUIET, compact && !percent ? 'w-8 justify-center' : 'px-3')}>
          {icon}
          {label(line)}
          {percent && <span className="tabular text-foreground/60">{percent}</span>}
        </span>,
      )
      break
    }
    case 'failed':
      // A failed check is for the settings; a failed install can be retried here.
      if (status.version)
        content = tip(
          updateError(t, status),
          <button
            type="button"
            onClick={() => void install()}
            className={cn(PILL, QUIET, compact ? 'w-8 justify-center' : 'px-3', 'hover:bg-white/[0.1] active:scale-[0.96]')}
          >
            <RotateCcw className="size-3.5 shrink-0 text-warning" />
            {label(t('update.retry'))}
          </button>,
          true,
        )
      break
  }

  // One key per kind of notice: the pill swaps between them, progress ticks don't.
  const kind = status.status === 'restarting' ? `restarting-${status.waiting}` : status.status
  return (
    <AnimatePresence initial={false} mode="popLayout">
      {content && (
        <motion.div key={kind} {...SWAP} className={cn('flex', className)}>
          {content}
        </motion.div>
      )}
    </AnimatePresence>
  )
}
