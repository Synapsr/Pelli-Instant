import { Button } from '@heroui/react'
import { CircleCheck, Columns2, Loader } from 'lucide-react'
import type { ReactNode } from 'react'
import { DECISION_ICON, DECISION_TEXT } from '@/components/ui/Decision'
import { ReasonIcon } from '@/components/ui/ReasonIcon'
import type { FaceView, PhotoView, ScoreBreakdown } from '@/lib/contract'
import { cn } from '@/lib/cn'
import { formatAperture, formatBytes, formatCaptureDate, formatFocal, formatNumber, formatPercent } from '@/lib/format'
import { useT } from '@/lib/i18n'
import { useReviewStore } from '@/store/review-store'
import { useWorkspaceStore } from '@/store/workspace-store'
import { CloseUps } from './CloseUps'

function Section({ title, children, aside }: { title: string; children: ReactNode; aside?: ReactNode }) {
  return (
    <section className="border-b border-default-200/60 px-5 py-4 last:border-b-0 dark:border-white/[0.06]">
      <div className="mb-3 flex items-center justify-between">
        <h3 className="text-[11px] font-semibold uppercase tracking-[0.08em] text-default-400">{title}</h3>
        {aside}
      </div>
      {children}
    </section>
  )
}

function Bar({ label, value }: { label: string; value: number | null }) {
  const t = useT()
  const low = value !== null && value < 0.4
  return (
    <div className="flex items-center gap-3">
      <span className="w-[88px] shrink-0 text-[13px] text-default-600">{label}</span>
      <span className="h-1.5 flex-1 overflow-hidden rounded-full bg-default-100 dark:bg-white/[0.06]">
        {value !== null && (
          <span
            className={cn('block h-full rounded-full transition-[width] duration-500 ease-out', low ? 'bg-danger/75' : 'bg-[#5840f0] dark:bg-[#7c6af0]')}
            style={{ width: `${Math.max(3, value * 100)}%` }}
          />
        )}
      </span>
      <span className={cn('tabular w-9 text-right text-xs', low ? 'text-danger' : 'text-default-500')}>{value === null ? t('inspector.notAvailable') : formatPercent(value)}</span>
    </div>
  )
}

function Field({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="min-w-0">
      <dt className="text-[11px] text-default-400">{label}</dt>
      <dd className="selectable truncate text-[13px] text-foreground">{value ?? '—'}</dd>
    </div>
  )
}

const BREAKDOWN: (keyof ScoreBreakdown)[] = ['focus', 'exposure', 'eyes', 'aesthetic', 'uniqueness']

