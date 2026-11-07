import { AnimatePresence, motion } from 'framer-motion'
import { AlertCircle, Check, Download, Pause, Play, RotateCcw } from 'lucide-react'
import { useState } from 'react'
import { ConfirmDialog } from '@/components/ui/ConfirmDialog'
import { cn } from '@/lib/cn'
import type { ModelPack, ModelPackState } from '@/lib/contract'
import { formatBytes, formatDuration, formatPercent } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { usePackStore } from '@/store/pack-store'

/** Share of the pack already on disk, null when nothing is downloading. */
export function packProgress(state: ModelPackState): number | null {
  if (state.status === 'downloading' || state.status === 'failed') return state.totalBytes > 0 ? state.receivedBytes / state.totalBytes : 0
  if (state.status === 'preparing') return 1
  return null
}

/** Thin progress line of the download, drawn inside the Maximum card. */
export function PackBar({ state, className }: { state: ModelPackState; className?: string }) {
  const progress = packProgress(state)
  if (progress === null) return null
  const moving = state.status === 'downloading' || state.status === 'preparing'
  return (
    <span className={cn('block h-1.5 overflow-hidden rounded-full bg-white/[0.08]', className)} aria-hidden>
      <span
        className={cn(
          'block h-full origin-left rounded-full transition-[transform,background-color] duration-300 ease-out',
          moving ? 'bg-gradient-to-r from-[#5840f0] to-[#a599f5]' : 'bg-white/30',
          state.status === 'preparing' && 'pack-preparing',
        )}
        style={{ transform: `scaleX(${Math.max(0.02, progress)})` }}
      />
    </span>
  )
}

const ICON = { initial: { opacity: 0, scale: 0.25, filter: 'blur(4px)' }, animate: { opacity: 1, scale: 1, filter: 'blur(0px)' }, exit: { opacity: 0, scale: 0.25, filter: 'blur(4px)' } }

/**
 * One line under the precision choice: where the Maximum pack stands and the
 * one action that makes sense (pause, resume, retry, delete). Nothing while
 * the pack was never asked for.
 */
export function PackStatus({ pack, wanted, removable = false, className }: { pack: ModelPack; wanted: boolean; removable?: boolean; className?: string }) {
  const t = useT()
  const { download, pause, remove } = usePackStore()
  const [confirm, setConfirm] = useState(false)
  const { state } = pack
  const line = describe(t, pack)
  const show = state.status !== 'absent' && (state.status !== 'ready' || removable)
  // Chosen but not usable yet: the next analyses run in Standard meanwhile.
  const waiting = wanted && state.status !== 'ready'

  const action = (() => {
    switch (state.status) {
      case 'downloading':
        return { label: t('settings.analysis.precision.pause'), icon: Pause, run: pause }
      case 'failed':
        return state.code === 'cancelled'
          ? { label: t('settings.analysis.precision.resume'), icon: Play, run: download }
          : { label: t('settings.analysis.precision.retry'), icon: RotateCcw, run: download }
      case 'ready':
        return removable ? { label: t('settings.analysis.precision.remove'), icon: null, run: async () => setConfirm(true) } : null
      default:
        return null
    }
  })()

  const iconName = state.status === 'ready' ? 'check' : state.status === 'failed' && state.code !== 'cancelled' ? 'alert' : 'download'
  const Icon = { check: Check, alert: AlertCircle, download: Download }[iconName]
  const tone = state.status === 'failed' && state.code !== 'cancelled' ? 'text-warning' : state.status === 'ready' ? 'text-success' : 'text-[#a599f5]'

  return (
    <>
      <AnimatePresence initial={false}>
        {show && (
          <motion.div
            key="status"
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: 'auto' }}
            exit={{ opacity: 0, height: 0 }}
            transition={{ type: 'spring', duration: 0.4, bounce: 0 }}
            className="overflow-hidden"
          >
            <div className={cn('flex items-center gap-3 pt-3', className)} role="status" aria-live="polite">
              <span className={cn('relative flex size-4 shrink-0 items-center justify-center', tone)}>
                <AnimatePresence initial={false} mode="popLayout">
                  <motion.span key={iconName} {...ICON} transition={{ type: 'spring', duration: 0.3, bounce: 0 }} className="flex">
                    <Icon className="size-4" strokeWidth={2} />
                  </motion.span>
                </AnimatePresence>
              </span>
              <div className="min-w-0 flex-1">
                <p className="tabular truncate text-[13px] text-foreground/90">{line}</p>
                {waiting && <p className="text-[11px] text-default-500">{t('settings.analysis.precision.meanwhile')}</p>}
              </div>
              {action && (
                <button
                  type="button"
                  onClick={() => void action.run()}
                  className={cn(
                    'focus-ring inline-flex h-8 shrink-0 items-center gap-1.5 rounded-full px-3 text-xs font-medium transition-[background-color,color,scale] duration-150 active:scale-[0.96]',
                    state.status === 'ready' ? 'text-default-500 hover:bg-danger/10 hover:text-danger' : 'bg-white/[0.06] text-foreground hover:bg-white/[0.1]',
                  )}
                >
                  {action.icon && <action.icon className="size-3.5" />}
                  {action.label}
                </button>
              )}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
      <ConfirmDialog
        open={confirm}
        title={t('settings.analysis.precision.removeTitle')}
        body={t('settings.analysis.precision.removeBody', { size: formatBytes(pack.diskBytes) })}
        confirmLabel={t('settings.analysis.precision.remove')}
        danger
        onConfirm={() => void remove()}
        onClose={() => setConfirm(false)}
      />
    </>
  )
}

function describe(t: ReturnType<typeof useT>, pack: ModelPack): string {
  const { state } = pack
  switch (state.status) {
    case 'absent':
      return t('settings.analysis.precision.toDownload', { size: formatBytes(pack.sizeBytes) })
    case 'downloading': {
      const done = t('settings.analysis.precision.downloading', { received: formatBytes(state.receivedBytes), total: formatBytes(state.totalBytes) })
      const left = state.bytesPerSecond ? ((state.totalBytes - state.receivedBytes) / state.bytesPerSecond) * 1000 : null
      return left !== null && left > 1000 ? `${done} · ${t('settings.analysis.precision.left', { time: formatDuration(left) })}` : done
    }
    case 'preparing':
      return t('settings.analysis.precision.preparing')
    case 'ready':
      return t('settings.analysis.precision.installed', { size: formatBytes(pack.diskBytes) })
    case 'failed': {
      if (state.code === 'cancelled') return t('settings.analysis.precision.paused', { percent: formatPercent(state.totalBytes ? state.receivedBytes / state.totalBytes : 0) })
      if (state.code === 'offline') return t('settings.analysis.precision.failedOffline')
      if (state.code === 'diskFull') return t('settings.analysis.precision.failedDisk', { size: formatBytes(Math.max(0, pack.sizeBytes - state.receivedBytes)) })
      if (state.code === 'checksum') return t('settings.analysis.precision.failedChecksum')
      if (state.code === 'notFound') return t('settings.analysis.precision.failedUnavailable')
      return t('settings.analysis.precision.failed')
    }
  }
}
