import { Tooltip } from '@heroui/react'
import { AnimatePresence, motion } from 'framer-motion'
import { CircleCheck, TriangleAlert } from 'lucide-react'
import { UpdateRing } from '@/components/update/UpdateNotice'
import { cn } from '@/lib/cn'
import { formatNumber } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { useAppStore } from '@/store/app-store'
import { useExportStore } from '@/store/export-store'

const SWAP = {
  initial: { opacity: 0, scale: 0.9, filter: 'blur(4px)' },
  animate: { opacity: 1, scale: 1, filter: 'blur(0px)' },
  exit: { opacity: 0, scale: 0.9, filter: 'blur(4px)' },
  transition: { type: 'spring', duration: 0.3, bounce: 0 },
} as const

/**
 * Photos sent to Pelli with the export dialog closed: their progress in the
 * title bar, then how it ended, until the dialog is opened again. Nothing
 * otherwise. `size`: the words (`full`), the count while sending (`count`),
 * or the ring alone (`icon`, for crowded bars); the tooltip always says it all.
 */
export function UploadPill({ size = 'full', className }: { size?: 'full' | 'count' | 'icon'; className?: string }) {
  const t = useT()
  const background = useExportStore((s) => s.background)
  const phase = useExportStore((s) => s.phase)
  const upload = useExportStore((s) => s.upload)
  const failed = useExportStore((s) => s.error !== null)
  const shown = background && (phase === 'uploading' || phase === 'done')

  const open = () => {
    useExportStore.getState().show()
    useAppStore.getState().openDialog('export')
  }
  const progress = upload && upload.total ? upload.done / upload.total : null
  const line =
    phase === 'uploading'
      ? upload
        ? t('export.pill.uploading', { done: formatNumber(upload.done), total: formatNumber(upload.total) })
        : t('export.preparingGallery')
      : failed
        ? t('export.pill.failed')
        : t('export.pill.done')
  const kind = phase === 'uploading' ? 'uploading' : failed ? 'failed' : 'done'
  const count = kind === 'uploading' && upload ? `${formatNumber(upload.done)}/${formatNumber(upload.total)}` : null
  const shownText = size === 'full' ? line : size === 'count' ? count : null

  return (
    <AnimatePresence initial={false} mode="popLayout">
      {shown && (
        <motion.div key={kind} {...SWAP} className={cn('flex', className)}>
          <Tooltip content={size === 'full' ? t('export.pill.show') : line} delay={300} closeDelay={0}>
            <button
              type="button"
              onClick={open}
              aria-label={`${line}. ${t('export.pill.show')}`}
              className={cn(
                'focus-ring relative inline-flex h-8 shrink-0 items-center gap-2 rounded-full bg-white/[0.06] text-[12px] font-medium text-foreground/85 transition-[background-color,scale] duration-150 ease-out after:absolute after:-inset-1 hover:bg-white/[0.1] active:scale-[0.96]',
                shownText === null ? 'w-8 justify-center' : 'px-3',
              )}
            >
              {kind === 'uploading' ? (
                <span className="text-[#a599f5]">
                  <UpdateRing progress={progress} />
                </span>
              ) : kind === 'failed' ? (
                <TriangleAlert className="size-3.5 shrink-0 text-warning" />
              ) : (
                <CircleCheck className="size-3.5 shrink-0 text-success" />
              )}
              {shownText !== null && <span className="tabular whitespace-nowrap">{shownText}</span>}
            </button>
          </Tooltip>
        </motion.div>
      )}
    </AnimatePresence>
  )
}
