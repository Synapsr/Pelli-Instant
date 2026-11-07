import { Button, Modal, ModalBody, ModalContent, ModalFooter } from '@heroui/react'
import { AnimatePresence, motion } from 'framer-motion'
import { Check, FolderInput, FolderOpen, HardDrive, Images, Tags, UploadCloud, type LucideIcon } from 'lucide-react'
import { useEffect, useMemo, useState, type ReactNode } from 'react'
import { PELLI_LINKS } from '@/lib/plans'
import { DECISION_BG } from '@/components/ui/Decision'
import type { ExportOptions, UploadRequest } from '@/lib/contract'
import { api } from '@/lib/api'
import { cn } from '@/lib/cn'
import { formatBytes, formatNumber, shortenPath } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { useAppStore } from '@/store/app-store'
import { useExportStore } from '@/store/export-store'
import { useReviewStore } from '@/store/review-store'
import { ExportReportView, ExportRunning } from './ExportProgress'
import { GallerySection, useGalleries, type GalleryChoice } from './GallerySection'

/**
 * Sending the selection to a Pelli gallery is built (engine, API, UI) but
 * left out of the V1 export: the three local outputs only. Flip to bring it
 * back.
 */
export const SHOW_GALLERY_EXPORT = false

export const DEFAULT_EXPORT: ExportOptions = {
  xmp: {
    enabled: true,
    rating: { selected: 3, toReview: 2, rejected: -1 },
    label: { selected: 'green', toReview: 'yellow', rejected: 'red' },
    keepUserMarks: true,
  },
  copySelected: { enabled: false, destination: null, includeRaw: true, includeJpeg: true, keepStructure: true },
  moveRejected: { enabled: false, folderName: '_Rejets Pelli' },
  fileList: false,
}

/** Last choices of the session, so a second export starts where the first ended. */
let remembered: { options: ExportOptions; upload: boolean; gallery: Omit<GalleryChoice, 'title'> } | null = null

export function ExportDialog() {
  const open = useAppStore((s) => s.dialog === 'export')
  const close = useAppStore((s) => s.closeDialog)
  const phase = useExportStore((s) => s.phase)
  const reset = useExportStore((s) => s.reset)
  const busy = phase === 'exporting' || phase === 'uploading'
  return (
    <Modal
      isOpen={open}
      onClose={() => {
        if (busy) return
        close()
        reset()
      }}
      isDismissable={!busy}
      hideCloseButton
      size="2xl"
      radius="lg"
      backdrop="blur"
      scrollBehavior="inside"
      classNames={{ base: 'glass-strong max-h-[calc(100vh-48px)] rounded-[28px] text-foreground', backdrop: 'bg-black/50' }}
    >
      <ModalContent>{(onClose) => <ExportContent onClose={onClose} />}</ModalContent>
    </Modal>
  )
}