/** Right panel of the loupe: decision and reasons, score, close-ups, EXIF, file. */
export function Inspector({ photo, previewUrl, onFocusFace, onHoverFace }: { photo: PhotoView; previewUrl: string | null; onFocusFace: (face: FaceView) => void; onHoverFace: (index: number | null) => void }) {
  const t = useT()
  const group = useReviewStore((s) => {
    const gid = photo.assessment?.groupId
    return gid === null || gid === undefined ? undefined : s.groups[gid]
  })
  const a = photo.assessment
  const Icon = DECISION_ICON[photo.decision]
  const userFlag = photo.mark.flag !== 'none'
  const aspect = photo.width && photo.height ? photo.width / photo.height : 1.5
  const megapixels = photo.width && photo.height ? (photo.width * photo.height) / 1e6 : null
  const ext = photo.fileName.split('.').pop()?.toUpperCase() ?? ''

  return (
    <aside aria-label={t('inspector.title')} className="flex h-full w-[340px] shrink-0 flex-col overflow-y-auto border-l border-default-200/60 bg-background dark:border-white/[0.06]">
      <Section title={t('inspector.decision')}>
        {photo.status !== 'analyzed' ? (
          <p className="flex items-center gap-2 text-[13px] text-default-500">
            <Loader className="size-4 animate-spin" aria-hidden />
            {t('inspector.pending')}
          </p>
        ) : (
          <>
            <div className="flex items-start justify-between gap-3">
              <div>
                <p className={cn('flex items-center gap-2 text-lg font-semibold', DECISION_TEXT[photo.decision])}>
                  <Icon className="size-5" aria-hidden />
                  {t(`verdict.${photo.decision}`)}
                </p>
                <p className="mt-0.5 text-xs text-default-500">
                  {userFlag ? t('verdict.yourChoice') : a ? t(`verdict.${a.verdict}`) : ''}
                  {userFlag && a && a.verdict !== (photo.decision === 'selected' ? 'keep' : photo.decision === 'rejected' ? 'reject' : 'maybe') && (
                    <span className="text-default-400"> · {t('verdict.overridden')}</span>
                  )}
                </p>
              </div>
              {a && (
                <div className="text-right">
                  <p className="tabular text-[26px] font-semibold leading-none text-foreground">{formatNumber(Math.round(a.score))}</p>
                  <p className="mt-1 text-[10px] uppercase tracking-wide text-default-400">{t('inspector.score')}</p>
                </div>
              )}
            </div>
            <div className="mt-4">
              {a && a.reasons.length > 0 ? (
                <ul className="space-y-2.5">
                  {a.reasons.map((r) => (
                    <li key={r} className="flex gap-2.5">
                      <span className="mt-0.5 flex size-6 shrink-0 items-center justify-center rounded-md bg-default-100 text-default-600 dark:bg-white/[0.06]">
                        <ReasonIcon reason={r} className="size-3.5" />
                      </span>
                      <span>
                        <span className="block text-[13px] font-medium text-foreground">{t(`reason.${r}`)}</span>
                        <span className="block text-xs leading-snug text-default-500">{t(`reasonDetail.${r}`)}</span>
                      </span>
                    </li>
                  ))}
                </ul>
              ) : (
                <p className="flex items-center gap-2 text-[13px] text-success">
                  <CircleCheck className="size-4" aria-hidden />
                  {t('inspector.noIssues')}
                </p>
              )}
            </div>
            {a && (
              <div className="mt-5 space-y-2.5">
                {BREAKDOWN.map((k) => (
                  <Bar key={k} label={t(`inspector.breakdown.${k}`)} value={a.breakdown[k]} />
                ))}
              </div>
            )}
          </>
        )}
      </Section>

      <Section title={t('inspector.closeUps')} aside={photo.faces.length > 0 && <span className="tabular text-xs text-default-400">{photo.faces.length}</span>}>
        <CloseUps faces={photo.faces} src={previewUrl ?? photo.thumbUrl} aspect={aspect} onFocusFace={onFocusFace} onHover={onHoverFace} />
      </Section>

      {group && group.photoIds.length > 1 && (
        <Section title={t('inspector.group')} aside={<span className="text-xs text-default-500">{t('inspector.groupRank', { rank: group.photoIds.indexOf(photo.id) + 1, count: group.photoIds.length })}</span>}>
          <GroupStrip ids={group.photoIds} current={photo.id} />
          <Button
            size="sm"
            variant="flat"
            radius="lg"
            className="mt-3 w-full bg-[#5840f0]/10 text-[13px] text-[#5840f0] hover:bg-[#5840f0]/20 dark:bg-[#5840f0]/20 dark:text-[#c3b9fa]"
            startContent={<Columns2 className="size-4" />}
            onPress={() => useWorkspaceStore.getState().openCompare(group.photoIds)}
          >
            {t('inspector.compareGroup')} <kbd className="ml-1 font-sans text-[11px] opacity-60">C</kbd>
          </Button>
        </Section>
      )}

      <Section title={t('inspector.capture')}>
        <dl className="grid grid-cols-2 gap-x-4 gap-y-3">
          <Field label={t('inspector.camera')} value={photo.exif.camera} />
          <Field label={t('inspector.lens')} value={photo.exif.lens} />
          <Field label={t('inspector.iso')} value={photo.exif.iso !== null ? formatNumber(photo.exif.iso) : null} />
          <Field label={t('inspector.shutter')} value={photo.exif.shutter ? `${photo.exif.shutter} s` : null} />
          <Field label={t('inspector.aperture')} value={formatAperture(photo.exif.aperture)} />
          <Field label={t('inspector.focal')} value={formatFocal(photo.exif.focalLength)} />
          <div className="col-span-2">
            <Field label={t('inspector.date')} value={photo.capturedAt !== null ? formatCaptureDate(photo.capturedAt) : null} />
          </div>
        </dl>
      </Section>

      <Section title={t('inspector.file')}>
        <dl className="grid grid-cols-2 gap-x-4 gap-y-3">
          <div className="col-span-2">
            <Field label={t('inspector.path')} value={<span className="font-mono text-xs">{photo.relPath}</span>} />
          </div>
          <Field label={t('inspector.format')} value={photo.hasRaw && photo.hasJpeg ? t('inspector.rawJpeg') : ext} />
          <Field label={t('inspector.size')} value={formatBytes(photo.sizeBytes)} />
          <div className="col-span-2">
            <Field
              label={t('inspector.dimensions')}
              value={
                photo.width && photo.height
                  ? `${formatNumber(photo.width)} × ${formatNumber(photo.height)}${megapixels ? ` · ${t('inspector.megapixels', { value: formatNumber(megapixels, undefined, { maximumFractionDigits: 1 }) })}` : ''}`
                  : null
              }
            />
          </div>
        </dl>
      </Section>
    </aside>
  )
}

function GroupStrip({ ids, current }: { ids: string[]; current: string }) {
  const photos = useReviewStore((s) => s.photos)
  return (
    <div className="flex gap-1.5 overflow-x-auto pb-1">
      {ids.slice(0, 12).map((id, i) => {
        const p = photos[id]
        if (!p) return null
        return (
          <button
            key={id}
            type="button"
            onClick={() => useReviewStore.getState().focus(id)}
            aria-label={p.fileName}
            className={cn(
              'focus-ring photo-well relative h-12 w-[68px] shrink-0 overflow-hidden rounded-md transition-opacity',
              id === current ? 'ring-2 ring-[#5840f0] dark:ring-[#a599f5]' : 'opacity-70 hover:opacity-100',
            )}
          >
            {p.thumbUrl && <img src={p.thumbUrl} alt="" className={cn('h-full w-full object-cover', p.decision === 'rejected' && 'opacity-50')} draggable={false} />}
            <span className="absolute left-1 top-1 rounded bg-black/55 px-1 text-[9px] font-semibold text-white tabular">{i + 1}</span>
          </button>
        )
      })}
    </div>
  )
}
