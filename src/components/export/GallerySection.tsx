import { Input, Spinner } from '@heroui/react'
import { Toggle } from '@/components/ui/Toggle'
import { CircleUserRound, Download, FileImage, Heart, ImageIcon, TriangleAlert, type LucideIcon } from 'lucide-react'
import { useEffect, useState, type ReactNode } from 'react'
import { SegmentedControl } from '@/components/ui/SegmentedControl'
import type { GallerySummary, UploadCheck, UploadRequest } from '@/lib/contract'
import { api } from '@/lib/api'
import { cn } from '@/lib/cn'
import { formatNumber, formatRelative } from '@/lib/format'
import { smallPreviews } from '@/lib/gallery'
import { useT } from '@/lib/i18n'

/**
 * Sending to an existing gallery, the gallery type (client selection or
 * delivery) and sending the photos To review too all work in the engine, but
 * stay out of the first version: a new proofing gallery of the Selected
 * photos. Flip to show them.
 */
export const SHOW_GALLERY_OPTIONS = false

export interface GalleryChoice {
  mode: 'new' | 'existing'
  title: string
  type: 'selection' | 'delivery'
  galleryId: string | null
  which: UploadRequest['which']
  folderPerScene: boolean
}

let galleriesCache: GallerySummary[] | null = null

export function useGalleries(enabled: boolean): { galleries: GallerySummary[] | null; error: boolean } {
  const [galleries, setGalleries] = useState<GallerySummary[] | null>(galleriesCache)
  const [error, setError] = useState(false)
  useEffect(() => {
    if (!enabled) return
    let alive = true
    api()
      .listGalleries()
      .then((list) => {
        galleriesCache = list
        if (alive) setGalleries(list)
      })
      .catch(() => alive && setError(true))
    return () => {
      alive = false
    }
  }, [enabled])
  return { galleries, error }
}

/**
 * What the gallery will get for the Selected photos: RAW files alone and the
 * size of their embedded previews. Local (a few KiB of a few files), asked
 * once the card is on.
 */
export function useUploadCheck(projectId: string | null, enabled: boolean): UploadCheck | null {
  const [check, setCheck] = useState<UploadCheck | null>(null)
  useEffect(() => {
    if (!enabled || !projectId) return
    let alive = true
    api()
      .uploadCheck(projectId)
      .then((c) => alive && setCheck(c))
      .catch(() => alive && setCheck(null))
    return () => {
      alive = false
    }
  }, [projectId, enabled])
  return check
}

/** One line of what is sent, with its icon. */
function Note({ icon: Icon, tone = 'muted', children }: { icon: LucideIcon; tone?: 'muted' | 'warning'; children: ReactNode }) {
  return (
    <p className={cn('flex items-start gap-2 text-xs leading-snug', tone === 'warning' ? 'text-warning' : 'text-default-500')}>
      <Icon className="mt-px size-3.5 shrink-0" aria-hidden />
      <span>{children}</span>
    </p>
  )
}

