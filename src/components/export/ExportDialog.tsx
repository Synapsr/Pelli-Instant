import { Button, Modal, ModalBody, ModalContent, ModalFooter, Spinner } from '@heroui/react'
import { AnimatePresence, motion } from 'framer-motion'
import { Check, ExternalLink, FolderInput, FolderOpen, HardDrive, Images, Tags, UploadCloud, type LucideIcon } from 'lucide-react'
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { DECISION_BG } from '@/components/ui/Decision'
import type { ExportOptions, UploadRequest } from '@/lib/contract'
import { api } from '@/lib/api'
import { cn } from '@/lib/cn'
import { formatBytes, formatNumber, shortenPath } from '@/lib/format'
import { galleryAccess } from '@/lib/gallery'
import { useI18nStore, useT } from '@/lib/i18n'
import { PELLI_GALLERY_PLANS_URL, PELLI_REGISTER_URL, PELLI_TRIAL_DAYS } from '@/lib/links'
import { useAppStore } from '@/store/app-store'
import { useExportStore } from '@/store/export-store'
import { DEFAULT_XMP_MAPPING, usePrefsStore, xmpExportMapping } from '@/store/prefs-store'
import { useReviewStore } from '@/store/review-store'
import { ExportReportView, ExportRunning } from './ExportProgress'
import { GallerySection, SHOW_GALLERY_OPTIONS, useGalleries, useUploadCheck, type GalleryChoice } from './GallerySection'
import { XmpMapping } from './XmpMapping'

/** How often the card checks a Pelli sign-in waiting in the browser. */
const SIGN_IN_POLL_MS = 3000

export const DEFAULT_EXPORT: ExportOptions = {
  // Stars, labels and their language: the photographer's choice, kept in the preferences.
  xmp: { enabled: true, rating: DEFAULT_XMP_MAPPING.rating, label: DEFAULT_XMP_MAPPING.label, keepUserMarks: true },
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
  const background = useExportStore((s) => s.background)
  // Opened again (the title bar's pill, the Export button) while photos are
  // sent, or after they were sent in the background: the dialog shows them.
  useEffect(() => {
    if (open && background) useExportStore.getState().show()
  }, [open, background])
  const dismiss = () => {
    if (phase === 'exporting') return
    close()
    // Photos on their way to Pelli: the upload goes on, the title bar shows it.
    if (phase === 'uploading') useExportStore.getState().hide()
    else useExportStore.getState().reset()
  }
  return (
    <Modal
      isOpen={open}
      onClose={dismiss}
      isDismissable={phase !== 'exporting'}
      isKeyboardDismissDisabled={phase === 'exporting'}
      hideCloseButton
      size="2xl"
      radius="lg"
      backdrop="blur"
      scrollBehavior="inside"
      classNames={{ base: 'glass-strong max-h-[calc(100vh-48px)] rounded-[28px] text-foreground [@media(max-height:760px)]:max-h-[calc(100vh-24px)]', backdrop: 'bg-black/50' }}
    >
      <ModalContent>{(onClose) => <ExportContent onClose={onClose} />}</ModalContent>
    </Modal>
  )
}

/**
 * One output: the whole card is the switch, details unfold when it is on.
 * With an `action` (sign in, see the plans), the card is not a switch: the
 * action sits where the check would.
 */