/** One output: the whole card is the switch, details unfold when it is on. */
function OutputCard({
  icon: Icon,
  title,
  body,
  on,
  disabled = false,
  onToggle,
  children,
}: {
  icon: LucideIcon
  title: string
  body: ReactNode
  on: boolean
  disabled?: boolean
  onToggle: (on: boolean) => void
  children?: ReactNode
}) {
  return (
    <div className={cn('glass glass-hover rounded-2xl', on && 'glass-selected', disabled && 'opacity-50')}>
      <button
        type="button"
        role="switch"
        aria-checked={on}
        disabled={disabled}
        onClick={() => onToggle(!on)}
        className="focus-ring flex w-full items-center gap-4 rounded-2xl p-4 text-left"
      >
        <span className={cn('flex size-11 shrink-0 items-center justify-center rounded-xl transition-[background-color,color,box-shadow] duration-300', on ? 'brand-fill' : 'bg-white/[0.06] text-default-500')}>
          <Icon className="size-5" />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block text-[15px] font-semibold text-foreground">{title}</span>
          <span className="mt-0.5 block text-[13px] leading-snug text-default-500">{body}</span>
        </span>
        <span className={cn('flex size-6 shrink-0 items-center justify-center rounded-full border transition-[background-color,border-color,color] duration-300', on ? 'border-transparent bg-white text-[#5840f0]' : 'border-white/20 text-transparent')}>
          <AnimatePresence initial={false}>
            {on && (
              <motion.span initial={{ opacity: 0, scale: 0.25, filter: 'blur(4px)' }} animate={{ opacity: 1, scale: 1, filter: 'blur(0px)' }} exit={{ opacity: 0, scale: 0.25, filter: 'blur(4px)' }} transition={{ type: 'spring', duration: 0.3, bounce: 0 }}>
                <Check className="size-3.5" strokeWidth={3.2} />
              </motion.span>
            )}
          </AnimatePresence>
        </span>
      </button>
      <AnimatePresence initial={false}>
        {on && children && (
          <motion.div initial={{ height: 0, opacity: 0 }} animate={{ height: 'auto', opacity: 1 }} exit={{ height: 0, opacity: 0 }} transition={{ duration: 0.28, ease: [0.22, 1, 0.36, 1] }} className="overflow-hidden">
            <div className="px-4 pb-4 pl-[76px]">{children}</div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  )
}

function ExportContent({ onClose }: { onClose: () => void }) {
  const t = useT()
  const project = useReviewStore((s) => s.project)
  const photos = useReviewStore((s) => s.photos)
  const auth = useAppStore((s) => s.auth)
  const online = useAppStore((s) => s.online)
  const phase = useExportStore((s) => s.phase)
  const run = useExportStore((s) => s.run)
  const [options, setOptions] = useState<ExportOptions>(remembered?.options ?? DEFAULT_EXPORT)
  const [uploadOn, setUploadOn] = useState(SHOW_GALLERY_EXPORT && (remembered?.upload ?? false))
  const [gallery, setGallery] = useState<GalleryChoice>({
    mode: remembered?.gallery.mode ?? 'new',
    type: 'selection',
    galleryId: remembered?.gallery.galleryId ?? null,
    which: remembered?.gallery.which ?? 'selected',
    folderPerScene: remembered?.gallery.folderPerScene ?? true,
    title: project?.name ?? '',
  })
  // Signed in but behind the Pelli paywall: local exports work, galleries do not.
  const planBlocked = auth.status === 'signedIn' && !auth.account.entitlements.sendToGallery
  const signedIn = auth.status === 'signedIn' && auth.account.entitlements.sendToGallery
  const { galleries } = useGalleries(uploadOn && gallery.mode === 'existing')

  useEffect(() => {
    remembered = { options, upload: uploadOn, gallery }
  }, [options, uploadOn, gallery])

  const counts = useMemo(() => {
    const c = { selected: 0, toReview: 0, rejected: 0, total: 0, raw: 0, selectedBytes: 0 }
    for (const p of Object.values(photos)) {
      c.total += 1
      if (p.hasRaw) c.raw += 1
      if (p.status !== 'analyzed') continue
      c[p.decision] += 1
      if (p.decision === 'selected') c.selectedBytes += p.sizeBytes
    }
    return c
  }, [photos])

  if (!project) return null
  if (phase === 'exporting' || phase === 'uploading') return <ExportRunning />
  if (phase === 'done') {
    return (
      <>
        <ModalBody className="p-0">
          <ExportReportView folder={options.moveRejected.folderName} revealPath={options.copySelected.enabled && options.copySelected.destination ? options.copySelected.destination : project.rootPath} xmp={options.xmp.enabled && counts.raw > 0} />
        </ModalBody>
        <ModalFooter className="justify-center pb-6 pt-0">
          <Button radius="full" className="btn-brand h-11 px-8 font-semibold" onPress={onClose}>
            {t('common.done')}
          </Button>
        </ModalFooter>
      </>
    )
  }

  const set = <K extends keyof ExportOptions>(key: K, value: ExportOptions[K]) => setOptions((o) => ({ ...o, [key]: value }))
  const galleryName = gallery.mode === 'new' ? gallery.title.trim() : (galleries?.find((g) => g.id === gallery.galleryId)?.title ?? '')
  // XMP sidecars only go next to RAW files: without RAW the output does not apply.
  const xmpOn = options.xmp.enabled && counts.raw > 0
  const anyOutput = xmpOn || options.copySelected.enabled || options.moveRejected.enabled || uploadOn
  const problem = !anyOutput
    ? t('export.nothing')
    : options.copySelected.enabled && !options.copySelected.destination
      ? t('export.needDestination')
      : uploadOn && !galleryName
        ? t('export.needGallery')
        : null

  const confirm = () => {
    const request: UploadRequest | null = uploadOn
      ? {
          galleryId: gallery.mode === 'existing' ? gallery.galleryId : null,
          newGallery: gallery.mode === 'new' ? { title: gallery.title.trim(), type: 'selection' } : null,
          which: gallery.which,
          folderPerScene: gallery.folderPerScene,
        }
      : null
    void run(project.id, { ...options, fileList: false, xmp: { ...options.xmp, enabled: xmpOn } }, request)
  }

  const pickDestination = async () => {
    const path = await api().pickFolder()
    if (path) set('copySelected', { ...options.copySelected, enabled: true, destination: path })
  }

  return (
    <>
      <div className="flex items-center gap-4 px-7 pb-2 pt-7">
        <span className="brand-fill flex size-12 items-center justify-center rounded-2xl">
          <UploadCloud className="size-6" />
        </span>
        <div className="min-w-0">
          <h2 className="text-xl font-semibold tracking-tight text-foreground">{t('export.title')}</h2>
          <p className="mt-0.5 flex items-center gap-3 text-[13px] text-default-500">
            <span className="truncate">{project.name}</span>
            {(['selected', 'toReview', 'rejected'] as const).map((d) => (
              <span key={d} className="inline-flex items-center gap-1.5">
                <span className={cn('size-1.5 rounded-full', DECISION_BG[d])} />
                <span className="tabular">{formatNumber(counts[d])}</span>
              </span>
            ))}
          </p>
        </div>
      </div>

      <ModalBody className="gap-2.5 px-7 py-4">
        <OutputCard icon={Tags} title={t('export.cards.xmp')} body={counts.raw > 0 ? t('export.cards.xmpBody') : t('export.xmp.noRaw')} on={xmpOn} disabled={counts.raw === 0} onToggle={(enabled) => set('xmp', { ...options.xmp, enabled })} />

        <OutputCard icon={Images} title={t('export.cards.copy', { count: counts.selected })} body={t('export.cards.copyBody')} on={options.copySelected.enabled} onToggle={(enabled) => set('copySelected', { ...options.copySelected, enabled })}>
          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => void pickDestination()}
              className={cn('focus-ring flex h-10 min-w-0 flex-1 items-center gap-2.5 rounded-xl px-3 text-left transition-colors', options.copySelected.destination ? 'bg-white/[0.06] hover:bg-white/[0.09]' : 'border border-dashed border-white/20 hover:border-white/35')}
            >
              <FolderOpen className="size-4 shrink-0 text-default-400" aria-hidden />
              <span className={cn('truncate', options.copySelected.destination ? 'font-mono text-xs text-foreground' : 'text-[13px] text-default-400')}>
                {options.copySelected.destination ? shortenPath(options.copySelected.destination, 52) : t('export.copy.noDestination')}
              </span>
            </button>
            <Button size="sm" radius="full" className="h-10 bg-white/[0.08] px-4 text-[13px] text-foreground hover:bg-white/[0.12]" onPress={() => void pickDestination()}>
              {t('export.copy.choose')}
            </Button>
          </div>
          <p className="mt-2.5 flex items-start gap-2 text-xs leading-snug text-default-500">
            <HardDrive className="mt-px size-3.5 shrink-0" aria-hidden />
            <span>
              <span className="tabular font-semibold text-default-700">{t('export.cards.copySize', { size: formatBytes(counts.selectedBytes) })}</span> · {t('export.cards.copySpace')}
            </span>
          </p>
        </OutputCard>

        <OutputCard icon={FolderInput} title={t('export.cards.move', { count: counts.rejected })} body={t('export.cards.moveBody')} on={options.moveRejected.enabled} onToggle={(enabled) => set('moveRejected', { ...options.moveRejected, enabled })} />

        {SHOW_GALLERY_EXPORT && (
          <OutputCard
            icon={UploadCloud}
            title={t('export.gallery.title')}
            body={
              signedIn ? (
                t('export.gallery.body')
              ) : planBlocked ? (
                <>
                  {t('export.gallery.planRequired')}{' '}
                  <span role="link" tabIndex={0} className="cursor-pointer font-semibold text-[#a599f5] hover:underline" onClick={(e) => {
                    e.stopPropagation()
                    void api().openExternal(PELLI_LINKS.billing)
                  }}>
                    {t('export.gallery.choosePlan')}
                  </span>
                </>
              ) : (
                t('export.gallery.signInRequired')
              )
            }
            on={uploadOn && signedIn}
            disabled={!signedIn || !online}
            onToggle={setUploadOn}
          >
            <GallerySection value={gallery} onChange={setGallery} counts={{ selected: counts.selected, selectedAndToReview: counts.selected + counts.toReview }} galleries={galleries} />
          </OutputCard>
        )}
      </ModalBody>

      <ModalFooter className="items-center px-7 pb-6 pt-2">
        <p className={cn('mr-auto text-xs', problem ? 'text-warning' : 'text-default-400')}>{problem}</p>
        <Button radius="full" variant="light" className="h-11 px-5 text-default-500" onPress={onClose}>
          {t('common.cancel')}
        </Button>
        <Button
          radius="full"
          className="btn-brand h-11 px-7 font-semibold"
          isDisabled={problem !== null}
          onPress={confirm}
        >
          {t('export.confirm')}
        </Button>
      </ModalFooter>
    </>
  )
}
