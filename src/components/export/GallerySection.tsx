import { Input, Spinner } from '@heroui/react'
import { Toggle } from '@/components/ui/Toggle'
import { Download, Heart, ImageIcon } from 'lucide-react'
import { useEffect, useState } from 'react'
import { SegmentedControl } from '@/components/ui/SegmentedControl'
import type { GallerySummary, UploadRequest } from '@/lib/contract'
import { api } from '@/lib/api'
import { cn } from '@/lib/cn'
import { formatNumber, formatRelative } from '@/lib/format'
import { useT } from '@/lib/i18n'

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

export function GallerySection({
  value,
  onChange,
  counts,
  galleries,
}: {
  value: GalleryChoice
  onChange: (v: GalleryChoice) => void
  counts: { selected: number; selectedAndToReview: number }
  galleries: GallerySummary[] | null
}) {
  const t = useT()
  const patch = (p: Partial<GalleryChoice>) => onChange({ ...value, ...p })
  const types = [
    { value: 'selection' as const, icon: Heart, title: t('export.gallery.typeSelection'), hint: t('export.gallery.typeSelectionHint') },
    { value: 'delivery' as const, icon: Download, title: t('export.gallery.typeDelivery'), hint: t('export.gallery.typeDeliveryHint') },
  ]

  return (
    <div className="space-y-4">
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

      {value.mode === 'new' ? (
        <div className="space-y-3">
          <Input
            size="sm"
            radius="lg"
            label={t('export.gallery.galleryTitle')}
            labelPlacement="outside"
            value={value.title}
            onValueChange={(title) => patch({ title })}
            classNames={{ label: 'text-xs text-default-500', inputWrapper: 'h-9' }}
          />
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
      <div className="flex items-center justify-between gap-4">
        <span>
          <span className="block text-[13px] text-foreground">{t('export.gallery.folderPerScene')}</span>
          <span className="block text-[11px] text-default-500">{t('export.gallery.folderPerSceneHint')}</span>
        </span>
        <Toggle isSelected={value.folderPerScene} onValueChange={(folderPerScene) => patch({ folderPerScene })} aria-label={t('export.gallery.folderPerScene')} />
      </div>
    </div>
  )
}
