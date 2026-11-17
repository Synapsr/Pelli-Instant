import { Button } from '@heroui/react'
import { motion } from 'framer-motion'
import { AlertCircle, ArrowLeft, ChartColumn, ShieldCheck } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { EmptyState } from '@/components/ui/EmptyState'
import { api } from '@/lib/api'
import type { StatsOverview } from '@/lib/contract'
import { useT } from '@/lib/i18n'
import { useAppStore } from '@/store/app-store'
import { usePlusStore } from '@/store/plus-store'
import { EASE } from './parts'
import { StatsDetailView } from './StatsDetail'
import { StatsLocked } from './StatsLocked'

const FADE = {
  initial: { opacity: 0, y: 6 },
  animate: { opacity: 1, y: 0, transition: { duration: 0.3, ease: EASE, delay: 0.08 } },
  exit: { opacity: 0, transition: { duration: 0.12, ease: 'easeIn' } },
} as const

/**
 * Statistics across every shoot, from the home screen. Computed by the engine
 * on this computer and never sent; the details are part of Plus, the free
 * version sees its totals and what Plus adds. Escape or Back returns.
 */
export function StatsView({ onBack }: { onBack: () => void }) {
  const t = useT()
  const plus = usePlusStore((s) => s.status.active)
  const [stats, setStats] = useState<StatsOverview | null>(null)
  const [failed, setFailed] = useState(false)

  const load = useCallback(async () => {
    setFailed(false)
    try {
      setStats(await api().statsOverview())
    } catch {
      setFailed(true)
    }
  }, [])

  // Again when Plus arrives (bought from this view) or leaves.
  useEffect(() => {
    void load()
  }, [load, plus])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || useAppStore.getState().dialog !== null || usePlusStore.getState().dialogOpen || e.defaultPrevented) return
      e.preventDefault()
      onBack()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [onBack])

  const open = (projectId: string) => {
    const project = useAppStore.getState().projects.find((p) => p.id === projectId)
    if (project) void useAppStore.getState().openProject(project)
  }

  return (
    <motion.div className="absolute inset-0 flex flex-col" exit={{ opacity: 0, transition: { duration: 0.14 } }}>
      <motion.div {...FADE} className="mx-auto mt-1 flex w-full max-w-[1180px] items-center gap-3 px-8">
        <button
          type="button"
          onClick={onBack}
          className="focus-ring -ml-2 inline-flex h-10 items-center gap-1.5 rounded-full pl-2 pr-3.5 text-[13px] font-medium text-white/60 transition-[color,background-color,scale] duration-150 hover:bg-white/[0.06] hover:text-white active:scale-[0.96]"
        >
          <ArrowLeft className="size-4" aria-hidden />
          {t('home.back')}
        </button>
        <h2 className="text-[17px] font-semibold tracking-[-0.01em] text-white">{t('stats.title')}</h2>
        <span className="ml-auto inline-flex min-w-0 items-center gap-1.5 rounded-full bg-white/[0.035] px-3 py-1.5 text-[11.5px] text-white/50 shadow-[inset_0_0_0_1px_rgba(255,255,255,0.06)]">
          <ShieldCheck className="size-3.5 shrink-0 text-[#a599f5]" strokeWidth={2} aria-hidden />
          <span className="truncate">{t('stats.local')}</span>
        </span>
      </motion.div>

      <div className="relative min-h-0 flex-1 overflow-y-auto overflow-x-hidden">
        <div className="mx-auto w-full max-w-[1180px] px-8 pb-10 pt-4 short:pt-3">
          {failed ? (
            <EmptyState
              icon={AlertCircle}
              tone="warning"
              title={t('stats.error')}
              className="mt-10"
              action={
                <Button radius="full" variant="flat" onPress={() => void load()}>
                  {t('common.retry')}
                </Button>
              }
            />
          ) : stats === null ? null : stats.shoots === 0 ? (
            <EmptyState icon={ChartColumn} title={t('stats.empty.title')} body={t('stats.empty.body')} className="mt-10" />
          ) : stats.detail ? (
            <StatsDetailView stats={stats} detail={stats.detail} onOpen={open} />
          ) : (
            <StatsLocked stats={stats} />
          )}
        </div>
      </div>
    </motion.div>
  )
}