function OutputCard({
  icon: Icon,
  title,
  body,
  on = false,
  disabled = false,
  onToggle,
  action,
  children,
}: {
  icon: LucideIcon
  title: string
  body: ReactNode
  on?: boolean
  disabled?: boolean
  onToggle?: (on: boolean) => void
  action?: ReactNode
  children?: ReactNode
}) {
  const head = (
    <>
      <span className={cn('flex size-11 shrink-0 items-center justify-center rounded-xl transition-[background-color,color,box-shadow] duration-300', on ? 'brand-fill' : 'bg-white/[0.06] text-default-500')}>
        <Icon className="size-5" />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-[15px] font-semibold text-foreground">{title}</span>
        <span className="mt-0.5 block text-[13px] leading-snug text-default-500">{body}</span>
      </span>
    </>
  )
  return (
    <div className={cn('glass rounded-2xl', onToggle && !action && 'glass-hover', on && 'glass-selected', disabled && 'opacity-50')}>
      {action !== undefined || !onToggle ? (
        <div className="flex w-full items-center gap-4 rounded-2xl p-4 text-left">
          {head}
          {action}
        </div>
      ) : (
        <button
          type="button"
          role="switch"
          aria-checked={on}
          disabled={disabled}
          onClick={() => onToggle(!on)}
          className="focus-ring flex w-full items-center gap-4 rounded-2xl p-4 text-left"
        >
          {head}
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
      )}
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

/** A link inside a card's text (opens the browser, never toggles the card). */
function TextLink({ onPress, children }: { onPress: () => void; children: ReactNode }) {
  return (
    <button type="button" onClick={onPress} className="focus-ring rounded font-semibold text-[#5840f0] hover:underline dark:text-[#a599f5]">
      {children}
    </button>
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
  const signInError = useExportStore((s) => s.signInError)
  const [options, setOptions] = useState<ExportOptions>(remembered?.options ?? DEFAULT_EXPORT)
  const [uploadOn, setUploadOn] = useState(remembered?.upload ?? false)
  const [gallery, setGallery] = useState<GalleryChoice>({
    mode: remembered?.gallery.mode ?? 'new',
    type: remembered?.gallery.type ?? 'selection',
    galleryId: remembered?.gallery.galleryId ?? null,
    which: remembered?.gallery.which ?? 'selected',
    folderPerScene: remembered?.gallery.folderPerScene ?? true,
    title: project?.name ?? '',
  })
  // The first version sends the Selected photos to a new proofing gallery.
  const choice: GalleryChoice = SHOW_GALLERY_OPTIONS ? gallery : { ...gallery, mode: 'new', type: 'selection', which: 'selected' }
  const access = galleryAccess(auth, online)
  const galleryOn = uploadOn && access === 'ready'
  const { galleries } = useGalleries(galleryOn && choice.mode === 'existing')
  const check = useUploadCheck(project?.id ?? null, galleryOn)
  // Turned on at the bottom of a short window: its options come into view.
  const galleryRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!galleryOn) return
    const timer = window.setTimeout(() => galleryRef.current?.scrollIntoView({ block: 'nearest', behavior: 'smooth' }), 320)
    return () => window.clearTimeout(timer)
  }, [galleryOn])

  useEffect(() => {
    remembered = { options, upload: uploadOn, gallery }
  }, [options, uploadOn, gallery])

  // Signing in from the card: checked every 3 s while the browser page is
  // open; once signed in, the card turns on (nothing is sent before Export).
  const authStatus = auth.status
  const previous = useRef(authStatus)
  useEffect(() => {
    if (previous.current === 'pending' && authStatus === 'signedIn') setUploadOn(true)
    previous.current = authStatus
    if (authStatus !== 'pending') return
    const timer = window.setInterval(() => void useExportStore.getState().pollSignIn(), SIGN_IN_POLL_MS)
    return () => window.clearInterval(timer)
  }, [authStatus])

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

  if (phase === 'exporting' || phase === 'uploading') return <ExportRunning onHide={onClose} />
  if (phase === 'done') {
    return (
      <>
        <ModalBody className="p-0">
          <ExportReportView />
        </ModalBody>
        <ModalFooter className="justify-center pb-6 pt-0">
          <Button radius="full" className="btn-brand h-11 px-8 font-semibold" onPress={onClose}>
            {t('common.done')}
          </Button>
        </ModalFooter>
      </>
    )
  }
  if (!project) return null

  const set = <K extends keyof ExportOptions>(key: K, value: ExportOptions[K]) => setOptions((o) => ({ ...o, [key]: value }))
  const galleryName = choice.mode === 'new' ? choice.title.trim() : (galleries?.find((g) => g.id === choice.galleryId)?.title ?? '')
  const toSend = choice.which === 'selected' ? counts.selected : counts.selected + counts.toReview
  // XMP sidecars only go next to RAW files: without RAW the output does not apply.
  const xmpOn = options.xmp.enabled && counts.raw > 0
  const localOn = xmpOn || options.copySelected.enabled || options.moveRejected.enabled
  const problem = !localOn && !galleryOn
    ? t('export.nothing')
    : options.copySelected.enabled && !options.copySelected.destination
      ? t('export.needDestination')
      : galleryOn && !galleryName
        ? t('export.needGallery')
        : galleryOn && toSend === 0
          ? t('export.nothingToSend')
          : null

  const confirm = () => {
    const request: UploadRequest | null = galleryOn
      ? {
          galleryId: choice.mode === 'existing' ? choice.galleryId : null,
          newGallery: choice.mode === 'new' ? { title: choice.title.trim(), type: choice.type } : null,
          which: choice.which,
          folderPerScene: choice.folderPerScene,
        }
      : null
    const mapping = xmpExportMapping(usePrefsStore.getState().xmpMapping, useI18nStore.getState().language)
    const revealPath = options.copySelected.enabled && options.copySelected.destination ? options.copySelected.destination : project.rootPath
    void run(project.id, { ...options, fileList: false, xmp: { ...options.xmp, ...mapping, enabled: xmpOn } }, request, {
      revealPath,
      rejectsFolder: options.moveRejected.folderName,
      xmp: xmpOn,
    })
  }

  const pickDestination = async () => {
    const path = await api().pickFolder()
    if (path) set('copySelected', { ...options.copySelected, enabled: true, destination: path })
  }

  const galleryTitle = t('export.cards.gallery', { count: toSend })
  const signInProblem = signInError === 'offline' ? t('export.gallery.signInOffline') : signInError ? t('export.gallery.signInFailed') : null
  const galleryCard =
    access === 'ready' ? (
      <OutputCard icon={UploadCloud} title={galleryTitle} body={t('export.gallery.body')} on={galleryOn} onToggle={setUploadOn}>
        <GallerySection
          value={gallery}
          onChange={setGallery}
          counts={{ selected: counts.selected, selectedAndToReview: counts.selected + counts.toReview }}
          galleries={galleries}
          check={check}
          accountName={auth.status === 'signedIn' ? auth.account.name || auth.account.email : null}
        />
      </OutputCard>
    ) : access === 'offline' ? (
      <OutputCard icon={UploadCloud} title={galleryTitle} body={t('export.gallery.offline')} disabled />
    ) : access === 'paywall' ? (
      <OutputCard
        icon={UploadCloud}
        title={galleryTitle}
        body={t('export.uploadErrors.subscription_required')}
        action={
          <Button size="sm" radius="full" className="h-9 shrink-0 bg-white/[0.08] px-4 text-[13px] text-foreground hover:bg-white/[0.12]" endContent={<ExternalLink className="size-3.5" />} onPress={() => void api().openExternal(PELLI_GALLERY_PLANS_URL)}>
            {t('export.gallery.seePlans')}
          </Button>
        }
      />
    ) : access === 'pending' ? (
      <OutputCard
        icon={UploadCloud}
        title={galleryTitle}
        body={
          <>
            {t('export.gallery.pending')}{' '}
            <TextLink onPress={() => auth.status === 'pending' && void api().openExternal(auth.authUrl)}>{t('export.gallery.reopen')}</TextLink>
            {' · '}
            <TextLink onPress={() => void useExportStore.getState().cancelSignIn()}>{t('common.cancel')}</TextLink>
          </>
        }
        action={
          <span className="flex shrink-0 items-center gap-2 text-xs text-default-500" role="status">
            <Spinner size="sm" color="current" className="text-[#a599f5]" classNames={{ wrapper: 'size-4' }} />
            {t('export.gallery.waiting')}
          </span>
        }
      />
    ) : (
      <OutputCard
        icon={UploadCloud}
        title={galleryTitle}
        body={
          <>
            {t('export.gallery.signedOut')} {t('export.gallery.noAccount')}{' '}
            <TextLink onPress={() => void api().openExternal(PELLI_REGISTER_URL)}>{t('export.gallery.trial', { days: PELLI_TRIAL_DAYS })}</TextLink>
            {signInProblem && <span className="mt-1 block text-warning">{signInProblem}</span>}
          </>
        }
        action={
          <Button size="sm" radius="full" className="btn-brand h-9 shrink-0 px-4 text-[13px] font-semibold" onPress={() => void useExportStore.getState().signIn()}>
            {t('export.gallery.signIn')}
          </Button>
        }
      />
    )

  return (
    <>
      <div className="flex items-center gap-4 px-7 pb-2 pt-7 [@media(max-height:760px)]:pt-5">
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

      <ModalBody className="gap-2.5 px-7 py-4 [@media(max-height:760px)]:py-3">
        <OutputCard icon={Tags} title={t('export.cards.xmp')} body={counts.raw > 0 ? t('export.cards.xmpBody') : t('export.xmp.noRaw')} on={xmpOn} disabled={counts.raw === 0} onToggle={(enabled) => set('xmp', { ...options.xmp, enabled })}>
          <XmpMapping />
        </OutputCard>

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

        <div ref={galleryRef}>{galleryCard}</div>
      </ModalBody>

      <ModalFooter className="items-center px-7 pb-6 pt-2 [@media(max-height:760px)]:pb-4">
        <p className={cn('mr-auto text-xs', problem ? 'text-warning' : 'text-default-400')}>{problem}</p>
        <Button radius="full" variant="light" className="h-11 px-5 text-default-500" onPress={onClose}>
          {t('common.cancel')}
        </Button>
        <Button radius="full" className="btn-brand h-11 px-7 font-semibold" isDisabled={problem !== null} onPress={confirm}>
          {galleryOn ? (localOn ? t('export.confirmSend') : t('export.confirmSendOnly')) : t('export.confirm')}
        </Button>
      </ModalFooter>
    </>
  )
}