export function GallerySection({
  value,
  onChange,
  counts,
  galleries,
  check,
  accountName,
}: {
  value: GalleryChoice
  onChange: (v: GalleryChoice) => void
  counts: { selected: number; selectedAndToReview: number }
  galleries: GallerySummary[] | null
  check: UploadCheck | null
  accountName: string | null
}) {
  const t = useT()
  const patch = (p: Partial<GalleryChoice>) => onChange({ ...value, ...p })
  const small = smallPreviews(check)
  const types = [
    { value: 'selection' as const, icon: Heart, title: t('export.gallery.typeSelection'), hint: t('export.gallery.typeSelectionHint') },
    { value: 'delivery' as const, icon: Download, title: t('export.gallery.typeDelivery'), hint: t('export.gallery.typeDeliveryHint') },
  ]

  return (
    <div className="space-y-3.5">
      {SHOW_GALLERY_OPTIONS && (
        <SegmentedControl
          size="sm"
          label={t('export.gallery.title')}
          value={value.mode}
          onChange={(mode) => patch({ mode })}
          options={[
            { value: 'new', label: t('export.gallery.new') },
            { value: 'existing', label: t('export.gallery.existing') },
          ]}
        />
      )}

      {value.mode === 'new' || !SHOW_GALLERY_OPTIONS ? (
        <div className="space-y-3">
          <Input
            size="sm"
            radius="lg"
            label={t('export.gallery.galleryTitle')}
            labelPlacement="outside"
            value={value.title}
            onValueChange={(title) => patch({ title })}
            maxLength={200}
            classNames={{ label: 'text-xs text-default-500', inputWrapper: 'h-9' }}
          />
          {SHOW_GALLERY_OPTIONS && (
            <div role="radiogroup" aria-label={t('export.gallery.type')} className="grid grid-cols-2 gap-2">
              {types.map(({ value: type, icon: Icon, title, hint }) => (
                <button
                  key={type}
                  type="button"
                  role="radio"
                  aria-checked={value.type === type}
                  onClick={() => patch({ type })}
                  className={cn(
                    'focus-ring flex items-start gap-2.5 rounded-xl border p-2.5 text-left transition-colors duration-200',
                    value.type === type ? 'border-[#5840f0]/40 bg-[#5840f0]/[0.06] dark:border-[#a599f5]/40 dark:bg-[#5840f0]/[0.14]' : 'border-default-200/80 hover:border-[#5840f0]/25 dark:border-white/[0.07]',
                  )}
                >
                  <Icon className={cn('mt-0.5 size-4 shrink-0', value.type === type ? 'text-[#5840f0] dark:text-[#a599f5]' : 'text-default-400')} />
                  <span>
                    <span className="block text-[13px] font-medium text-foreground">{title}</span>
                    <span className="block text-[11px] text-default-500">{hint}</span>
                  </span>
                </button>
              ))}
            </div>
          )}
        </div>
      ) : galleries === null ? (
        <p className="flex items-center gap-2 text-[13px] text-default-500">
          <Spinner size="sm" color="default" classNames={{ wrapper: 'size-4' }} />
          {t('export.gallery.loadingGalleries')}
        </p>
      ) : galleries.length === 0 ? (
        <p className="text-[13px] text-default-500">{t('export.gallery.noGalleries')}</p>
      ) : (
        <ul role="radiogroup" aria-label={t('export.gallery.existing')} className="max-h-48 space-y-1 overflow-y-auto pr-1">
          {galleries.map((g) => (
            <li key={g.id}>
              <button
                type="button"
                role="radio"
                aria-checked={value.galleryId === g.id}
                onClick={() => patch({ galleryId: g.id })}
                className={cn(
                  'focus-ring flex w-full items-center gap-3 rounded-xl border p-1.5 pr-3 text-left transition-colors duration-200',
                  value.galleryId === g.id ? 'border-[#5840f0]/40 bg-[#5840f0]/[0.06] dark:border-[#a599f5]/40 dark:bg-[#5840f0]/[0.14]' : 'border-transparent hover:bg-default-100 dark:hover:bg-white/[0.04]',
                )}
              >
                <span className="photo-well flex h-9 w-14 shrink-0 items-center justify-center overflow-hidden rounded-lg">
                  {g.coverUrl ? <img src={g.coverUrl} alt="" className="h-full w-full object-cover" /> : <ImageIcon className="size-4 text-default-300" />}
                </span>
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-[13px] font-medium text-foreground">{g.title}</span>
                  <span className="block text-[11px] text-default-500">
                    {t('export.gallery.images', { count: g.imageCount })} · {formatRelative(g.createdAt)}
                    {g.status === 'draft' && <span className="text-warning"> · {t('export.gallery.draft')}</span>}
                  </span>
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}

      {SHOW_GALLERY_OPTIONS && (
        <div>
          <p className="mb-1.5 text-xs text-default-500">{t('export.gallery.which')}</p>
          <SegmentedControl
            size="sm"
            label={t('export.gallery.which')}
            value={value.which}
            onChange={(which) => patch({ which })}
            options={[
              { value: 'selected', label: t('export.gallery.whichSelected'), meta: formatNumber(counts.selected) },
              { value: 'selectedAndToReview', label: t('export.gallery.whichSelectedAndToReview'), meta: formatNumber(counts.selectedAndToReview) },
            ]}
          />
        </div>
      )}
      <div className="flex items-center justify-between gap-4">
        <span>
          <span className="block text-[13px] text-foreground">{t('export.gallery.folderPerScene')}</span>
          <span className="block text-[11px] text-default-500">{t('export.gallery.folderPerSceneHint')}</span>
        </span>
        <Toggle isSelected={value.folderPerScene} onValueChange={(folderPerScene) => patch({ folderPerScene })} aria-label={t('export.gallery.folderPerScene')} />
      </div>
      <div className="space-y-1.5">
        {small ? (
          <Note icon={TriangleAlert} tone="warning">
            {t('export.gallery.smallPreviews', { count: small.count, size: formatNumber(small.size) })}
          </Note>
        ) : (
          <Note icon={FileImage}>{t('export.gallery.jpegOnly')}</Note>
        )}
        {accountName && <Note icon={CircleUserRound}>{t('export.gallery.account', { name: accountName })}</Note>}
      </div>
    </div>
  )
}
