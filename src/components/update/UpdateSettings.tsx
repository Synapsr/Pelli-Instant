import { Button } from '@heroui/react'
import { AnimatePresence, motion } from 'framer-motion'
import { Check, RotateCcw } from 'lucide-react'
import type { ReactNode } from 'react'
import { SettingRow as Row } from '@/components/ui/SettingRow'
import { formatPercent } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { useAnalysisStore } from '@/store/analysis-store'
import { useUpdateStore } from '@/store/update-store'
import { installLine, updateError, UpdateRing } from './UpdateNotice'

/** Settings › About: where updates stand, and the one action that fits (check, install, retry). */
export function UpdateSettings() {
  const t = useT()
  const status = useUpdateStore((s) => s.status)
  const check = useUpdateStore((s) => s.check)
  const install = useUpdateStore((s) => s.install)
  const analysing = useAnalysisStore((s) => s.running)

  let line: ReactNode
  switch (status.status) {
    case 'idle':
      line = t('update.auto')
      break
    case 'checking':
      line = t('update.checking')
      break
    case 'upToDate':
      line = (
        <span className="inline-flex items-center gap-1.5 text-success">
          <Check className="size-3.5" />
          {t('update.upToDate')}
        </span>
      )
      break
    case 'available':
      line = <span className="tabular text-foreground/90">{t('update.title', { version: status.version })}</span>
      break
    case 'downloading':
    case 'restarting':
      line = (
        <span className="inline-flex items-center gap-1.5 text-foreground/85" role="status" aria-live="polite">
          {status.status === 'downloading' && <UpdateRing progress={status.progress} />}
          {installLine(t, status, analysing)}
          {status.status === 'downloading' && status.progress !== null && <span className="tabular text-default-500">{formatPercent(status.progress)}</span>}
        </span>
      )
      break
    case 'failed':
      line = updateError(t, status)
      break
    case 'disabled':
      line = t('update.disabled')
      break
  }

  const action = (() => {
    switch (status.status) {
      case 'available':
        return (
          <Button key="install" radius="lg" className="btn-brand h-10 px-4 text-[13px] font-semibold data-[pressed=true]:scale-[0.96]" onPress={() => void install()}>
            {t('update.install')}
          </Button>
        )
      case 'failed':
        if (status.version)
          return (
            <Button key="retry" variant="flat" radius="lg" className="h-10 px-4 text-[13px] data-[pressed=true]:scale-[0.96]" startContent={<RotateCcw className="size-4" />} onPress={() => void install()}>
              {t('update.retry')}
            </Button>
          )
        break
      case 'downloading':
      case 'restarting':
      case 'disabled':
        return null
    }
    return (
      <Button key="check" variant="flat" radius="lg" className="h-10 px-4 text-[13px] data-[pressed=true]:scale-[0.96]" isLoading={status.status === 'checking'} onPress={() => void check()}>
        {t('update.check')}
      </Button>
    )
  })()

  return (
    <Row label={t('update.settingsTitle')} hint={line}>
      <AnimatePresence initial={false} mode="popLayout">
        {action && (
          <motion.div
            key={action.key}
            initial={{ opacity: 0, scale: 0.96, filter: 'blur(4px)' }}
            animate={{ opacity: 1, scale: 1, filter: 'blur(0px)' }}
            exit={{ opacity: 0, scale: 0.96, filter: 'blur(4px)' }}
            transition={{ type: 'spring', duration: 0.3, bounce: 0 }}
          >
            {action}
          </motion.div>
        )}
      </AnimatePresence>
    </Row>
  )
}
